use std::iter;
use std::result::Result;
use std::sync::Mutex;

use rusqlite;
use rusqlite::types::ToSql;

use crate::domain::Inode;

use crate::db::{DbError, TableName};

pub trait NextInodeDb: Sized {
    fn get_and_update_inode(&self) -> Result<Inode, DbError>;
}

pub fn ensure_schema(db: &Mutex<rusqlite::Connection>) -> Result<(), DbError> {
    let db = db.lock()?;

    // NextInode
    // inodes under 100 are for "special" nodes like the "albums" folder
    // these are not stored in the DB as it would just mirror code.
    db.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS '{}' (inode INTEGER NOT NULL);",
            TableName::NextInode
        ),
        iter::empty::<&dyn ToSql>(),
    )?;
    db.execute(
        &format!(
            "INSERT OR IGNORE INTO '{}' (inode) VALUES (100);",
            TableName::NextInode
        ),
        iter::empty::<&dyn ToSql>(),
    )?;

    Result::Ok(())
}

pub struct SqliteNextInodeDb {
    db: Mutex<rusqlite::Connection>,
}

unsafe impl Send for SqliteNextInodeDb {}
unsafe impl Sync for SqliteNextInodeDb {}

impl SqliteNextInodeDb {
    pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> Result<SqliteNextInodeDb, DbError> {
        let connection = rusqlite::Connection::open(path)?;
        SqliteNextInodeDb::try_new(Mutex::new(connection))
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<SqliteNextInodeDb, DbError> {
        let connection = rusqlite::Connection::open_in_memory()?;
        SqliteNextInodeDb::try_new(Mutex::new(connection))
    }

    fn try_new(db: Mutex<rusqlite::Connection>) -> Result<SqliteNextInodeDb, DbError> {
        ensure_schema(&db)?;
        Result::Ok(SqliteNextInodeDb { db })
    }
}

impl NextInodeDb for SqliteNextInodeDb {
    // TODO: Fix locking
    fn get_and_update_inode(&self) -> Result<Inode, DbError> {
        let db = self.db.lock()?;
        db.execute(
            &format!("UPDATE '{}' SET inode = inode + 1;", TableName::NextInode),
            iter::empty::<&dyn ToSql>(),
        )?;
        let result: Result<i64, rusqlite::Error> = db.query_row(
            &format!("SELECT inode FROM '{}';", TableName::NextInode),
            iter::empty::<&dyn ToSql>(),
            |row| row.get(0),
        );
        match result {
            Err(error) => Result::Err(DbError::from(error)),
            Ok(inode) => Result::Ok(inode as Inode),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sqlitedb_next_inode() -> Result<(), DbError> {
        let db = SqliteNextInodeDb::in_memory()?;

        assert_eq!(db.get_and_update_inode()?, 101);
        assert_eq!(db.get_and_update_inode()?, 102);

        Result::Ok(())
    }
}
