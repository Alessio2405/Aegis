//! Scale: External — run an arbitrary shell command inside the agent working directory.
//!
//! Input formats:
//!   Bare: ls -la
//!   JSON: {"cmd": "npm install", "cwd": "/optional/subdir/inside/work_dir"}
//!
//! Security:
//!   - `agent_work_dir` MUST be configured in Settings (refusing otherwise keeps
//!     document workspaces read-only).
//!   - If a `cwd` override is given, it must be inside `agent_work_dir`.
//!   - Hard timeout: 60 seconds.
//!   - Output capped at 8 KB.
//!
//! Platform: uses `cmd /C` on Windows, `sh -c` on Unix/macOS.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use serde_json::Value;
use rusqlite::params;

use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

const TIMEOUT_SECS: u64 = 60;
const MAX_OUTPUT:   usize = 8 * 1024; // 8 KB

pub struct ShellHandler;

impl ToolHandler for ShellHandler {
    fn id(&self) -> &'static str { "shell" }
    fn scale(&self) -> ActionScale { ActionScale::External }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let (cmd, cwd_override) = parse_input(ctx.input.trim());

        if cmd.is_empty() {
            return Err(AppError::InvalidInput(
                "shell: provide a command — bare string or JSON {\"cmd\":\"...\"}".into()
            ));
        }

        // Resolve work dir (requires agent_work_dir to be set)
        let work_dir = require_work_dir(ctx.conn)?;

        // Resolve cwd: optional override must still be inside work_dir
        let cwd = if let Some(ref sub) = cwd_override {
            let candidate = PathBuf::from(sub);
            let resolved = candidate.canonicalize().map_err(|_| {
                AppError::InvalidInput(format!("shell: cwd '{}' does not exist", sub))
            })?;
            let canon_work = work_dir.canonicalize().map_err(AppError::Io)?;
            if !resolved.starts_with(&canon_work) {
                return Err(AppError::InvalidInput(format!(
                    "shell: cwd '{}' is outside agent working directory ({})",
                    sub,
                    work_dir.display()
                )));
            }
            resolved
        } else {
            work_dir.clone()
        };

        // Spawn the shell process
        let start = Instant::now();

        #[cfg(target_os = "windows")]
        let mut child = Command::new("cmd")
            .args(["/C", &cmd])
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(AppError::Io)?;

        #[cfg(not(target_os = "windows"))]
        let mut child = Command::new("sh")
            .args(["-c", &cmd])
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(AppError::Io)?;

        // Watchdog thread — kill after TIMEOUT_SECS
        let child_id = child.id();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(TIMEOUT_SECS));
            kill_process(child_id);
        });

        let output = child.wait_with_output().map_err(AppError::Io)?;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let timed_out  = elapsed_ms >= TIMEOUT_SECS * 1000;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let raw = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
            (false, false) => format!("{}\n[stderr]\n{}", stdout.trim(), stderr.trim()),
            (false, true)  => stdout.trim().to_string(),
            (true,  false) => stderr.trim().to_string(),
            (true,  true)  => format!("[exit {}]", output.status.code().unwrap_or(-1)),
        };

        let body = if timed_out {
            format!("{}\n\n[timed out after {TIMEOUT_SECS}s]", &raw)
        } else if raw.len() > MAX_OUTPUT {
            format!("{}\n\n[... truncated at 8 KB]", &raw[..MAX_OUTPUT])
        } else {
            raw.clone()
        };

        let exit_code = output.status.code().unwrap_or(-1);
        let preview: String = body.chars().take(300).collect();
        let description = format!(
            "$ {} (exit {}, {}ms, cwd: {})",
            cmd.chars().take(60).collect::<String>(),
            exit_code,
            elapsed_ms,
            cwd.display()
        );

        Ok(ToolOutput {
            answer:           Some(body.clone()),
            context_fragment: Some(format!("SHELL $ {cmd}\n```\n{body}\n```")),
            step: AgentRunStep {
                step_type:   "shell".into(),
                description,
                output:      preview,
            },
            extra_steps: vec![],
        })
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn parse_input(raw: &str) -> (String, Option<String>) {
    if raw.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            let cmd = v.get("cmd").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let cwd = v.get("cwd").and_then(|x| x.as_str()).map(str::to_string);
            return (strip_shell_quotes(&cmd), cwd);
        }
    }
    (strip_shell_quotes(raw), None)
}

/// LLMs often wrap commands in backticks or quotes — strip them so `cmd /C` works.
fn strip_shell_quotes(s: &str) -> String {
    let s = s.trim();
    for (open, close) in &[('`', '`'), ('\'', '\''), ('"', '"')] {
        if s.starts_with(*open) && s.ends_with(*close) && s.len() > 1 {
            return s[1..s.len() - 1].trim().to_string();
        }
    }
    s.to_string()
}

/// Return the canonicalized `agent_work_dir`, or an error if it is not configured.
fn require_work_dir(conn: &rusqlite::Connection) -> AppResult<PathBuf> {
    let dir: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key='agent_work_dir'", params![], |r| r.get(0))
        .ok()
        .filter(|s: &String| !s.trim().is_empty());

    match dir {
        Some(d) => {
            let p = PathBuf::from(&d);
            if !p.exists() {
                return Err(AppError::InvalidInput(format!(
                    "shell: agent_work_dir '{}' does not exist. Update it in Settings → Agent Working Directory.",
                    d
                )));
            }
            Ok(p)
        }
        None => Err(AppError::InvalidInput(
            "shell: Agent Working Directory is not configured. \
             Set it in Settings → Agent Working Directory before using the shell tool.".into()
        )),
    }
}

fn kill_process(pid: u32) {
    #[cfg(target_os = "windows")]
    let _ = Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).output();

    #[cfg(not(target_os = "windows"))]
    let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
}
