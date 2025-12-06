pub mod event;
pub mod transaction;

use crate::Opt;
use diesel::r2d2::ConnectionManager;
use diesel::{migration::MigrationConnection, pg::PgConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use r2d2::{Pool, PooledConnection};
use std::error::Error;

pub type DatabasePool = Pool<ConnectionManager<PgConnection>>;
pub type DatabaseConn = PooledConnection<ConnectionManager<PgConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");

pub fn create_pool(opt: &Opt) -> Result<DatabasePool, Box<dyn Error>> {
    let db_manager: ConnectionManager<PgConnection> = ConnectionManager::new(&opt.database);
    let db_pool: Pool<ConnectionManager<PgConnection>> =
        Pool::builder().max_size(15).build(db_manager)?;
    Ok(db_pool)
}

pub fn run_migrations(db_pool: &DatabasePool) {
    let mut connection = db_pool.get().expect("Could not connect to database");

    connection.setup().expect("Could not set up database");

    let migrations = connection
        .run_pending_migrations(MIGRATIONS)
        .expect("Failed to run database migrations");

    for migration in migrations {
        println!("ran migration: {migration:?}");
    }
}
