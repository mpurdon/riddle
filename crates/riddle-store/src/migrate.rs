//! Numbered, embedded migrations with a backup before any structural change.

use crate::StoreKind;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, Category};
use rusqlite::Connection;
use std::path::Path;

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

const DIG: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("../migrations/dig/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "embeddings",
        sql: include_str!("../migrations/dig/0002_embeddings.sql"),
    },
    Migration {
        version: 3,
        name: "shadow_reviews",
        sql: include_str!("../migrations/dig/0003_shadow_reviews.sql"),
    },
    Migration {
        version: 4,
        name: "message_series",
        sql: include_str!("../migrations/dig/0004_message_series.sql"),
    },
    Migration {
        version: 5,
        name: "trace_recency",
        sql: include_str!("../migrations/dig/0005_trace_recency.sql"),
    },
    Migration {
        version: 6,
        name: "importance",
        sql: include_str!("../migrations/dig/0006_importance.sql"),
    },
    Migration {
        version: 7,
        name: "provider_labels",
        sql: include_str!("../migrations/dig/0007_provider_labels.sql"),
    },
];
const INDEX: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("../migrations/index/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "usage",
        sql: include_str!("../migrations/index/0002_usage.sql"),
    },
    Migration {
        version: 3,
        name: "find_series_and_expiry",
        sql: include_str!("../migrations/index/0003_find_series_and_expiry.sql"),
    },
    Migration {
        version: 4,
        name: "find_kept",
        sql: include_str!("../migrations/index/0004_find_kept.sql"),
    },
    Migration {
        version: 5,
        name: "comparison",
        sql: include_str!("../migrations/index/0005_comparison.sql"),
    },
    Migration {
        version: 6,
        name: "proposals",
        sql: include_str!("../migrations/index/0006_proposals.sql"),
    },
    Migration {
        version: 7,
        name: "activity_conversation",
        sql: include_str!("../migrations/index/0007_activity_conversation.sql"),
    },
    Migration {
        version: 8,
        name: "proposal_outcome_columns",
        sql: include_str!("../migrations/index/0008_proposal_outcome_columns.sql"),
    },
];

fn set_for(kind: StoreKind) -> &'static [Migration] {
    match kind {
        StoreKind::Dig => DIG,
        StoreKind::Index => INDEX,
    }
}

pub fn current_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL, name TEXT NOT NULL, applied_at INTEGER NOT NULL)")?;
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |r| r.get(0),
    )
}

pub fn latest_version(kind: StoreKind) -> u32 {
    set_for(kind).last().map(|m| m.version).unwrap_or(0)
}

/// Apply every pending migration. A store on disk is copied to
/// `legacy-backup/` before the first structural change.
pub fn run(conn: &Connection, kind: StoreKind, path: Option<&Path>) -> Result<()> {
    let have = current_version(conn).map_err(|e| RiddleError::store(e.to_string()))?;
    let pending: Vec<&Migration> = set_for(kind).iter().filter(|m| m.version > have).collect();
    if pending.is_empty() {
        return Ok(());
    }
    if have > 0 {
        if let Some(p) = path {
            backup(p, have)?;
        }
    }
    for m in pending {
        rinfo!(
            Category::Store,
            "migrating {:?} store to v{} ({})",
            kind,
            m.version,
            m.name
        );
        conn.execute_batch("BEGIN;")
            .map_err(|e| RiddleError::store(e.to_string()))?;
        let applied = conn.execute_batch(m.sql).and_then(|_| {
            conn.execute(
                "INSERT INTO schema_version (version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![m.version, m.name, crate::now_ts()],
            )
        });
        match applied {
            Ok(_) => conn
                .execute_batch("COMMIT;")
                .map_err(|e| RiddleError::store(e.to_string()))?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                return Err(RiddleError::store(format!(
                    "migration v{} ({}) failed: {e}",
                    m.version, m.name
                )));
            }
        }
    }
    Ok(())
}

fn backup(path: &Path, version: u32) -> Result<()> {
    let dir = path
        .parent()
        .unwrap_or(Path::new("."))
        .join("legacy-backup");
    std::fs::create_dir_all(&dir)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("store");
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let target = dir.join(format!("{stem}-v{version}-{stamp}.sqlite"));
    let src = Connection::open(path).map_err(|e| RiddleError::store(e.to_string()))?;
    let mut dst = Connection::open(&target).map_err(|e| RiddleError::store(e.to_string()))?;
    let b = rusqlite::backup::Backup::new(&src, &mut dst)
        .map_err(|e| RiddleError::store(e.to_string()))?;
    b.run_to_completion(256, std::time::Duration::from_millis(5), None)
        .map_err(|e| RiddleError::store(e.to_string()))?;
    rinfo!(
        Category::Store,
        "backed up {} to {}",
        path.display(),
        target.display()
    );
    Ok(())
}
