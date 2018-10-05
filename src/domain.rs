extern crate chrono;

use chrono::prelude::*;
use chrono::Utc;
use std::convert::From;
use std::fmt;

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
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
pub type GoogleId = str;

#[derive(Debug)]
pub struct PhotoDbMediaItemAlbum {
    id: String,
    pub name: String,
    pub media_type: MediaTypes,
    pub last_remote_check: UtcDateTime,
    pub inode: Inode,
}

impl PhotoDbMediaItemAlbum {
    pub fn new(
        id: String,
        name: String,
        media_type: MediaTypes,
        last_remote_check: UtcDateTime,
        inode: Inode,
    ) -> PhotoDbMediaItemAlbum {
        PhotoDbMediaItemAlbum {
            id,
            name,
            media_type,
            last_remote_check,
            inode,
        }
    }

    pub fn google_id(&self) -> &GoogleId {
        &self.id
    }
}

pub type PhotoDbAlbum = PhotoDbMediaItemAlbum;
pub type PhotoDbMediaItem = PhotoDbMediaItemAlbum;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn media_types_from_string() {
        assert_eq!(MediaTypes::from("album"), MediaTypes::Album);
        assert_eq!(MediaTypes::from("media_item"), MediaTypes::MediaItem);
    }

    #[test]
    fn media_types_to_string() {
        assert_eq!(format!("{}", MediaTypes::Album), "album");
        assert_eq!(format!("{:?}", MediaTypes::Album), "Album");

        assert_eq!(format!("{}", MediaTypes::MediaItem), "media_item");
        assert_eq!(format!("{:?}", MediaTypes::MediaItem), "MediaItem");
    }
}
