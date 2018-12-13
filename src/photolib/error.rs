use std;
use std::convert::From;
use std::fmt;

use crate::photoslibrary1;
use hyper;

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

impl std::error::Error for RemotePhotoLibError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RemotePhotoLibError::GoogleBackendError(err) => Option::Some(err),
            RemotePhotoLibError::HttpClientError(err) => Option::Some(err),
            RemotePhotoLibError::HttpApiError(_err) => Option::None,
            RemotePhotoLibError::IoError(err) => Option::Some(err),
        }
    }
}

impl fmt::Display for RemotePhotoLibError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemotePhotoLibError::GoogleBackendError(err) => {
                write!(f, "RemotePhotoLibError: GoogleBackendError({:?})", err)
            }
            RemotePhotoLibError::HttpClientError(err) => {
                write!(f, "RemotePhotoLibError: HttpClientError({:?})", err)
            }
            RemotePhotoLibError::HttpApiError(err) => {
                write!(f, "RemotePhotoLibError: HttpApiError({:?})", err)
            }
            RemotePhotoLibError::IoError(err) => {
                write!(f, "RemotePhotoLibError: IoError({:?})", err)
            }
        }
    }
}
