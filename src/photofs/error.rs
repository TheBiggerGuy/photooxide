use std::convert::From;
use std::fmt;

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

impl std::error::Error for PhotoFsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PhotoFsError::SqlError(err) => Option::Some(err),
            PhotoFsError::LockingError => Option::None,
        }
    }
}

impl fmt::Display for PhotoFsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhotoFsError::SqlError(err) => write!(f, "PhotoFsError: SqlError({:?})", err),
            PhotoFsError::LockingError => write!(f, "PhotoFsError: LockingError"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn photo_fs_error_display() {
        assert_eq!(
            format!(
                "{}",
                PhotoFsError::from(DbError::SqlError(rusqlite::Error::SqliteSingleThreadedMode))
            ),
            "PhotoFsError: SqlError(SqliteSingleThreadedMode)"
        );
        assert_eq!(
            format!("{}", PhotoFsError::LockingError),
            "PhotoFsError: LockingError"
        );
    }
}
