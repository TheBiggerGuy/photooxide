extern crate fuse;
extern crate libc;
extern crate time;

extern crate rusqlite;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::rc::Rc;

use db::{DbError, InodeDb};
use photolib::*;

use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry,
    ReplyOpen, Request,
};
use libc::ENOENT;
use time::Timespec;

const FIXED_INODE_ROOT: u64 = fuse::FUSE_ROOT_ID;
const FIXED_INODE_ALBUMS: u64 = 2;
const FIXED_INODE_MEDIA: u64 = 3;
const FIXED_INODE_HELLO_WORLD: u64 = 4;

const TTL: Timespec = Timespec { sec: 120, nsec: 0 }; // 2 minutes

const CREATE_TIME: Timespec = Timespec {
    sec: 1_381_237_736,
    nsec: 0,
}; // 2013-10-08 08:56

const HELLO_DIR_ATTR: FileAttr = FileAttr {
    ino: FIXED_INODE_ROOT,
    size: 0,
    blocks: 0,
    atime: CREATE_TIME,
    mtime: CREATE_TIME,
    ctime: CREATE_TIME,
    crtime: CREATE_TIME,
    kind: FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
};

const ALBUMS_DIR_ATTR: FileAttr = FileAttr {
    ino: FIXED_INODE_ALBUMS,
    size: 0,
    blocks: 0,
    atime: CREATE_TIME,
    mtime: CREATE_TIME,
    ctime: CREATE_TIME,
    crtime: CREATE_TIME,
    kind: FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
};

const MEDIA_DIR_ATTR: FileAttr = FileAttr {
    ino: FIXED_INODE_MEDIA,
    size: 0,
    blocks: 0,
    atime: CREATE_TIME,
    mtime: CREATE_TIME,
    ctime: CREATE_TIME,
    crtime: CREATE_TIME,
    kind: FileType::Directory,
    perm: 0o755,
    nlink: 2,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
};

const HELLO_TXT_CONTENT: &str = "Hello World!\n";

const HELLO_TXT_ATTR: FileAttr = FileAttr {
    ino: FIXED_INODE_HELLO_WORLD,
    size: 13,
    blocks: 1,
    atime: CREATE_TIME,
    mtime: CREATE_TIME,
    ctime: CREATE_TIME,
    crtime: CREATE_TIME,
    kind: FileType::RegularFile,
    perm: 0o644,
    nlink: 1,
    uid: 501,
    gid: 20,
    rdev: 0,
    flags: 0,
};

const GENERATION: u64 = 0;

#[derive(Debug)]
pub enum PhotoFsError {
    SqlError(rusqlite::Error),
}

impl From<DbError> for PhotoFsError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::SqlError(sql_error) => PhotoFsError::SqlError(sql_error),
        }
    }
}

pub struct PhotoFs<X, Y>
where
    X: PhotoLib,
    Y: InodeDb,
{
    photo_lib: X,
    inode_db: Rc<Y>,
    open_dirs: HashMap<u64, Vec<(u64, fuse::FileType, String)>>,
}

impl<X, Y> PhotoFs<X, Y>
where
    X: PhotoLib,
    Y: InodeDb,
{
    pub fn new(photo_lib: X, inode_db: Rc<Y>) -> Result<PhotoFs<X, Y>, PhotoFsError> {
        inode_db.insert(FIXED_INODE_ROOT, FIXED_INODE_ROOT, String::from(""))?;
        inode_db.insert(FIXED_INODE_ALBUMS, FIXED_INODE_ROOT, String::from("albums"))?;
        inode_db.insert(FIXED_INODE_MEDIA, FIXED_INODE_ROOT, String::from("media"))?;
        inode_db.insert(
            FIXED_INODE_HELLO_WORLD,
            FIXED_INODE_ROOT,
            String::from("hello.txt"),
        )?;
        Result::Ok(PhotoFs {
            photo_lib,
            inode_db,
            open_dirs: HashMap::new(),
        })
    }
}

