use std::borrow::BorrowMut;
use std::convert::From;
use std::io::Read;
use std::option::Option;
use std::result::Result;

use crate::oauth2;
use crate::photoslibrary1::{
    self, ListMediaItemsResponse, PhotosLibrary, SearchMediaItemsRequest, SearchMediaItemsResponse,
};
use hyper;

use crate::domain::*;

mod error;
pub use self::error::RemotePhotoLibError;

mod oauth_token_storage;
pub use self::oauth_token_storage::{OauthTokenStorage, OauthTokenStorageError};

#[derive(Debug, Clone, Copy)]
pub enum MediaListFilter<'a> {
    Album(&'a GoogleId),
    Year(i32),
}

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

pub trait RemotePhotoLibMetaData: Sized {
    fn media_items(
        &self,
        filter: Option<MediaListFilter>,
    ) -> Result<Vec<ItemListing>, RemotePhotoLibError>;

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

#[derive(Debug)]
struct SearchListResponse {
    media_items: Vec<photoslibrary1::MediaItem>,
    next_page_token: Option<String>,
}

fn unwrap_list_response(
    response: photoslibrary1::Result<(hyper::client::response::Response, ListMediaItemsResponse)>,
) -> Result<SearchListResponse, RemotePhotoLibError> {
    match response {
        Err(e) => {
            error!("{}", e);
            Result::Err(RemotePhotoLibError::from(e))
        }
        Ok(res) => {
            debug!("Success: listing photos");
            match res.1.media_items {
                Some(media_items) => Result::Ok(SearchListResponse {
                    media_items,
                    next_page_token: res.1.next_page_token,
                }),
                None => {
                    warn!("Media items list responded successfully but with an empty list of items. HTTP status code: {}", res.0.status);
                    Result::Ok(SearchListResponse {
                        media_items: Vec::new(),
                        next_page_token: res.1.next_page_token,
                    })
                }
            }
        }
    }
}

fn unwrap_search_response(
    response: photoslibrary1::Result<(hyper::client::response::Response, SearchMediaItemsResponse)>,
) -> Result<SearchListResponse, RemotePhotoLibError> {
    match response {
        Err(e) => {
            error!("{}", e);
            Result::Err(RemotePhotoLibError::from(e))
        }
        Ok(res) => {
            debug!("Success: listing photos");
            match res.1.media_items {
                Some(media_items) => Result::Ok(SearchListResponse {
                    media_items,
                    next_page_token: res.1.next_page_token,
                }),
                None => {
                    warn!("Media items list responded successfully but with an empty list of items. HTTP status code: {}", res.0.status);
                    Result::Ok(SearchListResponse {
                        media_items: Vec::new(),
                        next_page_token: res.1.next_page_token,
                    })
                }
            }
        }
    }
}

impl<C, A> RemotePhotoLibMetaData for HttpRemotePhotoLib<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    fn media_items(
        &self,
        filter: Option<MediaListFilter>,
    ) -> Result<Vec<ItemListing>, RemotePhotoLibError> {
        let mut all_media_items: Vec<ItemListing> = Vec::new();
        let mut page_token: Option<String> = Option::None;
        loop {
            let remote_result = match filter {
                Some(filter) => {
                    let mut album_id_filter = Option::None;
                    let mut other_filter = Option::None;
                    match filter {
                        MediaListFilter::Album(google_id) => {
                            album_id_filter = Option::Some(String::from(google_id));
                        }
                        MediaListFilter::Year(year) => {
                            let date = photoslibrary1::Date {
                                year: Option::Some(year),
                                day: Option::None,
                                month: Option::None,
                            };
                            let date_filter = photoslibrary1::DateFilter {
                                ranges: Option::None,
                                dates: Option::Some(vec![date]),
                            };
                            let full_filter = photoslibrary1::Filters {
                                date_filter: Option::Some(date_filter),
                                content_filter: Option::None,
                                include_archived_media: Option::None,
                                exclude_non_app_created_data: Option::None,
                                media_type_filter: Option::None,
                            };
                            other_filter = Option::Some(full_filter);
                        }
                    }
                    let request = SearchMediaItemsRequest {
                        page_token,
                        page_size: Option::Some(50),
                        filters: other_filter,
                        album_id: album_id_filter,
                    };
                    let remote_result = self.photos_library.media_items().search(request).doit();

                    unwrap_search_response(remote_result)
                }
                None => {
                    let mut result_builder = self.photos_library.media_items().list().page_size(50);
                    if page_token.is_some() {
                        result_builder = result_builder.page_token(page_token.unwrap().as_str());
                    }
                    let remote_result = result_builder.doit();

                    unwrap_list_response(remote_result)
                }
            }?;

            for media_item in remote_result.media_items {
                all_media_items.push(ItemListing::new(
                    media_item.id.unwrap(),
                    media_item.filename.unwrap(),
                ))
            }

            page_token = remote_result.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        Result::Ok(all_media_items)
    }

    fn albums(&self) -> Result<Vec<ItemListing>, RemotePhotoLibError> {
        let mut all_albums: Vec<ItemListing> = Vec::new();
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
                        let album_listing =
                            ItemListing::new(album.id.unwrap(), album.title.unwrap());
                        all_albums.push(album_listing);
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

    fn album(&self, google_id: &GoogleId) -> Result<Vec<ItemListing>, RemotePhotoLibError> {
        self.media_items(Option::Some(MediaListFilter::Album(google_id)))
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
