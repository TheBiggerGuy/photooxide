use std::fmt;

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub enum TableName {
    AlbumsAndMediaItems,
    NextInode,
    MediaItemsInAlbum,
}

impl fmt::Display for TableName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TableName::AlbumsAndMediaItems => write!(f, "albums_and_media_item"),
            TableName::NextInode => write!(f, "next_inode"),
            TableName::MediaItemsInAlbum => write!(f, "media_items_in_album"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn table_name_string() {
        assert_eq!(
            format!("{}", TableName::AlbumsAndMediaItems),
            "albums_and_media_item"
        );
        assert_eq!(
            format!("{:?}", TableName::AlbumsAndMediaItems),
            "AlbumsAndMediaItems"
        );
        assert_eq!(format!("{}", TableName::NextInode), "next_inode");
        assert_eq!(format!("{:?}", TableName::NextInode), "NextInode");
        assert_eq!(
            format!("{}", TableName::MediaItemsInAlbum),
            "media_items_in_album"
        );
        assert_eq!(
            format!("{:?}", TableName::MediaItemsInAlbum),
            "MediaItemsInAlbum"
        );
    }
}
