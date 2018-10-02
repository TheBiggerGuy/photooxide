extern crate chrono;

use chrono::prelude::*;
use chrono::Utc;

pub type Inode = u64;
pub type UtcDateTime = DateTime<Utc>;
pub type GoogleId = String;

#[derive(Debug)]
pub struct PhotoDbMediaItemAlbum {
    pub google_id: GoogleId,
    pub name: String,
    pub last_remote_check: UtcDateTime,
    pub inode: Inode,
}

pub type PhotoDbAlbum = PhotoDbMediaItemAlbum;
pub type PhotoDbMediaItem = PhotoDbMediaItemAlbum;
