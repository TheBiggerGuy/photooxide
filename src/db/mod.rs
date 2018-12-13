use std::convert::From;
use std::iter;
use std::option::Option;
use std::result::Result;
use std::sync::Mutex;

use rusqlite;
use rusqlite::types::ToSql;

use chrono::{TimeZone, Utc};

use crate::domain::{
    GoogleId, Inode, MediaTypes, PhotoDbAlbum, PhotoDbMediaItem, PhotoDbMediaItemAlbum, UtcDateTime,
};

mod error;
pub use self::error::DbError;

mod inode_db;
use self::inode_db::ensure_schema_next_inode;
pub use self::inode_db::NextInodeDb;

mod token_storage_db;
use self::token_storage_db::ensure_schema_token_storage;
pub use self::token_storage_db::TokenStorageDb;

mod table_name;
use self::table_name::TableName;

pub trait PhotoDbRo: Sized {
    // Listings
    fn media_items(&self) -> Result<Vec<PhotoDbMediaItem>, DbError>;
    fn albums(&self) -> Result<Vec<PhotoDbAlbum>, DbError>;
    fn media_items_in_album(&self, inode: Inode) -> Result<Vec<PhotoDbMediaItem>, DbError>;
    fn media_items_in_album_length(&self, inode: Inode) -> Result<usize, DbError>;

    // Single items
    fn media_item_by_name(&self, name: &str) -> Result<Option<PhotoDbMediaItem>, DbError>;
    fn media_item_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbMediaItem>, DbError>;
    fn album_by_name(&self, name: &str) -> Result<Option<PhotoDbAlbum>, DbError>;
    fn album_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbAlbum>, DbError>;
    fn item_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbMediaItemAlbum>, DbError>;

    // Check staleness
    fn last_updated_media(&self) -> Result<Option<UtcDateTime>, DbError>;
    fn last_updated_album(&self) -> Result<Option<UtcDateTime>, DbError>;

    // existence
    fn exists(&self, id: &GoogleId) -> Result<bool, DbError>;
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

fn ensure_schema(db: &Mutex<rusqlite::Connection>) -> Result<(), DbError> {
    let db = db.lock()?;

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
        iter::empty::<&ToSql>(),
    )?;
    db.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS '{}_by_indoe' ON '{}' (inode);",
            TableName::AlbumsAndMediaItems,
            TableName::AlbumsAndMediaItems
        ),
        iter::empty::<&ToSql>(),
    )?;
    db.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS '{}_by_name' ON '{}' (name);",
            TableName::AlbumsAndMediaItems,
            TableName::AlbumsAndMediaItems
        ),
        iter::empty::<&ToSql>(),
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
        iter::empty::<&ToSql>(),
    )?;
    db.execute(
        &format!(
            "CREATE INDEX IF NOT EXISTS '{}_by_album_google_id' ON '{}' (album_google_id);",
            TableName::MediaItemsInAlbum,
            TableName::MediaItemsInAlbum
        ),
        iter::empty::<&ToSql>(),
    )?;

    Result::Ok(())
}

pub struct SqliteDb {
    db: Mutex<rusqlite::Connection>,
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
        let db = self.db.lock()?;
        let mut statment = db.prepare(&format!(
            "SELECT google_id, type, name, last_remote_check, inode FROM '{}' WHERE type = '{}' ORDER BY google_id;",
            TableName::AlbumsAndMediaItems,
            MediaTypes::MediaItem
        ))?;
        let media_items_results = statment.query_map(iter::empty::<&ToSql>(), row_to_media_item)?;

