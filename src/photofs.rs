extern crate fuse;
extern crate libc;
extern crate time;

use std::ffi::OsStr;

use photolib::*;

use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use time::Timespec;

const FIXED_INODE_ROOT: u64 = fuse::FUSE_ROOT_ID;
const FIXED_INODE_ALBUMS: u64 = 2;
const FIXED_INODE_HELLO_WORLD: u64 = 3;

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

pub struct PhotoFs<X>
where
    X: PhotoLib,
{
    photo_lib: X,
}

impl<X> PhotoFs<X>
where
    X: PhotoLib,
{
    pub fn new(photo_lib: X) -> PhotoFs<X> {
        PhotoFs {
            photo_lib
        }
    }
}

impl<X> Filesystem for PhotoFs<X>
where
    X: PhotoLib,
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
            } else {
                debug!("Respond error");
                reply.error(ENOENT);
            }
        } else if parent == FIXED_INODE_ALBUMS {
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

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        debug!("FS readdir: ino={}, offset={}", ino, offset);

        // TODO: Error when not known inode
        // reply.error(ENOENT);

        let entries: Vec<(u64, fuse::FileType, String)> = if ino == FIXED_INODE_ROOT {
            vec![
                (FIXED_INODE_ROOT, FileType::Directory, String::from(".")),
                (FIXED_INODE_ROOT, FileType::Directory, String::from("..")),
                (
                    FIXED_INODE_ALBUMS,
                    FileType::RegularFile,
                    String::from("albums"),
                ),
                (
                    FIXED_INODE_HELLO_WORLD,
                    FileType::RegularFile,
                    String::from("hello.txt"),
                ),
            ]
        } else if ino == FIXED_INODE_ALBUMS {
            let mut entries = Vec::new();
            let result = self.photo_lib.list_albums();
            match result {
                Ok(album_titles) => {
                    debug!("Success: listing albums");
                    for album_title in album_titles.iter() {
                        debug!("album_title: {}", album_title);
                        entries.push((FIXED_INODE_HELLO_WORLD, FileType::RegularFile, album_title.clone()));
                    }
                }
                Err(error) => {
                    warn!("Failed backend listing albums: {:?}", error);
                }
            }
            entries
        } else {
            Vec::new()
        };

        let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;
        for (offset, entry) in entries.into_iter().enumerate().skip(to_skip) {
            debug!("Adding to response");
            let is_full = reply.add(entry.0, offset as i64, entry.1, entry.2);
            if is_full {
                debug!("is_full");
                break;
            }
        }
        reply.ok();
    }
}
