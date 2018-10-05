extern crate google_photoslibrary1 as photoslibrary1;
extern crate hyper;
extern crate yup_oauth2 as oauth2;

extern crate rusqlite;

extern crate chrono;
extern crate time;

use std::convert::From;
use std::fmt;
use std::option::Option;
use std::result::Result;
use std::sync;
use std::sync::RwLock;

use chrono::{TimeZone, Utc};

use domain::{
    GoogleId, Inode, MediaTypes, PhotoDbAlbum, PhotoDbMediaItem, PhotoDbMediaItemAlbum, UtcDateTime,
};

#[derive(Debug)]
pub enum DbError {
    SqlError(rusqlite::Error),
    LockingError,
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        DbError::SqlError(error)
    }
}

impl<T> From<sync::PoisonError<T>> for DbError {
    fn from(_error: sync::PoisonError<T>) -> Self {
        DbError::LockingError
    }
}

#[derive(Debug)]
enum TableName {
    AlbumsAndMediaItems,
    NextInode,
    MediaItemsInAlbum,
}

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TableName::AlbumsAndMediaItems => write!(f, "albums_and_media_item"),
            TableName::NextInode => write!(f, "next_inode"),
            TableName::MediaItemsInAlbum => write!(f, "media_items_in_album"),
        }
    }
}

pub trait NextInodeDb: Sized {
    fn get_and_update_inode(&self) -> Result<Inode, DbError>;
}

pub trait PhotoDbRo: Sized {
    // Listings
    fn media_items(&self) -> Result<Vec<PhotoDbMediaItem>, DbError>;
    fn albums(&self) -> Result<Vec<PhotoDbAlbum>, DbError>;
    fn media_items_in_album(&self, inode: Inode) -> Result<Vec<PhotoDbMediaItem>, DbError>;

    // Single items
    fn media_item_by_name(&self, name: &str) -> Result<Option<PhotoDbMediaItem>, DbError>;
    fn media_item_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbMediaItem>, DbError>;
    fn album_by_name(&self, name: &str) -> Result<Option<PhotoDbAlbum>, DbError>;
    fn album_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbAlbum>, DbError>;
    fn item_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbMediaItemAlbum>, DbError>;

    // Check staleness
    fn last_updated_media(&self) -> Result<Option<UtcDateTime>, DbError>;
    fn last_updated_album(&self) -> Result<Option<UtcDateTime>, DbError>;
}

pub trait PhotoDb: PhotoDbRo + Sized {
    // Insert/Update
    fn upsert_media_item(
        &self,
        id: &GoogleId,
        filename: &str,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError>;
    fn upsert_album(
        &self,
        id: &GoogleId,
        title: &str,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError>;
    fn upsert_media_item_in_album(
        &self,
        album_id: &GoogleId,
        media_item_id: &GoogleId,
    ) -> Result<(), DbError>;
}

fn ensure_schema(db: &RwLock<rusqlite::Connection>) -> Result<(), DbError> {
    let db = db.write()?;

    // AlbumsAndMediaItems
    db.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS '{}' (
                google_id         TEXT NOT NULL,
                type              TEXT NOT NULL,
                name              TEXT NOT NULL,
                inode             INTEGER NOT NULL,
                last_remote_check INTEGER NOT NULL,
                PRIMARY KEY (google_id)
            );",
            TableName::AlbumsAndMediaItems
        ),
        &[],
    )?;
    db.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS '{}_by_indoe' ON '{}' (inode);",
            TableName::AlbumsAndMediaItems,
            TableName::AlbumsAndMediaItems
        ),
        &[],
    )?;
    db.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS '{}_by_name' ON '{}' (name);",
            TableName::AlbumsAndMediaItems,
            TableName::AlbumsAndMediaItems
        ),
        &[],
    )?;

    // NextInode
    // inodes under 100 are for "special" nodes like the "albums" folder
    // these are not stored in the DB as it would just mirror code.
    db.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS '{}' (inode INTEGER NOT NULL);",
            TableName::NextInode
        ),
        &[],
    )?;
    db.execute(
        &format!(
            "INSERT OR IGNORE INTO '{}' (inode) VALUES (100);",
            TableName::NextInode
        ),
        &[],
    )?;

    // MediaItemsInAlbum
    db.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS '{}' (
                album_google_id      TEXT NOT NULL,
                media_item_google_id TEXT NOT NULL,
                PRIMARY KEY(album_google_id, media_item_google_id),
                FOREIGN KEY (album_google_id) REFERENCES '{}' (google_id) ON DELETE CASCADE,
                FOREIGN KEY (media_item_google_id) REFERENCES '{}' (google_id) ON DELETE CASCADE
            );",
            TableName::MediaItemsInAlbum,
            TableName::AlbumsAndMediaItems,
            TableName::AlbumsAndMediaItems
        ),
        &[],
    )?;
    db.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS '{}_by_album_google_id' ON '{}' (album_google_id);",
            TableName::MediaItemsInAlbum,
            TableName::MediaItemsInAlbum
        ),
        &[],
    )?;

    Result::Ok(())
}

