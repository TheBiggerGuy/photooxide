use std::fmt;
use std::option::Option;
use std::sync::Arc;

use crate::oauth2::{Token, TokenStorage};
use serde_json;

use crate::db;

#[derive(Debug)]
pub enum OauthTokenStorageError {
    DbError(db::DbError),
    SerializingDeserializingError(serde_json::error::Error),
}

impl From<db::DbError> for OauthTokenStorageError {
    fn from(error: db::DbError) -> Self {
        OauthTokenStorageError::DbError(error)
    }
}

impl From<serde_json::error::Error> for OauthTokenStorageError {
    fn from(error: serde_json::error::Error) -> Self {
        OauthTokenStorageError::SerializingDeserializingError(error)
    }
}

impl std::error::Error for OauthTokenStorageError {}

impl fmt::Display for OauthTokenStorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OauthTokenStorageError::DbError(err) => {
                write!(f, "OauthTokenStorageError: DbError({:?})", err)
            }
            OauthTokenStorageError::SerializingDeserializingError(err) => write!(
                f,
                "OauthTokenStorageError: SerializingDeserializingError({})",
                err
            ),
        }
    }
}

#[derive(Debug, Clone, new)]
pub struct OauthTokenStorage<A>
where
    A: db::TokenStorageDb,
{
    db: Arc<A>,
}

impl<A> TokenStorage for OauthTokenStorage<A>
where
    A: db::TokenStorageDb,
{
    type Error = OauthTokenStorageError;
    fn set(
        &mut self,
        scope_hash: u64,
        _scopes: &Vec<&str>,
        token: Option<Token>,
    ) -> Result<(), Self::Error> {
        let token_as_str = match token {
            Some(token_value) => Option::Some(serde_json::to_string(&token_value)?),
            None => Option::None,
        };
        self.db
            .set_oath_token(scope_hash, token_as_str)
            .map_err(OauthTokenStorageError::from)
    }

    fn get(&self, scope_hash: u64, _scopes: &Vec<&str>) -> Result<Option<Token>, Self::Error> {
        let result = match self.db.get_oath_token(scope_hash)? {
            Option::Some(token_as_str) => {
                let token: Token = serde_json::from_str(&token_as_str)?;
                Option::Some(token)
            }
            Option::None => Option::None,
        };
        Result::Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::oauth2::Token;

    use crate::db::SqliteTokenStorageDb;

    #[test]
    fn oauthtokenstorage_oath_token() -> Result<(), OauthTokenStorageError> {
        let scopes: Vec<&str> = Vec::new();
        let mut db = OauthTokenStorage::new(Arc::new(SqliteTokenStorageDb::in_memory()?));

        assert!(db.get(0, &scopes)?.is_none());

        db.set(0, &scopes, Option::None)?;
        assert!(db.get(0, &scopes)?.is_none());

        let token0_ver0 = Token {
            access_token: String::from("access_token_0_ver0"),
            refresh_token: String::from("refresh_token"),
            token_type: String::from("token_type"),
            expires_in: Option::None,
            expires_in_timestamp: Option::None,
        };
        db.set(0, &scopes, Option::Some(token0_ver0.clone()))?;
        assert_eq!(db.get(0, &scopes)?.unwrap(), token0_ver0.clone());

        db.set(0, &scopes, Option::None)?;
        assert!(db.get(0, &scopes)?.is_none());

        let token0_ver0 = Token {
            access_token: String::from("access_token_0_ver0"),
            refresh_token: String::from("refresh_token"),
            token_type: String::from("token_type"),
            expires_in: Option::None,
            expires_in_timestamp: Option::None,
        };
        db.set(0, &scopes, Option::Some(token0_ver0.clone()))?;
        assert_eq!(db.get(0, &scopes)?.unwrap(), token0_ver0.clone());

        let token0_ver1 = Token {
            access_token: String::from("access_token_0_ver1"),
            refresh_token: String::from("refresh_token"),
            token_type: String::from("token_type"),
            expires_in: Option::None,
            expires_in_timestamp: Option::None,
        };
        db.set(0, &scopes, Option::Some(token0_ver1.clone()))?;
        assert_eq!(db.get(0, &scopes)?.unwrap(), token0_ver1.clone());

        let token1_ver0 = Token {
            access_token: String::from("access_token_1_ver0"),
            refresh_token: String::from("refresh_token"),
            token_type: String::from("token_type"),
            expires_in: Option::None,
            expires_in_timestamp: Option::None,
        };
        db.set(1, &scopes, Option::Some(token1_ver0.clone()))?;
        assert_eq!(db.get(0, &scopes)?.unwrap(), token0_ver1.clone());
        assert_eq!(db.get(1, &scopes)?.unwrap(), token1_ver0.clone());

        db.set(0, &scopes, Option::None)?;
        assert!(db.get(0, &scopes)?.is_none());
        assert_eq!(db.get(1, &scopes)?.unwrap(), token1_ver0.clone());

        Result::Ok(())
    }
}
