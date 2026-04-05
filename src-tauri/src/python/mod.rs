//! Python interpreter module — run Python code snippets in a sandboxed subprocess.
//!
//! Uses the system Python (`python3` or `python`) via `std::process::Command`.
//! Code is written to a temporary file, executed, and the file is cleaned up.
//! A watchdog thread enforces a configurable timeout.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

// ── Public types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonResult {
    pub stdout:      String,
    pub stderr:      String,
    pub exit_code:   i32,
    pub duration_ms: u64,
    /// true when the process was killed due to timeout
    pub timed_out:   bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonInfo {
    pub version:   String,
    pub path:      String,
    pub available: bool,
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Detect the system Python binary and its version string.
/// Returns `PythonInfo { available: false }` if Python is not installed.
pub fn get_info() -> PythonInfo {
    if let Some((bin, ver)) = detect_python() {
        PythonInfo { version: ver, path: bin, available: true }
    } else {
        PythonInfo { version: String::new(), path: String::new(), available: false }
    }
}

/// Execute `code` as a Python script.
///
/// - Writes the code to a temp `.py` file.
/// - Runs `python3`/`python` with that file.
/// - Kills the process after `timeout_secs` if it hasn't finished.
/// - Captures stdout + stderr; deletes the temp file unconditionally.
pub fn run_code(code: &str, timeout_secs: u64) -> AppResult<PythonResult> {
    let python_bin = detect_python()
        .map(|(bin, _)| bin)
        .ok_or_else(|| AppError::InvalidInput(
            "Python is not installed or not found on PATH.".into()
        ))?;

    // Write code to a temp file
    let tmp_path = temp_script_path();
    std::fs::write(&tmp_path, code.as_bytes()).map_err(AppError::Io)?;

    let start = Instant::now();

    let child = Command::new(&python_bin)
        .arg(&tmp_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Run in the temp dir so relative file access is contained
        .current_dir(std::env::temp_dir())
        .spawn()
        .map_err(AppError::Io)?;

    let child_id = child.id();
    let timeout_dur = Duration::from_secs(timeout_secs);

    // Watchdog: kill the child after timeout_secs
    let watchdog = std::thread::spawn(move || {
        std::thread::sleep(timeout_dur);
        kill_process(child_id);
    });

    let output = child.wait_with_output().map_err(AppError::Io)?;
    let duration_ms = start.elapsed().as_millis() as u64;

    // Watchdog is no longer needed; it will finish sleeping naturally.
    drop(watchdog);

    // Clean up temp file (best-effort)
    let _ = std::fs::remove_file(&tmp_path);

    let timed_out = duration_ms >= timeout_secs * 1000;

    Ok(PythonResult {
        stdout:      String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr:      String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code:   output.status.code().unwrap_or(-1),
        duration_ms,
        timed_out,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Try `python3` then `python`; return `(binary_path, version_string)` or None.
fn detect_python() -> Option<(String, String)> {
    for candidate in &["python3", "python"] {
        if let Ok(out) = Command::new(candidate).arg("--version").output() {
            if out.status.success() {
                let raw = String::from_utf8_lossy(&out.stdout).into_owned();
                // Python 3.x prints to stdout; older versions print to stderr
                let ver = if raw.trim().is_empty() {
                    String::from_utf8_lossy(&out.stderr).trim().to_string()
                } else {
                    raw.trim().to_string()
                };
                return Some((candidate.to_string(), ver));
            }
        }
    }
    None
}

/// Build a unique temp file path for the script.
fn temp_script_path() -> PathBuf {
    let name = format!(
        "agent_os_{}.py",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    );
    std::env::temp_dir().join(name)
}

/// Kill a process by PID — platform-specific.
fn kill_process(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}