pub struct SqliteDb {
    db: RwLock<rusqlite::Connection>,
}

unsafe impl Send for SqliteDb {}
unsafe impl Sync for SqliteDb {}

fn row_to_album(row: &rusqlite::Row) -> PhotoDbAlbum {
    row_to_item(row)
}

fn row_to_media_item(row: &rusqlite::Row) -> PhotoDbMediaItem {
    row_to_item(row)
}

fn row_to_item(row: &rusqlite::Row) -> PhotoDbMediaItemAlbum {
    let google_id: String = row.get(0);
    let media_type: String = row.get(1);
    let name: String = row.get(2);
    let last_remote_check: i64 = row.get(3);
    let inode: i64 = row.get(4);
    PhotoDbMediaItemAlbum::new(
        google_id,
        name,
        MediaTypes::from(media_type.as_str()),
        Utc::timestamp(&Utc, last_remote_check, 0),
        inode as u64,
    )
}

fn row_to_option_datetime(row: &rusqlite::Row) -> Result<Option<UtcDateTime>, DbError> {
    match row.get_checked(0) {
        Ok(last_modified) => Result::Ok(Option::Some(Utc::timestamp(&Utc, last_modified, 0))),
        Err(rusqlite::Error::InvalidColumnType(_, rusqlite::types::Type::Null)) => {
            Result::Ok(Option::None)
        }
        Err(error) => Result::Err(DbError::from(error)),
    }
}

impl PhotoDbRo for SqliteDb {
    fn media_items(&self) -> Result<Vec<PhotoDbMediaItem>, DbError> {
        let db = self.db.read()?;
        let mut statment = db.prepare(&format!(
            "SELECT google_id, type, name, last_remote_check, inode FROM '{}' WHERE type = '{}';",
            TableName::AlbumsAndMediaItems,
            MediaTypes::MediaItem
        ))?;
        let media_items_results = statment.query_map(&[], row_to_media_item)?;

        let mut media_items: Vec<PhotoDbMediaItem> = Vec::new();
        for media_item_result in media_items_results {
            let media_item = media_item_result?;
            media_items.push(media_item);
        }
        Result::Ok(media_items)
    }

    fn albums(&self) -> Result<Vec<PhotoDbAlbum>, DbError> {
        let db = self.db.read()?;
        let mut statment = db.prepare(&format!(
            "SELECT google_id, type, name, last_remote_check, inode FROM '{}' WHERE type = '{}';",
            TableName::AlbumsAndMediaItems,
            MediaTypes::Album
        ))?;
        let media_items_results = statment.query_map(&[], row_to_album)?;

        let mut media_items: Vec<PhotoDbAlbum> = Vec::new();
        for media_item_result in media_items_results {
            let media_item = media_item_result?;
            media_items.push(media_item);
        }
        Result::Ok(media_items)
    }

