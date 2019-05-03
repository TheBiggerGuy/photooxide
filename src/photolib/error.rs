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
    fn remote_photo_lib_error_from_photoslibrary_error() -> std::result::Result<(), ()> {
        match RemotePhotoLibError::from(photoslibrary1::Error::MissingAPIKey) {
            RemotePhotoLibError::GoogleBackendError(_) => Result::Ok(()),
            _ => Result::Err(()),
        }
    }

    #[test]
    fn remote_photo_lib_error_from_hyper_error() -> std::result::Result<(), ()> {
        match RemotePhotoLibError::from(hyper::Error::Method) {
            RemotePhotoLibError::HttpClientError(_) => Result::Ok(()),
            _ => Result::Err(()),
        }
    }

    #[test]
    fn remote_photo_lib_error_from_io_error() -> std::result::Result<(), ()> {
        let io_error = std::io::Error::new(std::io::ErrorKind::Other, "I/O Error for test");

        match RemotePhotoLibError::from(io_error) {
            RemotePhotoLibError::IoError(_) => Result::Ok(()),
            _ => Result::Err(()),
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
        {
            let io_error = std::io::Error::new(std::io::ErrorKind::Other, "I/O Error for test");
            let io_error_str = io_error.to_string();
            assert_eq!(
                RemotePhotoLibError::IoError(io_error)
                    .source()
                    .unwrap()
                    .to_string(),
                io_error_str
            );
        }
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
        assert_eq!(
            format!(
                "{}",
                RemotePhotoLibError::HttpApiError(hyper::status::StatusCode::Ok)
            ),
            "RemotePhotoLibError: HttpApiError(Ok)"
        );
        assert_eq!(
            format!(
                "{}",
                RemotePhotoLibError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "I/O Error for test"
                ))
            ),
            "RemotePhotoLibError: IoError(Custom { kind: Other, error: StringError(\"I/O Error for test\") })"
        );
    }
}
