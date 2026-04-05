//! Scale: External — outbound HTTP fetch, strips HTML tags.

use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct FetchUrlHandler;

impl ToolHandler for FetchUrlHandler {
    fn id(&self) -> &'static str { "fetch_url" }
    fn scale(&self) -> ActionScale { ActionScale::External }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let urls = extract_urls(ctx.input);

        if urls.is_empty() {
            return Ok(ToolOutput {
                answer:           None,
                context_fragment: None,
                step: AgentRunStep {
                    step_type:   "fetch_url".into(),
                    description: "No URLs found in input".into(),
                    output:      String::new(),
                },
                extra_steps: vec![],
            });
        }

        let mut parts: Vec<String> = Vec::new();
        for url in &urls {
            match fetch_and_strip(url) {
                Ok(text) => {
                    let preview: String = text.chars().take(1500).collect();
                    parts.push(format!("=== {} ===\n{preview}", url));
                }
                Err(e) => parts.push(format!("=== {} ===\n[fetch failed: {}]", url, e)),
            }
        }
        let combined = parts.join("\n\n");

        Ok(ToolOutput {
            answer:           None,
            context_fragment: Some(format!("CONTEXT FROM WEB:\n{combined}")),
            step: AgentRunStep {
                step_type:   "fetch_url".into(),
                description: format!("Fetched {} URL(s)", urls.len()),
                output:      combined.chars().take(500).collect(),
            },
            extra_steps: vec![],
        })
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn extract_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
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

fn fetch_and_strip(url: &str) -> AppResult<String> {
    let body = reqwest::blocking::get(url)
        .map_err(|e| AppError::Connector(e.to_string()))?
        .text()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let mut out = String::with_capacity(body.len());
    let mut in_tag = false;
    for ch in body.chars() {
        match ch {
            '<' => { in_tag = true; }
            '>' => { in_tag = false; out.push(' '); }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    Ok(out.split_whitespace().collect::<Vec<_>>().join(" "))
}
