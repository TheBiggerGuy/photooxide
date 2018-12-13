use users;

use fuse::{FileAttr, FileType};
use time::Timespec;

use crate::domain::Inode;

const CREATE_TIME: Timespec = Timespec {
    sec: 1_381_237_736,
    nsec: 0,
}; // 2013-10-08 08:56

pub fn make_atr(inode: Inode, size: usize, file_type: FileType) -> FileAttr {
    FileAttr {
        ino: inode,
        size: size as u64,
        blocks: 1,
        atime: CREATE_TIME,
        mtime: CREATE_TIME,
        ctime: CREATE_TIME,
        crtime: CREATE_TIME,
        kind: file_type,
        perm: 0o644,
        nlink: 1,
        uid: users::get_current_uid(),
        gid: 20,
        rdev: 0,
        flags: 0,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn make_atr_test() {
        // Inode
        assert_eq!(make_atr(100, 0, FileType::RegularFile).ino, 100);

        // Size
        assert_eq!(make_atr(100, 1, FileType::RegularFile).size, 1);

        // FileType
        assert_eq!(
            make_atr(100, 1, FileType::RegularFile).kind,
            FileType::RegularFile
        );
        assert_eq!(
            make_atr(100, 1, FileType::Directory).kind,
            FileType::Directory
        );
    }
}
