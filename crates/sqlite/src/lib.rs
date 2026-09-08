use anyhow::{Result, anyhow};
use diesel::{
    SqliteConnection,
    connection::SimpleConnection,
    r2d2::{ConnectionManager, CustomizeConnection, Error as R2d2Error, Pool, PooledConnection},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::{ffi::c_char, fs, os::raw::c_int, path::Path, sync::Once, time::Duration};

mod id;
mod models;
mod schema;
mod storage;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

unsafe extern "C" {
    fn sqlite3_spellfix_init(
        db: *mut libsqlite3_sys::sqlite3,
        pz_err_msg: *mut *mut c_char,
        api: *const libsqlite3_sys::sqlite3_api_routines,
    ) -> c_int;
}

fn register_spellfix() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let rc = unsafe { libsqlite3_sys::sqlite3_auto_extension(Some(sqlite3_spellfix_init)) };
        assert_eq!(rc, 0, "failed to register the spellfix1 auto-extension");
    });
}

#[derive(Debug)]
struct ConnectionOptions {
    busy_timeout: Duration,
}

impl CustomizeConnection<SqliteConnection, R2d2Error> for ConnectionOptions {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), R2d2Error> {
        conn.batch_execute(&format!(
            "PRAGMA busy_timeout = {};\n\
             PRAGMA synchronous = NORMAL;\n\
             PRAGMA foreign_keys = ON;",
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
        register_spellfix();

        let path = sqlite_file_path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let path = path
            .to_str()
            .ok_or_else(|| anyhow!("non UTF-8 database path"))?;

        let manager = ConnectionManager::<SqliteConnection>::new(path);
        let pool = Pool::builder()
            .max_size(5)
            .connection_customizer(Box::new(ConnectionOptions {
                busy_timeout: BUSY_TIMEOUT,
            }))
            .build(manager)?;

        let mut connection = pool.get()?;

        connection
            .batch_execute("PRAGMA journal_mode = WAL;")
            .map_err(|e| anyhow!("failed to enable WAL: {e}"))?;

        connection
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("failed to run migrations: {e}"))?;

        Ok(Self { pool })
    }

    pub(crate) fn conn(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>> {
        Ok(self.pool.get()?)
    }
}
