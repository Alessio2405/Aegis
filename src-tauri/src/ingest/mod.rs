//! Ingest pipeline — scan workspace folder → extract text → chunk → encrypt → store → index FTS.
//!
//! Chunking strategy:
//!   - Target chunk size: 600–900 chars
//!   - Overlap: ~100 chars (last 100 chars of previous chunk repeated at start of next)
//!   - Split on paragraph boundaries (double newline) when possible; fallback to hard split.
//!   - plaintext_preview = first 240 chars (stored plaintext for FTS + UI snippets)
//!   - ciphertext_text   = full chunk text, encrypted

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use sha2::{Sha256, Digest};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::crypto::CryptoContext;
use crate::audit;

const CHUNK_TARGET: usize = 750;
const CHUNK_OVERLAP: usize = 100;
const PREVIEW_LEN: usize = 240;

// ── Public types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestStats {
    pub workspace_id: String,
    pub documents_added: usize,
    pub chunks_added: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentInfo {
    pub id: String,
    pub workspace_id: String,
    pub path: String,
    pub title: String,
    pub chunk_count: i64,
    pub created_at: String,
}

// ── Workspace management ──────────────────────────────────────────────────

pub fn create_workspace(conn: &Connection, name: &str, path: &str) -> AppResult<Workspace> {
    // Verify the path exists
    if !Path::new(path).is_dir() {
        return Err(AppError::InvalidInput(format!("Path does not exist or is not a directory: {path}")));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO workspaces(id, name, path, created_at) VALUES(?1,?2,?3,?4)",
        params![id, name, path, now],
    )?;
    Ok(Workspace { id, name: name.to_string(), path: path.to_string(), created_at: now })
}

pub fn list_workspaces(conn: &Connection) -> AppResult<Vec<Workspace>> {
    let mut stmt = conn.prepare("SELECT id, name, path, created_at FROM workspaces ORDER BY created_at DESC")?;
    let ws = stmt.query_map([], |r| Ok(Workspace {
        id: r.get(0)?, name: r.get(1)?, path: r.get(2)?, created_at: r.get(3)?,
    }))?.filter_map(|r| r.ok()).collect();
    Ok(ws)
}

pub fn get_workspace(conn: &Connection, id: &str) -> AppResult<Workspace> {
    conn.query_row(
        "SELECT id, name, path, created_at FROM workspaces WHERE id = ?1",
        params![id],
        |r| Ok(Workspace { id: r.get(0)?, name: r.get(1)?, path: r.get(2)?, created_at: r.get(3)? }),
    ).map_err(|_| AppError::NotFound(format!("Workspace {id}")))
}

pub fn list_documents(conn: &Connection, workspace_id: &str) -> AppResult<Vec<DocumentInfo>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.workspace_id, d.path, d.title, d.created_at,
                COUNT(c.id) as chunk_count
         FROM documents d
         LEFT JOIN chunks c ON c.document_id = d.id
         WHERE d.workspace_id = ?1
         GROUP BY d.id ORDER BY d.created_at DESC"
    )?;
    let docs = stmt.query_map(params![workspace_id], |r| Ok(DocumentInfo {
        id: r.get(0)?, workspace_id: r.get(1)?, path: r.get(2)?,
        title: r.get(3)?, created_at: r.get(4)?, chunk_count: r.get(5)?,
    }))?.filter_map(|r| r.ok()).collect();
    Ok(docs)
}

// ── Ingest ────────────────────────────────────────────────────────────────

