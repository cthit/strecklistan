#![allow(non_local_definitions)]
#[macro_use]
extern crate diesel;

#[macro_use]
extern crate log;

mod database;
pub mod models;
pub mod routes;
mod schema;
pub mod util;

use crate::routes::rest;
use crate::routes::rest::izettle::IZettleNotifier;
use crate::util::{FileResponder, catchers};

use clap::Parser;
use dotenv::dotenv;
use rocket::routes;
use rocket_dyn_templates::Template;

#[derive(Default, Parser, Clone, Debug)]
pub struct Opt {
    /// Database url specified as a postgres:// uri
    #[clap(long, short, env = "DATABASE_URL")]
    database: String,

    /// Run database migrations on startup
    #[clap(long, short = 'm', env = "RUN_MIGRATIONS")]
    run_migrations: bool,

    /// Enable HTTP cache control of statically served files
    #[clap(long, env = "ENABLE_STATIC_FILE_CACHE")]
    static_file_cache: bool,

    /// Time until a cached static file must be invalidated
    #[clap(long, env = "STATIC_FILES_MAX_AGE", default_value_t)]
    max_age: usize,

    /// Book account ID for CSV import asset account (e.g., Bankkonto)
    #[clap(long, env = "CSV_IMPORT_ASSET_ACCOUNT")]
    pub csv_import_asset_account: Option<i32>,

    /// Book account ID for CSV import expense account (e.g., Inköp)
    #[clap(long, env = "CSV_IMPORT_EXPENSE_ACCOUNT")]
    pub csv_import_expense_account: Option<i32>,

    /// Transaction description for CSV imports
    #[clap(long, env = "CSV_IMPORT_TRANSACTION_DESCRIPTION", default_value = "CSV Bulkhantering")]
    pub csv_import_transaction_description: String,

    /// Transaction description for CSV imports when stock decreases (optional, falls back to CSV_IMPORT_TRANSACTION_DESCRIPTION)
    #[clap(long, env = "CSV_IMPORT_TRANSACTION_DESCRIPTION_DECREASE")]
    pub csv_import_transaction_description_decrease: Option<String>,
}

#[rocket::main]
async fn main() {
    dotenv().ok();

    let opt = Opt::parse();

    let db_pool = database::create_pool(&opt).expect("Could not create database pool");

    if opt.run_migrations {
        database::run_migrations(&db_pool);
    }

    // Extract values we need before moving opt
    let static_file_cache = opt.static_file_cache;
    let max_age = opt.max_age;

    let rocket = rocket::build()
        .manage(db_pool)
        .manage(opt)
        .manage(IZettleNotifier::default())
        .register("/", catchers())
        .attach(FileResponder {
            folder: "www",
            enable_cache: static_file_cache,
            max_age,
        })
        .attach(Template::fairing())
        .mount(
            "/api/",
            routes![
                rest::event::get_event,
                rest::event::get_event_range,
                rest::inventory::get_items,
                rest::inventory::post_item,
                rest::inventory::put_item,
                rest::inventory::delete_item,
                rest::inventory::get_tags,
                rest::inventory::get_bundles,
                rest::inventory::put_bundle,
                rest::inventory::post_bundle,
                rest::inventory::delete_inventory_bundle,
                rest::inventory::generate_csv,
                rest::inventory::update_inventory_from_csv,
                rest::transaction::get_transactions,
                rest::transaction::post_transaction,
                rest::transaction::delete_transaction,
                rest::book_account::get_accounts,
                rest::book_account::get_master_accounts,
                rest::book_account::add_account,
                rest::member::get_members,
                rest::member::add_member_with_book_account,
                rest::receipt::get_receipt,
                rest::get_api_version,
                rest::izettle::izettle_bridge_poll::poll_for_transaction,
                rest::izettle::izettle_bridge_result::complete_izettle_transaction,
                rest::izettle::izettle_transaction::begin_izettle_transaction,
                rest::izettle::izettle_transaction_poll::poll_for_izettle,
            ],
        );

    rocket.launch().await.unwrap();
}
