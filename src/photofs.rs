extern crate fuse;
extern crate libc;
extern crate time;

extern crate rusqlite;

extern crate users;

use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};

use rust_filesystem::{
    FileAttrResponse, FileEntryResponse, FuseError, FuseResult, OpenResponse, ReadDirEntry,
    ReadDirResponse, ReadResponse,
};

use db::{DbError, PhotoDb};
use domain::{Inode, MediaTypes, PhotoDbAlbum};
use photolib::*;
use rust_filesystem::RustFilesystem;

use fuse::{FileAttr, FileType, Request};
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
}

impl From<DbError> for PhotoFsError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::SqlError(sql_error) => PhotoFsError::SqlError(sql_error),
            DbError::LockingError => PhotoFsError::LockingError,
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
    open_files: HashMap<u64, Vec<u8>>,
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
            open_files: HashMap::new(),
            open_dirs: HashMap::new(),
        }
    }

    fn lookup_root(&mut self, _req: &Request, name: &OsStr) -> FuseResult<FileEntryResponse> {
        match name.to_str().unwrap() {
            "hello.txt" => Result::Ok(FileEntryResponse {
                ttl: &TTL,
                attr: make_atr(
                    FIXED_INODE_HELLO_WORLD,
                    HELLO_TXT_CONTENT.len(),
                    FileType::RegularFile,
                ),
                generation: GENERATION,
            }),
            "albums" => Result::Ok(FileEntryResponse {
                ttl: &TTL,
                attr: make_atr(FIXED_INODE_ALBUMS, 0, FileType::Directory),
                generation: GENERATION,
            }),
            "media" => Result::Ok(FileEntryResponse {
                ttl: &TTL,
                attr: make_atr(FIXED_INODE_MEDIA, 0, FileType::Directory),
                generation: GENERATION,
            }),
            _ => {
                warn!(
                    "lookup: Failed to find a FileAttr for name={:?} in root",
                    name
                );
                Result::Err(FuseError::FunctionNotImplemented)
            }
        }
    }

    fn lookup_albums(&mut self, _req: &Request, name: &OsStr) -> FuseResult<FileEntryResponse> {
        let name = name.to_str().unwrap();
        match self.photo_db.album_by_name(&String::from(name)) {
            Ok(Option::Some(album)) => Result::Ok(FileEntryResponse {
                ttl: &TTL,
                attr: make_atr(album.inode, 0, FileType::Directory),
                generation: GENERATION,
            }),
            Ok(Option::None) => {
                warn!(
                    "lookup: Failed to find a FileAttr for name={:?} in albums",
                    name
                );
                Result::Err(FuseError::FunctionNotImplemented)
            }
            Err(error) => {
                warn!(
                    "lookup: Failed to find a FileAttr for name={:?} in albums: {:?}",
                    name, error
                );
                Result::Err(FuseError::FunctionNotImplemented)
            }
        }
    }

    // TODO: Check photo by name is actually in that album
    fn lookup_media_items_in_album(
        &mut self,
        _req: &Request,
        name: &OsStr,
    ) -> FuseResult<FileEntryResponse> {
        self.lookup_media(_req, name)
    }

    fn lookup_media(&mut self, _req: &Request, name: &OsStr) -> FuseResult<FileEntryResponse> {
        let name = name.to_str().unwrap();
        match self.photo_db.media_item_by_name(&String::from(name)) {
            Ok(Option::Some(media_item)) => Result::Ok(FileEntryResponse {
                ttl: &TTL,
                attr: make_atr(media_item.inode, 0, FileType::RegularFile),
                generation: GENERATION,
            }),
            Ok(Option::None) => {
                warn!(
                    "lookup: Failed to find a FileAttr for name={:?} in media",
                    name
                );
                Result::Err(FuseError::FunctionNotImplemented)
            }
            Err(error) => {
                error!(
                    "lookup: Failed to find a FileAttr for name={:?} in media WITH ERROR: {:?}",
                    name, error
                );
                Result::Err(FuseError::FunctionNotImplemented)
            }
        }
    }

    fn opendir_entries(
        &mut self,
        ino: u64,
        album_for_inode: &Option<PhotoDbAlbum>,
    ) -> Vec<(u64, fuse::FileType, String)> {
        let mut entries: Vec<(u64, fuse::FileType, String)> = Vec::new();
        entries.push((ino, FileType::Directory, String::from(".")));

        if ino == FIXED_INODE_ROOT {
            debug!("FS opendir: is for root");
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
            debug!("FS opendir: is for albums");
            entries.push((FIXED_INODE_ROOT, FileType::Directory, String::from("..")));
            let albums = self.photo_db.albums();
            let mut albums_dedupe = HashSet::new();
            match albums {
                Ok(albums) => {
                    debug!("FS opendir: Success: listing albums");
                    for album in albums {
                        debug!("FS opendir: \talbum: {:?}", album);
                        if albums_dedupe.insert(album.name.clone()) {
                            let entry = (album.inode, FileType::Directory, album.name.clone());
                            entries.push(entry);
                        } else {
                            warn!("FS opendir: skipping {} as duplicate name", album.name);
                        }
                    }
                }
                Err(error) => {
                    warn!("Failed backend listing albums: {:?}", error);
                }
            }
        } else if ino == FIXED_INODE_MEDIA || album_for_inode.is_some() {
            let media_items = if ino == FIXED_INODE_MEDIA {
                debug!("FS opendir: is for media");
                entries.push((FIXED_INODE_ROOT, FileType::Directory, String::from("..")));
                self.photo_db.media_items()
            } else {
                debug!("FS opendir: is for media in album");
                entries.push((FIXED_INODE_ALBUMS, FileType::Directory, String::from("..")));
                self.photo_db.media_items_in_album(ino)
            };
            let mut media_items_dedupe = HashSet::new();
            match media_items {
                Ok(media_items) => {
                    debug!(
                        "FS opendir: Success listing media len={}",
                        media_items.len()
                    );
                    for media_item in media_items {
                        debug!("media_item: {:?}", media_item);
                        if media_items_dedupe.insert(media_item.name.clone()) {
                            let entry = (
                                media_item.inode,
                                FileType::RegularFile,
                                media_item.name.clone(),
                            );
                            entries.push(entry);
                        } else {
                            warn!("FS opendir: skipping {} as duplicate name", media_item.name);
                        }
                    }
                }
                Err(error) => {
                    warn!("Failed backend listing media: {:?}", error);
                }
            }
        } else {
            panic!("Code should never reach this location");
        };

        entries
    }
}

