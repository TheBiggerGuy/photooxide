use std::result::Result;
use std::sync::RwLock;

use rusqlite;

use domain::Inode;

use db::{DbError, SqliteDb, TableName};

pub trait NextInodeDb: Sized {
    fn get_and_update_inode(&self) -> Result<Inode, DbError>;
}

pub fn ensure_schema_next_inode(db: &RwLock<rusqlite::Connection>) -> Result<(), DbError> {
    let db = db.write()?;

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

    Result::Ok(())
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sqlitedb_next_inode() {
        let in_mem_db = RwLock::new(rusqlite::Connection::open_in_memory().unwrap());
        let db = SqliteDb::new(in_mem_db).unwrap();

        assert_eq!(db.get_and_update_inode().unwrap(), 101);
        assert_eq!(db.get_and_update_inode().unwrap(), 102);
    }
}