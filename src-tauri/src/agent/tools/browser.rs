//! Scale: External — headless browser fetch with full JavaScript rendering.
//!
//! Launches Chrome/Chromium (or Edge) with `--headless --dump-dom` to get the
//! rendered DOM of each URL in the agent input. Unlike `fetch_url`, this executes
//! JavaScript so it works on SPAs and dynamic pages.
//!
//! Chrome discovery order:
//!   1. `CHROME_PATH` environment variable
//!   2. Common Windows install paths (Chrome, then Edge)
//!   3. `chrome` / `chromium` / `msedge` on PATH

use std::path::PathBuf;
use std::process::Command;

use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct BrowserHandler;

impl ToolHandler for BrowserHandler {
    fn id(&self) -> &'static str { "browser" }
    fn scale(&self) -> ActionScale { ActionScale::External }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let urls = extract_urls(ctx.input);

        if urls.is_empty() {
            return Ok(ToolOutput {
                answer:           None,
                context_fragment: None,
                step: AgentRunStep {
                    step_type:   "browser".into(),
                    description: "No URLs found in input".into(),
                    output:      String::new(),
                },
                extra_steps: vec![],
            });
        }

        let chrome = find_chrome().ok_or_else(|| AppError::Connector(
            "Chrome/Chromium/Edge not found. \
             Install Chrome or set the CHROME_PATH environment variable.".into()
        ))?;

        let mut parts: Vec<String> = Vec::new();
        for url in &urls {
            match headless_fetch(&chrome, url) {
                Ok(text) => {
                    // Browser renders more content than a plain HTTP GET, so give it more room
                    let preview: String = text.chars().take(3000).collect();
                    parts.push(format!("=== {} ===\n{preview}", url));
                }
                Err(e) => parts.push(format!("=== {} ===\n[browser error: {}]", url, e)),
            }
        }
        let combined = parts.join("\n\n");

        Ok(ToolOutput {
            answer:           None,
            context_fragment: Some(format!("CONTEXT FROM BROWSER:\n{combined}")),
            step: AgentRunStep {
                step_type:   "browser".into(),
                description: format!(
                    "Browser-fetched {} URL(s) via {} (JS-rendered)",
                    urls.len(),
                    chrome.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("chrome"),
                ),
                output: combined.chars().take(500).collect(),
            },
            extra_steps: vec![],
        })
    }
}

// ── Chrome discovery ──────────────────────────────────────────────────────

fn find_chrome() -> Option<PathBuf> {
    // 1. Explicit env override
    if let Ok(p) = std::env::var("CHROME_PATH") {
        let path = PathBuf::from(&p);
        if path.exists() { return Some(path); }
    }

    // 2. Known Windows install paths (Chrome before Edge)
    let windows_candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files\Chromium\Application\chrome.exe",
        r"C:\Program Files (x86)\Chromium\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ];
    for candidate in &windows_candidates {
        let path = PathBuf::from(candidate);
        if path.exists() { return Some(path); }
    }

    // 3. macOS / Linux locations
    let unix_candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];
    for candidate in &unix_candidates {
        let path = PathBuf::from(candidate);
        if path.exists() { return Some(path); }
    }

    // 4. PATH lookup (Windows: `where`, Unix: `which`)
    for name in &["chrome", "google-chrome", "chromium", "chromium-browser", "msedge"] {
        let result = if cfg!(target_os = "windows") {
            Command::new("where").arg(name).output()
        } else {
            Command::new("which").arg(name).output()
        };
        if let Ok(out) = result {
            if out.status.success() {
                let path_str = String::from_utf8_lossy(&out.stdout);
                let first = path_str.lines().next().unwrap_or("").trim();
                if !first.is_empty() {
                    return Some(PathBuf::from(first));
                }
            }
        }
    }

    None
}

// ── Headless fetch ────────────────────────────────────────────────────────

fn headless_fetch(chrome: &PathBuf, url: &str) -> AppResult<String> {
    let output = Command::new(chrome)
        .args([
            "--headless",
            "--disable-gpu",
            "--no-sandbox",
            "--disable-extensions",
            "--disable-dev-shm-usage",
            "--dump-dom",
            url,
        ])
        .output()
        .map_err(|e| AppError::Connector(format!("Browser launch failed: {e}")))?;

    // --dump-dom writes rendered DOM to stdout even on non-zero exit
    if output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Connector(format!(
            "Browser returned empty output. stderr: {}",
            stderr.chars().take(200).collect::<String>()
        )));
    }

    let html = String::from_utf8_lossy(&output.stdout);
    Ok(strip_html(&html))
}

// ── HTML stripping ────────────────────────────────────────────────────────

/// Remove `<script>` and `<style>` blocks, then strip all remaining tags.
fn strip_html(html: &str) -> String {
    let cleaned = remove_blocks(html, "script");
    let cleaned = remove_blocks(&cleaned, "style");

    let mut out = String::with_capacity(cleaned.len());
    let mut in_tag = false;
    for ch in cleaned.chars() {
        match ch {
            '<' => { in_tag = true; }
            '>' => { in_tag = false; out.push(' '); }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Remove all `<tag>...</tag>` blocks (case-insensitive, handles multiples).
fn remove_blocks(html: &str, tag: &str) -> String {
    let open_pat  = format!("<{}", tag);
    let close_pat = format!("</{}>", tag);
    let mut result = html.to_string();

    loop {
        let lower  = result.to_lowercase();
        let open_l = open_pat.to_lowercase();
        let close_l = close_pat.to_lowercase();

        if let Some(start) = lower.find(&open_l) {
            if let Some(rel_end) = lower[start..].find(&close_l) {
                let close_start = start + rel_end;
                let after_close = &lower[close_start + close_l.len()..];
                let gt_offset   = after_close.find('>').map(|p| p + 1).unwrap_or(0);
                let cut_end     = close_start + close_l.len() + gt_offset;
                result = format!("{} {}", &result[..start], &result[cut_end..]);
            } else {
                // No closing tag — drop everything from the open tag onward
                result.truncate(start);
                break;
            }
        } else {
            break;
        }
    }
    result
}

// ── URL extraction ────────────────────────────────────────────────────────

fn extract_urls(text: &str) -> Vec<String> {
    let mut urls  = Vec::new();
    let mut start = 0;
    while let Some(pos) = text[start..].find("http") {
        let abs = start + pos;
        if text[abs..].starts_with("http://") || text[abs..].starts_with("https://") {
            let end = text[abs..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '>')
                .map(|e| abs + e)
                .unwrap_or(text.len());
            let url = text[abs..end].to_string();
            if !url.is_empty() { urls.push(url); }
            start = end;
        } else {
            start = abs + 4;
        }
    }
    urls
}
