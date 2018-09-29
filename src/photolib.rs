extern crate google_photoslibrary1 as photoslibrary1;
extern crate hyper;
extern crate yup_oauth2 as oauth2;

extern crate rusqlite;

extern crate chrono;
extern crate time;

use std::borrow::BorrowMut;
use std::convert::From;
use std::option::Option;
use std::result::Result;

use photoslibrary1::{Error, PhotosLibrary};

use chrono::Utc;
use time::Duration;

use db::{DbError, PhotoDb};

#[derive(Debug)]
pub enum PhotoLibError {
    SqlError(rusqlite::Error),
    CorruptDatabase,
    GoogleBackendError,
}

impl From<DbError> for PhotoLibError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::SqlError(sql_error) => PhotoLibError::SqlError(sql_error),
            DbError::CorruptDatabase => PhotoLibError::CorruptDatabase,
        }
    }
}

pub trait PhotoLib: Sized {
    fn media(&self) -> Result<Vec<String>, PhotoLibError>;
    fn albums(&self) -> Result<Vec<String>, PhotoLibError>;
}

pub struct DbBackedPhotoLib<C, A, D>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
    D: PhotoDb,
{
    photos_library: PhotosLibrary<C, A>,
    db: D,
}

impl<C, A, D> DbBackedPhotoLib<C, A, D>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
    D: PhotoDb,
{
    pub fn new(
        photos_library: PhotosLibrary<C, A>,
        db: D,
    ) -> Result<DbBackedPhotoLib<C, A, D>, PhotoLibError> {
        Result::Ok(DbBackedPhotoLib { photos_library, db })
    }

    fn update_media_allowed_staleness(
        &self,
        allowed_staleness: Duration,
    ) -> Result<(), PhotoLibError> {
        let last_updated_media_option = self.db.last_updated_media()?;
        let should_update = match last_updated_media_option {
            Some(last_updated_media) => (Utc::now() - last_updated_media) > allowed_staleness,
            None => true,
        };
        let result = if should_update {
            self.update_media()
        } else {
            Result::Ok(())
        };
        result
    }

    fn update_albums_allowed_staleness(
        &self,
        allowed_staleness: Duration,
    ) -> Result<(), PhotoLibError> {
        let last_updated_media_option = self.db.last_updated_album()?;
        let should_update = match last_updated_media_option {
            Some(last_updated_media) => (Utc::now() - last_updated_media) > allowed_staleness,
            None => true,
        };
        let result = if should_update {
            self.update_albums()
        } else {
            Result::Ok(())
        };
        result
    }

    fn update_media(&self) -> Result<(), PhotoLibError> {
        let mut page_token: Option<String> = Option::None;
        loop {
            let last_modified_time = Utc::now();
            let mut result_builder = self.photos_library.media_items().list().page_size(50);
            if page_token.is_some() {
                result_builder = result_builder.page_token(page_token.unwrap().as_str());
            }
            let result = result_builder.doit();

            match result {
                Err(e) => match e {
                    // The Error enum provides details about what exactly happened.
                    // You can also just use its `Debug`, `Display` or `Error` traits
                    Error::HttpError(_)
                    | Error::MissingAPIKey
                    | Error::MissingToken(_)
                    | Error::Cancelled
                    | Error::UploadSizeLimitExceeded(_, _)
                    | Error::Failure(_)
                    | Error::BadRequest(_)
                    | Error::FieldClash(_)
                    | Error::JsonDecodeError(_, _) => {
                        debug!("{}", e);
                        return Result::Err(PhotoLibError::GoogleBackendError);
                    }
                },
                Ok(res) => {
                    debug!("Success: listing photod");
                    for media_item in res.1.media_items.unwrap().into_iter() {
                        self.db.insert_media(
                            media_item.id.unwrap(),
                            media_item.filename.unwrap(),
                            last_modified_time,
                        )?
                    }

                    page_token = res.1.next_page_token;
                    if page_token.is_none() {
                        break;
                    }
                }
            };
        }
        Result::Ok(())
    }

    fn update_albums(&self) -> Result<(), PhotoLibError> {
        let mut page_token: Option<String> = Option::None;
        loop {
            let last_modified_time = Utc::now();
            let mut result_builder = self.photos_library.albums().list().page_size(50);
            if page_token.is_some() {
                result_builder = result_builder.page_token(page_token.unwrap().as_str());
            }
            let result = result_builder.doit();

            match result {
                Err(e) => match e {
                    // The Error enum provides details about what exactly happened.
                    // You can also just use its `Debug`, `Display` or `Error` traits
                    Error::HttpError(_)
                    | Error::MissingAPIKey
                    | Error::MissingToken(_)
                    | Error::Cancelled
                    | Error::UploadSizeLimitExceeded(_, _)
                    | Error::Failure(_)
                    | Error::BadRequest(_)
                    | Error::FieldClash(_)
                    | Error::JsonDecodeError(_, _) => {
                        debug!("{}", e);
                        return Result::Err(PhotoLibError::GoogleBackendError);
                    }
                },
                Ok(res) => {
                    debug!("Success: listing albums");
                    for album in res.1.albums.unwrap().into_iter() {
                        self.db.insert_album(
                            album.id.unwrap(),
                            album.title.unwrap(),
                            last_modified_time,
                        )?;
                    }

                    page_token = res.1.next_page_token;
                    if page_token.is_none() {
                        break;
                    }
                }
            };
        }
        Result::Ok(())
    }
}

impl<C, A, D> PhotoLib for DbBackedPhotoLib<C, A, D>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
    D: PhotoDb,
{
    fn media(&self) -> Result<Vec<String>, PhotoLibError> {
        self.update_media_allowed_staleness(Duration::minutes(30))?;
        self.db.media().map_err(PhotoLibError::from)
    }

    fn albums(&self) -> Result<Vec<String>, PhotoLibError> {
        self.update_albums_allowed_staleness(Duration::minutes(30))?;
        self.db.albums().map_err(PhotoLibError::from)
    }
}
