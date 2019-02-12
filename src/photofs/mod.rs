use std::collections::HashSet;
use std::convert::From;
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};

use fuse::{self, FileType};
use time::Timespec;

use crate::rust_filesystem::{
    FileAttrResponse, FileEntryResponse, FuseError, FuseResult, OpenResponse, ReadDirEntry,
    ReadDirResponse, ReadResponse,
};

use crate::db::{Filter, PhotoDbRo};
use crate::domain::{Inode, MediaTypes, PhotoDbAlbum};
use crate::photolib::*;
use crate::rust_filesystem::{RustFilesystem, UniqRequest};

mod error;
pub use self::error::PhotoFsError;

mod utils;
use self::utils::{make_atr, OpenFileHandles};

const FIXED_INODE_ROOT: u64 = fuse::FUSE_ROOT_ID;
const FIXED_INODE_ALBUMS: u64 = 2;
const FIXED_INODE_MEDIA: u64 = 3;
const FIXED_INODE_HELLO_WORLD: u64 = 4;

const TTL: Timespec = Timespec { sec: 120, nsec: 0 }; // 2 minutes

const HELLO_TXT_CONTENT: &[u8] = b"Hello World!\n";

const GENERATION: u64 = 0;

const DEFAULT_MEDIA_ITEM_SIZE: usize = 1024;

#[derive(Debug, new)]
struct ReadFhEntry {
    inode: Inode,
    data: Vec<u8>,
}

#[derive(Debug, new)]
struct ReadDirFhEntry {
    inode: Inode,
    entries: Vec<(u64, fuse::FileType, String)>,
}

pub struct PhotoFs<X, Y>
where
    X: RemotePhotoLibData,
    Y: PhotoDbRo,
{
    photo_lib: Arc<Mutex<X>>,
    photo_db: Arc<Y>,
    open_files: OpenFileHandles<ReadFhEntry>,
    open_dirs: OpenFileHandles<ReadDirFhEntry>,
}

