//! SQLite persistence: one store per Dig plus an index store (§11).
//!
//! Every write passes through the persistence gate: it is logged with its call
//! site, and three consecutive failures flip a fault flag that pauses Sift and
//! shows in the Task Centre. Silent write failure is the incident this
//! prevents. Migrations are numbered SQL files embedded at compile time; before
//! any structural migration the previous file is copied to `legacy-backup/`.
//!
//! Repositories live in `repo::*`, one module per table family, so the rest of
//! the workspace never writes SQL against another crate's tables.

pub mod migrate;
pub mod repo;
pub mod stores;

use parking_lot::Mutex;
use r2d2_sqlite::SqliteConnectionManager;
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_observability::{rdebug, rerror, rinfo, rwarn, Category};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub use rusqlite;
pub use stores::Stores;

/// Which schema a store carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    Dig,
    Index,
}

/// Consecutive failures before the gate faults.
const FAULT_THRESHOLD: u32 = 3;

type FaultHook = dyn Fn(Option<String>) + Send + Sync;

pub struct Store {
    kind: StoreKind,
    path: Option<PathBuf>,
    /// The single writer. All mutations serialize here.
    writer: Mutex<Connection>,
    /// Pooled readers; `None` for in-memory stores, which share the writer.
    readers: Option<r2d2::Pool<SqliteConnectionManager>>,
    consecutive_failures: AtomicU32,
    fault: Mutex<Option<String>>,
    on_fault: Mutex<Option<Arc<FaultHook>>>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("kind", &self.kind).field("path", &self.path).finish()
    }
}

fn map_sql(e: rusqlite::Error) -> RiddleError {
    RiddleError::new(ErrorKind::Store, e.to_string())
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA temp_store = MEMORY;",
    )
}

impl Store {
    /// Open (or create) a store on disk and bring its schema up to date.
    pub fn open(path: impl AsRef<Path>, kind: StoreKind) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let writer = Connection::open(&path).map_err(map_sql)?;
        configure(&writer).map_err(map_sql)?;
        migrate::run(&writer, kind, Some(&path))?;
        let manager = SqliteConnectionManager::file(&path).with_init(|c| configure(c));
        let readers = r2d2::Pool::builder().max_size(4).build(manager).map_err(|e| RiddleError::store(e.to_string()))?;
        rinfo!(Category::Store, "opened {:?} store {}", kind, path.display());
        Ok(Self {
            kind,
            path: Some(path),
            writer: Mutex::new(writer),
            readers: Some(readers),
            consecutive_failures: AtomicU32::new(0),
            fault: Mutex::new(None),
            on_fault: Mutex::new(None),
        })
    }

    /// An in-memory store for tests and the test-host guard (§59).
    pub fn in_memory(kind: StoreKind) -> Result<Self> {
        let writer = Connection::open_in_memory().map_err(map_sql)?;
        writer.execute_batch("PRAGMA foreign_keys = ON;").map_err(map_sql)?;
        migrate::run(&writer, kind, None)?;
        Ok(Self {
            kind,
            path: None,
            writer: Mutex::new(writer),
            readers: None,
            consecutive_failures: AtomicU32::new(0),
            fault: Mutex::new(None),
            on_fault: Mutex::new(None),
        })
    }

    pub fn kind(&self) -> StoreKind { self.kind }
    pub fn path(&self) -> Option<&Path> { self.path.as_deref() }

    /// Size on disk including WAL, for the Developer pane's storage row.
    pub fn size_bytes(&self) -> u64 {
        let Some(p) = &self.path else { return 0 };
        let mut total = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        for suffix in ["-wal", "-shm"] {
            let mut side = p.as_os_str().to_owned();
            side.push(suffix);
            total += std::fs::metadata(side).map(|m| m.len()).unwrap_or(0);
        }
        total
    }

    /// Register the hook the app uses to surface a fault in the Task Centre.
    pub fn on_fault(&self, hook: Arc<FaultHook>) { *self.on_fault.lock() = Some(hook); }

    pub fn fault(&self) -> Option<String> { self.fault.lock().clone() }

    /// Clear the fault after the user or a maintenance job intervened.
    pub fn clear_fault(&self) {
        *self.fault.lock() = None;
        self.consecutive_failures.store(0, Ordering::SeqCst);
        if let Some(h) = self.on_fault.lock().as_ref() { h(None); }
    }

    /// Run a read against a pooled reader (or the shared connection in memory).
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        match &self.readers {
            Some(pool) => {
                let conn = pool.get().map_err(|e| RiddleError::store(e.to_string()))?;
                f(&conn).map_err(map_sql)
            }
            None => {
                let conn = self.writer.lock();
                f(&conn).map_err(map_sql)
            }
        }
    }

    /// The persistence gate. Runs `f` in a transaction on the single writer,
    /// logs the call site, and counts consecutive failures.
    #[track_caller]
    pub fn write<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let caller = std::panic::Location::caller();
        let site = format!("{}:{}", short_file(caller.file()), caller.line());
        let mut conn = self.writer.lock();
        let result = (|| {
            let tx = conn.transaction()?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok::<T, rusqlite::Error>(out)
        })();
        match result {
            Ok(v) => {
                let prior = self.consecutive_failures.swap(0, Ordering::SeqCst);
                if prior > 0 {
                    rinfo!(Category::Store, "write recovered at {site} after {prior} failures");
                }
                rdebug!(Category::Store, "write ok at {site}");
                Ok(v)
            }
            Err(e) => {
                let n = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
                rerror!(Category::Store, "write failed at {site} ({n} consecutive): {e}");
                if n >= FAULT_THRESHOLD {
                    let msg = format!("{n} consecutive write failures; last at {site}: {e}");
                    *self.fault.lock() = Some(msg.clone());
                    rwarn!(Category::Store, "persistence gate faulted");
                    if let Some(h) = self.on_fault.lock().as_ref() { h(Some(msg)); }
                }
                Err(map_sql(e))
            }
        }
    }

    /// Run a checkpoint and vacuum; called by the maintenance job.
    pub fn compact(&self) -> Result<()> {
        let conn = self.writer.lock();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA optimize;").map_err(map_sql)
    }
}

