use std::convert::From;
use std::fmt;

use crate::rust_filesystem::FuseError;

use crate::db::DbError;

#[derive(Debug)]
pub enum PhotoFsError {
    PhotoDbError(DbError),
}

impl From<DbError> for PhotoFsError {
    fn from(error: DbError) -> Self {
        PhotoFsError::PhotoDbError(error)
    }
}

impl std::error::Error for PhotoFsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PhotoFsError::PhotoDbError(err) => Option::Some(err),
        }
    }
}

impl fmt::Display for PhotoFsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhotoFsError::PhotoDbError(err) => write!(f, "PhotoFsError: PhotoDbError({:?})", err),
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

#[cfg(test)]
mod test {
    use super::*;
    use std::error::Error;

    #[test]
    fn photo_fs_error_display() {
        assert_eq!(
            format!("{}", PhotoFsError::PhotoDbError(DbError::LockingError)),
            "PhotoFsError: PhotoDbError(LockingError)"
        );
    }

    #[test]
    fn photo_fs_error_source() {
        assert_eq!(
            format!(
                "{}",
                PhotoFsError::PhotoDbError(DbError::LockingError)
                    .source()
                    .unwrap()
            ),
            "DbError: LockingError"
        );
    }

    #[test]
    fn photo_fs_error_from_dberror() {
        assert_eq!(
            format!("{}", PhotoFsError::from(DbError::LockingError)),
            "PhotoFsError: PhotoDbError(LockingError)"
        );
    }

    #[test]
    fn fuse_error_from_photo_fs_error() {
        assert_eq!(
            format!(
                "{}",
                FuseError::from(PhotoFsError::PhotoDbError(DbError::LockingError))
            ),
            "FuseError: FunctionNotImplemented"
        );
    }

    #[test]
    fn fuse_error_from_photo_db_error() {
        assert_eq!(
            format!("{}", FuseError::from(DbError::LockingError)),
            "FuseError: FunctionNotImplemented"
        );
    }
}