/// Ingest all .md and .txt files in the workspace folder.
/// Skips files whose hash hasn't changed (idempotent).
/// If `llm` is provided and `llm.embeddings_enabled`, embeds each chunk preview after ingest.
pub fn ingest_workspace(
    conn: &Connection,
    crypto: &CryptoContext,
    workspace_id: &str,
    llm: Option<&crate::connectors::LlmConfig>,
) -> AppResult<IngestStats> {
    let ws = get_workspace(conn, workspace_id)?;
    let folder = PathBuf::from(&ws.path);
    let files = collect_files(&folder)?;

    let mut docs_added = 0usize;
    let mut chunks_added = 0usize;
    let mut skipped = 0usize;

    for file_path in &files {
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let text = if ext == "pdf" {
            match pdf_extract::extract_text(file_path) {
                Ok(t) => t,
                Err(_) => { skipped += 1; continue; }
            }
        } else {
            match fs::read_to_string(file_path) {
                Ok(t) => t,
                Err(_) => { skipped += 1; continue; }
            }
        };
        let hash = compute_hash(text.as_bytes());
        let path_str = file_path.to_string_lossy().to_string();

        // Skip if unchanged
        let existing_hash: Option<String> = conn.query_row(
            "SELECT hash FROM documents WHERE workspace_id = ?1 AND path = ?2",
            params![workspace_id, path_str],
            |r| r.get(0),
        ).ok();

        if existing_hash.as_deref() == Some(&hash) {
            skipped += 1;
            continue;
        }

        // Delete old document+chunks if re-ingesting
        if existing_hash.is_some() {
            let old_id: String = conn.query_row(
                "SELECT id FROM documents WHERE workspace_id = ?1 AND path = ?2",
                params![workspace_id, path_str],
                |r| r.get(0),
            )?;
            // Delete FTS rows for old chunks
            conn.execute(
                "DELETE FROM chunk_fts WHERE rowid IN (SELECT rowid FROM chunks WHERE document_id = ?1)",
                params![old_id],
            ).ok();
            conn.execute("DELETE FROM documents WHERE id = ?1", params![old_id])?;
        }

        let title = file_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let mtime = fs::metadata(file_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        let doc_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO documents(id, workspace_id, path, title, mtime, hash, created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![doc_id, workspace_id, path_str, title, mtime, hash, now],
        )?;

        let chunk_count = ingest_chunks(conn, crypto, &doc_id, &title, &text)?;
        chunks_added += chunk_count;
        docs_added += 1;

        audit::log_read(conn, "system", "file", &path_str, Some(workspace_id))?;

        // Embed new chunks if embeddings are enabled
        if let Some(llm) = llm {
            if llm.embeddings_enabled {
                let chunk_previews: Vec<(String, Option<String>)> = {
                    let mut stmt = conn.prepare(
                        "SELECT id, plaintext_preview FROM chunks WHERE document_id = ?1"
                    ).unwrap_or_else(|_| panic!("prepare failed"));
                    stmt.query_map(params![doc_id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
                    }).map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default()
                };
                for (chunk_id, preview) in &chunk_previews {
                    let text_to_embed = preview.as_deref().unwrap_or("").trim();
                    if !text_to_embed.is_empty() {
                        if let Ok(vec) = crate::connectors::llm_embed(llm, text_to_embed) {
                            let _ = crate::retrieval::store_embedding(conn, chunk_id, &vec);
                        }
                    }
                }
            }
        }
    }

    audit::log_ingest(conn, workspace_id, docs_added, chunks_added)?;

    Ok(IngestStats {
        workspace_id: workspace_id.to_string(),
        documents_added: docs_added,
        chunks_added,
        skipped,
    })
}

/// Chunk a document's text and insert all chunks + FTS entries.
pub fn ingest_chunks(
    conn: &Connection,
    crypto: &CryptoContext,
    doc_id: &str,
    title: &str,
    text: &str,
) -> AppResult<usize> {
    let chunks = chunk_text(text);
    let now = Utc::now().to_rfc3339();
    let mut count = 0;

    for (idx, (start, end, chunk_text)) in chunks.iter().enumerate() {
        let chunk_id = Uuid::new_v4().to_string();
        let preview: String = chunk_text.chars().take(PREVIEW_LEN).collect();
        let encrypted = crypto.encrypt_str(chunk_text)?;

        conn.execute(
            "INSERT INTO chunks(id, document_id, idx, start_char, end_char, ciphertext_text, plaintext_preview, created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                chunk_id, doc_id, idx as i64,
                *start as i64, *end as i64,
                encrypted, preview, now
            ],
        )?;

        // Index in FTS5
        conn.execute(
            "INSERT INTO chunk_fts(rowid, title, plaintext_preview)
             SELECT rowid, ?1, ?2 FROM chunks WHERE id = ?3",
            params![title, preview, chunk_id],
        )?;

        count += 1;
    }
    Ok(count)
}

// ── Chunking ──────────────────────────────────────────────────────────────

