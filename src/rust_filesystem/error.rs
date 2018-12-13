use std::fmt;
use std::result;

use libc;

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub enum FuseError {
    FunctionNotImplemented,
}

impl FuseError {
    pub fn libc_error_code(self) -> i32 {
        match self {
            _ => libc::ENOENT,
        }
    }
}

impl std::error::Error for FuseError {}

impl fmt::Display for FuseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FuseError::FunctionNotImplemented => write!(f, "FuseError: FunctionNotImplemented"),
        }
    }
}

pub type FuseResult<T> = result::Result<T, FuseError>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fuse_error_libc_error_code() {
        assert_eq!(FuseError::FunctionNotImplemented.libc_error_code(), 2);
    }

    #[test]
    fn fuse_error_display() {
        assert_eq!(format!("{}", FuseError::FunctionNotImplemented), "FuseError: FunctionNotImplemented");
    }
}
