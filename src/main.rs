#[macro_use]
extern crate log;

extern crate env_logger;
extern crate fuse;
extern crate google_photoslibrary1 as photoslibrary1;
extern crate hyper;
extern crate hyper_rustls;
extern crate libc;
extern crate serde;
extern crate serde_json;
extern crate time;
extern crate yup_oauth2 as oauth2;

use std::env;
use std::ffi::OsStr;
use std::option::Option;

use oauth2::{
    Authenticator, ConsoleApplicationSecret, DefaultAuthenticatorDelegate, DiskTokenStorage,
    FlowType,
};
use photoslibrary1::{Error, PhotosLibrary};
use serde_json as json;

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
    sec: 1381237736,
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

const HELLO_TXT_CONTENT: &'static str = "Hello World!\n";

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
use std::borrow::BorrowMut;

const CLIENT_SECRET: &'static str = include_str!("../client_secret.json");

const GENERATION: u64 = 0;

struct PhotoFs<'a, C: 'a, A: 'a>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    photos_library: &'a mut PhotosLibrary<C, A>,
}

impl<'a, C, A> Filesystem for PhotoFs<'a, C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
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

        if ino == FIXED_INODE_ROOT {
            let entries = vec![
                (FIXED_INODE_ROOT, FileType::Directory, "."),
                (FIXED_INODE_ROOT, FileType::Directory, ".."),
                (FIXED_INODE_ALBUMS, FileType::RegularFile, "albums"),
                (FIXED_INODE_HELLO_WORLD, FileType::RegularFile, "hello.txt"),
            ];

            // Offset of 0 means no offset.
            // Non-zero offset means the passed offset has already been seen, and we should start after
            // it.
            let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;
            for (offset, entry) in entries.into_iter().enumerate().skip(to_skip) {
                debug!("Adding to response");
                reply.add(entry.0, offset as i64, entry.1, entry.2);
            }
            reply.ok();
        } else if ino == FIXED_INODE_ALBUMS {
            // You can configure optional parameters by calling the respective setters at will, and
            // execute the final call using `doit()`.
            // Values shown here are possibly random and not representative !
            let result = self.photos_library.albums().list().page_size(50).doit();

            match result {
                Err(e) => match e {
                    // The Error enum provides details about what exactly happened.
                    // You can also just use its `Debug`, `Display` or `Error` traits
                    Error::HttpError(_)
                    | Error::MissingAPIKey
                    | Error::MissingToken(_)
                    | Error::Cancelled
                    | Error::UploadSizeLimitExceeded(_, _)
                    | Error::Failure(_)
                    | Error::BadRequest(_)
                    | Error::FieldClash(_)
                    | Error::JsonDecodeError(_, _) => debug!("{}", e),
                },
                Ok(res) => {
                    debug!("Success: listing albums");
                    let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;
                    for (offset, album) in res.1.albums.unwrap_or(Vec::new()).into_iter().enumerate().skip(to_skip) {
                        debug!("album={:?}", album.title);
                        let is_full = reply.add(FIXED_INODE_HELLO_WORLD, offset as i64, FileType::RegularFile, album.title.unwrap());
                        if is_full {
                            debug!("is_full");
                            break;
                        }
                    }
                },
            }

            reply.ok();
        } else {
            reply.error(ENOENT);
        }
        return;
    }
}

fn main() {
    env_logger::init();
    println!("Hello, world!");
    debug!("Hello, world!");

    // Get an ApplicationSecret instance by some means. It contains the `client_id` and
    // `client_secret`, among other things.
    let secret = json::from_str::<ConsoleApplicationSecret>(CLIENT_SECRET)
        .unwrap()
        .installed
        .unwrap();

    let token_storage = DiskTokenStorage::new(&"token_storage.json".to_string()).unwrap();
    let auth = Authenticator::new(
        &secret,
        DefaultAuthenticatorDelegate,
        hyper::Client::with_connector(hyper::net::HttpsConnector::new(
            hyper_rustls::TlsClient::new(),
        )),
        token_storage,
        Option::Some(FlowType::InstalledInteractive),
    );

    let http_client = hyper::Client::with_connector(hyper::net::HttpsConnector::new(
        hyper_rustls::TlsClient::new(),
    ));

    let mut hub = PhotosLibrary::new(http_client, auth);

    let fs = PhotoFs {
        photos_library: &mut hub,
    };

    let mountpoint = env::args_os().nth(1).unwrap();
    let options = ["-o", "ro", "-o", "fsname=hello"]
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();

    fuse::mount(fs, &mountpoint, &options).unwrap();
}
