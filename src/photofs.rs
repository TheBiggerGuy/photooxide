extern crate fuse;
extern crate libc;
extern crate time;

extern crate rusqlite;

extern crate users;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;

use db::{DbError, PhotoDb};
use domain::Inode;
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

const HELLO_TXT_CONTENT: &str = "Hello World!\n";

const GENERATION: u64 = 0;

fn make_atr(inode: Inode, size: usize, file_type: FileType) -> FileAttr {
    FileAttr {
        ino: inode,
        size: size as u64,
        blocks: 1,
        atime: CREATE_TIME,
        mtime: CREATE_TIME,
        ctime: CREATE_TIME,
        crtime: CREATE_TIME,
        kind: file_type,
        perm: 0o644,
        nlink: 1,
        uid: users::get_current_uid(),
        gid: 20,
        rdev: 0,
        flags: 0,
    }
}

#[derive(Debug)]
pub enum PhotoFsError {
    SqlError(rusqlite::Error),
    LockingError,
    NotImpYet,
}

impl From<DbError> for PhotoFsError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::SqlError(sql_error) => PhotoFsError::SqlError(sql_error),
            DbError::LockingError => PhotoFsError::LockingError,
            DbError::NotImpYet => PhotoFsError::NotImpYet,
        }
    }
}

pub struct PhotoFs<X, Y>
where
    X: RemotePhotoLib,
    Y: PhotoDb,
{
    photo_lib: Arc<Mutex<X>>,
    photo_db: Arc<Y>,
    open_dirs: HashMap<u64, Vec<(u64, fuse::FileType, String)>>,
}

impl<X, Y> PhotoFs<X, Y>
where
    X: RemotePhotoLib,
    Y: PhotoDb,
{
    pub fn new(photo_lib: Arc<Mutex<X>>, photo_db: Arc<Y>) -> PhotoFs<X, Y> {
        PhotoFs {
            photo_lib,
            photo_db,
            open_dirs: HashMap::new(),
        }
    }

    fn lookup_root(&mut self, _req: &Request, name: &OsStr, reply: ReplyEntry) {
        match name.to_str().unwrap() {
            "hello.txt" => reply.entry(
                &TTL,
                &make_atr(
                    FIXED_INODE_HELLO_WORLD,
                    HELLO_TXT_CONTENT.len(),
                    FileType::RegularFile,
                ),
                GENERATION,
            ),
            "albums" => reply.entry(
                &TTL,
                &make_atr(
                    FIXED_INODE_ALBUMS,
                    0,
                    FileType::Directory,
                ),
                GENERATION,
            ),
            "media" => reply.entry(
                &TTL,
                &make_atr(
                    FIXED_INODE_MEDIA,
                    0,
                    FileType::Directory,
                ),
                GENERATION,
            ),
            _ => {
                error!(
                    "lookup: Failed to find a FileAttr for name={:?} in root",
                    name
                );
                reply.error(ENOENT);
                return;
            }
        }
    }

    fn lookup_albums(&mut self, _req: &Request, name: &OsStr, reply: ReplyEntry) {
        let name = name.to_str().unwrap();
        match self.photo_db.album(&String::from(name)) {
            Ok(_album) => reply.entry(
                &TTL,
                &make_atr(
                    FIXED_INODE_HELLO_WORLD,
                    HELLO_TXT_CONTENT.len(),
                    FileType::RegularFile,
                ),
                GENERATION,
            ),
            Err(error) => {
                error!(
                    "lookup: Failed to find a FileAttr for name={:?} in albums: {:?}",
                    name, error
                );
                reply.error(ENOENT);
                return;
            }
        }
    }

    fn lookup_media(&mut self, _req: &Request, name: &OsStr, reply: ReplyEntry) {
        let name = name.to_str().unwrap();
        match self.photo_db.media_item_by_name(&String::from(name)) {
            Ok(_media_item) => reply.entry(
                &TTL,
                &make_atr(
                    FIXED_INODE_HELLO_WORLD,
                    HELLO_TXT_CONTENT.len(),
                    FileType::RegularFile,
                ),
                GENERATION,
            ),
            Err(error) => {
                error!(
                    "lookup: Failed to find a FileAttr for name={:?} in albums: {:?}",
                    name, error
                );
                reply.error(ENOENT);
                return;
            }
        }
    }
}

