extern crate google_photoslibrary1 as photoslibrary1;
extern crate hyper;
extern crate yup_oauth2 as oauth2;

extern crate rusqlite;

extern crate chrono;
extern crate time;

use std::convert::From;
use std::option::Option;
use std::result::Result;
use std::sync;
use std::sync::RwLock;

use chrono::{TimeZone, Utc};

use domain::{GoogleId, Inode, PhotoDbAlbum, PhotoDbMediaItem, UtcDateTime};

#[derive(Debug)]
pub enum DbError {
    SqlError(rusqlite::Error),
    LockingError,
    NotImpYet,
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

pub trait PhotoDbRo: Sized {
    // Listings
    fn media_items(&self) -> Result<Vec<PhotoDbMediaItem>, DbError>;
    fn albums(&self) -> Result<Vec<PhotoDbAlbum>, DbError>;
    fn media_items_in_album(&self, inode: Inode) -> Result<Vec<PhotoDbMediaItem>, DbError>;

    // Single items
    fn media_item_by_name(&self, name: &str) -> Result<Option<PhotoDbMediaItem>, DbError>;
    fn media_item_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbMediaItem>, DbError>;
    fn album(&self, name: &str) -> Result<Option<PhotoDbAlbum>, DbError>;

    // Check staleness
    fn last_updated_media(&self) -> Result<Option<UtcDateTime>, DbError>;
    fn last_updated_album(&self) -> Result<Option<UtcDateTime>, DbError>;
}

pub trait PhotoDb: PhotoDbRo + Sized {
    // Insert/Update
    fn upsert_media_item(
        &self,
        id: &GoogleId,
        filename: &String,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError>;
    fn upsert_album(
        &self,
        id: &GoogleId,
        title: &String,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError>;
}

const TABLE_ALBUMS_AND_MEDIA_ITEMS: &str = "albums_and_media_item";

fn ensure_schema(db: &RwLock<rusqlite::Connection>) -> Result<(), DbError> {
    let db = db.write()?;
    db.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS {} (
        google_id         TEXT NOT NULL,
        type              TEXT NOT NULL,
        name              TEXT NOT NULL,
        inode             INTEGER NOT NULL,
        last_remote_check INTEGER NOT NULL,
        PRIMARY KEY (google_id));",
            TABLE_ALBUMS_AND_MEDIA_ITEMS
        ),
        &[],
    )?;
    // inodes under 100 are for "special" nodes like the "albums" folder
    // these are not stored in the DB as it would just mirror code.
    db.execute(
        "CREATE TABLE IF NOT EXISTS next_inode (inode INTEGER NOT NULL DEFAULT (100));",
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
    let google_id: String = row.get(0);
    let name: String = row.get(1);
    let last_remote_check: i64 = row.get(2);
    let inode: i64 = row.get(3);
    PhotoDbAlbum {
        google_id,
        name,
        last_remote_check: Utc::timestamp(&Utc, last_remote_check, 0),
        inode: inode as u64,
    }
}

fn row_to_media_item(row: &rusqlite::Row) -> PhotoDbMediaItem {
    let google_id: String = row.get(0);
    let name: String = row.get(1);
    let last_remote_check: i64 = row.get(2);
    let inode: i64 = row.get(3);
    PhotoDbMediaItem {
        google_id,
        name,
        last_remote_check: Utc::timestamp(&Utc, last_remote_check, 0),
        inode: inode as u64,
    }
}

impl PhotoDbRo for SqliteDb {
    fn media_items(&self) -> Result<Vec<PhotoDbMediaItem>, DbError> {
        let db = self.db.read()?;
        let mut statment = db.prepare(&format!(
            "SELECT google_id, name, last_remote_check, inode FROM {} WHERE type = 'media_item';",
            TABLE_ALBUMS_AND_MEDIA_ITEMS
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
            "SELECT google_id, name, last_remote_check, inode FROM {} WHERE type = 'album';",
            TABLE_ALBUMS_AND_MEDIA_ITEMS
        ))?;
        let media_items_results = statment.query_map(&[], row_to_album)?;

        let mut media_items: Vec<PhotoDbAlbum> = Vec::new();
        for media_item_result in media_items_results {
            let media_item = media_item_result?;
            media_items.push(media_item);
        }
        Result::Ok(media_items)
    }