impl<X, Y> RustFilesystem for PhotoFs<X, Y>
where
    X: RemotePhotoLib,
    Y: PhotoDb,
{
    fn lookup(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
    ) -> FuseResult<FileEntryResponse> {
        match parent {
            FIXED_INODE_ROOT => self.lookup_root(req, name),
            FIXED_INODE_ALBUMS => self.lookup_albums(req, name),
            FIXED_INODE_MEDIA => self.lookup_media(req, name),
            _ => match self.photo_db.album_by_inode(parent) {
                Ok(Option::Some(_)) => self.lookup_media_items_in_album(req, name),
                Ok(Option::None) => {
                    warn!(
                        "FS lookup: Failed to find a FileAttr for inode={} (name={:?})",
                        parent, name
                    );
                    Result::Err(FuseError::FunctionNotImplemented)
                }
                Err(error) => {
                    error!(
                        "FS lookup: Failed to lookup a FileAttr for inode={} (name={:?}) with {:?}",
                        parent, name, error
                    );
                    Result::Err(FuseError::FunctionNotImplemented)
                }
            },
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64) -> FuseResult<FileAttrResponse> {
        debug!("FS getattr: ino={}", ino);
        match ino {
            FIXED_INODE_ROOT => Result::Ok(FileAttrResponse {
                ttl: &TTL,
                attr: make_atr(FIXED_INODE_ROOT, 0, FileType::Directory),
            }),
            FIXED_INODE_ALBUMS => Result::Ok(FileAttrResponse {
                ttl: &TTL,
                attr: make_atr(FIXED_INODE_ALBUMS, 0, FileType::Directory),
            }),
            FIXED_INODE_MEDIA => Result::Ok(FileAttrResponse {
                ttl: &TTL,
                attr: make_atr(FIXED_INODE_MEDIA, 0, FileType::Directory),
            }),
            FIXED_INODE_HELLO_WORLD => Result::Ok(FileAttrResponse {
                ttl: &TTL,
                attr: make_atr(
                    FIXED_INODE_HELLO_WORLD,
                    HELLO_TXT_CONTENT.len(),
                    FileType::RegularFile,
                ),
            }),
            _ => match self.photo_db.item_by_inode(ino) {
                Err(error) => {
                    error!("FS getattr: Failed to lookup item in local db: {:?}", error);
                    Result::Err(FuseError::FunctionNotImplemented)
                }
                Ok(Option::None) => {
                    warn!("FS getattr: No item found in local DB: {:?}", ino);
                    Result::Err(FuseError::FunctionNotImplemented)
                }
                Ok(Option::Some(item)) => {
                    let file_type = match item.media_type {
                        MediaTypes::Album => FileType::Directory,
                        MediaTypes::MediaItem => FileType::RegularFile,
                    };
                    Result::Ok(FileAttrResponse {
                        ttl: &TTL,
                        attr: make_atr(item.inode, 0, file_type),
                    })
                }
            },
        }
    }

    fn open(&mut self, _req: &Request, ino: u64, _flags: u32) -> FuseResult<OpenResponse> {
        debug!("FS open: ino={}", ino);

        let file_data: Vec<u8>;
        if ino == FIXED_INODE_HELLO_WORLD {
            file_data = String::from(HELLO_TXT_CONTENT).into_bytes();
        } else {
            match self.photo_db.media_item_by_inode(ino) {
                Err(error) => {
                    error!(
                        "FS open: Failed to lookup media item in local db: {:?}",
                        error
                    );
                    return Result::Err(FuseError::FunctionNotImplemented);
                }
                Ok(Option::None) => {
                    warn!("FS open: No media items found in local DB: {:?}", ino);
                    return Result::Err(FuseError::FunctionNotImplemented);
                }
                Ok(Option::Some(media_item)) => {
                    let photo_lib = self.photo_lib.lock().unwrap();
                    match photo_lib.media_item(media_item.google_id()) {
                        Err(error) => {
                            error!(
                                "FS open: Failed to fetch media item from remote: {:?}",
                                error
                            );
                            return Result::Err(FuseError::FunctionNotImplemented);
                        }
                        Ok(data) => {
                            file_data = data;
                        }
                    }
                }
            }
        }

        let mut fh = ino;
        loop {
            if self.open_files.contains_key(&fh) {
                fh += 1;
            } else {
                break;
            }
        }

        self.open_files.insert(fh, file_data);
        Result::Ok(OpenResponse {
            fh,
            flags: fuse::consts::FOPEN_DIRECT_IO,
        })
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
    ) -> FuseResult<ReadResponse> {
        debug!("FS read: ino={}, offset={} size={}", ino, offset, size);

        match self.open_files.get(&fh) {
            None => Result::Err(FuseError::FunctionNotImplemented),
            Some(data) => {
                let slice_end: usize = usize::min((offset as usize) + (size as usize), data.len());
                Result::Ok(ReadResponse {
                    data: &data[offset as usize..slice_end],
                })
            }
        }
    }

    fn release(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> FuseResult<()> {
        debug!("FS release: ino={}, fh={}", ino, fh);

        match self.open_files.remove(&fh) {
            None => Result::Err(FuseError::FunctionNotImplemented),
            Some(_) => Result::Ok(()),
        }
    }

    fn opendir(&mut self, _req: &Request, ino: u64, _flags: u32) -> FuseResult<OpenResponse> {
        let album_for_inode: Option<PhotoDbAlbum> = match ino {
            FIXED_INODE_ROOT | FIXED_INODE_MEDIA | FIXED_INODE_ALBUMS => Result::Ok(Option::None),
            _ => match self.photo_db.album_by_inode(ino) {
                Err(error) => {
                    error!(
                        "FS opendir: Error checking inode is a album (ino={}): {:?}",
                        ino, error
                    );
                    Result::Err(FuseError::FunctionNotImplemented)
                }
                Ok(Option::None) => {
                    warn!("FS opendir: Failed to find album for inode (ino={})", ino);
                    Result::Err(FuseError::FunctionNotImplemented)
                }
                Ok(Option::Some(album)) => {
                    debug!(
                        "FS opendir: open request for album that is found in DB: {:?}",
                        album
                    );
                    Result::Ok(Option::Some(album))
                }
            },
        }?;

        let entries = self.opendir_entries(ino, &album_for_inode);

        let mut fh = ino;
        loop {
            if self.open_dirs.contains_key(&fh) {
                fh += 1;
            } else {
                break;
            }
        }

        self.open_dirs.insert(fh, entries);
        Result::Ok(OpenResponse { fh, flags: 0 }) // TODO: Flags
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
    ) -> FuseResult<ReadDirResponse> {
        debug!("FS readdir: ino={}, offset={}", ino, offset);

        let dir_context_option = self.open_dirs.get(&fh);
        if dir_context_option.is_none() {
            return Result::Err(FuseError::FunctionNotImplemented);
        }
        let entries = dir_context_option.unwrap();

        // TODO: Error when not known inode
        // reply.error(ENOENT);

        let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;
        let result_entries: Vec<ReadDirEntry> = entries
            .into_iter()
            .enumerate()
            .skip(to_skip)
            .map(|(offset, entry)| {
                let ino = entry.0;
                let kind = entry.1;
                let name = OsStr::new(&entry.2);
                ReadDirEntry {
                    ino,
                    offset: (offset + 1) as i64,
                    kind,
                    name,
                }
            }).collect();
        Result::Ok(ReadDirResponse {
            entries: result_entries,
        })
    }

    fn releasedir(&mut self, _req: &Request, ino: u64, fh: u64, _flags: u32) -> FuseResult<()> {
        debug!("FS releasedir: ino={}, fh={}", ino, fh);

        match self.open_dirs.remove(&fh) {
            None => Result::Err(FuseError::FunctionNotImplemented),
            Some(_) => Result::Ok(()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn make_atr_test() {
        // Inode
        assert_eq!(make_atr(100, 0, FileType::RegularFile).ino, 100);

        // Size
        assert_eq!(make_atr(100, 1, FileType::RegularFile).size, 1);

        // FileType
        assert_eq!(
            make_atr(100, 1, FileType::RegularFile).kind,
            FileType::RegularFile
        );
        assert_eq!(
            make_atr(100, 1, FileType::Directory).kind,
            FileType::Directory
        );
    }
}