        let mut media_items: Vec<PhotoDbMediaItem> = Vec::new();
        for media_item_result in media_items_results {
            let media_item = media_item_result?;
            media_items.push(media_item);
        }
        Result::Ok(media_items)
    }

    fn albums(&self) -> Result<Vec<PhotoDbAlbum>, DbError> {
        let db = self.db.lock()?;
        let mut statment = db.prepare(&format!(
            "SELECT google_id, type, name, last_remote_check, inode FROM '{}' WHERE type = '{}' ORDER BY google_id;",
            TableName::AlbumsAndMediaItems,
            MediaTypes::Album
        ))?;
        let media_items_results = statment.query_map(iter::empty::<&ToSql>(), row_to_album)?;

        let mut media_items: Vec<PhotoDbAlbum> = Vec::new();
        for media_item_result in media_items_results {
            let media_item = media_item_result?;
            media_items.push(media_item);
        }
        Result::Ok(media_items)
    }

    fn media_items_in_album(&self, inode: Inode) -> Result<Vec<PhotoDbMediaItem>, DbError> {
        let db = self.db.lock()?;
        let mut statment = db.prepare(&format!(
            "SELECT google_id, type, name, last_remote_check, inode
            FROM '{}' INNER JOIN '{}' ON '{}'.google_id = '{}'.media_item_google_id
            WHERE type = '{}' AND album_google_id = (SELECT google_id FROM {} WHERE inode = ?) ORDER BY google_id;",
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

    fn media_items_in_album_length(&self, inode: Inode) -> Result<usize, DbError> {
        self.media_items_in_album(inode)
            .map(|media_items| media_items.len()) // TODO: Custom SQL
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
        let db = self.db.lock()?;
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
        let db = self.db.lock()?;
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
        let db = self.db.lock()?;
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

    fn exists(&self, id: &GoogleId) -> Result<bool, DbError> {
        let db = self.db.lock()?;
        let result: Result<(), rusqlite::Error> = db.query_row(
            &format!(
                "SELECT 1 FROM '{}' WHERE google_id = ?;",
                TableName::AlbumsAndMediaItems
            ),
            &[&id],
            |_row| (),
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(false),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(_) => Result::Ok(true),
        }
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
        self.db.lock()?.execute(
            &format!("INSERT OR REPLACE INTO '{}' (album_google_id, media_item_google_id) VALUES (?, ?);", TableName::MediaItemsInAlbum),
            &[&album_id, &media_item_id],
        )?;
        Result::Ok(())
    }
}

impl SqliteDb {
    pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> Result<SqliteDb, DbError> {
        let connection = rusqlite::Connection::open(path)?;
        SqliteDb::try_new(Mutex::new(connection))
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<SqliteDb, DbError> {
        let connection = rusqlite::Connection::open_in_memory()?;
        SqliteDb::try_new(Mutex::new(connection))
    }

    fn try_new(db: Mutex<rusqlite::Connection>) -> Result<SqliteDb, DbError> {
        ensure_schema(&db)?;
        ensure_schema_next_inode(&db)?;
        ensure_schema_token_storage(&db)?;
        Result::Ok(SqliteDb { db })
    }

    fn last_updated_x(&self, media_type: MediaTypes) -> Result<Option<UtcDateTime>, DbError> {
        self.db.lock()?.query_row(
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
        self.db.lock()?.execute(
            &format!("INSERT OR REPLACE INTO '{}' (google_id, type, name, inode, last_remote_check) VALUES (?, ?, ?, ?, ?);", TableName::AlbumsAndMediaItems),
            &[&id as &ToSql, &media_type, &name, &inode_signed, &last_modified_time],
        )?;
        Result::Ok(inode)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sqlitedb_last_updated_album() {
        let db = SqliteDb::in_memory().unwrap();

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
        )
        .unwrap();
        assert_eq!(db.last_updated_album().unwrap().unwrap(), now);

        db.upsert_album(
            &String::from("GoogleId3"),
            &String::from("Title 3"),
            &now_earlier,
        )
        .unwrap();
        assert_eq!(db.last_updated_album().unwrap().unwrap(), now_earlier);

        // Test non album types are ignored
        db.upsert_media_item(
            &String::from("GoogleId4"),
            &String::from("Photo 1"),
            &now_earlier_earlier,
        )
        .unwrap();
        assert_eq!(db.last_updated_album().unwrap().unwrap(), now_earlier);

        // Test upsert old item
        db.upsert_album(
            &String::from("GoogleId1"),
            &String::from("Title 1"),
            &now_earlier_earlier,
        )
        .unwrap();
        assert_eq!(
            db.last_updated_album().unwrap().unwrap(),
            now_earlier_earlier
        );
    }

    #[test]
    fn sqlitedb_last_updated_media() {
        let db = SqliteDb::in_memory().unwrap();

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
        )
        .unwrap();
        assert_eq!(db.last_updated_media().unwrap().unwrap(), now);

        db.upsert_media_item(
            &String::from("GoogleId3"),
            &String::from("Title 3"),
            &now_earlier,
        )
        .unwrap();
        assert_eq!(db.last_updated_media().unwrap().unwrap(), now_earlier);

        // Test non media_ites types are ignored
        db.upsert_album(
            &String::from("GoogleId4"),
            &String::from("Album 1"),
            &now_earlier_earlier,
        )
        .unwrap();
        assert_eq!(db.last_updated_media().unwrap().unwrap(), now_earlier);

        // Test upsert old item
        db.upsert_media_item(
            &String::from("GoogleId1"),
            &String::from("Title 1"),
            &now_earlier_earlier,
        )
        .unwrap();
        assert_eq!(
            db.last_updated_album().unwrap().unwrap(),
            now_earlier_earlier
        );
    }

    #[test]
    fn sqlitedb_upsert_media_item() {
        let db = SqliteDb::in_memory().unwrap();

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);

        // Assert DB is empty
        let media_items = db.media_items().unwrap();
        assert_eq!(media_items.len(), 0);

        // Test insert
        let inode = db
            .upsert_media_item(&String::from("GoogleId1"), &String::from("Title 1"), &now)
            .unwrap();
        assert_eq!(inode, 101);

        let media_items = db.media_items().unwrap();
        assert_eq!(media_items.len(), 1);
        assert_eq!(media_items[0].google_id(), "GoogleId1");

        // Test insert a second
        let inode = db
            .upsert_media_item(&String::from("GoogleId2"), &String::from("Title 2"), &now)
            .unwrap();
        assert_eq!(inode, 102);

        let media_items = db.media_items().unwrap();
        assert_eq!(media_items.len(), 2);
        assert_eq!(media_items[0].google_id(), "GoogleId1");
        assert_eq!(media_items[1].google_id(), "GoogleId2");

        // Test upsert
        let inode = db
            .upsert_media_item(
                &String::from("GoogleId1"),
                &String::from("Title 1 new title"),
                &now,
            )
            .unwrap();
        assert_eq!(inode, 103); // TODO: should be 101

        let media_items = db.media_items().unwrap();
        assert_eq!(media_items.len(), 2);
        assert_eq!(media_items[0].google_id(), "GoogleId1");
        assert_eq!(media_items[0].name, "Title 1 new title");
        assert_eq!(media_items[1].google_id(), "GoogleId2");
    }

    #[test]
    fn sqlitedb_upsert_album() {
        let db = SqliteDb::in_memory().unwrap();

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);

        // Assert DB is empty
        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 0);

        // Test insert
        let inode = db
            .upsert_album(&"GoogleIdAlbum1", &"Album 1", &now)
            .unwrap();
        assert_eq!(inode, 101);

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].google_id(), "GoogleIdAlbum1");

        // Test insert a second
        let inode = db
            .upsert_album(&"GoogleIdAlbum2", &"Album 2", &now)
            .unwrap();
        assert_eq!(inode, 102);

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].google_id(), "GoogleIdAlbum1");
        assert_eq!(albums[1].google_id(), "GoogleIdAlbum2");

        // Test upsert
        let inode = db
            .upsert_album(&"GoogleIdAlbum1", &"Album 1 new title", &now)
            .unwrap();
        assert_eq!(inode, 103); // TODO: should be 101

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].google_id(), "GoogleIdAlbum1");
        assert_eq!(albums[0].name, "Album 1 new title");
        assert_eq!(albums[1].google_id(), "GoogleIdAlbum2");
    }

    #[test]
    fn sqlitedb_upsert_incroments_inode() {
        let db = SqliteDb::in_memory().unwrap();

        let now_unix = Utc::now().timestamp();
        let now = Utc::timestamp(&Utc, now_unix, 0);

        assert_eq!(db.get_and_update_inode().unwrap(), 101);
        assert_eq!(
            db.upsert_media_item(&String::from("GoogleId1"), &String::from("Title 1"), &now,)
                .unwrap(),
            102
        );
        assert_eq!(
            db.upsert_album(&String::from("GoogleId2"), &String::from("Album 1"), &now,)
                .unwrap(),
            103
        );
        assert_eq!(db.get_and_update_inode().unwrap(), 104);
    }

    #[test]
    fn sqlitedb_upsert_media_item_in_album() {
        let db = SqliteDb::in_memory().unwrap();

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
        assert!(db
            .upsert_media_item_in_album("GoogleIdAlbum2", "GoogleIdMediaItem1")
            .is_err());
        assert!(db
            .upsert_media_item_in_album("GoogleIdAlbum1", "GoogleIdMediaItem3")
            .is_err());
    }

    #[test]
    fn sqlitedb_media_items() {
        let db = SqliteDb::in_memory().unwrap();

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);

        // Assert DB is empty
        let media_items = db.media_items().unwrap();
        assert_eq!(media_items.len(), 0);

        // Test insert
        db.upsert_media_item(&String::from("GoogleId1"), &String::from("Title 1"), &now)
            .unwrap();

        let media_items = db.media_items().unwrap();
        assert_eq!(media_items.len(), 1);
        assert_eq!(media_items[0].google_id(), "GoogleId1");

        // Test insert a second
        let inode = db
            .upsert_media_item(&String::from("GoogleId2"), &String::from("Title 2"), &now)
            .unwrap();
        assert_eq!(inode, 102);

        let media_items = db.media_items().unwrap();
        assert_eq!(media_items.len(), 2);
        assert_eq!(media_items[0].google_id(), "GoogleId1");
        assert_eq!(media_items[1].google_id(), "GoogleId2");
    }

    #[test]
    fn sqlitedb_albums() {
        let db = SqliteDb::in_memory().unwrap();

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);

        // Assert DB is empty
        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 0);

        // Test insert
        let inode = db
            .upsert_album(&"GoogleIdAlbum1", &"Album 1", &now)
            .unwrap();
        assert_eq!(inode, 101);

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].google_id(), "GoogleIdAlbum1");

        // Test insert a second
        let inode = db
            .upsert_album(&"GoogleIdAlbum2", &"Album 2", &now)
            .unwrap();
        assert_eq!(inode, 102);

        let albums = db.albums().unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].google_id(), "GoogleIdAlbum1");
        assert_eq!(albums[1].google_id(), "GoogleIdAlbum2");
    }

    #[test]
    fn sqlitedb_media_item_by_x() {
        let db = SqliteDb::in_memory().unwrap();

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);

        // Assert when DB is empty
        assert!(db.media_item_by_inode(100).unwrap().is_none());
        assert!(db.media_item_by_name("foo").unwrap().is_none());

        // insert some data
        let inode1 = db
            .upsert_media_item(&String::from("GoogleId1"), &String::from("Title 1"), &now)
            .unwrap();
        let inode2 = db
            .upsert_media_item(&String::from("GoogleId2"), &String::from("Title 2"), &now)
            .unwrap();

        // Lookup by name and inode are equal
        let by_inode = db.media_item_by_inode(inode1).unwrap().unwrap();
        let by_name = db.media_item_by_name("Title 1").unwrap().unwrap();
        assert_eq!(by_inode.google_id(), "GoogleId1");
        assert_eq!(by_inode, by_name);

        // Lookup find the correct node
        assert_eq!(
            db.media_item_by_inode(inode1).unwrap().unwrap().google_id(),
            "GoogleId1"
        );
        assert_eq!(
            db.media_item_by_inode(inode2).unwrap().unwrap().google_id(),
            "GoogleId2"
        );

        assert_eq!(
            db.media_item_by_name("Title 1")
                .unwrap()
                .unwrap()
                .google_id(),
            "GoogleId1"
        );
        assert_eq!(
            db.media_item_by_name("Title 2")
                .unwrap()
                .unwrap()
                .google_id(),
            "GoogleId2"
        );
    }

    #[test]
    fn sqlitedb_album_by_x() {
        let db = SqliteDb::in_memory().unwrap();

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);

        // Assert when DB is empty
        assert!(db.album_by_inode(100).unwrap().is_none());
        assert!(db.album_by_name("foo").unwrap().is_none());

        // insert some data
        let inode1 = db
            .upsert_album(&String::from("GoogleId1"), &String::from("Album 1"), &now)
            .unwrap();
        let inode2 = db
            .upsert_album(&String::from("GoogleId2"), &String::from("Album 2"), &now)
            .unwrap();

        // Lookup by name and inode are equal
        let by_inode = db.album_by_inode(inode1).unwrap().unwrap();
        let by_name = db.album_by_name("Album 1").unwrap().unwrap();
        assert_eq!(by_inode.google_id(), "GoogleId1");
        assert_eq!(by_inode, by_name);

        // Lookup find the correct node
        assert_eq!(
            db.album_by_inode(inode1).unwrap().unwrap().google_id(),
            "GoogleId1"
        );
        assert_eq!(
            db.album_by_inode(inode2).unwrap().unwrap().google_id(),
            "GoogleId2"
        );

        assert_eq!(
            db.album_by_name("Album 1").unwrap().unwrap().google_id(),
            "GoogleId1"
        );
        assert_eq!(
            db.album_by_name("Album 2").unwrap().unwrap().google_id(),
            "GoogleId2"
        );
    }

    #[test]
    fn sqlitedb_exists() {
        let db = SqliteDb::in_memory().unwrap();

        let now = Utc::timestamp(&Utc, Utc::now().timestamp(), 0);

        // Assert when DB is empty
        assert_eq!(db.exists("GoogleId1").unwrap(), false);

        // insert some data
        db.upsert_album(&String::from("GoogleId1"), &String::from("Album 1"), &now)
            .unwrap();
        db.upsert_media_item(&String::from("GoogleId2"), &String::from("Title 1"), &now)
            .unwrap();

        // normal
        assert_eq!(db.exists("GoogleId1").unwrap(), true);
        assert_eq!(db.exists("GoogleId2").unwrap(), true);
        assert_eq!(db.exists("GoogleId3").unwrap(), false);
    }
}
