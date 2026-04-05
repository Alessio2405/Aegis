//! Scale: Retrieval — reads file content from within the agent work dir or a workspace.
//!
//! Input formats:
//!   JSON:  {"path": "/work/dir/notes.md"}
//!   Bare:  /path/to/file.md
//!
//! Security: same path rules as write_file — must be inside agent_work_dir or a workspace.
//! Size limit: 50 KB to avoid flooding the LLM context window.

use std::path::{Path, PathBuf};
use rusqlite::params;
use serde_json::Value;

use crate::permissions::{self, Capability};
use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

const MAX_BYTES: usize = 50 * 1024; // 50 KB

pub struct ReadFileHandler;

impl ToolHandler for ReadFileHandler {
    fn id(&self) -> &'static str { "read_file" }
    fn scale(&self) -> ActionScale { ActionScale::Retrieval }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        permissions::require(
            ctx.conn, &Capability::VaultWrite, ctx.workspace_id,
            "agent", Some("file"), None,
        )?;

        let path_str = parse_path(ctx.input.trim());
        if path_str.is_empty() {
            return Err(AppError::InvalidInput(
                "read_file: provide a path — JSON {\"path\":\"...\"} or bare path".into(),
            ));
        }

        let path = resolve_path(ctx.conn, &path_str);
        guard_readable_path(ctx.conn, &path)?;

        if path.is_dir() {
            // List directory entries instead of erroring
            let entries: Vec<String> = std::fs::read_dir(&path)
                .map_err(AppError::Io)?
                .filter_map(|e| e.ok())
                .map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if e.path().is_dir() { format!("{name}/") } else { name }
                })
                .collect();
            let listing = entries.join("\n");
            return Ok(ToolOutput {
                answer:           None,
                context_fragment: Some(format!("DIRECTORY: {path_str}\n{listing}")),
                step: AgentRunStep {
                    step_type:   "read_file".into(),
                    description: format!("Listed directory {path_str}"),
                    output:      path_str,
                },
                extra_steps: vec![],
            });
        }

        let bytes = std::fs::read(&path).map_err(AppError::Io)?;
        let total_bytes = bytes.len();
        let (content, truncated) = if total_bytes > MAX_BYTES {
            let text = String::from_utf8_lossy(&bytes[..MAX_BYTES]).into_owned();
            (format!("{text}\n\n[... truncated — file is {total_bytes} bytes, showing first {MAX_BYTES}]"), true)
        } else {
            (String::from_utf8_lossy(&bytes).into_owned(), false)
        };

        let display = path.display().to_string();

        Ok(ToolOutput {
            answer:           None,
            context_fragment: Some(format!(
                "FILE: {display} ({total_bytes} bytes{})\n```\n{content}\n```",
                if truncated { ", truncated" } else { "" }
            )),
            step: AgentRunStep {
                step_type:   "read_file".into(),
                description: format!("Read {total_bytes} bytes from {display}"),
                output:      display,
            },
            extra_steps: vec![],
        })
    }
}

fn parse_path(raw: &str) -> String {
    if raw.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            if let Some(p) = v.get("path").and_then(|x| x.as_str()) {
                return strip_path_quotes(p);
            }
        }
    }
    strip_path_quotes(raw.lines().next().unwrap_or("").trim())
}

/// LLMs often wrap paths in quotes — strip them.
fn strip_path_quotes(s: &str) -> String {
    let s = s.trim();
    for q in &['"', '\'', '`'] {
        if s.starts_with(*q) && s.ends_with(*q) && s.len() > 1 {
            return s[1..s.len() - 1].trim().to_string();
        }
    }
    // Also handle JSON-escaped paths like \"path\\to\\file\"
    if s.starts_with("\\\"") && s.ends_with("\\\"") {
        return s[2..s.len() - 2].replace("\\\\", "\\").replace("\\/", "/");
    }
    s.to_string()
}

/// If `path_str` is relative, resolve it against `agent_work_dir` (if set).
fn resolve_path(conn: &rusqlite::Connection, path_str: &str) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() {
        return p;
    }
    // Try to resolve relative to agent_work_dir
    let work_dir: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key='agent_work_dir'", params![], |r| r.get(0))
        .ok()
        .filter(|s: &String| !s.trim().is_empty());
    if let Some(dir) = work_dir {
        return PathBuf::from(dir).join(&p);
    }
    p
}

fn guard_readable_path(conn: &rusqlite::Connection, path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::InvalidInput(format!(
            "read_file: '{}' does not exist", path.display()
        )));
    }

    let resolved = path.canonicalize().map_err(AppError::Io)?;

    // Check agent_work_dir first
    let work_dir: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key='agent_work_dir'", params![], |r| r.get(0))
        .ok()
        .filter(|s: &String| !s.trim().is_empty());

    if let Some(ref dir) = work_dir {
        if let Ok(canon) = PathBuf::from(dir).canonicalize() {
            if resolved.starts_with(&canon) { return Ok(()); }
        }
        // agent_work_dir is set but path is outside — also check workspace directories
    }

    // Check workspace directories
    let mut stmt = conn.prepare("SELECT path FROM workspaces").map_err(AppError::Database)?;
    let ws_paths: Vec<String> = stmt
        .query_map(params![], |r| r.get::<_, String>(0))
        .map_err(AppError::Database)?
        .filter_map(|r| r.ok())
        .collect();

    for ws in &ws_paths {
        if let Ok(canon_ws) = PathBuf::from(ws).canonicalize() {
            if resolved.starts_with(&canon_ws) { return Ok(()); }
        }
    }

    let valid = ws_paths.iter().map(|p| format!("  • {p}")).collect::<Vec<_>>().join("\n");
    Err(AppError::InvalidInput(format!(
        "read_file: '{}' is outside all allowed directories.\n\
         Allowed: agent working directory + any configured workspaces:\n{}",
        resolved.display(),
        if valid.is_empty() { "  (no workspaces configured)".into() } else { valid }
    )))
}
