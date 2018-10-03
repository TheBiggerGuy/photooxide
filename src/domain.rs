extern crate chrono;

use chrono::prelude::*;
use chrono::Utc;
use std::fmt;
use std::convert::From;

#[derive(Debug)]
pub enum MediaTypes {
    Album,
    MediaItem,
}

impl fmt::Display for MediaTypes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MediaTypes::Album => write!(f, "album"),
            MediaTypes::MediaItem => write!(f, "media_item"),
        }
    }
}

impl<'a> From<&'a str> for MediaTypes {
    fn from(media_type: &str) -> Self {
        match media_type {
            "album" => MediaTypes::Album,
            "media_item" => MediaTypes::MediaItem,
            _ => panic!("Unknown media type {}", media_type),
        }
    }
}

pub type Inode = u64;
pub type UtcDateTime = DateTime<Utc>;
pub type GoogleId = String;

#[derive(Debug)]
pub struct PhotoDbMediaItemAlbum {
    pub google_id: GoogleId,
    pub name: String,
    pub media_type: MediaTypes,
    pub last_remote_check: UtcDateTime,
    pub inode: Inode,
}

pub type PhotoDbAlbum = PhotoDbMediaItemAlbum;
pub type PhotoDbMediaItem = PhotoDbMediaItemAlbum;