    fn media_items_in_album(&self, inode: Inode) -> Result<Vec<PhotoDbMediaItem>, DbError> {
        let db = self.db.read()?;
        let mut statment = db.prepare(&format!(
            "SELECT google_id, type, name, last_remote_check, inode
            FROM '{}' INNER JOIN '{}' ON '{}'.google_id = '{}'.media_item_google_id
            WHERE type = '{}' AND album_google_id = (SELECT google_id FROM {} WHERE inode = ?);",
            TableName::AlbumsAndMediaItems,
            TableName::MediaItemsInAlbum,
            TableName::AlbumsAndMediaItems,
            TableName::MediaItemsInAlbum,
            MediaTypes::MediaItem,
            TableName::AlbumsAndMediaItems,
        ))?;
        let media_items_results = statment.query_map(&[&(inode as i64)], row_to_media_item)?;

        let mut media_items: Vec<PhotoDbMediaItem> = Vec::new();
        for media_item_result in media_items_results {
            let media_item = media_item_result?;
            media_items.push(media_item);
        }
        Result::Ok(media_items)
    }

    fn media_item_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbMediaItem>, DbError> {
        let result = self.item_by_inode(inode)?;
        match result {
            None => Result::Ok(Option::None),
            Some(item) => match item.media_type {
                MediaTypes::MediaItem => Result::Ok(Option::Some(item)),
                _ => Result::Ok(Option::None),
            },
        }
    }

    fn media_item_by_name(&self, name: &str) -> Result<Option<PhotoDbMediaItem>, DbError> {
        let db = self.db.read()?;
        let result: Result<PhotoDbMediaItem, rusqlite::Error> = db.query_row(
            &format!("SELECT google_id, type, name, last_remote_check, inode FROM '{}' WHERE type = '{}' AND name = ?;", TableName::AlbumsAndMediaItems, MediaTypes::MediaItem),
            &[&name], row_to_media_item,
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(album) => Result::Ok(Option::Some(album)),
        }
    }

    fn album_by_name(&self, name: &str) -> Result<Option<PhotoDbAlbum>, DbError> {
        let db = self.db.read()?;
        let result: Result<PhotoDbAlbum, rusqlite::Error> = db.query_row(
            &format!("SELECT google_id, type, name, last_remote_check, inode FROM '{}' WHERE type = '{}' AND name = ?;", TableName::AlbumsAndMediaItems, MediaTypes::Album),
            &[&name], row_to_album,
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(album) => Result::Ok(Option::Some(album)),
        }
    }

    fn album_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbAlbum>, DbError> {
        let result = self.item_by_inode(inode)?;
        match result {
            None => Result::Ok(Option::None),
            Some(item) => match item.media_type {
                MediaTypes::Album => Result::Ok(Option::Some(item)),
                _ => Result::Ok(Option::None),
            },
        }
    }

    fn item_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbMediaItemAlbum>, DbError> {
        let db = self.db.read()?;
        let result: Result<PhotoDbMediaItemAlbum, rusqlite::Error> = db.query_row(
            &format!(
                "SELECT google_id, type, name, last_remote_check, inode FROM '{}' WHERE inode = ?;",
                TableName::AlbumsAndMediaItems
            ),
            &[&(inode as i64)],
            row_to_item,
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(album) => Result::Ok(Option::Some(album)),
        }
    }

    fn last_updated_media(&self) -> Result<Option<UtcDateTime>, DbError> {
        self.last_updated_x(MediaTypes::MediaItem)
    }

    fn last_updated_album(&self) -> Result<Option<UtcDateTime>, DbError> {
        self.last_updated_x(MediaTypes::Album)
    }
}

