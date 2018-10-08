#[macro_use]
extern crate log;

extern crate env_logger;

extern crate fuse;
extern crate libc;
extern crate time;

extern crate google_photoslibrary1 as photoslibrary1;
extern crate hyper;
extern crate hyper_rustls;
extern crate serde;
extern crate serde_json;
extern crate yup_oauth2 as oauth2;

extern crate rusqlite;

extern crate chrono;

extern crate users;

extern crate scheduled_executor;

use std::env;
use std::ffi::OsStr;
use std::option::Option;
use std::sync::{Arc, Mutex, RwLock};

use oauth2::{
    Authenticator, ConsoleApplicationSecret, DefaultAuthenticatorDelegate, DiskTokenStorage,
    FlowType,
};
use photoslibrary1::PhotosLibrary;
use serde_json as json;

use chrono::Utc;

mod domain;

mod db;
use db::{PhotoDb, PhotoDbRo, SqliteDb};

mod photolib;
use photolib::{HttpRemotePhotoLib, RemotePhotoLibMetaData};

mod photofs;
use photofs::*;

mod rust_filesystem;
use rust_filesystem::RustFilesystemReal;

const CLIENT_SECRET: &str = include_str!("../client_secret.json");

fn main() {
    env_logger::init();
    println!("Hello, world!");
    debug!("Hello, world!");

    // Get an ApplicationSecret instance by some means. It contains the `client_id` and
    // `client_secret`, among other things.
    let secret = json::from_str::<ConsoleApplicationSecret>(CLIENT_SECRET)
        .unwrap()
        .installed
        .unwrap();

    let token_storage = DiskTokenStorage::new(&"token_storage.json".to_string()).unwrap();
    let auth = Authenticator::new(
        &secret,
        DefaultAuthenticatorDelegate,
        hyper::Client::with_connector(hyper::net::HttpsConnector::new(
            hyper_rustls::TlsClient::new(),
        )),
        token_storage,
        Option::Some(FlowType::InstalledInteractive),
    );

    let api_http_client = hyper::Client::with_connector(hyper::net::HttpsConnector::new(
        hyper_rustls::TlsClient::new(),
    ));
    let data_http_client = hyper::Client::with_connector(hyper::net::HttpsConnector::new(
        hyper_rustls::TlsClient::new(),
    ));

    let photos_library = PhotosLibrary::new(api_http_client, auth);

    let sqlite_connection = rusqlite::Connection::open("cache.sqlite").unwrap();
    let db = Arc::new(SqliteDb::new(RwLock::new(sqlite_connection)).unwrap());

    let remote_photo_lib = Arc::new(Mutex::new(HttpRemotePhotoLib::new(
        photos_library,
        data_http_client,
    )));

    let fs = RustFilesystemReal::new(PhotoFs::new(remote_photo_lib.clone(), db.clone()));

    if option_env!("PHOTOOXIDE_DISABLE_REFRESH").is_none() {
        let executor = scheduled_executor::ThreadPoolExecutor::new(1).unwrap();
        {
            let remote_photo_lib = remote_photo_lib.clone();
            let db = db.clone();

            let album_update_delay = match db.last_updated_album().unwrap() {
                Some(_) => time::Duration::seconds(60),
                None => time::Duration::seconds(5),
            };
            info!("album_update_delay: {}", album_update_delay);

            executor.schedule_fixed_rate(
                album_update_delay.to_std().unwrap(),
                time::Duration::hours(12).to_std().unwrap(),
                move |_remote| {
                    warn!("Start background albums refresh");
                    let remote_photo_lib = remote_photo_lib.lock().unwrap();
                    for album in remote_photo_lib.albums().unwrap() {
                        match db.upsert_album(&album.google_id(), &album.name, &Utc::now()) {
                            Ok(inode) => {
                                debug!("upserted album='{:?}' into inode={:?}", album, inode)
                            }
                            Err(error) => {
                                error!("Failed to upsert album='{:?}' due to {:?}", album, error)
                            }
                        }
                        for media_item_in_album in
                            remote_photo_lib.album(&album.google_id()).unwrap()
                        {
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
                        }
                    }
                    warn!("End background albums refresh");
                },
            );
        }
        {
            let remote_photo_lib = remote_photo_lib.clone();
            let db = db.clone();

            let media_update_delay = match db.last_updated_media().unwrap() {
                Some(_) => time::Duration::minutes(5),
                None => time::Duration::seconds(10),
            };
            info!("media_update_delay: {}", media_update_delay);

            executor.schedule_fixed_rate(
                media_update_delay.to_std().unwrap(),
                time::Duration::days(5).to_std().unwrap(),
                move |_remote| {
                    warn!("Start background media_items refresh");
                    let remote_photo_lib = remote_photo_lib.lock().unwrap();
                    for media_item in remote_photo_lib.media_items().unwrap() {
                        match db.upsert_media_item(
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
                },
            );
        }
    }

    let mountpoint = env::args_os().nth(1).unwrap();
    let options = ["-o", "ro", "-o", "fsname=photooxide"] // "-o", "default_permissions",
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();

    fuse::mount(fs, &mountpoint, &options).unwrap();
}
