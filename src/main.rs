#[macro_use]
extern crate log;
#[macro_use]
extern crate derive_new;

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
use std::sync::{Arc, Mutex};

use oauth2::{
    Authenticator, ConsoleApplicationSecret, DefaultAuthenticatorDelegate, DiskTokenStorage,
    FlowType,
};
use photoslibrary1::PhotosLibrary;
use serde_json as json;

mod background_update;
mod domain;
use background_update::{BackgroundAlbumUpdate, BackgroundMediaUpdate, BackgroundUpdate};

mod db;
use db::SqliteDb;

mod photolib;
use photolib::HttpRemotePhotoLib;

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
    let db = Arc::new(SqliteDb::new(Mutex::new(sqlite_connection)).unwrap());

    let remote_photo_lib = Arc::new(Mutex::new(HttpRemotePhotoLib::new(
        photos_library,
        data_http_client,
    )));

    let fs = RustFilesystemReal::new(PhotoFs::new(remote_photo_lib.clone(), db.clone()));

    let executor;
    if env::var("PHOTOOXIDE_DISABLE_REFRESH").is_err() {
        executor = scheduled_executor::ThreadPoolExecutor::new(1).unwrap();
        {
            let remote_photo_lib = remote_photo_lib.clone();
            let db = db.clone();

            executor.schedule_fixed_rate(
                time::Duration::seconds(5).to_std().unwrap(),
                time::Duration::hours(12).to_std().unwrap(),
                move |_remote| match BackgroundAlbumUpdate::update(&remote_photo_lib, &db) {
                    Err(msg) => error!("Background update of albums failed: {}", msg),
                    Ok(_) => debug!("Background update of albums OK!"),
                },
            );
        }
        {
            let remote_photo_lib = remote_photo_lib.clone();
            let db = db.clone();

            executor.schedule_fixed_rate(
                time::Duration::seconds(30).to_std().unwrap(),
                time::Duration::days(5).to_std().unwrap(),
                move |_remote| match BackgroundMediaUpdate::update(&remote_photo_lib, &db) {
                    Err(msg) => error!("Background update of media failed: {}", msg),
                    Ok(_) => debug!("Background update of media OK!"),
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
