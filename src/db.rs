extern crate google_photoslibrary1 as photoslibrary1;
extern crate hyper;
extern crate yup_oauth2 as oauth2;

extern crate rusqlite;

extern crate chrono;
extern crate time;

use std::collections::HashSet;
use std::convert::From;
use std::option::Option;
use std::result::Result;

use chrono::prelude::*;
use chrono::{TimeZone, Utc};

#[derive(Debug)]
pub enum DbError {
    SqlError(rusqlite::Error),
    CorruptDatabase,
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        DbError::SqlError(error)
    }
}

pub trait PhotoDb: Sized {
    fn media(&self) -> Result<Vec<String>, DbError>;
    fn albums(&self) -> Result<Vec<String>, DbError>;

    fn insert_media(
        &self,
        id: String,
        filename: String,
        last_modified_time: DateTime<Utc>,
    ) -> Result<(), DbError>;
    fn insert_album(
        &self,
        id: String,
        title: String,
        last_modified_time: DateTime<Utc>,
    ) -> Result<(), DbError>;

    fn last_updated_media(&self) -> Result<Option<DateTime<Utc>>, DbError>;
    fn last_updated_album(&self) -> Result<Option<DateTime<Utc>>, DbError>;
}

pub trait InodeDb: Sized {
    fn children(&self, inode: u64) -> Result<HashSet<u64>, DbError>;
    fn parent(&self, inode: u64) -> Result<Option<u64>, DbError>;
    fn name(&self, inode: u64) -> Result<Option<String>, DbError>;
    fn insert(&self, inode: u64, parent_inode: u64, name: String) -> Result<(), DbError>;
}

fn ensure_schema(db: &rusqlite::Connection) -> Result<(), DbError> {
    db.execute("CREATE TABLE IF NOT EXISTS albums (id TEXT PRIMARY KEY, title TEXT, last_modified INTEGER);", &[])?;
    db.execute("CREATE TABLE IF NOT EXISTS media_items (id TEXT PRIMARY KEY, filename TEXT, last_modified INTEGER);", &[])?;
    db.execute("CREATE TABLE IF NOT EXISTS inodes (id INTEGER PRIMARY KEY, parent_id INTEGER, name TEXT, FOREIGN KEY(parent_id) REFERENCES inodes(id));", &[])?;
    Result::Ok(())
}

pub struct SqliteDb {
    db: rusqlite::Connection,
}

impl InodeDb for SqliteDb {
    fn children(&self, inode: u64) -> Result<HashSet<u64>, DbError> {
        let mut statment = self
            .db
            .prepare("SELECT id FROM inodes WHERE parent_id = ?;")?;
        let titles = statment.query_map(&[&(inode as i64)], |row| row.get(0))?;

        let mut uniq_children: HashSet<u64> = HashSet::new();
        for title_result in titles {
            let title: i64 = title_result?;
            debug!("title: {}", title);
            uniq_children.insert(title as u64);
        }
        Result::Ok(uniq_children)
    }

    fn parent(&self, inode: u64) -> Result<Option<u64>, DbError> {
        let result: Result<Result<i64, rusqlite::Error>, rusqlite::Error> = self.db.query_row(
            "SELECT parent_id FROM inodes WHERE id = ?",
            &[&(inode as i64)],
            |row| row.get_checked(0),
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(parent_id) => Result::Ok(Option::Some(parent_id? as u64)),
        }
    }

    fn name(&self, inode: u64) -> Result<Option<String>, DbError> {
        let result: Result<Result<String, rusqlite::Error>, rusqlite::Error> = self.db.query_row(
            "SELECT name FROM inodes WHERE id = ?",
            &[&(inode as i64)],
            |row| row.get_checked(0),
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(name) => Result::Ok(Option::Some(name?)),
        }
    }

    fn insert(&self, inode: u64, parent_inode: u64, name: String) -> Result<(), DbError> {
        let result = self.db.execute(
            "INSERT OR REPLACE INTO inodes (id, parent_id, name) VALUES (?, ?, ?);",
            &[&(inode as i64), &(parent_inode as i64), &name],
        );
        match result {
            Err(error) => Result::Err(DbError::from(error)),
            Ok(_) => Result::Ok(()),
        }
    }
}

impl PhotoDb for SqliteDb {
    fn media(&self) -> Result<Vec<String>, DbError> {
        let mut statment = self.db.prepare("SELECT filename FROM media_items;")?;
        let filenames = statment.query_map(&[], |row| row.get(0))?;

        let mut uniq_filenames: HashSet<String> = HashSet::new();
        for filename_result in filenames {
            let filename = filename_result?;
            debug!("filename: {}", filename);
            uniq_filenames.insert(filename);
        }
        let result = uniq_filenames.into_iter().collect();
        Result::Ok(result)
    }

    fn albums(&self) -> Result<Vec<String>, DbError> {
        let mut statment = self.db.prepare("SELECT title FROM albums;")?;
        let titles = statment.query_map(&[], |row| row.get(0))?;

        let mut uniq_titles: HashSet<String> = HashSet::new();
        for title_result in titles {
            let title = title_result?;
            debug!("title: {}", title);
            uniq_titles.insert(title);
        }
        let result = uniq_titles.into_iter().collect();
        Result::Ok(result)
    }