impl<X, Y> Filesystem for PhotoFs<X, Y>
where
    X: RemotePhotoLib,
    Y: PhotoDb,
{
    fn lookup(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        match parent {
            FIXED_INODE_ROOT => self.lookup_root(req, name, reply),
            FIXED_INODE_ALBUMS => self.lookup_albums(req, name, reply),
            FIXED_INODE_MEDIA => self.lookup_media(req, name, reply),
            _ => {
                error!(
                    "lookup: Failed to find a FileAttr for inode={} (name={:?})",
                    parent, name
                );
                reply.error(ENOENT);
                return;
            }
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        debug!("FS getattr: ino={}", ino);
        match ino {
            FIXED_INODE_ROOT => {
                reply.attr(&TTL, &make_atr(FIXED_INODE_ROOT, 0, FileType::Directory))
            }
            FIXED_INODE_ALBUMS => {
                reply.attr(&TTL, &make_atr(FIXED_INODE_ALBUMS, 0, FileType::Directory))
            }
            FIXED_INODE_MEDIA => {
                reply.attr(&TTL, &make_atr(FIXED_INODE_MEDIA, 0, FileType::Directory))
            }
            FIXED_INODE_HELLO_WORLD => reply.attr(
                &TTL,
                &make_atr(
                    FIXED_INODE_HELLO_WORLD,
                    HELLO_TXT_CONTENT.len(),
                    FileType::RegularFile,
                ),
            ),
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
            return
        }

        match self.photo_db.media_item_by_inode(ino) {
            Err(error) => {
                error!("FS read: Failed to lookup media item in local db: {:?}", error);
                reply.error(ENOENT);
            },
            Ok(Option::None) => {
                warn!("FS read: No media items found in local DB: {:?}", ino);
                reply.error(ENOENT);
            },
            Ok(Option::Some(media_item)) => {
                let photo_lib = self.photo_lib.lock().unwrap();
                match photo_lib.media_item(media_item.google_id) {
                    Err(error) => {
                        error!("FS read: Failed to fetch media item from remote: {:?}", error);
                        reply.error(ENOENT);
                    },
                    Ok(data) => {
                        reply.data(&data[offset as usize..]);  
                    },
                }
            },
        }
    }

    fn opendir(&mut self, _req: &Request, ino: u64, _flags: u32, reply: ReplyOpen) {
        if ino != FIXED_INODE_ROOT && ino != FIXED_INODE_MEDIA && ino != FIXED_INODE_ALBUMS {
            error!("FS readdir is for ?? error (ino={})", ino);
            reply.error(ENOENT);
            return;
        }

        let mut entries: Vec<(u64, fuse::FileType, String)> = Vec::new();
        entries.push((ino, FileType::Directory, String::from(".")));

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
            entries.push((FIXED_INODE_ROOT, FileType::Directory, String::from("..")));
            let albums = self.photo_db.albums();
            let mut albums_dedupe = HashSet::new();
            match albums {
                Ok(albums) => {
                    debug!("Success: listing albums");
                    for album in albums {
                        debug!("album: {:?}", album);
                        if albums_dedupe.contains(&album.name) {
                            continue;
                        }
                        albums_dedupe.insert(album.name.clone());
                        let entry = (
                            FIXED_INODE_HELLO_WORLD,
                            FileType::RegularFile,
                            album.name.clone(),
                        );
                        entries.push(entry);
                    }
                }
                Err(error) => {
                    warn!("Failed backend listing albums: {:?}", error);
                }
            }
        } else if ino == FIXED_INODE_MEDIA {
            debug!("FS readdir is for media");
            entries.push((FIXED_INODE_ROOT, FileType::Directory, String::from("..")));
            let media_items = self.photo_db.media_items();
            let mut media_items_dedupe = HashSet::new();
            match media_items {
                Ok(media_items) => {
                    debug!("Success: listing media");
                    for media_item in media_items {
                        debug!("media_item: {:?}", media_item);
                        if media_items_dedupe.contains(&media_item.name) {
                            continue;
                        }
                        media_items_dedupe.insert(media_item.name.clone());
                        let entry = (
                            FIXED_INODE_HELLO_WORLD,
                            FileType::RegularFile,
                            media_item.name.clone(),
                        );
                        entries.push(entry);
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
                fh += 1;
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
