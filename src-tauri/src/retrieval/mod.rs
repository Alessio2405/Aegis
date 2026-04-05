//! Retrieval module — FTS5 keyword search + optional cosine-similarity over embeddings.
//!
//! Strategy:
//!   1. Always run FTS5 BM25 query (fast, no model needed).
//!   2. If embeddings are enabled and Ollama is reachable, also run cosine-similarity.
//!   3. Merge results, deduplicate by chunk_id, sort by combined score.
//!   4. Return top-K with full decrypted text (decrypted in-process, never written to disk).

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::error::{AppResult, AppError};
use crate::crypto::CryptoContext;
use crate::audit;

pub const DEFAULT_TOP_K: usize = 5;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedChunk {
    pub chunk_id: String,
    pub document_id: String,
    pub doc_title: String,
    pub doc_path: String,
    pub chunk_idx: i64,
    pub start_char: i64,
    pub end_char: i64,
    /// Short preview (plaintext, first 240 chars) — used in citation cards
    pub preview: String,
    /// Full decrypted chunk text — only present when "Open source" is requested
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_text: Option<String>,
    pub score: f64,
}

// ── FTS retrieval ─────────────────────────────────────────────────────────

/// Run FTS5 BM25 search. Returns at most `top_k` results.
pub fn fts_search(
    conn: &Connection,
    query: &str,
    top_k: usize,
) -> AppResult<Vec<RetrievedChunk>> {
    // Sanitize query for FTS5 — escape special chars
    let fts_query = sanitize_fts_query(query);

    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, d.title, d.path, c.idx, c.start_char, c.end_char,
                c.plaintext_preview,
                bm25(chunk_fts) as score
         FROM chunk_fts
         JOIN chunks c ON chunk_fts.rowid = c.rowid
         JOIN documents d ON c.document_id = d.id
         WHERE chunk_fts MATCH ?1
         ORDER BY score
         LIMIT ?2"
    )?;

    let chunks = stmt.query_map(params![fts_query, top_k as i64], |r| {
        Ok(RetrievedChunk {
            chunk_id:    r.get(0)?,
            document_id: r.get(1)?,
            doc_title:   r.get(2)?,
            doc_path:    r.get(3)?,
            chunk_idx:   r.get(4)?,
            start_char:  r.get(5)?,
            end_char:    r.get(6)?,
            preview:     r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            full_text:   None,
            score:       r.get::<_, f64>(8)?.abs(), // bm25 returns negative; negate for sort
        })
    })?.filter_map(|r| r.ok()).collect();

    Ok(chunks)
}

/// Decrypt and return the full text for a specific chunk.
pub fn get_chunk_full_text(
    conn: &Connection,
    crypto: &CryptoContext,
    chunk_id: &str,
) -> AppResult<RetrievedChunk> {
    let row: Option<(String, String, String, String, i64, i64, i64, Option<Vec<u8>>, Option<String>)> = conn.query_row(
        "SELECT c.id, c.document_id, d.title, d.path, c.idx, c.start_char, c.end_char,
                c.ciphertext_text, c.plaintext_preview
         FROM chunks c
         JOIN documents d ON c.document_id = d.id
         WHERE c.id = ?1",
        params![chunk_id],
        |r| Ok((
            r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?,
            r.get(4)?, r.get(5)?, r.get(6)?,
            r.get(7)?, r.get(8)?,
        )),
    ).ok();

    let (chunk_id, doc_id, title, path, idx, start, end, cipher, preview) =
        row.ok_or_else(|| AppError::NotFound(format!("Chunk {chunk_id}")))?;

    let full_text = if let Some(ref blob) = cipher {
        Some(crypto.decrypt_str(blob)?)
    } else {
        None
    };

    Ok(RetrievedChunk {
        chunk_id,
        document_id: doc_id,
        doc_title: title,
        doc_path: path,
        chunk_idx: idx,
        start_char: start,
        end_char: end,
        preview: preview.unwrap_or_default(),
        full_text,
        score: 0.0,
    })
}

// ── Embedding retrieval (optional) ────────────────────────────────────────

/// Retrieve top-K chunks by cosine similarity over stored embeddings.
/// Falls back gracefully if no embeddings exist.
pub fn embedding_search(
    conn: &Connection,
    query_vec: &[f32],
    top_k: usize,
) -> AppResult<Vec<RetrievedChunk>> {
    // Load all embeddings into memory (acceptable for MVP / small data sets)
    let mut stmt = conn.prepare(
        "SELECT e.chunk_id, e.dim, e.vector_blob,
                c.document_id, d.title, d.path, c.idx, c.start_char, c.end_char, c.plaintext_preview
         FROM embeddings e
         JOIN chunks c ON e.chunk_id = c.id
         JOIN documents d ON c.document_id = d.id"
    )?;

    let mut scored: Vec<(f64, RetrievedChunk)> = stmt.query_map([], |r| {
        let chunk_id: String     = r.get(0)?;
        let dim: i64             = r.get(1)?;
        let blob: Vec<u8>        = r.get(2)?;
        let doc_id: String       = r.get(3)?;
        let title: String        = r.get(4)?;
        let path: String         = r.get(5)?;
        let idx: i64             = r.get(6)?;
        let start: i64           = r.get(7)?;
        let end: i64             = r.get(8)?;
        let preview: Option<String> = r.get(9)?;
        Ok((chunk_id, dim, blob, doc_id, title, path, idx, start, end, preview))
    })?.filter_map(|r| r.ok()).filter_map(|(chunk_id, dim, blob, doc_id, title, path, idx, start, end, preview)| {
        let vec = blob_to_f32(&blob, dim as usize)?;
        let score = cosine_similarity(query_vec, &vec);
        let chunk = RetrievedChunk {
            chunk_id, document_id: doc_id, doc_title: title, doc_path: path,
            chunk_idx: idx, start_char: start, end_char: end,
            preview: preview.unwrap_or_default(), full_text: None, score: score as f64,
        };
        Some((score as f64, chunk))
    }).collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored.into_iter().take(top_k).map(|(_, c)| c).collect())
}

