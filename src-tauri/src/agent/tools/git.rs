//! Scale: External — run git commands inside the agent working directory.
//!
//! Input formats:
//!   JSON: {"action": "status", "repo": "/path/to/repo"}
//!   JSON: {"action": "commit", "repo": "/path", "message": "my commit"}
//!   JSON: {"action": "add", "repo": "/path", "files": ["file.md"]}
//!   JSON: {"action": "clone", "repo": "/dest", "url": "https://..."}
//!   Bare: status /path/to/repo
//!
//! Allowed actions: status | diff | log | add | commit | pull | push | clone | show
//!
//! Security: repo path must be inside agent_work_dir or a known workspace.
//! Output capped at 4 KB to avoid flooding the LLM context window.

use std::path::{Path, PathBuf};
use std::process::Command;
use serde_json::Value;
use rusqlite::params;

use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

const MAX_OUTPUT: usize = 4 * 1024; // 4 KB

const ALLOWED_ACTIONS: &[&str] = &[
    "status", "diff", "log", "add", "commit", "pull", "push", "clone", "show",
];

pub struct GitHandler;

impl ToolHandler for GitHandler {
    fn id(&self) -> &'static str { "git" }
    fn scale(&self) -> ActionScale { ActionScale::External }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let args = parse_input(ctx.input.trim())?;

        if !ALLOWED_ACTIONS.contains(&args.action.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "git: unknown action '{}'. Allowed: {}",
                args.action,
                ALLOWED_ACTIONS.join(", ")
            )));
        }

        let cwd = resolve_cwd(ctx.conn, args.repo.as_deref())?;

        // Build argument list
        let mut cmd_args: Vec<String> = vec![args.action.clone()];

        match args.action.as_str() {
            "log"    => { cmd_args.push("--oneline".into()); cmd_args.push("-20".into()); }
            "diff"   => { cmd_args.push("HEAD".into()); }
            "add"    => {
                let files = args.files.unwrap_or_else(|| vec![".".into()]);
                cmd_args.extend(files);
            }
            "commit" => {
                let msg = args.message.ok_or_else(|| {
                    AppError::InvalidInput("git commit: 'message' is required".into())
                })?;
                cmd_args.extend(["-m".into(), msg]);
            }
            "clone"  => {
                let url = args.url.ok_or_else(|| {
                    AppError::InvalidInput("git clone: 'url' is required".into())
                })?;
                cmd_args.push(url);
                // Optionally clone into a specific subdirectory
                // (by default, git clones into a subdirectory named after the repo)
            }
            _ => {}
        }

        let result = Command::new("git")
            .args(&cmd_args)
            .current_dir(&cwd)
            .output()
            .map_err(|e| AppError::InvalidInput(format!("git: could not run git: {e}")))?;

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);

        let raw = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
            (false, false) => format!("{}\n[stderr]\n{}", stdout.trim(), stderr.trim()),
            (false, true)  => stdout.trim().to_string(),
            (true,  false) => stderr.trim().to_string(),
            (true,  true)  => format!("[exit {}]", result.status.code().unwrap_or(-1)),
        };

        let output = if raw.len() > MAX_OUTPUT {
            format!("{}\n\n[... truncated at 4 KB]", &raw[..MAX_OUTPUT])
        } else {
            raw
        };

        let status_word = if result.status.success() { "ok" } else { "failed" };
        let description = format!("git {} in {} ({})", args.action, cwd.display(), status_word);

        Ok(ToolOutput {
            answer:           Some(output.clone()),
            context_fragment: Some(format!(
                "GIT {}:\n```\n{output}\n```",
                args.action.to_uppercase()
            )),
            step: AgentRunStep {
                step_type:   "git".into(),
                description,
                output:      output.chars().take(300).collect(),
            },
            extra_steps: vec![],
        })
    }
}

// ── Input parsing ────────────────────────────────────────────────────────────

