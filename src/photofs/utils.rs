use std::collections::HashMap;

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

// TODO: Global fh
#[derive(Debug)]
pub struct OpenFileHandles<X> {
    fhs: HashMap<u64, X>,
}

impl<X> OpenFileHandles<X> {
    pub fn new() -> OpenFileHandles<X> {
        OpenFileHandles {
            fhs: HashMap::new(),
        }
    }

    pub fn open(&mut self, x: X) -> u64 {
        let mut fh = 0;
        loop {
            if self.fhs.contains_key(&fh) {
                fh += 1;
            } else {
                break;
            }
        }
        self.fhs.insert(fh, x);

        fh
    }

    pub fn get(&self, fh: u64) -> Option<&X> {
        self.fhs.get(&fh)
    }

    pub fn remove(&mut self, fh: u64) -> Option<X> {
        self.fhs.remove(&fh)
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

    #[test]
    fn open_file_handles_test() {
        let mut ofs: OpenFileHandles<u8> = OpenFileHandles::new();

        assert!(ofs.get(0).is_none());
        assert!(ofs.remove(0).is_none());

        assert_eq!(ofs.open(0), 0);
        assert_eq!(ofs.get(0).unwrap(), &0);

        assert_eq!(ofs.open(1), 1);
        assert_eq!(ofs.get(1).unwrap(), &1);

        assert_eq!(ofs.remove(0).unwrap(), 0);
        assert!(ofs.get(0).is_none());
        assert!(ofs.get(1).is_some());
        assert_eq!(ofs.open(0), 0);

        assert_eq!(ofs.remove(1).unwrap(), 1);
        assert!(ofs.get(0).is_some());
        assert!(ofs.get(1).is_none());
        assert_eq!(ofs.open(1), 1);

        assert_eq!(ofs.open(2), 2);
    }
}
