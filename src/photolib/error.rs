use std;
use std::convert::From;

use hyper;
use crate::photoslibrary1;

#[derive(Debug)]
pub enum RemotePhotoLibError {
    GoogleBackendError(photoslibrary1::Error),
    HttpClientError(hyper::error::Error),
    HttpApiError(hyper::status::StatusCode),
    IoError(std::io::Error),
}

impl From<std::io::Error> for RemotePhotoLibError {
    fn from(error: std::io::Error) -> RemotePhotoLibError {
        RemotePhotoLibError::IoError(error)
    }
}

impl From<hyper::error::Error> for RemotePhotoLibError {
    fn from(error: hyper::error::Error) -> RemotePhotoLibError {
        RemotePhotoLibError::HttpClientError(error)
    }
}

impl From<photoslibrary1::Error> for RemotePhotoLibError {
    fn from(error: photoslibrary1::Error) -> RemotePhotoLibError {
        RemotePhotoLibError::GoogleBackendError(error)
    }
}
