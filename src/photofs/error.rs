use rusqlite;

use db::DbError;

#[derive(Debug)]
pub enum PhotoFsError {
    SqlError(rusqlite::Error),
    LockingError,
}

impl From<DbError> for PhotoFsError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::SqlError(sql_error) => PhotoFsError::SqlError(sql_error),
            DbError::LockingError => PhotoFsError::LockingError,
        }
    }
}
