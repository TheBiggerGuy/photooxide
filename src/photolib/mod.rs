use std::borrow::BorrowMut;
use std::convert::From;
use std::io::Read;
use std::option::Option;
use std::result::Result;

use hyper;
use oauth2;
use photoslibrary1::{PhotosLibrary, SearchMediaItemsRequest};

use futures::executor::block_on;
use futures::future;
use futures::stream;
use futures::StreamExt;

use domain::*;

mod error;
pub use self::error::RemotePhotoLibError;

#[derive(Debug, new)]
pub struct ItemListing {
    id: String,
    pub name: String,
}

impl ItemListing {
    pub fn google_id(&self) -> &GoogleId {
        &self.id
    }
}

#[derive(Clone, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
enum State {
    Start,
    Next(String),
    End,
}

pub trait RemotePhotoLibMetaData: Sized {
    fn media_items(&self) -> Result<Vec<ItemListing>, RemotePhotoLibError>;

    fn albums(&self) -> Result<Vec<ItemListing>, RemotePhotoLibError>;
    fn album(&self, google_id: &GoogleId) -> Result<Vec<ItemListing>, RemotePhotoLibError>;
}

pub trait RemotePhotoLibData: Sized {
    fn media_item(
        &self,
        google_id: &GoogleId,
        is_video: bool,
    ) -> Result<Vec<u8>, RemotePhotoLibError>;
}

pub struct HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    photos_library: PhotosLibrary<C, A>,
    data_http_client: hyper::Client,
}

unsafe impl<C, A> Send for HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{}
unsafe impl<C, A> Sync for HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{}

impl<C, A> HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    pub fn new(
        photos_library: PhotosLibrary<C, A>,
        data_http_client: hyper::Client,
    ) -> HttpRemotePhotoLib<C, A> {
        HttpRemotePhotoLib {
            photos_library,
            data_http_client,
        }
    }
}

impl<C, A> RemotePhotoLibMetaData for HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    fn media_items(&self) -> Result<Vec<ItemListing>, RemotePhotoLibError> {
        info!("media_items");
        let fat_stream = stream::unfold(State::Start, move |state| {
            debug!("media_items: {:?}", state);
            let page_token = match state {
                State::Start => None,
                State::Next(pt) => Some(pt),
                State::End => return None,
            };

            let mut result_builder = self.photos_library.media_items().list().page_size(50);
            if page_token.is_some() {
                result_builder = result_builder.page_token(page_token.unwrap().as_str());
            }

            match result_builder.doit() {
                Err(e) => {
                    error!("{}", e);
                    Option::Some(future::err(RemotePhotoLibError::from(e)))
                }
                Ok(res) => {
                    debug!("Success: listing photos");
                    let all_media_items = res
                        .1
                        .media_items
                        .unwrap()
                        .into_iter()
                        .map(|media_item| {
                            ItemListing::new(media_item.id.unwrap(), media_item.filename.unwrap())
                        }).map(future::ok);

                    let next_state = match res.1.next_page_token {
                        Some(pt) => State::Next(pt),
                        None => State::End,
                    };

                    Option::Some(future::ok((stream::iter_ok(all_media_items), next_state)))
                }
            }
        });
        let all_media_items: Vec<ItemListing> =
            block_on(fat_stream.flatten().buffered(51).collect())?;

        Result::Ok(all_media_items)
    }

    fn albums(&self) -> Result<Vec<ItemListing>, RemotePhotoLibError> {
        info!("albums");
        let fat_stream = stream::unfold(State::Start, move |state| {
            debug!("albums: {:?}", state);
            let page_token = match state {
                State::Start => None,
                State::Next(pt) => Some(pt),
                State::End => return None,
            };

            let mut result_builder = self.photos_library.albums().list().page_size(50);
            if page_token.is_some() {
                result_builder = result_builder.page_token(page_token.unwrap().as_str());
            }

            match result_builder.doit() {
                Err(e) => {
                    error!("{}", e);
                    Option::Some(future::err(RemotePhotoLibError::from(e)))
                }
                Ok(res) => {
                    debug!("Success: listing albums");
                    let all_albums = res
                        .1
                        .albums
                        .unwrap()
                        .into_iter()
                        .map(|album| ItemListing::new(album.id.unwrap(), album.title.unwrap()))
                        .map(future::ok);

                    let next_state = match res.1.next_page_token {
                        Some(pt) => State::Next(pt),
                        None => State::End,
                    };

                    Option::Some(future::ok((stream::iter_ok(all_albums), next_state)))
                }
            }
        });
        let all_albums: Vec<ItemListing> = block_on(fat_stream.flatten().buffered(51).collect())?;

        Result::Ok(all_albums)
    }

    fn album(&self, google_id: &GoogleId) -> Result<Vec<ItemListing>, RemotePhotoLibError> {
        info!("album");
        let fat_stream = stream::unfold(State::Start, move |state| {
            debug!("album: {:?}", state);
            let page_token = match state {
                State::Start => None,
                State::Next(pt) => Some(pt),
                State::End => return None,
            };

            let request = SearchMediaItemsRequest {
                page_token,
                page_size: Option::Some(50),
                filters: Option::None,
                album_id: Option::Some(String::from(google_id)),
            };
            let result_builder = self.photos_library.media_items().search(request);

            match result_builder.doit() {
                Err(e) => {
                    error!("{}", e);
                    Option::Some(future::err(RemotePhotoLibError::from(e)))
                }
                Ok(res) => {
                    debug!("Success: album");
                    let all_media_items = res
                        .1
                        .media_items
                        .unwrap()
                        .into_iter()
                        .map(|media_item| {
                            ItemListing::new(media_item.id.unwrap(), media_item.filename.unwrap())
                        }).map(future::ok);

                    let next_state = match res.1.next_page_token {
                        Some(pt) => State::Next(pt),
                        None => State::End,
                    };

                    Option::Some(future::ok((stream::iter_ok(all_media_items), next_state)))
                }
            }
        });
        let all_media_items: Vec<ItemListing> =
            block_on(fat_stream.flatten().buffered(51).collect())?;

        Result::Ok(all_media_items)
    }
}

impl<C, A> RemotePhotoLibData for HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    fn media_item(
        &self,
        google_id: &GoogleId,
        is_video: bool,
    ) -> Result<Vec<u8>, RemotePhotoLibError> {
        let media_item = self.photos_library.media_items().get(&google_id).doit()?;
        let base_url = media_item.1.base_url.unwrap();
        let download_url = if is_video {
            format!("{}=dv", base_url)
        } else {
            format!("{}=d", base_url)
        };
        info!("Have base_url={} download_url={} )", base_url, download_url);

        let mut http_response = self.data_http_client.get(&download_url).send()?;
        match http_response.status {
            hyper::status::StatusCode::Ok => {
                let mut buffer: Vec<u8> = Vec::new();
                info!("Downloading {:?}", media_item.1.filename);
                let size = http_response.read_to_end(&mut buffer)?;
                info!("Downloaded {:?}, size={}", media_item.1.filename, size);
                Result::Ok(buffer)
            }
            error => Result::Err(RemotePhotoLibError::HttpApiError(error)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn item_listing_google_id() {
        assert_eq!(
            ItemListing::new(String::from("id"), String::from("name")).google_id(),
            "id"
        );
    }
}
