use anyhow::{Result, anyhow};
use diesel::{
    Connection, QueryResult, SqliteConnection,
    connection::SimpleConnection,
    r2d2::{ConnectionManager, CustomizeConnection, Error as R2d2Error, Pool},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use parking_lot::{Mutex, MutexGuard};
use std::{
    ffi::c_char,
    fs,
    os::raw::c_int,
    path::Path,
    sync::{
        Once,
        mpsc::{self, SyncSender},
    },
    thread,
    time::Duration,
};
use tracing::warn;

mod id;
mod models;
mod schema;
mod storage;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const READ_POOL_SIZE: u32 = 4;
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);
const WAL_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

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
struct ReadOnly;

impl CustomizeConnection<SqliteConnection, R2d2Error> for ReadOnly {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), R2d2Error> {
        configure(conn)
            .and_then(|()| conn.batch_execute("PRAGMA query_only = ON;"))
            .map_err(R2d2Error::QueryError)
    }
}

fn configure(conn: &mut SqliteConnection) -> QueryResult<()> {
    conn.batch_execute(&format!(
        "PRAGMA busy_timeout = {};
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA wal_autocheckpoint = 0;
         PRAGMA journal_size_limit = {WAL_SIZE_LIMIT};",
        BUSY_TIMEOUT.as_millis()
    ))
}

pub struct SqliteStorage {
    readers: Pool<ConnectionManager<SqliteConnection>>,
    writer: Mutex<SqliteConnection>,
    checkpointer: SyncSender<()>,
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

        let mut writer = SqliteConnection::establish(path)?;

        writer
            .batch_execute("PRAGMA journal_mode = WAL;")
            .map_err(|e| anyhow!("failed to enable WAL: {e}"))?;
        configure(&mut writer)?;

        writer
            .run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow!("failed to run migrations: {e}"))?;

        if let Err(e) = storage::delete_expired_data(&mut writer) {
            warn!("failed to delete expired component data: {e}");
        }

        let readers = Pool::builder()
            .max_size(READ_POOL_SIZE)
            .connection_customizer(Box::new(ReadOnly))
            .build(ConnectionManager::new(path))?;

        Ok(Self {
            checkpointer: spawn_checkpointer(readers.clone()),
            readers,
            writer: Mutex::new(writer),
        })
    }

    /// Runs `f` in a read transaction.
    pub(crate) fn read<T>(&self, f: impl FnOnce(&mut SqliteConnection) -> Result<T>) -> Result<T> {
        let mut conn = self.readers.get()?;
        (*conn).transaction(f)
    }

    /// Runs `f` in a write transaction.
    /// Writers are queued here instead of in SQLite to avoid writer starvation.
    pub(crate) fn write<T>(&self, f: impl FnOnce(&mut SqliteConnection) -> Result<T>) -> Result<T> {
        let mut writer = self.writer.lock();
        let result = writer.immediate_transaction(f);
        MutexGuard::unlock_fair(writer);

        if result.is_ok() {
            let _ = self.checkpointer.try_send(());
        }
        result
    }
}

/// Checkpoints the WAL in the background so writers don't have to do it.
fn spawn_checkpointer(pool: Pool<ConnectionManager<SqliteConnection>>) -> SyncSender<()> {
    let (notify, notified) = mpsc::sync_channel(1);
    thread::spawn(move || {
        while notified.recv().is_ok() {
            let result = pool
                .get()
                .map_err(anyhow::Error::from)
                .and_then(|mut conn| Ok(conn.batch_execute("PRAGMA wal_checkpoint(PASSIVE)")?));

            if let Err(err) = result {
                warn!("failed to checkpoint WAL: {err}");
            }

            thread::sleep(CHECKPOINT_INTERVAL);
        }
    });
    notify
}
