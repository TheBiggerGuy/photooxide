use std::ffi::OsStr;

use fuse::{FileAttr, FileType};
use time::Timespec;

#[derive(Clone, Copy, Debug)]
pub struct FileEntryResponse<'a> {
    pub ttl: &'a Timespec,
    pub attr: FileAttr,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct FileAttrResponse<'a> {
    pub ttl: &'a Timespec,
    pub attr: FileAttr,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OpenResponse {
    pub fh: u64,
    pub flags: u32,
}

#[derive(PartialEq, Eq, Debug)]
pub struct ReadResponse<'a> {
    pub data: &'a [u8],
}

#[derive(PartialEq, Debug)]
pub struct ReadDirResponse<'a> {
    pub entries: Vec<ReadDirEntry<'a>>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ReadDirEntry<'a> {
    pub ino: u64,
    pub offset: i64,
    pub kind: FileType,
    pub name: &'a OsStr,
}
