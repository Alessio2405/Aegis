//! Scale: Retrieval — query a web search API and inject results as context.
//!
//! Supported providers (configured in Settings → Search):
//!   "brave"   — Brave Search API (requires api key)
//!   "searxng" — Self-hosted SearXNG instance (requires base URL)
//!
//! Input: a plain search query string.
//! Output: up to 5 results formatted as a context fragment for the LLM.

use rusqlite::params;
use serde::Deserialize;
use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct WebSearchHandler;

impl ToolHandler for WebSearchHandler {
    fn id(&self) -> &'static str { "web_search" }
    fn scale(&self) -> ActionScale { ActionScale::Retrieval }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let query = ctx.input.trim();
        if query.is_empty() {
            return Ok(ToolOutput {
                answer:           None,
                context_fragment: None,
                step: AgentRunStep {
                    step_type:   "web_search".into(),
                    description: "No query provided".into(),
                    output:      String::new(),
                },
                extra_steps: vec![],
            });
        }

        let provider = ctx.conn.query_row(
            "SELECT value FROM settings WHERE key = 'search_provider'",
            params![],
            |r| r.get::<_, String>(0),
        ).unwrap_or_else(|_| "brave".into());

        let results_text = match provider.as_str() {
            "searxng" => {
                let base_url = ctx.conn.query_row(
                    "SELECT value FROM settings WHERE key = 'searxng_url'",
                    params![],
                    |r| r.get::<_, String>(0),
                ).unwrap_or_default();
                if base_url.is_empty() {
                    return Err(AppError::Connector(
                        "web_search: SearXNG URL not set. Configure it in Settings → Search.".into()
                    ));
                }
                search_searxng(&base_url, query)?
            }
            _ => {
                // Default: Brave
                let api_key = ctx.conn.query_row(
                    "SELECT value FROM settings WHERE key = 'brave_api_key'",
                    params![],
                    |r| r.get::<_, String>(0),
                ).unwrap_or_default();
                if api_key.is_empty() {
                    return Err(AppError::Connector(
                        "web_search: Brave API key not set. Configure it in Settings → Search.".into()
                    ));
                }
                search_brave(&api_key, query)?
            }
        };

        let preview: String = results_text.chars().take(300).collect();

        Ok(ToolOutput {
            answer:           None,
            context_fragment: Some(results_text.clone()),
            step: AgentRunStep {
                step_type:   "web_search".into(),
                description: format!("Web search: {}", query.chars().take(80).collect::<String>()),
                output:      preview,
            },
            extra_steps: vec![],
        })
    }
}

// ── Brave Search ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BraveResponse {
    web: Option<BraveWeb>,
}

#[derive(Deserialize)]
struct BraveWeb {
    results: Vec<BraveResult>,
}

#[derive(Deserialize)]
struct BraveResult {
    title:       String,
    url:         String,
    description: Option<String>,
}

fn search_brave(api_key: &str, query: &str) -> AppResult<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let resp = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .query(&[("q", query), ("count", "5")])
        .header("Accept", "application/json")
        .header("X-Subscription-Token", api_key)
        .send()
        .map_err(|e| AppError::Connector(format!("Brave search request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(AppError::Connector(format!("Brave search returned HTTP {status}")));
    }

    let data: BraveResponse = resp.json()
        .map_err(|e| AppError::Connector(format!("Brave search parse error: {e}")))?;

    let results = data.web.map(|w| w.results).unwrap_or_default();
    Ok(format_results(&results.iter().map(|r| SearchResult {
        title: r.title.clone(),
        url:   r.url.clone(),
        snippet: r.description.clone().unwrap_or_default(),
    }).collect::<Vec<_>>()))
}

// ── SearXNG ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearxngResponse {
    results: Vec<SearxngResult>,
}

#[derive(Deserialize)]
struct SearxngResult {
    title:   String,
    url:     String,
    content: Option<String>,
}

fn search_searxng(base_url: &str, query: &str) -> AppResult<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let search_url = format!("{}/search", base_url.trim_end_matches('/'));

    let resp = client
        .get(&search_url)
        .query(&[("q", query), ("format", "json"), ("categories", "general")])
        .header("Accept", "application/json")
        .send()
        .map_err(|e| AppError::Connector(format!("SearXNG request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(AppError::Connector(format!("SearXNG returned HTTP {status}")));
    }

    let data: SearxngResponse = resp.json()
        .map_err(|e| AppError::Connector(format!("SearXNG parse error: {e}")))?;

    let top5 = data.results.into_iter().take(5);
    Ok(format_results(&top5.map(|r| SearchResult {
        title:   r.title,
        url:     r.url,
        snippet: r.content.unwrap_or_default(),
    }).collect::<Vec<_>>()))
}

// ── Shared formatting ──────────────────────────────────────────────────────

struct SearchResult {
    title:   String,
    url:     String,
    snippet: String,
}

fn format_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "No web search results found.".into();
    }
    results.iter().enumerate().map(|(i, r)| {
        let snippet: String = r.snippet.chars().take(300).collect();
        format!("[{}] {}\n{}\n{}", i + 1, r.title, r.url, snippet)
    }).collect::<Vec<_>>().join("\n\n")
}
