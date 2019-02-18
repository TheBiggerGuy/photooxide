use std::iter;
use std::result::Result;
use std::sync::Mutex;

use rusqlite;
use rusqlite::types::ToSql;

use crate::db::{DbError, TableName};

pub trait TokenStorageDb: Sized {
    fn get_oath_token(&self, scope_hash: u64) -> Result<Option<String>, DbError>;
    fn set_oath_token(&self, scope_hash: u64, token: Option<String>) -> Result<(), DbError>;
}

pub fn ensure_schema(db: &Mutex<rusqlite::Connection>) -> Result<(), DbError> {
    let db = db.lock()?;

    db.execute(
        &format!(
            "CREATE TABLE IF NOT EXISTS '{}' (
                scope_hash        INTEGER NOT NULL,
                token             TEXT NOT NULL,
                PRIMARY KEY (scope_hash)
            );",
            TableName::OauthTokenStorage
        ),
        iter::empty::<&dyn ToSql>(),
    )?;

    Result::Ok(())
}

pub struct SqliteTokenStorageDb {
    db: Mutex<rusqlite::Connection>,
}

unsafe impl Send for SqliteTokenStorageDb {}
unsafe impl Sync for SqliteTokenStorageDb {}

impl SqliteTokenStorageDb {
    pub fn from_path<P: AsRef<std::path::Path>>(path: P) -> Result<SqliteTokenStorageDb, DbError> {
        let connection = rusqlite::Connection::open(path)?;
        SqliteTokenStorageDb::try_new(Mutex::new(connection))
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<SqliteTokenStorageDb, DbError> {
        let connection = rusqlite::Connection::open_in_memory()?;
        SqliteTokenStorageDb::try_new(Mutex::new(connection))
    }

    fn try_new(db: Mutex<rusqlite::Connection>) -> Result<SqliteTokenStorageDb, DbError> {
        ensure_schema(&db)?;
        Result::Ok(SqliteTokenStorageDb { db })
    }
}

impl TokenStorageDb for SqliteTokenStorageDb {
    fn get_oath_token(&self, scope_hash: u64) -> Result<Option<String>, DbError> {
        let scope_hash = scope_hash as i64;
        let result: Result<String, rusqlite::Error> = self.db.lock()?.query_row(
            &format!(
                "SELECT token FROM '{}' WHERE scope_hash = ?;",
                TableName::OauthTokenStorage
            ),
            &[&scope_hash],
            |row| row.get(0),
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Result::Ok(Option::None),
            Err(error) => Result::Err(DbError::from(error)),
            Ok(token) => Result::Ok(Option::Some(token)),
        }
    }

    fn set_oath_token(&self, scope_hash: u64, token: Option<String>) -> Result<(), DbError> {
        let scope_hash = scope_hash as i64;
        match token {
            Some(token_value) => {
                self.db.lock()?.execute(
                    &format!(
                        "INSERT OR REPLACE INTO '{}' (scope_hash, token) VALUES (?, ?);",
                        TableName::OauthTokenStorage
                    ),
                    &[&scope_hash as &dyn ToSql, &token_value],
                )?;
            }
            None => {
                self.db.lock()?.execute(
                    &format!(
                        "DELETE FROM '{}' WHERE scope_hash = ?;",
                        TableName::OauthTokenStorage
                    ),
                    &[&scope_hash],
                )?;
            }
        }
        Result::Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sqlitedb_oath_token() {
        let db = SqliteTokenStorageDb::in_memory().unwrap();

        assert!(db.get_oath_token(0).unwrap().is_none());

        db.set_oath_token(0, Option::None).unwrap();
        assert!(db.get_oath_token(0).unwrap().is_none());

        let token0_ver0 = "{\"token\": \"abc123\"}";
        db.set_oath_token(0, Option::Some(String::from(token0_ver0)))
            .unwrap();
        assert_eq!(db.get_oath_token(0).unwrap().unwrap(), token0_ver0);

        db.set_oath_token(0, Option::None).unwrap();
        assert!(db.get_oath_token(0).unwrap().is_none());

        let token0_ver0 = "{\"token\": \"abc123\"}";
        db.set_oath_token(0, Option::Some(String::from(token0_ver0)))
            .unwrap();
        assert_eq!(db.get_oath_token(0).unwrap().unwrap(), token0_ver0);

        let token0_ver1 = "{\"token\": \"abc123_2\"}";
        db.set_oath_token(0, Option::Some(String::from(token0_ver1)))
            .unwrap();
        assert_eq!(db.get_oath_token(0).unwrap().unwrap(), token0_ver1);

        let token1_ver0 = "{\"token\": \"abc123_3\"}";
        db.set_oath_token(1, Option::Some(String::from(token1_ver0)))
            .unwrap();
        assert_eq!(db.get_oath_token(0).unwrap().unwrap(), token0_ver1);
        assert_eq!(db.get_oath_token(1).unwrap().unwrap(), token1_ver0);

        db.set_oath_token(0, Option::None).unwrap();
        assert!(db.get_oath_token(0).unwrap().is_none());
        assert_eq!(db.get_oath_token(1).unwrap().unwrap(), token1_ver0);
    }
}
