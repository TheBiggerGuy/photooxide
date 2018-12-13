use std::convert::From;
use std::fmt;

use crate::db;

#[derive(Debug)]
pub enum PhotoOxideError {
    DbError(db::DbError),
}

impl From<db::DbError> for PhotoOxideError {
    fn from(error: db::DbError) -> Self {
        PhotoOxideError::DbError(error)
    }
}

impl std::error::Error for PhotoOxideError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PhotoOxideError::DbError(err) => Option::Some(err),
        }
    }
}

impl fmt::Display for PhotoOxideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhotoOxideError::DbError(err) => write!(f, "PhotoOxideError: DbError({:?})", err),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn photo_oxide_error_display() {
        assert_eq!(
            format!("{}", PhotoOxideError::DbError(db::DbError::LockingError)),
            "PhotoOxideError: DbError(LockingError)"
        );
    }
}
