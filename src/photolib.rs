extern crate google_photoslibrary1 as photoslibrary1;
extern crate hyper;
extern crate yup_oauth2 as oauth2;

extern crate sqlite;

use std::option::Option;
use std::result::Result;

use photoslibrary1::{Error, PhotosLibrary};

use std::borrow::BorrowMut;

#[derive(Debug)]
pub enum PhotoLibError {
    SqlError(sqlite::Error),
    GoogleBackendError,
}

pub trait PhotoLib: Sized {
    fn list_albums(&self) -> Result<Vec<String>, PhotoLibError>;
}

fn ensure_schema(db: &sqlite::Connection) {
    db.execute("CREATE TABLE IF NOT EXISTS albums (id TEXT PRIMARY KEY, title TEXT);")
        .unwrap();
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
    ) -> DbBackedPhotoLib<C, A> {
        ensure_schema(&db);
        DbBackedPhotoLib { photos_library, db }
    }
}
impl<C, A> PhotoLib for DbBackedPhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    fn list_albums(&self) -> Result<Vec<String>, PhotoLibError> {
        // You can configure optional parameters by calling the respective setters at will, and
        // execute the final call using `doit()`.
        // Values shown here are possibly random and not representative !
        {
            let mut page_token: Option<String> = Option::None;
            loop {
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
                            .prepare("INSERT OR REPLACE INTO albums (id, title) VALUES (?, ?);")
                            .map_err(|err| PhotoLibError::SqlError(err))?;
                        for album in res.1.albums.unwrap().into_iter() {
                            statment.reset().unwrap();
                            statment.bind(1, album.id.unwrap().as_str()).unwrap();
                            statment.bind(2, album.title.unwrap().as_str()).unwrap();
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
        }
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
