//! Scale: External — writes content to a file within a workspace directory.
//!
//! Input format (from the LLM in ReAct mode, or from ctx.input in pipeline mode):
//!   Line 1: file path (absolute or relative to a workspace root)
//!   Lines 2+: file content to write
//!
//! Security:
//!   - Path must be inside a workspace directory known to the DB.
//!   - Only whitelisted extensions are allowed.
//!   - Requires VaultWrite (file.write) capability.

use std::path::{Path, PathBuf};
use rusqlite::params;
use serde_json::Value;

use crate::permissions::{self, Capability};
use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

const ALLOWED_EXTENSIONS: &[&str] = &[
    "md", "txt", "py", "js", "ts", "json", "csv", "toml", "yaml", "yml", "rs", "sh",
];

pub struct WriteFileHandler;

impl ToolHandler for WriteFileHandler {
    fn id(&self) -> &'static str { "write_file" }
    fn scale(&self) -> ActionScale { ActionScale::External }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        permissions::require(
            ctx.conn, &Capability::VaultWrite, ctx.workspace_id,
            "agent", Some("file"), None,
        )?;

        let raw = ctx.input.trim();

        // Accept three formats:
        // 1. JSON:  {"path": "file.md", "content": "hello"}
        // 2. path-prefix: path: file.md\n---\ncontent
        // 3. bare:  file.md\ncontent (first line = path, rest = content)
        let parsed = parse_write_input(raw);
        let (path_str, content_owned) = match parsed {
            Some(pair) => pair,
            None => {
                return Ok(ToolOutput {
                    answer:           None,
                    context_fragment: None,
                    step: AgentRunStep {
                        step_type:   "write_file".into(),
                        description: "Missing path or content. Use JSON: {\"path\":\"file.md\",\"content\":\"text\"}".into(),
                        output:      String::new(),
                    },
                    extra_steps: vec![],
                });
            }
        };
        let path_str: &str = &path_str;
        let content: &str = &content_owned;

        if path_str.is_empty() {
            return Err(AppError::InvalidInput("write_file: file path cannot be empty".into()));
        }

        // Resolve relative paths against agent_work_dir
        let path = resolve_write_path(ctx.conn, path_str);

        // Validate extension
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ALLOWED_EXTENSIONS.contains(&ext) {
            return Err(AppError::InvalidInput(format!(
                "write_file: .{ext} not allowed. Allowed: {}",
                ALLOWED_EXTENSIONS.join(", ")
            )));
        }

        // Security: path must be inside a known workspace directory
        guard_workspace_path(ctx.conn, &path)?;

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(AppError::Io)?;
            }
        }

        std::fs::write(&path, content.as_bytes()).map_err(AppError::Io)?;

        let display = path.display().to_string();
        let byte_count = content.len();

        Ok(ToolOutput {
            answer:           Some(format!("Written {} bytes to {display}", byte_count)),
            context_fragment: Some(format!("FILE WRITTEN: {display} ({byte_count} bytes)")),
            step: AgentRunStep {
                step_type:   "write_file".into(),
                description: format!("Wrote {byte_count} bytes to {display}"),
                output:      display,
            },
            extra_steps: vec![],
        })
    }
}

// ── Input parser ──────────────────────────────────────────────────────────

/// Returns `(path, content)` or `None` if the input is malformed.
fn parse_write_input(raw: &str) -> Option<(String, String)> {
    // Format 1: JSON object with "path" and "content" keys
    if raw.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            let path = v.get("path").and_then(|p| p.as_str()).map(strip_path_quotes);
            let content = v.get("content").and_then(|c| c.as_str()).map(str::to_string);
            if let (Some(p), Some(c)) = (path, content) {
                if !p.is_empty() {
                    return Some((p, c));
                }
            }
        }
    }

    // Format 2 & 3: need a newline separating path from content
    let (first, rest) = raw.split_once('\n')?;
    let first = first.trim();

    // Format 2: "path: <p>\n[---\n]<content>"
    if let Some(after) = first.strip_prefix("path:") {
        let p = strip_path_quotes(after.trim());
        let body = if let Some((_sep, c)) = rest.split_once("---") {
            c.trim_start_matches('\n').to_string()
        } else {
            rest.to_string()
        };
        if !p.is_empty() { return Some((p, body)); }
    }

    // Format 3: bare — first line is path
    if !first.is_empty() {
        return Some((strip_path_quotes(first), rest.to_string()));
    }

    None
}

fn strip_path_quotes(s: &str) -> String {
    let s = s.trim();
    for q in &['"', '\'', '`'] {
        if s.starts_with(*q) && s.ends_with(*q) && s.len() > 1 {
            return s[1..s.len() - 1].trim().to_string();
        }
    }
    s.to_string()
}

// ── Path resolution ───────────────────────────────────────────────────────

/// Resolve a relative path against agent_work_dir (same logic as read_file).
fn resolve_write_path(conn: &rusqlite::Connection, path_str: &str) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() { return p; }
    let work_dir: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key='agent_work_dir'", params![], |r| r.get(0))
        .ok()
        .filter(|s: &String| !s.trim().is_empty());
    if let Some(dir) = work_dir {
        return PathBuf::from(dir).join(&p);
    }
    p
}

// ── Security guard ────────────────────────────────────────────────────────

/// Ensure `path` (resolved or its parent) is inside the agent work dir or a workspace directory.
fn guard_workspace_path(conn: &rusqlite::Connection, path: &Path) -> AppResult<()> {
    // Resolve: if file doesn't exist yet, canonicalize its parent
    let resolved = if path.exists() {
        path.canonicalize().map_err(AppError::Io)?
    } else if let Some(parent) = path.parent() {
        let canon_parent = if parent.as_os_str().is_empty() {
            std::env::current_dir().map_err(AppError::Io)?
        } else {
            parent.canonicalize().map_err(|_| {
                AppError::InvalidInput(format!(
                    "write_file: parent directory '{}' does not exist",
                    parent.display()
                ))
            })?
        };
        canon_parent.join(path.file_name().unwrap_or_default())
    } else {
        return Err(AppError::InvalidInput("write_file: invalid path".into()));
    };

    // Check agent_work_dir first (highest priority)
    let work_dir: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key='agent_work_dir'", params![], |r| r.get(0))
        .ok()
        .filter(|s: &String| !s.trim().is_empty());

    if let Some(ref dir) = work_dir {
        let work_path = PathBuf::from(dir);
        if let Ok(canon_work) = work_path.canonicalize() {
            if resolved.starts_with(&canon_work) {
                return Ok(());
            }
        }
        // agent_work_dir is set but path is outside it — also check workspace directories
    }

    // Check workspace directories
    let mut stmt = conn.prepare("SELECT path FROM workspaces")
        .map_err(AppError::Database)?;
    let ws_paths: Vec<String> = stmt
        .query_map(params![], |r: &rusqlite::Row| r.get::<_, String>(0))
        .map_err(AppError::Database)?
        .filter_map(|r| r.ok())
        .collect();

    for ws_path_str in &ws_paths {
        let ws = PathBuf::from(ws_path_str);
        if let Ok(canon_ws) = ws.canonicalize() {
            if resolved.starts_with(&canon_ws) {
                return Ok(());
            }
        }
    }

    let valid = ws_paths
        .iter()
        .map(|p| format!("  • {p}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(AppError::InvalidInput(format!(
        "write_file: '{}' is outside all allowed directories.\n\
         Allowed: agent working directory + any configured workspaces:\n{}",
        resolved.display(),
        if valid.is_empty() { "  (no workspaces configured)".into() } else { valid }
    )))
}
