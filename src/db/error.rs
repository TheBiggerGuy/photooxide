use std::convert::From;
use std::fmt;
use std::sync;

use rusqlite;

#[derive(Debug)]
pub enum DbError {
    SqlError(rusqlite::Error),
    LockingError,
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

impl std::error::Error for DbError {}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::SqlError(err) => write!(f, "DbError: SqlError({:?})", err),
            DbError::LockingError => write!(f, "DbError: LockingError"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn db_error_from_rusqlite() {
        match DbError::from(rusqlite::Error::SqliteSingleThreadedMode) {
            DbError::SqlError(_) => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn db_error_display() {
        assert_eq!(
            format!(
                "{}",
                DbError::from(rusqlite::Error::SqliteSingleThreadedMode)
            ),
            "DbError: SqlError(SqliteSingleThreadedMode)"
        );
        assert_eq!(format!("{}", DbError::LockingError), "DbError: LockingError");
    }
}
