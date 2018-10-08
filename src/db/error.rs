use std::convert::From;
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
}
