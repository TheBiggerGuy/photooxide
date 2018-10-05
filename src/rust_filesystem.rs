use std::ffi::OsStr;
use std::result;

use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry,
    ReplyOpen, Request,
};
use libc;
use time::Timespec;

#[derive(Debug)]
pub enum FuseError {
    FunctionNotImplemented,
}

impl FuseError {
    fn libc_error_code(&self) -> i32 {
        match self {
            _ => libc::ENOENT,
        }
    }
}

pub type FuseResult<T> = result::Result<T, FuseError>;

#[derive(Debug)]
pub struct FileEntryResponse<'a> {
    pub ttl: &'a Timespec,
    pub attr: FileAttr,
    pub generation: u64,
}

#[derive(Debug)]
pub struct FileAttrResponse<'a> {
    pub ttl: &'a Timespec,
    pub attr: FileAttr,
}

#[derive(Debug)]
pub struct OpenResponse {
    pub fh: u64,
    pub flags: u32,
}

#[derive(Debug)]
pub struct ReadResponse<'a> {
    pub data: &'a [u8],
}

#[derive(Debug)]
pub struct ReadDirResponse<'a> {
    pub entries: Vec<ReadDirEntry<'a>>,
}

#[derive(Debug)]
pub struct ReadDirEntry<'a> {
    pub ino: u64,
    pub offset: i64,
    pub kind: FileType,
    pub name: &'a OsStr,
}

pub trait RustFilesystem {
    fn lookup(&mut self, req: &Request, parent: u64, name: &OsStr)
        -> FuseResult<FileEntryResponse>;
    fn getattr(&mut self, req: &Request, ino: u64) -> FuseResult<FileAttrResponse>;
    fn open(&mut self, req: &Request, ino: u64, flags: u32) -> FuseResult<OpenResponse>;
    fn read(
        &mut self,
        req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
    ) -> FuseResult<ReadResponse>;
    fn release(
        &mut self,
        req: &Request,
        ino: u64,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
    ) -> FuseResult<()>;
    fn opendir(&mut self, req: &Request, ino: u64, flags: u32) -> FuseResult<OpenResponse>;
    fn readdir(
        &mut self,
        req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
    ) -> FuseResult<ReadDirResponse>;
    fn releasedir(&mut self, req: &Request, ino: u64, fh: u64, flags: u32) -> FuseResult<()>;
}

#[derive(Debug)]
pub struct RustFilesystemReal<X>
where
    X: RustFilesystem,
{
    fs: X,
}

impl<X> RustFilesystemReal<X>
where
    X: RustFilesystem,
{
    pub fn new(fs: X) -> RustFilesystemReal<X> {
        RustFilesystemReal { fs }
    }
}

impl<X> Filesystem for RustFilesystemReal<X>
where
    X: RustFilesystem,
{
    fn lookup(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        match self.fs.lookup(req, parent, name) {
            Ok(response) => reply.entry(response.ttl, &response.attr, response.generation),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn getattr(&mut self, req: &Request, ino: u64, reply: ReplyAttr) {
        match self.fs.getattr(req, ino) {
            Ok(response) => reply.attr(response.ttl, &response.attr),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn open(&mut self, req: &Request, ino: u64, flags: u32, reply: ReplyOpen) {
        match self.fs.open(req, ino, flags) {
            Ok(response) => reply.opened(response.fh, response.flags),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn read(&mut self, req: &Request, ino: u64, fh: u64, offset: i64, size: u32, reply: ReplyData) {
        match self.fs.read(req, ino, fh, offset, size) {
            Ok(response) => reply.data(response.data),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn release(
        &mut self,
        req: &Request,
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

    fn opendir(&mut self, req: &Request, ino: u64, flags: u32, reply: ReplyOpen) {
        match self.fs.opendir(req, ino, flags) {
            Ok(response) => reply.opened(response.fh, response.flags),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }

    fn readdir(
        &mut self,
        req: &Request,
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

    fn releasedir(&mut self, req: &Request, ino: u64, fh: u64, flags: u32, reply: ReplyEmpty) {
        match self.fs.releasedir(req, ino, fh, flags) {
            Ok(_) => reply.ok(),
            Err(error) => reply.error(error.libc_error_code()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fuse_error_libc_error_code() {
        assert_eq!(FuseError::FunctionNotImplemented.libc_error_code(), 2);
    }
}