impl PhotoDb for SqliteDb {
    fn upsert_media_item(
        &self,
        id: &GoogleId,
        filename: &str,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError> {
        let inode = self.get_and_update_inode()?;
        self.upsert_x(
            id,
            MediaTypes::MediaItem,
            filename,
            inode,
            &last_modified_time,
        )
    }

    fn upsert_album(
        &self,
        id: &GoogleId,
        title: &str,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError> {
        let inode = self.get_and_update_inode()?;
        self.upsert_x(id, MediaTypes::Album, title, inode, &last_modified_time)
    }

    fn upsert_media_item_in_album(
        &self,
        album_id: &GoogleId,
        media_item_id: &GoogleId,
    ) -> Result<(), DbError> {
        self.db.write()?.execute(
            &format!("INSERT OR REPLACE INTO '{}' (album_google_id, media_item_google_id) VALUES (?, ?);", TableName::MediaItemsInAlbum),
            &[&album_id, &media_item_id],
        )?;
        Result::Ok(())
    }
}

impl NextInodeDb for SqliteDb {
    // TODO: Fix locking
    fn get_and_update_inode(&self) -> Result<Inode, DbError> {
        let db = self.db.write()?;
        db.execute(
            &format!("UPDATE '{}' SET inode = inode + 1;", TableName::NextInode),
            &[],
        )?;
        let result: Result<i64, rusqlite::Error> = db.query_row(
            &format!("SELECT inode FROM '{}';", TableName::NextInode),
            &[],
            |row| row.get(0),
        );
        match result {
            Err(error) => Result::Err(DbError::from(error)),
            Ok(inode) => Result::Ok(inode as Inode),
        }
    }
}

impl SqliteDb {
    pub fn new(db: RwLock<rusqlite::Connection>) -> Result<SqliteDb, DbError> {
        ensure_schema(&db)?;
        Result::Ok(SqliteDb { db })
    }

    fn last_updated_x(&self, media_type: MediaTypes) -> Result<Option<UtcDateTime>, DbError> {
        self.db.read()?.query_row(
            &format!(
                "SELECT MIN(last_remote_check) AS min_last_remote_check FROM '{}' WHERE type = ?;",
                TableName::AlbumsAndMediaItems
            ),
            &[&format!("{}", media_type)],
            row_to_option_datetime,
        )?
    }

    fn upsert_x(
        &self,
        id: &GoogleId,
        media_type: MediaTypes,
        name: &str,
        inode: Inode,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError> {
        let media_type = format!("{}", media_type);
        let inode_signed = inode as i64;
        let last_modified_time = last_modified_time.timestamp();
        self.db.write()?.execute(
            &format!("INSERT OR REPLACE INTO '{}' (google_id, type, name, inode, last_remote_check) VALUES (?, ?, ?, ?, ?);", TableName::AlbumsAndMediaItems),
            &[&id, &media_type, &name, &inode_signed, &last_modified_time],
        )?;
        Result::Ok(inode)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sqlitedb_last_updated_album() {
        let in_mem_db = RwLock::new(rusqlite::Connection::open_in_memory().unwrap());
        let db = SqliteDb::new(in_mem_db).unwrap();

        let now_unix = Utc::now().timestamp();
        let now = Utc::timestamp(&Utc, now_unix, 0);
        let now_later = Utc::timestamp(&Utc, now_unix + 100, 0);
        let now_earlier = Utc::timestamp(&Utc, now_unix - 100, 0);
        let now_earlier_earlier = Utc::timestamp(&Utc, now_unix - 200, 0);

        // Test Empty DB
        assert!(db.last_updated_album().unwrap().is_none());

        // Test single item
        db.upsert_album(&String::from("GoogleId1"), &String::from("Title 1"), &now)
            .unwrap();
        assert_eq!(db.last_updated_album().unwrap().unwrap(), now);

        // Test that the oldest item it returned
        db.upsert_album(
            &String::from("GoogleId2"),
            &String::from("Title 2"),
            &now_later,
        ).unwrap();
        assert_eq!(db.last_updated_album().unwrap().unwrap(), now);

        db.upsert_album(
            &String::from("GoogleId3"),
            &String::from("Title 3"),
            &now_earlier,
        ).unwrap();
        assert_eq!(db.last_updated_album().unwrap().unwrap(), now_earlier);

        // Test non album types are ignored
        db.upsert_media_item(
            &String::from("GoogleId4"),
            &String::from("Photo 1"),
            &now_earlier_earlier,
        ).unwrap();
        assert_eq!(db.last_updated_album().unwrap().unwrap(), now_earlier);

        // Test upsert old item
        db.upsert_album(
            &String::from("GoogleId1"),
            &String::from("Title 1"),
            &now_earlier_earlier,
        ).unwrap();
        assert_eq!(
            db.last_updated_album().unwrap().unwrap(),
            now_earlier_earlier
        );
    }

    #[test]
    fn sqlitedb_last_updated_media() {
        let in_mem_db = RwLock::new(rusqlite::Connection::open_in_memory().unwrap());
        let db = SqliteDb::new(in_mem_db).unwrap();

        let now_unix = Utc::now().timestamp();
        let now = Utc::timestamp(&Utc, now_unix, 0);
        let now_later = Utc::timestamp(&Utc, now_unix + 100, 0);
        let now_earlier = Utc::timestamp(&Utc, now_unix - 100, 0);
        let now_earlier_earlier = Utc::timestamp(&Utc, now_unix - 200, 0);

        // Test Empty DB
        assert!(db.last_updated_album().unwrap().is_none());

        // Test single item
        db.upsert_media_item(&String::from("GoogleId1"), &String::from("Title 1"), &now)
            .unwrap();
        assert_eq!(db.last_updated_media().unwrap().unwrap(), now);

        // Test that the oldest item it returned
        db.upsert_media_item(
            &String::from("GoogleId2"),
            &String::from("Title 2"),
            &now_later,
        ).unwrap();
        assert_eq!(db.last_updated_media().unwrap().unwrap(), now);

        db.upsert_media_item(
            &String::from("GoogleId3"),
            &String::from("Title 3"),
            &now_earlier,
        ).unwrap();
        assert_eq!(db.last_updated_media().unwrap().unwrap(), now_earlier);

        // Test non media_ites types are ignored
        db.upsert_album(
            &String::from("GoogleId4"),
            &String::from("Album 1"),
            &now_earlier_earlier,
        ).unwrap();
        assert_eq!(db.last_updated_media().unwrap().unwrap(), now_earlier);

        // Test upsert old item
        db.upsert_media_item(
            &String::from("GoogleId1"),
            &String::from("Title 1"),
            &now_earlier_earlier,
        ).unwrap();
        assert_eq!(
            db.last_updated_album().unwrap().unwrap(),
            now_earlier_earlier
        );
    }

    #[test]
    fn sqlitedb_next_inode() {
        let in_mem_db = RwLock::new(rusqlite::Connection::open_in_memory().unwrap());
        let db = SqliteDb::new(in_mem_db).unwrap();

        let now_unix = Utc::now().timestamp();
        let now = Utc::timestamp(&Utc, now_unix, 0);

        assert_eq!(db.get_and_update_inode().unwrap(), 101);
        assert_eq!(db.get_and_update_inode().unwrap(), 102);
        assert_eq!(
            db.upsert_media_item(&String::from("GoogleId1"), &String::from("Title 1"), &now,)
                .unwrap(),
            103
        );
        assert_eq!(
            db.upsert_album(&String::from("GoogleId2"), &String::from("Album 1"), &now,)
                .unwrap(),
            104
        );
        assert_eq!(db.get_and_update_inode().unwrap(), 105);
    }

    #[test]
    fn sqlitedb_upsert_media_item_in_album() {
        let in_mem_db = RwLock::new(rusqlite::Connection::open_in_memory().unwrap());
        let db = SqliteDb::new(in_mem_db).unwrap();

        let now_unix = Utc::now().timestamp();
        let now = Utc::timestamp(&Utc, now_unix, 0);

        // Test Empty DB
        assert_eq!(db.media_items_in_album(101).unwrap().len(), 0);

        // Test empty album item
        let album_inode = db
            .upsert_album(&"GoogleIdAlbum1", &"Album 1", &now)
            .unwrap();
        assert_eq!(db.media_items_in_album(album_inode).unwrap().len(), 0);

        // Test single photo in Album
        db.upsert_media_item(&"GoogleIdMediaItem1", &"Media Item 1", &now)
            .unwrap();
        db.upsert_media_item_in_album("GoogleIdAlbum1", "GoogleIdMediaItem1")
            .unwrap();

        let media_items_in_album = db.media_items_in_album(album_inode).unwrap();
        assert_eq!(media_items_in_album.len(), 1);
        assert_eq!(media_items_in_album[0].google_id(), "GoogleIdMediaItem1");

        // Test upsert updates correctly
        db.upsert_media_item_in_album("GoogleIdAlbum1", "GoogleIdMediaItem1")
            .unwrap();
        let media_items_in_album = db.media_items_in_album(album_inode).unwrap();
        assert_eq!(media_items_in_album.len(), 1);
        assert_eq!(media_items_in_album[0].google_id(), "GoogleIdMediaItem1");

        // Test multiple photos in album
        db.upsert_media_item(&"GoogleIdMediaItem2", &"Media Item 2", &now)
            .unwrap();
        db.upsert_media_item_in_album("GoogleIdAlbum1", "GoogleIdMediaItem2")
            .unwrap();

        let media_items_in_album = db.media_items_in_album(album_inode).unwrap();
        assert_eq!(media_items_in_album.len(), 2);
        assert_eq!(media_items_in_album[0].google_id(), "GoogleIdMediaItem1");
        assert_eq!(media_items_in_album[1].google_id(), "GoogleIdMediaItem2");

        // Upsert fails if no album or media item is present in other tables
        assert!(
            db.upsert_media_item_in_album("GoogleIdAlbum2", "GoogleIdMediaItem1")
                .is_err()
        );
        assert!(
            db.upsert_media_item_in_album("GoogleIdAlbum1", "GoogleIdMediaItem3")
                .is_err()
        );
    }

    #[test]
    fn table_name_string() {
        assert_eq!(
            format!("{}", TableName::AlbumsAndMediaItems),
            "albums_and_media_item"
        );
        assert_eq!(
            format!("{:?}", TableName::AlbumsAndMediaItems),
            "AlbumsAndMediaItems"
        );
        assert_eq!(format!("{}", TableName::NextInode), "next_inode");
        assert_eq!(format!("{:?}", TableName::NextInode), "NextInode");
        assert_eq!(
            format!("{}", TableName::MediaItemsInAlbum),
            "media_items_in_album"
        );
        assert_eq!(
            format!("{:?}", TableName::MediaItemsInAlbum),
            "MediaItemsInAlbum"
        );
    }

    /*
    #[test]
    fn sqlitedb_inode_children() {
        let in_mem_db = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteDb::new(in_mem_db).unwrap();

        db.insert(1, 1, String::from("")).unwrap();
        db.insert(2, 1, String::from("test_file.txt")).unwrap();

        db.insert(3, 1, String::from("dir1")).unwrap();
        db.insert(4, 3, String::from("file_in_dir_1")).unwrap();

        assert_eq!(db.children(1).unwrap(), set_of_inodes(&[1, 2, 3]));
        assert_eq!(db.children(2).unwrap(), set_of_inodes(&[]));

        assert_eq!(db.children(3).unwrap(), set_of_inodes(&[4]));
        assert_eq!(db.children(4).unwrap(), set_of_inodes(&[]));
    }

    #[test]
    fn sqlitedb_inode_inode() {
        let in_mem_db = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteDb::new(in_mem_db).unwrap();

        db.insert(1, 1, String::from("")).unwrap();
        db.insert(2, 1, String::from("test_file.txt")).unwrap();

        db.insert(3, 1, String::from("dir1")).unwrap();
        db.insert(4, 3, String::from("file_in_dir_1")).unwrap();

        assert_eq!(db.inode(1, String::from("")).unwrap().unwrap(), 1);
        assert_eq!(
            db.inode(1, String::from("test_file.txt")).unwrap().unwrap(),
            2
        );

        assert_eq!(db.inode(1, String::from("dir1")).unwrap().unwrap(), 3);
        assert_eq!(
            db.inode(3, String::from("file_in_dir_1")).unwrap().unwrap(),
            4
        );

        assert!(
            db.inode(1, String::from("not_a_file_in_dir.txt"))
                .unwrap()
                .is_none()
        );
        assert!(
            db.inode(10, String::from("not_a_real_inode"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn sqlitedb_inode_new_inode() {
        let in_mem_db = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteDb::new(in_mem_db).unwrap();

        assert_eq!(db.new_inode().unwrap(), 1);

        db.insert(1, 1, String::from("")).unwrap();
        assert_eq!(db.new_inode().unwrap(), 2);

        db.insert(2, 1, String::from("test_file.txt")).unwrap();
        assert_eq!(db.new_inode().unwrap(), 3);
    }

    fn set_of_inodes(inodes: &[u64]) -> HashSet<u64> {
        let mut set: HashSet<u64> = HashSet::new();
        for inode in inodes {
            set.insert(*inode);
        }
        set
    }
*/
}