impl<X, Y> PhotoFs<X, Y>
where
    X: RemotePhotoLibData,
    Y: PhotoDbRo,
{
    pub fn new(photo_lib: Arc<Mutex<X>>, photo_db: Arc<Y>) -> PhotoFs<X, Y> {
        PhotoFs {
            photo_lib,
            photo_db,
            open_files: OpenFileHandles::new(),
            open_dirs: OpenFileHandles::new(),
        }
    }

    fn lookup_root(
        &mut self,
        _req: &dyn UniqRequest,
        name: &OsStr,
    ) -> FuseResult<FileEntryResponse<'_>> {
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

    fn lookup_albums(
        &mut self,
        _req: &dyn UniqRequest,
        name: &OsStr,
    ) -> FuseResult<FileEntryResponse<'_>> {
        let name = name.to_str().unwrap();
        match self.photo_db.album_by_name(&String::from(name)) {
            Ok(Option::Some(album)) => {
                let size = self.photo_db.media_items_in_album_length(album.inode)?;
                Result::Ok(FileEntryResponse {
                    ttl: &TTL,
                    attr: make_atr(album.inode, size, FileType::Directory),
                    generation: GENERATION,
                })
            }
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

    fn lookup_media(
        &mut self,
        _req: &dyn UniqRequest,
        name: &OsStr,
        filter: Filter,
    ) -> FuseResult<FileEntryResponse<'_>> {
        let name = name.to_str().unwrap();
        match self
            .photo_db
            .media_item_by_name(&String::from(name), filter)
        {
            Ok(Option::Some(media_item)) => Result::Ok(FileEntryResponse {
                ttl: &TTL,
                attr: make_atr(
                    media_item.inode,
                    DEFAULT_MEDIA_ITEM_SIZE,
                    FileType::RegularFile,
                ),
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
    X: RemotePhotoLibData,
    Y: PhotoDbRo,
{
    fn lookup(
        &mut self,
        req: &dyn UniqRequest,
        parent: u64,
        name: &OsStr,
    ) -> FuseResult<FileEntryResponse<'_>> {
        match parent {
            FIXED_INODE_ROOT => self.lookup_root(req, name),
            FIXED_INODE_ALBUMS => self.lookup_albums(req, name),
            FIXED_INODE_MEDIA => self.lookup_media(req, name, Filter::NoFilter),
            _ => match self.photo_db.album_by_inode(parent) {
                Ok(Option::Some(album)) => {
                    self.lookup_media(req, name, Filter::ByAlbum(album.google_id()))
                }
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

    fn getattr(&mut self, _req: &dyn UniqRequest, ino: u64) -> FuseResult<FileAttrResponse<'_>> {
        debug!("FS getattr: ino={}", ino);
        match ino {
            FIXED_INODE_ROOT => Result::Ok(FileAttrResponse {
                ttl: &TTL,
                attr: make_atr(FIXED_INODE_ROOT, 4, FileType::Directory),
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
                    let size = match item.media_type {
                        MediaTypes::Album => {
                            self.photo_db.media_items_in_album_length(item.inode)?
                        }
                        MediaTypes::MediaItem => DEFAULT_MEDIA_ITEM_SIZE,
                    };

                    Result::Ok(FileAttrResponse {
                        ttl: &TTL,
                        attr: make_atr(item.inode, size, file_type),
                    })
                }
            },
        }
    }

    fn open(&mut self, _req: &dyn UniqRequest, ino: u64, _flags: u32) -> FuseResult<OpenResponse> {
        debug!("FS open: ino={}", ino);

        let file_data: Vec<u8>;
        if ino == FIXED_INODE_HELLO_WORLD {
            file_data = HELLO_TXT_CONTENT.to_vec();
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
                    let filename_lowercase = media_item.name.to_lowercase();
                    let is_video = filename_lowercase.ends_with(".mp4")
                        || filename_lowercase.ends_with(".mts")
                        || filename_lowercase.ends_with(".avi"); // TODO: Use MIME Type
                    match photo_lib.media_item(media_item.google_id(), is_video) {
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

        let fh = self.open_files.open(ReadFhEntry::new(ino, file_data));

        Result::Ok(OpenResponse {
            fh,
            flags: fuse::consts::FOPEN_DIRECT_IO,
        })
    }

    fn read(
        &mut self,
        _req: &dyn UniqRequest,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
    ) -> FuseResult<ReadResponse<'_>> {
        let offset = offset as usize;
        debug!("FS read: ino={}, offset={} size={}", ino, offset, size);

        match self.open_files.get(fh) {
            None => Result::Err(FuseError::FunctionNotImplemented),
            Some(entry) => {
                if entry.inode != ino {
                    error!("Read file handle found entry for a different inode");
                    return Result::Err(FuseError::FunctionNotImplemented);
                }

                let data_len = entry.data.len();
                if offset >= data_len {
                    warn!(
                        "Attempt to read past end of file: file_size={} offset={}",
                        data_len, offset
                    );
                    return Result::Ok(ReadResponse { data: &[] });
                }
                let slice_end: usize = usize::min(offset as usize + size as usize, data_len);
                Result::Ok(ReadResponse {
                    data: &entry.data[offset as usize..slice_end],
                })
            }
        }
    }

    fn release(
        &mut self,
        _req: &dyn UniqRequest,
        ino: u64,
        fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
    ) -> FuseResult<()> {
        debug!("FS release: ino={}, fh={}", ino, fh);

        match self.open_files.remove(fh) {
            None => Result::Err(FuseError::FunctionNotImplemented),
            Some(_) => Result::Ok(()),
        }
    }

    fn opendir(
        &mut self,
        _req: &dyn UniqRequest,
        ino: u64,
        _flags: u32,
    ) -> FuseResult<OpenResponse> {
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
        let fh = self.open_dirs.open(ReadDirFhEntry::new(ino, entries));

        Result::Ok(OpenResponse { fh, flags: 0 }) // TODO: Flags
    }

    fn readdir(
        &mut self,
        _req: &dyn UniqRequest,
        ino: u64,
        fh: u64,
        offset: i64,
    ) -> FuseResult<ReadDirResponse<'_>> {
        debug!("FS readdir: ino={}, offset={}", ino, offset);

        let fh_entry = match self.open_dirs.get(fh) {
            None => return Result::Err(FuseError::FunctionNotImplemented),
            Some(entry) => entry,
        };

        if fh_entry.inode != ino {
            error!("Read dir handle found entry for a different inode");
            return Result::Err(FuseError::FunctionNotImplemented);
        }

        // TODO: Error when not known inode
        // reply.error(ENOENT);

        let to_skip = if offset == 0 { offset } else { offset + 1 } as usize;
        let result_entries: Vec<ReadDirEntry<'_>> = (&fh_entry.entries)
            .iter()
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
            })
            .collect();
        Result::Ok(ReadDirResponse {
            entries: result_entries,
        })
    }

    fn releasedir(
        &mut self,
        _req: &dyn UniqRequest,
        ino: u64,
        fh: u64,
        _flags: u32,
    ) -> FuseResult<()> {
        debug!("FS releasedir: ino={}, fh={}", ino, fh);

        match self.open_dirs.remove(fh) {
            None => Result::Err(FuseError::FunctionNotImplemented),
            Some(_) => Result::Ok(()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::collections::HashMap;
    use std::sync::Mutex;

    use hyper;

    use chrono::{TimeZone, Utc};

    use crate::domain::{GoogleId, Inode};

    use crate::db::{PhotoDb, SqliteDb};

    #[test]
    fn lookup_root() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        {
            assert!(fs
                .lookup(
                    &TestUniqRequest {},
                    FIXED_INODE_ROOT,
                    OsStr::new("not_in_root")
                )
                .is_err());
        }

        {
            let response =
                fs.lookup(&TestUniqRequest {}, FIXED_INODE_ROOT, OsStr::new("albums"))?;

            assert_eq!(response.attr.ino, FIXED_INODE_ALBUMS);
            assert_eq!(response.attr.kind, FileType::Directory);
        }

        {
            let response = fs.lookup(&TestUniqRequest {}, FIXED_INODE_ROOT, OsStr::new("media"))?;

            assert_eq!(response.attr.ino, FIXED_INODE_MEDIA);
            assert_eq!(response.attr.kind, FileType::Directory);
        }

        {
            let response = fs.lookup(
                &TestUniqRequest {},
                FIXED_INODE_ROOT,
                OsStr::new("hello.txt"),
            )?;

            assert_eq!(response.attr.ino, FIXED_INODE_HELLO_WORLD);
            assert_eq!(response.attr.kind, FileType::RegularFile);
        }

        Result::Ok(())
    }

    #[test]
    fn lookup_albums() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        {
            let response = fs
                .lookup(&TestUniqRequest {}, FIXED_INODE_ROOT, OsStr::new("albums"))
                .unwrap();

            assert_eq!(response.attr.ino, FIXED_INODE_ALBUMS);
            assert_eq!(response.attr.kind, FileType::Directory);
        }

        {
            assert!(fs
                .lookup(
                    &TestUniqRequest {},
                    FIXED_INODE_ALBUMS,
                    OsStr::new("not_a_album")
                )
                .is_err());
        }

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);
        let album_inode = photo_db.upsert_album("GoogleId2", "Album1", &now).unwrap();
        {
            let response = fs.lookup(
                &TestUniqRequest {},
                FIXED_INODE_ALBUMS,
                OsStr::new("Album1"),
            )?;

            assert_eq!(response.attr.ino, album_inode);
            assert_eq!(response.attr.kind, FileType::Directory);
        }

        Result::Ok(())
    }

    #[test]
    fn lookup_media_item_in_album() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);
        let media_item_inode = photo_db
            .upsert_media_item("GoogleId1", "Photo1.jpg", &now)
            .unwrap();
        let album_inode = photo_db.upsert_album("GoogleId2", "Album1", &now).unwrap();

        // Empty album
        {
            let response = fs.lookup(&TestUniqRequest {}, album_inode, OsStr::new("Photo1.jpg"));

            assert!(response.is_err());
        }

        // Correct lookup
        photo_db
            .upsert_media_item_in_album("GoogleId2", "GoogleId1")
            .unwrap();
        {
            let response = fs.lookup(&TestUniqRequest {}, album_inode, OsStr::new("Photo1.jpg"))?;

            assert_eq!(response.attr.ino, media_item_inode);
            assert_eq!(response.attr.kind, FileType::RegularFile);
        }

        // Incorrect lookup
        {
            let response = fs.lookup(&TestUniqRequest {}, album_inode, OsStr::new("Photo2.jpg"));

            assert!(response.is_err());
        }

        Result::Ok(())
    }

    #[test]
    fn getattr_static() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        {
            let response = fs.getattr(&TestUniqRequest {}, FIXED_INODE_ROOT)?;

            assert_eq!(response.attr.ino, FIXED_INODE_ROOT);
            assert_eq!(response.attr.kind, FileType::Directory);
            assert_eq!(response.attr.size, 4);
        }

        {
            let response = fs.getattr(&TestUniqRequest {}, FIXED_INODE_ALBUMS)?;

            assert_eq!(response.attr.ino, FIXED_INODE_ALBUMS);
            assert_eq!(response.attr.kind, FileType::Directory);
            assert_eq!(response.attr.size, 0);
        }

        {
            let response = fs.getattr(&TestUniqRequest {}, FIXED_INODE_MEDIA)?;

            assert_eq!(response.attr.ino, FIXED_INODE_MEDIA);
            assert_eq!(response.attr.kind, FileType::Directory);
            assert_eq!(response.attr.size, 0);
        }

        {
            let response = fs.getattr(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD)?;

            assert_eq!(response.attr.ino, FIXED_INODE_HELLO_WORLD);
            assert_eq!(response.attr.kind, FileType::RegularFile);
            assert_eq!(response.attr.size, 13);
        }

        Result::Ok(())
    }

    #[test]
    fn getattr_dynamic() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);
        let media_item_inode = photo_db
            .upsert_media_item("GoogleId1", "Photo1.jpg", &now)
            .unwrap();
        let album_inode = photo_db.upsert_album("GoogleId2", "Album1", &now).unwrap();

        {
            let response = fs.getattr(&TestUniqRequest {}, media_item_inode)?;

            assert_eq!(response.attr.ino, media_item_inode);
            assert_eq!(response.attr.kind, FileType::RegularFile);
            assert_eq!(response.attr.size, 1024);
        }

        {
            let response = fs.getattr(&TestUniqRequest {}, album_inode)?;

            assert_eq!(response.attr.ino, album_inode);
            assert_eq!(response.attr.kind, FileType::Directory);
            assert_eq!(response.attr.size, 0);
        }

        {
            assert!(fs.getattr(&TestUniqRequest {}, album_inode + 1).is_err());
        }

        photo_db
            .upsert_media_item_in_album("GoogleId2", "GoogleId1")
            .unwrap();

        {
            let response = fs.getattr(&TestUniqRequest {}, album_inode)?;

            assert_eq!(response.attr.size, 1);
        }

        Result::Ok(())
    }

    #[test]
    fn open_read_release_hello_txt() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let fh = fs.open(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, 0)?.fh;

        {
            let response = fs.read(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, fh, 0, 13)?;

            assert_eq!(response.data, b"Hello World!\n");
        }

        assert!(fs
            .release(
                &TestUniqRequest {},
                FIXED_INODE_HELLO_WORLD,
                fh,
                0,
                0,
                false
            )
            .is_ok());

        Result::Ok(())
    }

    #[test]
    fn read_offset() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let fh = fs.open(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, 0)?.fh;

        {
            let response = fs.read(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, fh, 0, 13)?;
            assert_eq!(response.data, b"Hello World!\n");
        }

        {
            let response = fs.read(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, fh, 1, 12)?;
            assert_eq!(response.data, b"ello World!\n");
        }

        {
            let response = fs.read(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, fh, 12, 1)?;
            assert_eq!(response.data, b"\n");
        }

        // Offset past the end of the file
        {
            let response = fs.read(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, fh, 13, 1)?;
            assert_eq!(response.data, b"");
        }

        Result::Ok(())
    }

    #[test]
    fn read_size() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let open = fs.open(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, 0)?;

        {
            let response = fs.read(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, open.fh, 0, 13)?;
            assert_eq!(response.data, b"Hello World!\n");
        }

        {
            let response = fs.read(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, open.fh, 0, 5)?;
            assert_eq!(response.data, b"Hello");
        }

        {
            let response = fs.read(&TestUniqRequest {}, FIXED_INODE_HELLO_WORLD, open.fh, 0, 15)?;
            assert_eq!(response.data, b"Hello World!\n");
            assert_eq!(open.flags, 1); // assert direct IO or the response should be zero padded
        }

        Result::Ok(())
    }

    #[test]
    fn read_media_item() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let inode: Inode;
        {
            let mut lib = photo_lib.lock().unwrap();
            lib.test_data.insert("GoogleId1", vec![65, 66, 67]);

            let now_unix = Utc::now().timestamp();
            let now = Utc::timestamp(&Utc, now_unix, 0);
            inode = photo_db
                .upsert_media_item(&String::from("GoogleId1"), &String::from("Photo 1"), &now)
                .unwrap();
        }

        // read real file
        {
            let open = fs.open(&TestUniqRequest {}, inode, 0)?;
            let response = fs.read(&TestUniqRequest {}, inode, open.fh, 0, 5)?;
            assert_eq!(response.data, b"ABC");
        }

        // read unknown inode or fh
        {
            let open = fs.open(&TestUniqRequest {}, inode, 0)?;
            assert!(fs
                .read(&TestUniqRequest {}, inode + 1, open.fh, 0, 5)
                .is_err());
            assert!(fs
                .read(&TestUniqRequest {}, inode, open.fh + 1, 0, 5)
                .is_err());
        }

        Result::Ok(())
    }

    #[test]
    fn opendir_multiple_calls() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let response1 = fs.opendir(&TestUniqRequest {}, FIXED_INODE_ROOT, 0)?;
        let response2 = fs.opendir(&TestUniqRequest {}, FIXED_INODE_ROOT, 0)?;

        assert_eq!(response1.fh, 0);
        assert_eq!(response2.fh, 1);

        Result::Ok(())
    }

    #[test]
    fn readdir_root() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let fh = fs.opendir(&TestUniqRequest {}, FIXED_INODE_ROOT, 0)?.fh;

        let response = fs.readdir(&TestUniqRequest {}, FIXED_INODE_ROOT, fh, 0)?;

        assert_eq!(response.entries.len(), 4);
        assert_eq!(response.entries[0].ino, FIXED_INODE_ROOT);
        assert_eq!(response.entries[1].ino, FIXED_INODE_ALBUMS);
        assert_eq!(response.entries[2].ino, FIXED_INODE_MEDIA);
        assert_eq!(response.entries[3].ino, FIXED_INODE_HELLO_WORLD);

        Result::Ok(())
    }

    #[test]
    fn readdir_invalid_inode_or_fh() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let fh = fs.opendir(&TestUniqRequest {}, FIXED_INODE_ROOT, 0)?.fh;

        {
            assert!(fs
                .readdir(&TestUniqRequest {}, FIXED_INODE_ROOT + 1, fh, 0)
                .is_err());
        }
        {
            assert!(fs
                .readdir(&TestUniqRequest {}, FIXED_INODE_ROOT, fh + 1, 0)
                .is_err());
        }

        Result::Ok(())
    }

    #[test]
    fn releasedir_no_previous_opendir() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        assert!(fs.releasedir(&TestUniqRequest {}, 1, 0, 0).is_err());

        Result::Ok(())
    }

    #[test]
    fn releasedir_from_previous_opendir() -> Result<(), FuseError> {
        let photo_lib = Arc::new(Mutex::new(TestRemotePhotoLib::new()));
        let photo_db = Arc::new(SqliteDb::in_memory()?);
        let mut fs = PhotoFs::new(photo_lib.clone(), photo_db.clone());

        let fh = fs.opendir(&TestUniqRequest {}, FIXED_INODE_ROOT, 0)?.fh;

        assert!(fs
            .releasedir(&TestUniqRequest {}, FIXED_INODE_ROOT, fh, 0)
            .is_ok());

        Result::Ok(())
    }

    #[derive(Debug)]
    struct TestUniqRequest {}

    impl UniqRequest for TestUniqRequest {
        fn unique(&self) -> u64 {
            0
        }
        fn uid(&self) -> u32 {
            0
        }
        fn gid(&self) -> u32 {
            0
        }
        fn pid(&self) -> u32 {
            0
        }
    }

    #[derive(Debug)]
    struct TestRemotePhotoLib<'a> {
        test_data: HashMap<&'a GoogleId, Vec<u8>>,
    }

    impl<'a> TestRemotePhotoLib<'a> {
        fn new() -> TestRemotePhotoLib<'a> {
            TestRemotePhotoLib {
                test_data: HashMap::new(),
            }
        }
    }

    impl<'a> RemotePhotoLibData for TestRemotePhotoLib<'a> {
        fn media_item(
            &self,
            google_id: &GoogleId,
            _is_video: bool,
        ) -> Result<Vec<u8>, RemotePhotoLibError> {
            match self.test_data.get(google_id) {
                Some(data) => Result::Ok(data.clone()),
                None => Result::Err(RemotePhotoLibError::HttpApiError(
                    hyper::status::StatusCode::NotFound,
                )),
            }
        }
    }
}
