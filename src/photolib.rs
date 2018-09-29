extern crate google_photoslibrary1 as photoslibrary1;
extern crate hyper;
extern crate yup_oauth2 as oauth2;

extern crate sqlite;

extern crate chrono;
extern crate time;

use std::borrow::BorrowMut;
use std::collections::HashSet;
use std::option::Option;
use std::result::Result;

use photoslibrary1::{Error, PhotosLibrary};

use chrono::prelude::*;
use chrono::{TimeZone, Utc};
use time::Duration;

#[derive(Debug)]
pub enum PhotoLibError {
    SqlError(sqlite::Error),
    CorruptDatabase,
    GoogleBackendError,
}

pub trait PhotoLib: Sized {
    fn media(&self) -> Result<Vec<String>, PhotoLibError>;
    fn albums(&self) -> Result<Vec<String>, PhotoLibError>;
}

fn ensure_schema(db: &sqlite::Connection) -> Result<(), PhotoLibError> {
    db.execute("CREATE TABLE IF NOT EXISTS albums (id TEXT PRIMARY KEY, title TEXT, last_modified INTEGER);")
        .map_err(|err| PhotoLibError::SqlError(err))?;
    db.execute("CREATE TABLE IF NOT EXISTS media_items (id TEXT PRIMARY KEY, filename TEXT, last_modified INTEGER);")
        .map_err(|err| PhotoLibError::SqlError(err))?;
    Result::Ok(())
}

pub struct DbBackedPhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    photos_library: PhotosLibrary<C, A>,
    db: sqlite::Connection,
}

impl<C, A> DbBackedPhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    pub fn new(
        photos_library: PhotosLibrary<C, A>,
        db: sqlite::Connection,
    ) -> Result<DbBackedPhotoLib<C, A>, PhotoLibError> {
        ensure_schema(&db)?;
        Result::Ok(DbBackedPhotoLib { photos_library, db })
    }

    fn last_updated_media(&self) -> Result<Option<DateTime<Utc>>, PhotoLibError> {
        self.last_updated_x("media_items")
    }

    fn last_updated_album(&self) -> Result<Option<DateTime<Utc>>, PhotoLibError> {
        self.last_updated_x("albums")
    }

    fn last_updated_x(&self, table: &str) -> Result<Option<DateTime<Utc>>, PhotoLibError> {
        let mut statment = self
            .db
            .prepare(format!("SELECT MIN(last_modified) FROM {};", table))
            .map_err(|err| PhotoLibError::SqlError(err))?;
        let mut results = Vec::new();
        loop {
            if statment.next().unwrap() == sqlite::State::Done {
                break;
            }
            let last_modified: i64 = statment.read(0).unwrap();
            results.push(last_modified);
        }
        if results.len() == 0 {
            return Result::Ok(Option::None);
        } else if results.len() == 1 {
            let last_modified = results.pop().unwrap();
            return Result::Ok(Option::Some(Utc::timestamp(&Utc, last_modified, 0)));
        } else {
            return Result::Err(PhotoLibError::CorruptDatabase);
        }
    }

    fn update_media_allowed_staleness(
        &self,
        allowed_staleness: Duration,
    ) -> Result<(), PhotoLibError> {
        let last_updated_media_option = self.last_updated_media()?;
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
        let last_updated_media_option = self.last_updated_album()?;
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
                    let mut statment = self
                            .db
                            .prepare("INSERT OR REPLACE INTO media_items (id, filename, last_modified) VALUES (?, ?, ?);")
                            .map_err(|err| PhotoLibError::SqlError(err))?;
                    let last_modified_time_unix = last_modified_time.timestamp() as i64;
                    for media_item in res.1.media_items.unwrap().into_iter() {
                        statment.reset().unwrap();
                        statment.bind(1, media_item.id.unwrap().as_str()).unwrap();
                        statment
                            .bind(2, media_item.filename.unwrap().as_str())
                            .unwrap();
                        statment.bind(3, last_modified_time_unix).unwrap();
                        loop {
                            if statment.next().unwrap() == sqlite::State::Done {
                                break;
                            }
                        }
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
                    let mut statment = self
                            .db
                            .prepare("INSERT OR REPLACE INTO albums (id, title, last_modified) VALUES (?, ?, ?);")
                            .map_err(|err| PhotoLibError::SqlError(err))?;
                    let last_modified_time_unix = last_modified_time.timestamp() as i64;
                    for album in res.1.albums.unwrap().into_iter() {
                        statment.reset().unwrap();
                        statment.bind(1, album.id.unwrap().as_str()).unwrap();
                        statment.bind(2, album.title.unwrap().as_str()).unwrap();
                        statment.bind(3, last_modified_time_unix).unwrap();
                        loop {
                            if statment.next().unwrap() == sqlite::State::Done {
                                break;
                            }
                        }
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

impl<C, A> PhotoLib for DbBackedPhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    fn media(&self) -> Result<Vec<String>, PhotoLibError> {
        self.update_media_allowed_staleness(Duration::minutes(30))?;

        let mut filenames: HashSet<String> = HashSet::new();
        let mut statment = self
            .db
            .prepare("SELECT filename FROM media_items;")
            .unwrap();
        loop {
            if statment.next().unwrap() == sqlite::State::Done {
                break;
            }
            let filename: String = statment.read(0).unwrap();
            debug!("filename: {}", filename);
            filenames.insert(filename);
        }
        let filenames_vec = filenames.into_iter().collect();
        Result::Ok(filenames_vec)
    }

    fn albums(&self) -> Result<Vec<String>, PhotoLibError> {
        self.update_albums_allowed_staleness(Duration::minutes(30))?;

        let mut titles: Vec<String> = Vec::new();
        let mut statment = self.db.prepare("SELECT title FROM albums;").unwrap();
        loop {
            if statment.next().unwrap() == sqlite::State::Done {
                break;
            }
            let title: String = statment.read(0).unwrap();
            debug!("Title: {}", title);
            titles.push(title);
        }
        Result::Ok(titles)
    }
}