fn short_file(f: &str) -> &str {
    f.rsplit("crates/").next().unwrap_or(f)
}

/// Seconds since epoch as stored in every `*_at INTEGER` column.
pub fn ts(t: &chrono::DateTime<chrono::Utc>) -> i64 { t.timestamp_millis() }
pub fn from_ts(ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default()
}
pub fn now_ts() -> i64 { chrono::Utc::now().timestamp_millis() }

/// JSON column helpers so repositories stay short.
pub fn to_json<T: serde::Serialize>(v: &T) -> rusqlite::Result<String> {
    serde_json::to_string(v).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}
pub fn from_json<T: serde::de::DeserializeOwned>(s: String) -> rusqlite::Result<T> {
    serde_json::from_str(&s).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_in_memory_store_migrates_both_schemas() {
        let dig = Store::in_memory(StoreKind::Dig).unwrap();
        let n: i64 = dig.read(|c| c.query_row("SELECT count(*) FROM messages", [], |r| r.get(0))).unwrap();
        assert_eq!(n, 0);
        let index = Store::in_memory(StoreKind::Index).unwrap();
        let n: i64 = index.read(|c| c.query_row("SELECT count(*) FROM finds", [], |r| r.get(0))).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn three_consecutive_write_failures_flip_the_fault_flag_and_a_success_clears_the_count() {
        let s = Store::in_memory(StoreKind::Index).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let hook = seen.clone();
        s.on_fault(Arc::new(move |f| hook.lock().push(f)));
        for _ in 0..2 {
            assert!(s.write(|c| c.execute("INSERT INTO nope VALUES (1)", [])).is_err());
        }
        assert!(s.fault().is_none());
        s.write(|c| c.execute("INSERT INTO kv (key, value, updated_at) VALUES ('a','b',0)", [])).unwrap();
        for _ in 0..3 {
            let _ = s.write(|c| c.execute("INSERT INTO nope VALUES (1)", []));
        }
        assert!(s.fault().is_some());
        assert_eq!(seen.lock().len(), 1);
        s.clear_fault();
        assert!(s.fault().is_none());
    }

    #[test]
    fn a_disk_store_opens_migrates_and_reports_its_size() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("index.sqlite");
        let s = Store::open(&p, StoreKind::Index).unwrap();
        s.write(|c| c.execute("INSERT INTO kv (key, value, updated_at) VALUES ('a','b',0)", [])).unwrap();
        assert!(s.size_bytes() > 0);
        drop(s);
        let again = Store::open(&p, StoreKind::Index).unwrap();
        let v: String = again.read(|c| c.query_row("SELECT value FROM kv WHERE key='a'", [], |r| r.get(0))).unwrap();
        assert_eq!(v, "b");
    }
}