/// Store embedding vector for a chunk.
pub fn store_embedding(conn: &Connection, chunk_id: &str, vector: &[f32]) -> AppResult<()> {
    let blob = f32_to_blob(vector);
    conn.execute(
        "INSERT INTO embeddings(chunk_id, dim, vector_blob) VALUES(?1,?2,?3)
         ON CONFLICT(chunk_id) DO UPDATE SET vector_blob=excluded.vector_blob",
        params![chunk_id, vector.len() as i64, blob],
    )?;
    Ok(())
}

// ── Combined retrieval ────────────────────────────────────────────────────

/// Main retrieval entry point used by the agent executor.
/// Logs an audit read event for each retrieved chunk.
/// If `llm` is provided and embeddings are enabled, merges FTS results with
/// cosine-similarity results from the embeddings table.
pub fn retrieve(
    conn: &Connection,
    _crypto: &CryptoContext,
    query: &str,
    workspace_id: Option<&str>,
    top_k: usize,
    llm: Option<&crate::connectors::LlmConfig>,
) -> AppResult<Vec<RetrievedChunk>> {
    let mut results = fts_search(conn, query, top_k * 2)?;

    // Optional embedding-augmented retrieval
    if let Some(llm) = llm {
        if llm.embeddings_enabled {
            if let Ok(query_vec) = crate::connectors::llm_embed(llm, query) {
                let embed_results = embedding_search(conn, &query_vec, top_k)?;
                let existing_ids: std::collections::HashSet<String> =
                    results.iter().map(|c| c.chunk_id.clone()).collect();
                for chunk in embed_results {
                    if !existing_ids.contains(&chunk.chunk_id) {
                        results.push(chunk);
                    }
                }
                results.sort_by(|a, b| b.score.partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal));
            }
        }
    }

    // Filter to workspace if provided — evaluate workspace_id up-front to avoid closure borrow issues
    if let Some(ws_id) = workspace_id {
        let ws_id_owned = ws_id.to_string();
        results = results
            .into_iter()
            .filter(|c| {
                let row_ws: Option<String> = conn.query_row(
                    "SELECT workspace_id FROM documents WHERE id = ?1",
                    params![c.document_id],
                    |r| r.get(0),
                ).ok();
                row_ws.as_deref() == Some(ws_id_owned.as_str())
            })
            .collect();
    }

    results.truncate(top_k);

    // Audit each retrieved chunk
    for chunk in &results {
        audit::log_read(conn, "agent", "chunk", &chunk.chunk_id, workspace_id)?;
    }

    Ok(results)
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn sanitize_fts_query(query: &str) -> String {
    // FTS5 special chars: " * ^ ( ) OR AND NOT
    // Simple approach: wrap each token in double quotes
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| {
            let clean: String = t.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '\'')
                .collect();
            if clean.is_empty() { "".to_string() } else { format!("\"{clean}\"") }
        })
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() { "\"\"".to_string() } else { tokens.join(" OR ") }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn blob_to_f32(blob: &[u8], dim: usize) -> Option<Vec<f32>> {
    if blob.len() != dim * 4 {
        return None;
    }
    Some(blob.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use crate::db::schema::SCHEMA_SQL;
    use crate::crypto::{CryptoContext, generate_salt};
    use crate::ingest;
    use uuid::Uuid;

    fn setup() -> (Connection, CryptoContext) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        let salt = generate_salt();
        let crypto = CryptoContext::derive("test", &salt).unwrap();
        (conn, crypto)
    }

    #[test]
    fn test_fts_returns_results() {
        let (conn, crypto) = setup();
        let ws_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO workspaces(id,name,path,created_at) VALUES(?1,'Test','/tmp','2024-01-01T00:00:00Z')",
            params![ws_id],
        ).unwrap();
        let doc_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO documents(id,workspace_id,path,title,created_at) VALUES(?1,?2,'/tmp/a.md','Alpha','2024-01-01T00:00:00Z')",
            params![doc_id, ws_id],
        ).unwrap();

        let text = "Project Alpha uses Rust and React for local-first development. ".repeat(15);
        ingest::ingest_chunks(&conn, &crypto, &doc_id, "Alpha", &text).unwrap();

        let results = fts_search(&conn, "Rust React", 5).unwrap();
        assert!(!results.is_empty(), "FTS search should return results");
        assert!(results[0].doc_title == "Alpha");
    }

    #[test]
    fn test_no_results_for_unknown_term() {
        let (conn, _) = setup();
        let results = fts_search(&conn, "xyznomatch", 5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0f32, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_blob_roundtrip() {
        let v: Vec<f32> = vec![1.0, 2.0, 3.14, -1.5];
        let blob = f32_to_blob(&v);
        let back = blob_to_f32(&blob, v.len()).unwrap();
        for (a, b) in v.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-7);
        }
    }
}
