use std::convert::From;

use rusqlite;

use crate::rust_filesystem::FuseError;

use crate::db::DbError;

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

impl From<PhotoFsError> for FuseError {
    fn from(_error: PhotoFsError) -> Self {
        FuseError::FunctionNotImplemented
    }
}

impl From<DbError> for FuseError {
    fn from(_error: DbError) -> Self {
        FuseError::FunctionNotImplemented
    }
}
