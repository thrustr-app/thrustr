use anyhow::{Result, anyhow};
use diesel::{
    SqliteConnection,
    connection::SimpleConnection,
    r2d2::{ConnectionManager, CustomizeConnection, Error as R2d2Error, Pool},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::{path::Path, time::Duration};

mod models;
mod schema;
mod storage;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct ConnectionOptions {
    busy_timeout: Duration,
}

impl CustomizeConnection<SqliteConnection, R2d2Error> for ConnectionOptions {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), R2d2Error> {
        conn.batch_execute(&format!(
            "PRAGMA journal_mode = WAL;\n\
             PRAGMA synchronous = NORMAL;\n\
             PRAGMA foreign_keys = ON;\n\
             PRAGMA busy_timeout = {};",
            self.busy_timeout.as_millis()
        ))
        .map_err(R2d2Error::QueryError)
    }
}

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
        let pool = Pool::builder()
            .max_size(5)
            .connection_customizer(Box::new(ConnectionOptions {
                busy_timeout: BUSY_TIMEOUT,
            }))
            .build(manager)?;

        let mut connection = pool.get()?;
        connection
            .run_pending_migrations(MIGRATIONS)
            .expect("Failed to run migrations");

        Ok(Self { pool })
    }
}