    fn insert_media(
        &self,
        id: String,
        filename: String,
        last_modified_time: DateTime<Utc>,
    ) -> Result<(), DbError> {
        self.db.execute(
            "INSERT OR REPLACE INTO media_items (id, filename, last_modified) VALUES (?, ?, ?);",
            &[&id, &filename, &last_modified_time.timestamp()],
        )?;
        Result::Ok(())
    }
    fn insert_album(
        &self,
        id: String,
        title: String,
        last_modified_time: DateTime<Utc>,
    ) -> Result<(), DbError> {
        self.db.execute(
            "INSERT OR REPLACE INTO albums (id, title, last_modified) VALUES (?, ?, ?);",
            &[&id, &title, &last_modified_time.timestamp()],
        )?;
        Result::Ok(())
    }

    fn last_updated_media(&self) -> Result<Option<DateTime<Utc>>, DbError> {
        self.last_updated_x("media_items")
    }

    fn last_updated_album(&self) -> Result<Option<DateTime<Utc>>, DbError> {
        self.last_updated_x("albums")
    }
}

impl SqliteDb {
    pub fn new(db: rusqlite::Connection) -> Result<SqliteDb, DbError> {
        ensure_schema(&db)?;
        Result::Ok(SqliteDb { db })
    }

    fn last_updated_x(&self, table: &str) -> Result<Option<DateTime<Utc>>, DbError> {
        let result: Result<i64, rusqlite::Error> = self.db.query_row(
            &format!(
                "SELECT IFNULL(MIN(last_modified), 0) AS min_last_modified FROM {};",
                table
            ),
            &[],
            |row| row.get_checked(0),
        )?;
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(last_modified) => Result::Ok(Option::Some(Utc::timestamp(&Utc, last_modified, 0))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sqlitedb_inode_insert() {
        let in_mem_db = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteDb::new(in_mem_db).unwrap();

        db.insert(1, 1, String::from(".")).unwrap();
        db.insert(2, 1, String::from("..")).unwrap();
        db.insert(3, 1, String::from("test_file.txt")).unwrap();

        db.insert(4, 1, String::from("dir1")).unwrap();
        db.insert(5, 4, String::from("file_in_dir_1")).unwrap();
    }

    #[test]
    fn sqlitedb_inode_parent() {
        let in_mem_db = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteDb::new(in_mem_db).unwrap();

        db.insert(1, 1, String::from(".")).unwrap();
        db.insert(2, 1, String::from("..")).unwrap();
        db.insert(3, 1, String::from("test_file.txt")).unwrap();

        db.insert(4, 1, String::from("dir1")).unwrap();
        db.insert(5, 4, String::from("file_in_dir_1")).unwrap();

        assert_eq!(db.parent(1).unwrap().unwrap(), 1);
        assert_eq!(db.parent(2).unwrap().unwrap(), 1);
        assert_eq!(db.parent(3).unwrap().unwrap(), 1);

        assert_eq!(db.parent(4).unwrap().unwrap(), 1);
        assert_eq!(db.parent(5).unwrap().unwrap(), 4);

        assert!(db.parent(6).unwrap().is_none());
    }

    #[test]
    fn sqlitedb_inode_name() {
        let in_mem_db = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteDb::new(in_mem_db).unwrap();

        db.insert(1, 1, String::from(".")).unwrap();
        db.insert(2, 1, String::from("..")).unwrap();
        db.insert(3, 1, String::from("test_file.txt")).unwrap();

        db.insert(4, 1, String::from("dir1")).unwrap();
        db.insert(5, 4, String::from("file_in_dir_1")).unwrap();

        assert_eq!(db.name(1).unwrap().unwrap(), String::from("."));
        assert_eq!(db.name(2).unwrap().unwrap(), String::from(".."));
        assert_eq!(db.name(3).unwrap().unwrap(), String::from("test_file.txt"));

        assert_eq!(db.name(4).unwrap().unwrap(), String::from("dir1"));
        assert_eq!(db.name(5).unwrap().unwrap(), String::from("file_in_dir_1"));

        assert!(db.name(6).unwrap().is_none());
    }

    #[test]
    fn sqlitedb_inode_children() {
        let in_mem_db = rusqlite::Connection::open_in_memory().unwrap();
        let db = SqliteDb::new(in_mem_db).unwrap();

        db.insert(1, 1, String::from(".")).unwrap();
        db.insert(2, 1, String::from("..")).unwrap();
        db.insert(3, 1, String::from("test_file.txt")).unwrap();

        db.insert(4, 1, String::from("dir1")).unwrap();
        db.insert(5, 4, String::from("file_in_dir_1")).unwrap();

        assert_eq!(db.children(1).unwrap(), set_of_inodes(&[1, 2, 3, 4]));
        assert_eq!(db.children(2).unwrap(), set_of_inodes(&[])); // TODO: Is this correct?
        assert_eq!(db.children(3).unwrap(), set_of_inodes(&[]));

        assert_eq!(db.children(4).unwrap(), set_of_inodes(&[5]));
        assert_eq!(db.children(5).unwrap(), set_of_inodes(&[]));

        assert_eq!(db.children(6).unwrap(), set_of_inodes(&[]));
    }

    fn set_of_inodes(inodes: &[u64]) -> HashSet<u64> {
        let mut set: HashSet<u64> = HashSet::new();
        for inode in inodes {
            set.insert(*inode);
        }
        set
    }
}
