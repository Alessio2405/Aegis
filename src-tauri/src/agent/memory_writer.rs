//! Memory writer — creates proposed memories that go to the Inbox (never auto-saved).

use rusqlite::{Connection, params};
use uuid::Uuid;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::crypto::CryptoContext;
use crate::error::{AppError, AppResult};
use crate::audit;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub workspace_id: Option<String>,
    pub memory_type: String,
    pub status: String,
    pub content: Option<String>,       // decrypted, present only when unlocked
    pub plaintext_preview: Option<String>,
    pub source: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryStatus {
    Proposed,
    Saved,
    Rejected,
}

impl MemoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Saved    => "saved",
            Self::Rejected => "rejected",
        }
    }
}

// ── Core operations ───────────────────────────────────────────────────────

/// Propose a memory (goes to Inbox with status=proposed, NOT auto-saved).
pub fn propose(
    conn: &Connection,
    crypto: &CryptoContext,
    content: &str,
    memory_type: &str,
    source: &str,
    workspace_id: Option<&str>,
) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let encrypted = crypto.encrypt_str(content)?;
    let preview: String = content.chars().take(120).collect();

    conn.execute(
        "INSERT INTO memories(id, workspace_id, type, status, content_ciphertext, plaintext_preview, source, created_at, updated_at)
         VALUES(?1,?2,?3,'proposed',?4,?5,?6,?7,?7)",
        params![id, workspace_id, memory_type, encrypted, preview, source, now],
    )?;

    audit::log_write(conn, "system", "memory", &id, workspace_id)?;
    Ok(id)
}

/// Accept a proposed memory (status → saved).
pub fn accept(conn: &Connection, memory_id: &str) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let rows = conn.execute(
        "UPDATE memories SET status='saved', updated_at=?1 WHERE id=?2 AND status='proposed'",
        params![now, memory_id],
    )?;
    if rows == 0 {
        Err(AppError::NotFound(format!("Proposed memory {memory_id}")))
    } else {
        Ok(())
    }
}

/// Edit memory content and accept it (status → saved).
pub fn edit_and_accept(
    conn: &Connection,
    crypto: &CryptoContext,
    memory_id: &str,
    new_content: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let encrypted = crypto.encrypt_str(new_content)?;
    let preview: String = new_content.chars().take(120).collect();
    let rows = conn.execute(
        "UPDATE memories SET status='saved', content_ciphertext=?1, plaintext_preview=?2, updated_at=?3 WHERE id=?4",
        params![encrypted, preview, now, memory_id],
    )?;
    if rows == 0 {
        Err(AppError::NotFound(format!("Memory {memory_id}")))
    } else {
        Ok(())
    }
}

/// Reject a proposed memory (status → rejected).
pub fn reject(conn: &Connection, memory_id: &str) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE memories SET status='rejected', updated_at=?1 WHERE id=?2",
        params![now, memory_id],
    )?;
    Ok(())
}

/// Permanently delete a memory.
pub fn delete(conn: &Connection, memory_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM memories WHERE id=?1", params![memory_id])?;
    Ok(())
}

/// List memories filtered by status, decrypting content.
pub fn list(
    conn: &Connection,
    crypto: &CryptoContext,
    status: Option<&str>,
    workspace_id: Option<&str>,
) -> AppResult<Vec<Memory>> {
    let mut sql = "SELECT id, workspace_id, type, status, content_ciphertext, plaintext_preview, source, created_at, updated_at
                   FROM memories WHERE 1=1".to_string();
    let mut param_vals: Vec<Box<dyn rusqlite::ToSql>> = vec![];

    if let Some(s) = status {
        sql.push_str(&format!(" AND status = ?{}", param_vals.len() + 1));
        param_vals.push(Box::new(s.to_string()));
    }
    if let Some(ws) = workspace_id {
        sql.push_str(&format!(" AND (workspace_id = ?{} OR workspace_id IS NULL)", param_vals.len() + 1));
        param_vals.push(Box::new(ws.to_string()));
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = param_vals.iter().map(|p| p.as_ref()).collect();

    let memories = stmt.query_map(refs.as_slice(), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<Vec<u8>>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, String>(8)?,
        ))
    })?.filter_map(|r| r.ok()).map(|(id, ws, mtype, status, cipher, preview, source, created, updated)| {
        let content = cipher.as_deref().and_then(|b| crypto.decrypt_str(b).ok());
        Memory {
            id, workspace_id: ws, memory_type: mtype, status,
            content, plaintext_preview: preview, source, created_at: created, updated_at: updated,
        }
    }).collect();

    Ok(memories)
}

