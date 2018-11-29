use std::borrow::BorrowMut;
use std::sync::{Arc, Mutex};

use oauth2;

use chrono::Utc;

use db::{PhotoDb, PhotoDbRo, SqliteDb};
use photolib::{HttpRemotePhotoLib, RemotePhotoLibMetaData};

pub trait BackgroundUpdate {
    fn update<C, A>(
        remote_photo_lib: &Arc<Mutex<HttpRemotePhotoLib<C, A>>>,
        db: &Arc<SqliteDb>,
    ) -> Result<(), String>
    where
        C: BorrowMut<hyper::Client>,
        A: oauth2::GetToken;
}

#[derive(Debug)]
pub struct BackgroundAlbumUpdate {}

impl BackgroundUpdate for BackgroundAlbumUpdate {
    fn update<C, A>(
        remote_photo_lib: &Arc<Mutex<HttpRemotePhotoLib<C, A>>>,
        db: &Arc<SqliteDb>,
    ) -> Result<(), String>
    where
        C: BorrowMut<hyper::Client>,
        A: oauth2::GetToken,
    {
        warn!("Start background albums refresh");
        let albums;
        {
            let remote_photo_lib_unlocked = remote_photo_lib
                .lock()
                .map_err(|err| format!("{:?}", err))?;
            albums = remote_photo_lib_unlocked
                .albums()
                .map_err(|err| format!("{:?}", err))?;
        }
        for album in albums {
            match db.upsert_album(&album.google_id(), &album.name, &Utc::now()) {
                Ok(inode) => debug!("upserted album='{:?}' into inode={:?}", album, inode),
                Err(error) => error!("Failed to upsert album='{:?}' due to {:?}", album, error),
            }
            let media_items_in_album;
            {
                let remote_photo_lib_unlocked = remote_photo_lib
                    .lock()
                    .map_err(|err| format!("{:?}", err))?;
                media_items_in_album = remote_photo_lib_unlocked
                    .album(&album.google_id())
                    .map_err(|err| format!("{:?}", err))?;
            }
            media_items_in_album
                .iter()
                .filter(|item| db.exists(item.google_id()).unwrap())
                .for_each(|media_item_in_album| {
                    warn!("Found {} in album {}", media_item_in_album.name, album.name);
                    match db.upsert_media_item_in_album(
                        album.google_id(),
                        media_item_in_album.google_id(),
                    ) {
                        Ok(()) => debug!(
                            "upsert media_item='{:?}' into album='{:?}'",
                            media_item_in_album, album
                        ),
                        Err(error) => error!(
                            "Failed to upsert media_item='{:?}' into album='{:?}' due to {:?}",
                            media_item_in_album, album, error
                        ),
                    }
                });
        }
        warn!("End background albums refresh");

        Result::Ok(())
    }
}

#[derive(Debug)]
pub struct BackgroundMediaUpdate {}

impl BackgroundUpdate for BackgroundMediaUpdate {
    fn update<C, A>(
        remote_photo_lib: &Arc<Mutex<HttpRemotePhotoLib<C, A>>>,
        db: &Arc<SqliteDb>,
    ) -> Result<(), String>
    where
        C: BorrowMut<hyper::Client>,
        A: oauth2::GetToken,
    {
        {
            warn!("Start background media_items refresh");
            let media_items;
            {
                let remote_photo_lib_unlocked = remote_photo_lib
                    .lock()
                    .map_err(|err| format!("{:?}", err))?;
                media_items = remote_photo_lib_unlocked
                    .media_items()
                    .map_err(|err| format!("{:?}", err))?;
            }
            for media_item in media_items {
                match db.upsert_media_item(&media_item.google_id(), &media_item.name, &Utc::now()) {
                    Ok(inode) => debug!(
                        "upserted media_item='{:?}' into inode={:?}",
                        media_item, inode
                    ),
                    Err(error) => error!(
                        "Failed to upsert media_item='{:?}' due to {:?}",
                        media_item, error
                    ),
                }
            }
            warn!("End background media_items refresh");
        }

        Result::Ok(())
    }
}
