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

    let auth;
    {
        // Get an ApplicationSecret instance by some means. It contains the `client_id` and
        // `client_secret`, among other things.
        let secret = json::from_str::<ConsoleApplicationSecret>(CLIENT_SECRET)
            .unwrap()
            .installed
            .unwrap();

        let token_storage = DiskTokenStorage::new(&"token_storage.json".to_string()).unwrap();
        auth = Authenticator::new(
            &secret,
            DefaultAuthenticatorDelegate,
            hyper::Client::with_connector(hyper::net::HttpsConnector::new(
                hyper_rustls::TlsClient::new(),
            )),
            token_storage,
            Option::Some(FlowType::InstalledInteractive),
        );
    }

    let db;
    {
        let sqlite_connection = rusqlite::Connection::open("cache.sqlite").unwrap();
        db = Arc::new(SqliteDb::new(Mutex::new(sqlite_connection)).unwrap());
    }

    let remote_photo_lib;
    {
        let api_http_client = hyper::Client::with_connector(hyper::net::HttpsConnector::new(
            hyper_rustls::TlsClient::new(),
        ));
        let data_http_client = hyper::Client::with_connector(hyper::net::HttpsConnector::new(
            hyper_rustls::TlsClient::new(),
        ));

        let photos_library = PhotosLibrary::new(api_http_client, auth);
        remote_photo_lib = Arc::new(Mutex::new(HttpRemotePhotoLib::new(
            photos_library,
            data_http_client,
        )));
    }

    let fs = RustFilesystemReal::new(PhotoFs::new(remote_photo_lib.clone(), db.clone()));

    let executor;
    let mut scheduled_tasks: Vec<(&str, scheduled_executor::executor::TaskHandle)> = Vec::new();
    if env::var("PHOTOOXIDE_DISABLE_REFRESH").is_err() {
        executor = scheduled_executor::ThreadPoolExecutor::new(2).unwrap();
        let updaters: Vec<Box<BackgroundUpdate>> = vec![
            Box::new(BackgroundAlbumUpdate {
                remote_photo_lib: remote_photo_lib.clone(),
                db: db.clone(),
            }),
            Box::new(BackgroundMediaUpdate {
                remote_photo_lib: remote_photo_lib.clone(),
                db: db.clone(),
            }),
        ];
        for updater in updaters {
            let name = updater.name();
            let delay = updater
                .delay()
                .to_std()
                .expect("Failed to convert to std::time::duration");
            let interval = updater
                .interval()
                .to_std()
                .expect("Failed to convert to std::time::duration");

            let task = executor.schedule_fixed_rate(delay, interval, move |_remote| match updater
                .update()
            {
                Err(msg) => error!("Background update of {} failed: {}", name, msg),
                Ok(_) => debug!("Background update of {} OK!", name),
            });
            scheduled_tasks.push((name, task));
        }
    }

    let mountpoint = env::args_os().nth(1).unwrap();
    let options = ["-o", "ro", "-o", "fsname=photooxide"] // "-o", "default_permissions",
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();

    info!("starting FUSE mount at {:?} with {:?}", mountpoint, options);
    match fuse::mount(fs, &mountpoint, &options) {
        Err(msg) => error!("FUSE mount failed: {}", msg),
        Ok(_) => info!("FUSE mount ended without error"),
    }
    info!("Ended FUSE mount");

    info!("Stopping background tasks...");
    for task in &scheduled_tasks {
        task.1.stop();
    }
    for task in &scheduled_tasks {
        while !task.1.stopped() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        debug!("Task {:?} stopped", task.0);
    }
    info!("...stopped background tasks");
}
