use std::borrow::BorrowMut;
use std::sync::{Arc, Mutex};

use crate::oauth2;

use chrono::Utc;

use crate::db::{PhotoDb, PhotoDbRo, SqliteDb};
use crate::photolib::{HttpRemotePhotoLib, RemotePhotoLibMetaData};

pub trait BackgroundUpdate: Sync + Send {
    fn update(&self) -> Result<(), String>;

    fn delay(&self) -> time::Duration;

    fn interval(&self) -> time::Duration;

    fn name(&self) -> &'static str;
}

pub struct BackgroundAlbumUpdate<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    pub remote_photo_lib: Arc<Mutex<HttpRemotePhotoLib<C, A>>>,
    pub db: Arc<SqliteDb>,
}

unsafe impl<C, A> Sync for BackgroundAlbumUpdate<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
}
unsafe impl<C, A> Send for BackgroundAlbumUpdate<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
}

impl<C, A> BackgroundUpdate for BackgroundAlbumUpdate<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    fn update(&self) -> Result<(), String> {
        warn!("Start background albums refresh");
        let albums;
        {
            let remote_photo_lib_unlocked = self
                .remote_photo_lib
                .lock()
                .map_err(|err| format!("{:?}", err))?;
            albums = remote_photo_lib_unlocked
                .albums()
                .map_err(|err| format!("{:?}", err))?;
        }
        for album in albums {
            match self
                .db
                .upsert_album(&album.google_id(), &album.name, &Utc::now())
            {
                Ok(inode) => debug!("upserted album='{:?}' into inode={:?}", album, inode),
                Err(error) => error!("Failed to upsert album='{:?}' due to {:?}", album, error),
            }
            let media_items_in_album;
            {
                let remote_photo_lib_unlocked = self
                    .remote_photo_lib
                    .lock()
                    .map_err(|err| format!("{:?}", err))?;
                media_items_in_album = remote_photo_lib_unlocked
                    .album(&album.google_id())
                    .map_err(|err| format!("{:?}", err))?;
            }
            media_items_in_album
                .iter()
                .filter(|item| self.db.exists(item.google_id()).unwrap())
                .for_each(|media_item_in_album| {
                    warn!("Found {} in album {}", media_item_in_album.name, album.name);
                    match self.db.upsert_media_item_in_album(
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

    fn delay(&self) -> time::Duration {
        time::Duration::seconds(5)
    }

    fn interval(&self) -> time::Duration {
        time::Duration::hours(12)
    }

    fn name(&self) -> &'static str {
        "Albums"
    }
}

pub struct BackgroundMediaUpdate<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    pub remote_photo_lib: Arc<Mutex<HttpRemotePhotoLib<C, A>>>,
    pub db: Arc<SqliteDb>,
}

unsafe impl<C, A> Sync for BackgroundMediaUpdate<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
}
unsafe impl<C, A> Send for BackgroundMediaUpdate<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
}

impl<C, A> BackgroundUpdate for BackgroundMediaUpdate<C, A>
where
    C: BorrowMut<hyper::Client>,
    A: oauth2::GetToken,
{
    fn update(&self) -> Result<(), String> {
        {
            warn!("Start background media_items refresh");
            let media_items;
            {
                let remote_photo_lib_unlocked = self
                    .remote_photo_lib
                    .lock()
                    .map_err(|err| format!("{:?}", err))?;
                media_items = remote_photo_lib_unlocked
                    .media_items()
                    .map_err(|err| format!("{:?}", err))?;
            }
            for media_item in media_items {
                match self.db.upsert_media_item(
                    &media_item.google_id(),
                    &media_item.name,
                    &Utc::now(),
                ) {
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

    fn delay(&self) -> time::Duration {
        time::Duration::seconds(15)
    }

    fn interval(&self) -> time::Duration {
        time::Duration::days(2)
    }

    fn name(&self) -> &'static str {
        "Media Items"
    }
}
