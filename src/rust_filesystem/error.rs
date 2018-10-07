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

pub type FuseResult<T> = result::Result<T, FuseError>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fuse_error_libc_error_code() {
        assert_eq!(FuseError::FunctionNotImplemented.libc_error_code(), 2);
    }
}
