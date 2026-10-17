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
    Migration {
        version: 8,
        name: "sift_markers",
        sql: include_str!("../migrations/dig/0008_sift_markers.sql"),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A Dig store brought up to the version before `v`, with no backup.
    fn dig_store_before(v: u32) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        current_version(&conn).unwrap();
        for m in DIG.iter().filter(|m| m.version < v) {
            conn.execute_batch(m.sql).unwrap();
            conn.execute(
                "INSERT INTO schema_version (version, name, applied_at) VALUES (?1, ?2, 0)",
                rusqlite::params![m.version, m.name],
            )
            .unwrap();
        }
        conn
    }

    fn one<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn the_sift_markers_migration_backfills_from_the_rows_it_mirrors() {
        let conn = dig_store_before(8);
        // One message decided, one decided then sent back, one with only a
        // Shadow proposal; a mixed-case sender; threads with and without
        // an open ask of mine.
        for (id, from) in [
            ("m1", "Billing@Acme.example"),
            ("m2", "bob@example.com"),
            ("m3", "carol@example.com"),
        ] {
            conn.execute(
                "INSERT INTO messages (id, provider_id, thread_id, subject, from_email, to_json, cc_json, bcc_json, date, snippet, labels_json, headers_json, attachments_json, size_bytes, is_sent, synced_at)
                 VALUES (?1, ?1, 't', 'Statement', ?2, '[]', '[]', '[]', 1, '', '[]', '{}', '[]', 0, 0, 1)",
                rusqlite::params![id, from],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO classifications (id, message_id, disposition_kind, json, shadow, created_at, superseded_at) VALUES
               ('k1', 'm1', 'spoil', '{}', 0, 100, NULL),
               ('k2', 'm2', 'spoil', '{}', 0, 200, 300),
               ('k3', 'm3', 'find', '{}', 1, 400, NULL);
             INSERT INTO thread_state (thread_id, json, updated_at) VALUES
               ('t1', '{\"openAsksFromMe\":[\"Can you send it?\"]}', 0),
               ('t2', '{\"openAsksFromMe\":[]}', 0),
               ('t3', '{}', 0);",
        )
        .unwrap();

        run(&conn, StoreKind::Dig, None).unwrap();

        assert_eq!(
            current_version(&conn).unwrap(),
            latest_version(StoreKind::Dig)
        );
        let markers = |id: &str| -> (Option<i64>, Option<i64>) {
            conn.query_row(
                "SELECT sifted_at, shadow_sifted_at FROM messages WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert_eq!(markers("m1"), (Some(100), None));
        assert_eq!(markers("m2"), (None, None));
        assert_eq!(markers("m3"), (None, Some(400)));
        assert_eq!(
            one::<String>(&conn, "SELECT from_email FROM messages WHERE id = 'm1'"),
            "billing@acme.example"
        );
        // The FTS mirror followed the lowercase rewrite.
        assert_eq!(
            one::<i64>(
                &conn,
                "SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'acme'"
            ),
            1
        );
        assert_eq!(
            one::<String>(
                &conn,
                "SELECT group_concat(thread_id) FROM thread_state WHERE has_open_asks = 1"
            ),
            "t1"
        );
    }
}