    fn media_items_in_album(&self, _inode: Inode) -> Result<Vec<PhotoDbMediaItem>, DbError> {
        Result::Err(DbError::NotImpYet)
    }

    fn media_item_by_inode(&self, inode: Inode) -> Result<Option<PhotoDbMediaItem>, DbError> {
        let db = self.db.read()?;
        let result: Result<PhotoDbMediaItem, rusqlite::Error> = db.query_row(
            &format!("SELECT google_id, name, last_remote_check, inode FROM {} WHERE type = 'media_item' AND inode = ?;", TABLE_ALBUMS_AND_MEDIA_ITEMS),
            &[&(inode as i64)], row_to_media_item,
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(album) => Result::Ok(Option::Some(album)),
        }
    }

    fn media_item_by_name(&self, name: &str) -> Result<Option<PhotoDbMediaItem>, DbError> {
        let db = self.db.read()?;
        let result: Result<PhotoDbMediaItem, rusqlite::Error> = db.query_row(
            &format!("SELECT google_id, name, last_remote_check, inode FROM {} WHERE type = 'media_item' AND name = ?;", TABLE_ALBUMS_AND_MEDIA_ITEMS),
            &[&name], row_to_media_item,
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(album) => Result::Ok(Option::Some(album)),
        }
    }

    fn album(&self, name: &str) -> Result<Option<PhotoDbAlbum>, DbError> {
        let db = self.db.read()?;
        let result: Result<PhotoDbAlbum, rusqlite::Error> = db.query_row(
            &format!("SELECT google_id, name, last_remote_check, inode FROM {} WHERE type = 'album' AND name = ?;", TABLE_ALBUMS_AND_MEDIA_ITEMS),
            &[&name], row_to_album,
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(album) => Result::Ok(Option::Some(album)),
        }
    }

    fn last_updated_media(&self) -> Result<Option<UtcDateTime>, DbError> {
        self.last_updated_x("media_item")
    }

    fn last_updated_album(&self) -> Result<Option<UtcDateTime>, DbError> {
        self.last_updated_x("album")
    }
}

impl PhotoDb for SqliteDb {
    fn upsert_media_item(
        &self,
        id: &GoogleId,
        filename: &String,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError> {
        self.db.write()?.execute(
            &format!("INSERT OR REPLACE INTO {} (google_id, type, name, inode, last_remote_check) VALUES (?, ?, ?, ?, ?);", TABLE_ALBUMS_AND_MEDIA_ITEMS),
            &[id, &"media_item", filename, &4, &last_modified_time.timestamp()],
        )?;
        Result::Ok(4)
    }

    fn upsert_album(
        &self,
        id: &GoogleId,
        title: &String,
        last_modified_time: &UtcDateTime,
    ) -> Result<Inode, DbError> {
        self.db.write()?.execute(
            &format!("INSERT OR REPLACE INTO {} (google_id, type, name, inode, last_remote_check) VALUES (?, ?, ?, ?, ?);", TABLE_ALBUMS_AND_MEDIA_ITEMS),
            &[id, &"album", title, &4, &last_modified_time.timestamp()],
        )?;
        Result::Ok(4)
    }
}

impl SqliteDb {
    pub fn new(db: RwLock<rusqlite::Connection>) -> Result<SqliteDb, DbError> {
        ensure_schema(&db)?;
        Result::Ok(SqliteDb { db })
    }

    fn last_updated_x(&self, table: &str) -> Result<Option<UtcDateTime>, DbError> {
        let result: Result<i64, rusqlite::Error> = self.db.read()?.query_row(
            &format!(
                "SELECT IFNULL(MIN(last_modified), 0) AS min_last_modified FROM {} WHERE type = ?;",
                TABLE_ALBUMS_AND_MEDIA_ITEMS
            ),
            &[&table],
            |row| row.get_checked(0),
        )?;
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(last_modified) => Result::Ok(Option::Some(Utc::timestamp(&Utc, last_modified, 0))),
        }
    }
}

/*
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sqlitedb_inode_insert() {
        let in_mem_db = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteDb::new(in_mem_db).unwrap();

        db.insert(1, 1, String::from("")).unwrap();
        db.insert(2, 1, String::from("test_file.txt")).unwrap();

        db.insert(3, 1, String::from("dir1")).unwrap();
        db.insert(4, 3, String::from("file_in_dir_1")).unwrap();
    }

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
}
*/