struct GitArgs {
    action:  String,
    repo:    Option<String>,
    message: Option<String>,
    files:   Option<Vec<String>>,
    url:     Option<String>,
}

fn parse_input(raw: &str) -> AppResult<GitArgs> {
    if raw.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            let action = v.get("action")
                .and_then(|x| x.as_str())
                .ok_or_else(|| AppError::InvalidInput("git: 'action' is required in JSON input".into()))?
                .to_string();
            let repo    = v.get("repo").and_then(|x| x.as_str()).map(str::to_string);
            let message = v.get("message").and_then(|x| x.as_str()).map(str::to_string);
            let url     = v.get("url").and_then(|x| x.as_str()).map(str::to_string);
            let files   = v.get("files").and_then(|x| x.as_array()).map(|arr| {
                arr.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()
            });
            return Ok(GitArgs { action, repo, message, files, url });
        }
    }

    // Bare: "action [/optional/path]"
    let (action, rest) = raw.split_once(' ')
        .map(|(a, b)| (a.trim().to_string(), Some(b.trim().to_string())))
        .unwrap_or_else(|| (raw.trim().to_string(), None));

    if action.is_empty() {
        return Err(AppError::InvalidInput(
            "git: input must be JSON or bare: <action> [/repo/path]".into(),
        ));
    }

    let repo = rest.filter(|s| !s.is_empty());
    Ok(GitArgs { action, repo, message: None, files: None, url: None })
}

// ── Path resolution & security guard ────────────────────────────────────────

/// Resolve the git working directory. Falls back to agent_work_dir, then first workspace.
fn resolve_cwd(conn: &rusqlite::Connection, repo: Option<&str>) -> AppResult<PathBuf> {
    let candidate = if let Some(r) = repo {
        PathBuf::from(r)
    } else {
        let work_dir: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key='agent_work_dir'", params![], |r| r.get(0))
            .ok()
            .filter(|s: &String| !s.trim().is_empty());

        if let Some(d) = work_dir {
            PathBuf::from(d)
        } else {
            conn.query_row("SELECT path FROM workspaces LIMIT 1", params![], |r| r.get::<_, String>(0))
                .map(PathBuf::from)
                .map_err(|_| AppError::InvalidInput(
                    "git: no repo path and no Agent Working Directory configured. \
                     Provide 'repo' in input or set Agent Working Directory in Settings.".into()
                ))?
        }
    };

    if !candidate.exists() {
        return Err(AppError::InvalidInput(format!(
            "git: '{}' does not exist", candidate.display()
        )));
    }

    let resolved = candidate.canonicalize().map_err(AppError::Io)?;
    guard_path(conn, &resolved)?;
    Ok(resolved)
}

fn guard_path(conn: &rusqlite::Connection, path: &Path) -> AppResult<()> {
    let work_dir: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key='agent_work_dir'", params![], |r| r.get(0))
        .ok()
        .filter(|s: &String| !s.trim().is_empty());

    if let Some(ref dir) = work_dir {
        if let Ok(canon) = PathBuf::from(dir).canonicalize() {
            if path.starts_with(&canon) { return Ok(()); }
        }
        return Err(AppError::InvalidInput(format!(
            "git: '{}' is outside the agent working directory ({})",
            path.display(), dir
        )));
    }

    let mut stmt = conn.prepare("SELECT path FROM workspaces").map_err(AppError::Database)?;
    let ws_paths: Vec<String> = stmt
        .query_map(params![], |r| r.get::<_, String>(0))
        .map_err(AppError::Database)?
        .filter_map(|r| r.ok())
        .collect();

    for ws in &ws_paths {
        if let Ok(canon) = PathBuf::from(ws).canonicalize() {
            if path.starts_with(&canon) { return Ok(()); }
        }
    }

    Err(AppError::InvalidInput(format!(
        "git: '{}' is outside all allowed directories", path.display()
    )))
}