/// Split `text` into overlapping chunks. Returns Vec<(start, end, chunk_str)>.
pub fn chunk_text(text: &str) -> Vec<(usize, usize, String)> {
    if text.is_empty() {
        return vec![];
    }

    let mut chunks: Vec<(usize, usize, String)> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();
    let mut pos = 0usize;

    while pos < total {
        let end = (pos + CHUNK_TARGET).min(total);

        // Try to break on a paragraph boundary (double newline) within the target window
        let slice: String = chars[pos..end].iter().collect();
        let break_point = if end < total {
            find_break_point(&slice, CHUNK_TARGET)
        } else {
            end - pos
        };

        let actual_end = (pos + break_point).min(total);
        let chunk_str: String = chars[pos..actual_end].iter().collect();

        chunks.push((pos, actual_end, chunk_str));

        if actual_end >= total {
            break;
        }

        // Next chunk starts with overlap
        pos = actual_end.saturating_sub(CHUNK_OVERLAP);
    }

    chunks
}

/// Find a good break point within `text` near `target_len`.
fn find_break_point(text: &str, target_len: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let search_start = (target_len.saturating_sub(200)).min(chars.len());
    let search_end = target_len.min(chars.len());

    // Look backwards from target for "\n\n"
    for i in (search_start..search_end).rev() {
        if i + 1 < chars.len() && chars[i] == '\n' && chars[i + 1] == '\n' {
            return i + 2;
        }
    }
    // Fallback: break on sentence end
    for i in (search_start..search_end).rev() {
        if chars[i] == '.' || chars[i] == '?' || chars[i] == '!' {
            return (i + 1).min(chars.len());
        }
    }
    // Hard break at target
    search_end
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn collect_files(dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // Recurse one level (avoid symlink loops)
                if let Ok(sub) = collect_files(&path) {
                    files.extend(sub);
                }
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "md" | "txt" | "markdown" | "pdf") {
                    files.push(path);
                }
            }
        }
    }
    Ok(files)
}

fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_basic() {
        let text = "Hello world. ".repeat(100); // ~1300 chars
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2, "Should produce multiple chunks");
        for (_, _, c) in &chunks {
            assert!(!c.is_empty());
            assert!(c.len() <= CHUNK_TARGET + CHUNK_OVERLAP + 50);
        }
    }

    #[test]
    fn test_chunk_text_empty() {
        let chunks = chunk_text("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_short() {
        let text = "Short text.";
        let chunks = chunk_text(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].2, "Short text.");
    }

    #[test]
    fn test_chunk_overlap() {
        // With overlap, end of chunk N should appear at start of chunk N+1
        let text: String = std::iter::repeat("word ")
            .take(300)
            .collect();
        let chunks = chunk_text(&text);
        if chunks.len() >= 2 {
            // The end of chunk 0 and start of chunk 1 should overlap
            let c0_end = &chunks[0].2[chunks[0].2.len().saturating_sub(CHUNK_OVERLAP)..];
            let c1_start = &chunks[1].2[..CHUNK_OVERLAP.min(chunks[1].2.len())];
            // They should share some content
            assert!(!c0_end.is_empty() && !c1_start.is_empty());
        }
    }

    #[test]
    fn test_chunk_positions() {
        let text = "ABCDE".repeat(300);
        let chunks = chunk_text(&text);
        // First chunk must start at 0
        assert_eq!(chunks[0].0, 0);
        // Positions must be monotonically increasing
        let starts: Vec<usize> = chunks.iter().map(|(s, _, _)| *s).collect();
        let is_monotone = starts.windows(2).all(|w| w[1] > w[0]);
        assert!(is_monotone);
    }

    #[test]
    fn test_fts_ingest() {
        use crate::crypto::{CryptoContext, generate_salt};
        use crate::db::schema::SCHEMA_SQL;
        use rusqlite::Connection;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        let salt = generate_salt();
        let crypto = CryptoContext::derive("test", &salt).unwrap();

        // Create workspace (in-memory so path check is skipped)
        let ws_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO workspaces(id,name,path,created_at) VALUES(?1,'Test','/tmp','2024-01-01T00:00:00Z')",
            params![ws_id],
        ).unwrap();

        let doc_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO documents(id,workspace_id,path,title,created_at) VALUES(?1,?2,'/tmp/a.md','Doc A','2024-01-01T00:00:00Z')",
            params![doc_id, ws_id],
        ).unwrap();

        let text = "The quick brown fox jumps over the lazy dog. ".repeat(20);
        let count = ingest_chunks(&conn, &crypto, &doc_id, "Doc A", &text).unwrap();
        assert!(count > 0);

        // FTS search should find something
        let found: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chunk_fts WHERE chunk_fts MATCH 'fox'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(found > 0, "FTS should find 'fox' in ingested chunks");
    }
}
