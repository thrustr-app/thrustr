use anyhow::{Result, anyhow};
use diesel::{
    SqliteConnection,
    r2d2::{ConnectionManager, Pool},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::path::Path;

mod models;
mod repositories;
mod schema;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub struct SqliteStorage {
    pool: Pool<ConnectionManager<SqliteConnection>>,
}

impl SqliteStorage {
    pub fn new(sqlite_file_path: impl AsRef<Path>) -> Result<Self> {
        let url = format!(
            "sqlite://{}?mode=rwc",
            sqlite_file_path
                .as_ref()
                .to_str()
                .ok_or_else(|| anyhow!("non UTF-8 database path"))?
        );

        let manager = ConnectionManager::<SqliteConnection>::new(url);
        let pool = Pool::builder().max_size(5).build(manager)?;

        let mut connection = pool.get()?;
        connection
            .run_pending_migrations(MIGRATIONS)
            .expect("Failed to run migrations");

        Ok(Self { pool })
    }
}