impl<X, Y> Filesystem for PhotoFs<X, Y>
where
    X: PhotoLib,
    Y: InodeDb,
{
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        debug!("FS lookup: parent={}, name={:?}", parent, name);
        if parent == FIXED_INODE_ROOT {
            if name.to_str() == Some("hello.txt") {
                debug!("Respond hello.txt");
                reply.entry(&TTL, &HELLO_TXT_ATTR, GENERATION);
            } else if name.to_str() == Some("albums") {
                debug!("Respond albums");
                reply.entry(&TTL, &ALBUMS_DIR_ATTR, GENERATION);
            } else if name.to_str() == Some("media") {
                debug!("Respond media");
                reply.entry(&TTL, &MEDIA_DIR_ATTR, GENERATION);
            } else {
                debug!("Respond error");
                reply.error(ENOENT);
            }
        } else if parent == FIXED_INODE_ALBUMS {
            debug!("Respond hello.txt");
            reply.entry(&TTL, &HELLO_TXT_ATTR, GENERATION);
        } else if parent == FIXED_INODE_MEDIA {
            debug!("Respond hello.txt");
            reply.entry(&TTL, &HELLO_TXT_ATTR, GENERATION);
        } else {
            debug!("Respond error");
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        debug!("FS getattr: ino={}", ino);
        match ino {
            FIXED_INODE_ROOT => reply.attr(&TTL, &HELLO_DIR_ATTR),
            FIXED_INODE_ALBUMS => reply.attr(&TTL, &ALBUMS_DIR_ATTR),
            FIXED_INODE_MEDIA => reply.attr(&TTL, &MEDIA_DIR_ATTR),
            FIXED_INODE_HELLO_WORLD => reply.attr(&TTL, &HELLO_TXT_ATTR),
            _ => reply.error(ENOENT),
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        _size: u32,
        reply: ReplyData,
    ) {
        debug!("FS read: ino={}, offset={}", ino, offset);
        if ino == FIXED_INODE_HELLO_WORLD {
            reply.data(&HELLO_TXT_CONTENT.as_bytes()[offset as usize..]);
        } else {
            reply.error(ENOENT);
        }
    }

    fn opendir(&mut self, _req: &Request, ino: u64, _flags: u32, reply: ReplyOpen) {
        let parent_inode_lookup = self.inode_db.parent(ino);
        let parent_inode;
        match parent_inode_lookup {
            Err(error) => {
                error!("opendir: inode lookup failed: {:?}", error);
                reply.error(ENOENT);
                return;
            }
            Ok(parent_inode_option) => {
                match parent_inode_option {
                    None => {
                        error!(
                            "opendir: inode lookup failed: {:?}",
                            parent_inode_lookup.unwrap_err()
                        );
                        reply.error(ENOENT);
                        return;
                    }
                    Some(parent_inode_2) => {
                        parent_inode = parent_inode_2;
                    }
                };
            }
        };

        if ino != FIXED_INODE_ROOT && ino != FIXED_INODE_MEDIA && ino != FIXED_INODE_ALBUMS {
            error!("FS readdir is for ?? error (ino={})", ino);
            reply.error(ENOENT);
            return;
        }

        let mut entries: Vec<(u64, fuse::FileType, String)> = Vec::new();
        entries.push((parent_inode, FileType::Directory, String::from(".")));
        entries.push((parent_inode, FileType::Directory, String::from("..")));

        if ino == FIXED_INODE_ROOT {
            debug!("FS readdir is for root");
            entries.push((
                FIXED_INODE_ALBUMS,
                FileType::Directory,
                String::from("albums"),
            ));
            entries.push((
                FIXED_INODE_MEDIA,
                FileType::Directory,
                String::from("media"),
            ));
            entries.push((
                FIXED_INODE_HELLO_WORLD,
                FileType::RegularFile,
                String::from("hello.txt"),
            ));
        } else if ino == FIXED_INODE_ALBUMS {
            debug!("FS readdir is for albums");
            let result = self.photo_lib.albums();
            match result {
                Ok(album_titles) => {
                    debug!("Success: listing albums");
                    for album_title in album_titles {
                        debug!("album_title: {}", album_title);
                        entries.push((
                            FIXED_INODE_HELLO_WORLD,
                            FileType::RegularFile,
                            album_title.clone(),
                        ));
                    }
                }
                Err(error) => {
                    warn!("Failed backend listing albums: {:?}", error);
                }
            }
        } else if ino == FIXED_INODE_MEDIA {
            debug!("FS readdir is for media");
            let result = self.photo_lib.media();
            match result {
                Ok(media_filenames) => {
                    debug!("Success: listing media");
                    for media_filename in media_filenames {
                        debug!("media_filename: {}", media_filename);
                        entries.push((
                            FIXED_INODE_HELLO_WORLD,
                            FileType::RegularFile,
                            media_filename.clone(),
                        ));
                    }
                }
                Err(error) => {
                    warn!("Failed backend listing media: {:?}", error);
                }
            }
        } else {
            debug!("FS readdir is for ?? error");
            reply.error(ENOENT);
            return;
        };

        let mut fh = ino;
        loop {
            if self.open_dirs.contains_key(&fh) {
                fh = fh + 1;
            } else {
                break;
            }
        }

        self.open_dirs.insert(fh, entries);
        reply.opened(fh, 0); // TODO: Flags
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        debug!("FS readdir: ino={}, offset={}", ino, offset);

        let dir_context_option = self.open_dirs.get(&fh);
        if dir_context_option.is_none() {
            reply.error(ENOENT);
            return;
        }
        let entries = dir_context_option.unwrap();

        // TODO: Error when not known inode
        // reply.error(ENOENT);

        let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;
        for (offset, entry) in entries.into_iter().enumerate().skip(to_skip) {
            debug!("Adding to response");
            let ino = entry.0;
            let kind = entry.1;
            let name = entry.2.clone();
            let is_full = reply.add(ino, offset as i64, kind, name);
            if is_full {
                info!("is_full: to_skip={} offset={}", to_skip, offset);
                break;
            }
        }
        reply.ok();
    }

    fn releasedir(&mut self, _req: &Request, _ino: u64, fh: u64, _flags: u32, reply: ReplyEmpty) {
        self.open_dirs.remove(&fh);
        reply.ok();
    }
}
