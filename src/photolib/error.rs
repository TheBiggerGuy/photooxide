use std;
use std::convert::From;
use std::error::Error as StdError;
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

impl StdError for RemotePhotoLibError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn remote_photo_lib_error_from_photoslibrary_error() {
        match RemotePhotoLibError::from(photoslibrary1::Error::MissingAPIKey) {
            RemotePhotoLibError::GoogleBackendError(_) => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn remote_photo_lib_error_from_hyper_error() {
        match RemotePhotoLibError::from(hyper::Error::Method) {
            RemotePhotoLibError::HttpClientError(_) => assert!(true),
            _ => assert!(false),
        }
    }

    #[test]
    fn remote_photo_lib_error_source() {
        assert_eq!(
            RemotePhotoLibError::GoogleBackendError(photoslibrary1::Error::MissingAPIKey)
                .source()
                .unwrap()
                .to_string(),
            photoslibrary1::Error::MissingAPIKey.to_string()
        );
        assert_eq!(
            RemotePhotoLibError::HttpClientError(hyper::Error::Method)
                .source()
                .unwrap()
                .to_string(),
            hyper::Error::Method.to_string()
        );
        assert!(
            RemotePhotoLibError::HttpApiError(hyper::status::StatusCode::Ok)
                .source()
                .is_none()
        );
    }

    #[test]
    fn remote_photo_lib_error_display() {
        assert_eq!(
            format!(
                "{}",
                RemotePhotoLibError::GoogleBackendError(photoslibrary1::Error::MissingAPIKey)
            ),
            "RemotePhotoLibError: GoogleBackendError(MissingAPIKey)"
        );
        assert_eq!(
            format!(
                "{}",
                RemotePhotoLibError::HttpClientError(hyper::Error::Method)
            ),
            "RemotePhotoLibError: HttpClientError(Method)"
        );
    }
}
