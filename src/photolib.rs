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

use photoslibrary1::PhotosLibrary;

use domain::*;

#[derive(Debug)]
pub enum RemotePhotoLibError {
    GoogleBackendError(photoslibrary1::Error),
    NotImpYet,
}

impl From<photoslibrary1::Error> for RemotePhotoLibError {
    fn from(error: photoslibrary1::Error) -> RemotePhotoLibError {
        RemotePhotoLibError::GoogleBackendError(error)
    }
}

pub trait RemotePhotoLib: Sized {
    fn media_items(&self) -> Result<Vec<(GoogleId, String)>, RemotePhotoLibError>;
    fn albums(&self) -> Result<Vec<(GoogleId, String)>, RemotePhotoLibError>;
    fn media_item(&self, google_id: GoogleId) -> Result<&[u8], RemotePhotoLibError>;
}

pub struct HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    photos_library: PhotosLibrary<C, A>,
}

impl<C, A> HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    pub fn new(photos_library: PhotosLibrary<C, A>) -> HttpRemotePhotoLib<C, A> {
        HttpRemotePhotoLib { photos_library }
    }
}

impl<C, A> RemotePhotoLib for HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    fn media_items(&self) -> Result<Vec<(GoogleId, String)>, RemotePhotoLibError> {
        let mut all_media_items = Vec::new();
        let mut page_token: Option<String> = Option::None;
        loop {
            let mut result_builder = self.photos_library.media_items().list().page_size(50);
            if page_token.is_some() {
                result_builder = result_builder.page_token(page_token.unwrap().as_str());
            }
            let remote_result = result_builder.doit();

            match remote_result {
                Err(e) => {
                    error!("{}", e);
                    return Result::Err(RemotePhotoLibError::from(e));
                }
                Ok(res) => {
                    debug!("Success: listing photos");
                    for media_item in res.1.media_items.unwrap() {
                        all_media_items.push((media_item.id.unwrap(), media_item.filename.unwrap()))
                    }

                    page_token = res.1.next_page_token;
                    if page_token.is_none() {
                        break;
                    }
                }
            };
        }
        Result::Ok(all_media_items)
    }

    fn albums(&self) -> Result<Vec<(GoogleId, String)>, RemotePhotoLibError> {
        let mut all_albums = Vec::new();
        let mut page_token: Option<String> = Option::None;
        loop {
            let mut result_builder = self.photos_library.albums().list().page_size(50);
            if page_token.is_some() {
                result_builder = result_builder.page_token(page_token.unwrap().as_str());
            }
            let remote_result = result_builder.doit();

            match remote_result {
                Err(e) => {
                    error!("{}", e);
                    return Result::Err(RemotePhotoLibError::from(e));
                }
                Ok(res) => {
                    debug!("Success: listing albums");
                    for album in res.1.albums.unwrap() {
                        all_albums.push((album.id.unwrap(), album.title.unwrap()));
                    }

                    page_token = res.1.next_page_token;
                    if page_token.is_none() {
                        break;
                    }
                }
            };
        }
        Result::Ok(all_albums)
    }

    fn media_item(&self, google_id: GoogleId) -> Result<&[u8], RemotePhotoLibError> {
        let media_item = self.photos_library.media_items().get(&google_id).doit()?;
        let base_url = media_item.1.base_url.unwrap();
        let download_url = format!("{}=d", base_url);
        info!("Have base_url={:?} but still nedd HTTP client ( {} )", base_url, download_url); // TODO
        Result::Err(RemotePhotoLibError::NotImpYet)
    }
}
