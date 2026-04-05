//! Database module — connection management, migrations, and typed row helpers.

pub mod schema;

use rusqlite::{Connection, params};
use std::path::Path;
use crate::error::AppResult;

/// Open (or create) the SQLite database at `path` and run the schema migration.
pub fn open(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    // Enable WAL and foreign keys for this connection
    conn.execute_batch(schema::SCHEMA_SQL)?;
    // Incremental migrations — idiomatic SQLite: ignore "duplicate column" errors
    let _ = conn.execute("ALTER TABLE user_agents ADD COLUMN schedule TEXT", []);
    let _ = conn.execute("ALTER TABLE user_agents ADD COLUMN last_run_at TEXT", []);
    let _ = conn.execute("ALTER TABLE user_agents ADD COLUMN schedule_paused INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE user_agents ADD COLUMN react_mode INTEGER NOT NULL DEFAULT 0", []);
    Ok(conn)
}

// ── Generic setting helpers ────────────────────────────────────────────────

pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Check whether the FTS chunk_fts table has the given rowid.
/// Used in retrieval tests.
#[allow(dead_code)]
pub fn fts_count(conn: &Connection) -> AppResult<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM chunk_fts",
        [],
        |r| r.get(0),
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(schema::SCHEMA_SQL).unwrap();
        conn
    }

    #[test]
    fn test_schema_creates_tables() {
        let conn = test_db();
        // Check that required tables exist
        for table in &["workspaces", "documents", "chunks", "memories", "permissions", "audit_events", "settings"] {
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                params![table],
                |r| r.get(0),
            ).unwrap();
            assert!(exists, "Table '{}' should exist", table);
        }
    }

    #[test]
    fn test_settings_defaults() {
        let conn = test_db();
        let val = get_setting(&conn, "model_enabled").unwrap();
        assert_eq!(val, Some("false".to_string()));
    }

    #[test]
    fn test_set_and_get_setting() {
        let conn = test_db();
        set_setting(&conn, "model_enabled", "true").unwrap();
        let val = get_setting(&conn, "model_enabled").unwrap();
        assert_eq!(val, Some("true".to_string()));
    }
}