/// Extract possible memories from Q&A text using simple heuristics.
/// Returns a list of (content, type) pairs to propose.
pub fn extract_candidate_memories(text: &str) -> Vec<(String, String)> {
    let mut candidates = Vec::new();

    // Heuristic 1: lines with "is a", "are" that look like definitions
    for line in text.lines() {
        let lower = line.to_lowercase();
        if (lower.contains(" is a ") || lower.contains(" is an ") || lower.contains(" refers to "))
            && line.len() > 20 && line.len() < 300
        {
            candidates.push((line.trim().to_string(), "Project".to_string()));
        }
    }

    // Heuristic 2: decisions (lines starting with "decision:", "we decided", "agreed to")
    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("decision:") || lower.starts_with("we decided")
            || lower.starts_with("agreed to") || lower.starts_with("# decision")
        {
            candidates.push((line.trim().to_string(), "Procedure".to_string()));
        }
    }

    // Deduplicate
    candidates.dedup_by(|a, b| a.0 == b.0);
    candidates.truncate(5); // at most 5 per document
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::schema::SCHEMA_SQL;
    use crate::crypto::{CryptoContext, generate_salt};

    fn setup() -> (Connection, CryptoContext) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        let salt = generate_salt();
        let crypto = CryptoContext::derive("test", &salt).unwrap();
        (conn, crypto)
    }

    #[test]
    fn test_propose_memory() {
        let (conn, crypto) = setup();
        let id = propose(&conn, &crypto, "Project Alpha uses Rust", "Project", "test", None).unwrap();
        assert!(!id.is_empty());
        let mems = list(&conn, &crypto, Some("proposed"), None).unwrap();
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].content.as_deref(), Some("Project Alpha uses Rust"));
    }

    #[test]
    fn test_accept_memory() {
        let (conn, crypto) = setup();
        let id = propose(&conn, &crypto, "Rust is used for the backend", "Project", "test", None).unwrap();
        accept(&conn, &id).unwrap();
        let saved = list(&conn, &crypto, Some("saved"), None).unwrap();
        assert_eq!(saved.len(), 1);
        let proposed = list(&conn, &crypto, Some("proposed"), None).unwrap();
        assert_eq!(proposed.len(), 0);
    }

    #[test]
    fn test_reject_memory() {
        let (conn, crypto) = setup();
        let id = propose(&conn, &crypto, "Temp note", "Episodic", "test", None).unwrap();
        reject(&conn, &id).unwrap();
        let rejected = list(&conn, &crypto, Some("rejected"), None).unwrap();
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn test_edit_and_accept() {
        let (conn, crypto) = setup();
        let id = propose(&conn, &crypto, "Original content", "Project", "test", None).unwrap();
        edit_and_accept(&conn, &crypto, &id, "Edited content").unwrap();
        let saved = list(&conn, &crypto, Some("saved"), None).unwrap();
        assert_eq!(saved[0].content.as_deref(), Some("Edited content"));
    }

    #[test]
    fn test_extract_candidate_memories() {
        let text = "SQLite is a local-first database engine.\nWe decided to use Tauri for the desktop shell.\nRandom text here.";
        let candidates = extract_candidate_memories(text);
        assert!(!candidates.is_empty());
    }
}
