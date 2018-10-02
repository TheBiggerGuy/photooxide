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
use db::{PhotoDb, SqliteDb};

mod photolib;
use photolib::{HttpRemotePhotoLib, RemotePhotoLib};

mod photofs;
use photofs::*;

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

    let http_client = hyper::Client::with_connector(hyper::net::HttpsConnector::new(
        hyper_rustls::TlsClient::new(),
    ));

    let photos_library = PhotosLibrary::new(http_client, auth);

    let sqlite_connection = rusqlite::Connection::open("cache.sqlite").unwrap();
    let db = Arc::new(SqliteDb::new(RwLock::new(sqlite_connection)).unwrap());

    let remote_photo_lib = Arc::new(Mutex::new(HttpRemotePhotoLib::new(photos_library)));

    let fs = PhotoFs::new(remote_photo_lib.clone(), db.clone());

    let executor = scheduled_executor::ThreadPoolExecutor::new(1).unwrap();
    {
        let remote_photo_lib = remote_photo_lib.clone();
        let db = db.clone();
        executor.schedule_fixed_rate(
            time::Duration::seconds(5).to_std().unwrap(),
            time::Duration::hours(12).to_std().unwrap(),
            move |_remote| {
                warn!("Start background albums refresh");
                let remote_photo_lib = remote_photo_lib.lock().unwrap();
                for album in remote_photo_lib.albums().unwrap() {
                    match db.upsert_album(&album.0, &album.1, &Utc::now()) {
                        Ok(inode) => {
                            debug!("upserted album='{:?}' into inode={:?}", album.1, inode)
                        }
                        Err(error) => {
                            error!("Failed to upsert album {:?} due to {:?}", album.1, error)
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
        executor.schedule_fixed_rate(
            time::Duration::hours(1).to_std().unwrap(),
            time::Duration::days(5).to_std().unwrap(),
            move |_remote| {
                warn!("Start background media_items refresh");
                let remote_photo_lib = remote_photo_lib.lock().unwrap();
                for media_item in remote_photo_lib.media_items().unwrap() {
                    match db.upsert_media_item(&media_item.0, &media_item.1, &Utc::now()) {
                        Ok(inode) => {
                            debug!("upserted media_item='{:?}' into inode={:?}", media_item.1, inode)
                        }
                        Err(error) => {
                            error!("Failed to upsert media_item {:?} due to {:?}", media_item.1, error)
                        }
                    }
                }
                warn!("End background media_items refresh");
            },
        );
    }

    let mountpoint = env::args_os().nth(1).unwrap();
    let options = ["-o", "ro", "-o", "fsname=photooxide"]
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();

    fuse::mount(fs, &mountpoint, &options).unwrap();
}
