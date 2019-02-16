use std::ffi::OsStr;

use fuse::{
    self, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen,
};

mod error;
pub use self::error::{FuseError, FuseResult};

mod response;
pub use self::response::{
    FileAttrResponse, FileEntryResponse, OpenResponse, ReadDirEntry, ReadDirResponse, ReadResponse,
};

mod request;
pub use self::request::UniqRequest;

pub trait RustFilesystem {
    fn lookup(
        &mut self,
        req: &dyn UniqRequest,
        parent: u64,
        name: &OsStr,
    ) -> FuseResult<FileEntryResponse<'_>>;
    fn getattr(&mut self, req: &dyn UniqRequest, ino: u64) -> FuseResult<FileAttrResponse<'_>>;
    fn open(&mut self, req: &dyn UniqRequest, ino: u64, flags: u32) -> FuseResult<OpenResponse>;
    fn read(
        &mut self,
        req: &dyn UniqRequest,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
    ) -> FuseResult<ReadResponse<'_>>;
    fn release(
        &mut self,
        req: &dyn UniqRequest,
        ino: u64,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
    ) -> FuseResult<()>;
    fn opendir(&mut self, req: &dyn UniqRequest, ino: u64, flags: u32) -> FuseResult<OpenResponse>;
    fn readdir(
        &mut self,
        req: &dyn UniqRequest,
        ino: u64,
        fh: u64,
        offset: i64,
    ) -> FuseResult<ReadDirResponse<'_>>;
    fn releasedir(
        &mut self,
        req: &dyn UniqRequest,
        ino: u64,
        fh: u64,
        flags: u32,
    ) -> FuseResult<()>;
    fn destroy(&mut self, req: &dyn UniqRequest);
}

#[derive(Debug, new)]
pub struct RustFilesystemReal<X>
where
    X: RustFilesystem,
{
    fs: X,
}

impl<X> Filesystem for RustFilesystemReal<X>
where
    X: RustFilesystem,
{
    fn lookup(&mut self, req: &fuse::Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        debug!("lookup: {:?}", req);
        match self.fs.lookup(req, parent, name) {
            Ok(response) => reply.entry(response.ttl, &response.attr, response.generation),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn getattr(&mut self, req: &fuse::Request<'_>, ino: u64, reply: ReplyAttr) {
        match self.fs.getattr(req, ino) {
            Ok(response) => reply.attr(response.ttl, &response.attr),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn open(&mut self, req: &fuse::Request<'_>, ino: u64, flags: u32, reply: ReplyOpen) {
        match self.fs.open(req, ino, flags) {
            Ok(response) => reply.opened(response.fh, response.flags),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn read(
        &mut self,
        req: &fuse::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        reply: ReplyData,
    ) {
        match self.fs.read(req, ino, fh, offset, size) {
            Ok(response) => reply.data(response.data),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn release(
        &mut self,
        req: &fuse::Request<'_>,
        ino: u64,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
        reply: ReplyEmpty,
    ) {
        match self.fs.release(req, ino, fh, flags, lock_owner, flush) {
            Ok(_) => reply.ok(),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn opendir(&mut self, req: &fuse::Request<'_>, ino: u64, flags: u32, reply: ReplyOpen) {
        match self.fs.opendir(req, ino, flags) {
            Ok(response) => reply.opened(response.fh, response.flags),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn readdir(
        &mut self,
        req: &fuse::Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        match self.fs.readdir(req, ino, fh, offset) {
            Ok(response) => {
                let mut counter = 0;
                let entries_size = response.entries.len();
                for entry in response.entries {
                    if reply.add(entry.ino, entry.offset, entry.kind, &entry.name) {
                        debug!("readdir reply.add returned full");
                        break;
                    }
                    counter += 1;
                }
                debug!("Returned {} out of {} entries", counter, entries_size);
                reply.ok();
            }
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn releasedir(
        &mut self,
        req: &fuse::Request<'_>,
        ino: u64,
        fh: u64,
        flags: u32,
        reply: ReplyEmpty,
    ) {
        match self.fs.releasedir(req, ino, fh, flags) {
            Ok(_) => reply.ok(),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn destroy(&mut self, req: &fuse::Request<'_>) {
        self.fs.destroy(req);
    }
}
