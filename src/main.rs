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

extern crate sqlite;

extern crate chrono;

use std::env;
use std::ffi::OsStr;
use std::option::Option;

use oauth2::{
    Authenticator, ConsoleApplicationSecret, DefaultAuthenticatorDelegate, DiskTokenStorage,
    FlowType,
};
use photoslibrary1::PhotosLibrary;
use serde_json as json;

mod photolib;
use photolib::*;

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

    let db = sqlite::open("cache.sqlite").unwrap();

    let db_backed_photo_lib = DbBackedPhotoLib::new(photos_library, db).unwrap();

    let fs = PhotoFs::new(db_backed_photo_lib);

    let mountpoint = env::args_os().nth(1).unwrap();
    let options = ["-o", "ro", "-o", "fsname=photooxide"]
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();

    fuse::mount(fs, &mountpoint, &options).unwrap();
}
