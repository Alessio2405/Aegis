//! Tool handler registry — MyceliumCortex-style typed action dispatch.
//!
//! Each built-in tool implements [`ToolHandler`]. The [`HandlerRegistry`] maps
//! capability IDs to handlers and executes them in [`ActionScale`] order:
//!
//!   Retrieval (0) → External (1: fetch_url, browser, MCP) → Synthesis (2) → Memory (3)
//!
//! Adding a new tool = implement one struct, register one line. No touching runner.rs.

pub mod search_documents;
pub mod web_search;
pub mod fetch_url;
pub mod browser;
pub mod run_python;
pub mod write_file;
pub mod chat_llm;
pub mod propose_memory;
pub mod desktop;
pub mod save_artifact;
pub mod mcp_tool;
pub mod enqueue;
pub mod read_file;
pub mod recall;
pub mod remember;
pub mod git;
pub mod shell;

use std::collections::HashMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::crypto::CryptoContext;
use crate::connectors::LlmConfig;
use crate::error::AppResult;
use crate::agent::registry::{AgentRunStep, UserAgent};

// ── Scale ──────────────────────────────────────────────────────────────────

/// Execution tier. Handlers are sorted and run in ascending order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ActionScale {
    /// Local FTS lookup — fast, no network, no token cost.
    Retrieval = 0,
    /// Outbound HTTP fetch — slower, external dependency.
    External  = 1,
    /// LLM inference — slow, may cost money, uses accumulated context.
    Synthesis = 2,
    /// Episodic memory write — runs after Synthesis, before Actuation.
    Memory    = 3,
    /// Desktop actuation — keyboard/mouse/open actions on the final answer. Always last.
    Actuation = 4,
}

// ── Shared context passed to every handler ─────────────────────────────────

pub struct RunCtx<'a> {
    pub conn:           &'a Connection,
    pub crypto:         &'a CryptoContext,
    pub workspace_id:   Option<&'a str>,
    pub llm:            &'a LlmConfig,
    pub agent:          &'a UserAgent,
    /// Original user input — never mutated.
    pub input:          &'a str,
    /// Context fragments accumulated by earlier-scale handlers (e.g. retrieved chunks, web pages).
    /// Passed as a slice so handlers can read but not append directly.
    pub context_parts:  &'a [String],
    /// Running answer at this point in the chain. Starts as `input`, updated by Synthesis handlers.
    pub current_answer: &'a str,
}

// ── Handler output ─────────────────────────────────────────────────────────

pub struct ToolOutput {
    /// If `Some`, replaces the running answer for subsequent handlers.
    pub answer:           Option<String>,
    /// If `Some`, appended to `context_parts` for later handlers to read.
    pub context_fragment: Option<String>,
    /// Primary step record for the run trace.
    pub step:             AgentRunStep,
    /// Additional step records (e.g. multiple memory proposals).
    pub extra_steps:      Vec<AgentRunStep>,
}

// ── Trait ──────────────────────────────────────────────────────────────────

pub trait ToolHandler: Send + Sync {
    /// Unique capability ID, e.g. `"search_documents"`.
    fn id(&self) -> &'static str;
    /// Optional description override (used by dynamic MCP tools).
    fn description_hint(&self) -> Option<&str> { None }
    /// Which execution tier this handler belongs to.
    fn scale(&self) -> ActionScale;
    /// Execute the handler and return its output.
    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput>;
}

// ── Registry ───────────────────────────────────────────────────────────────

pub struct HandlerRegistry {
    handlers: HashMap<String, Box<dyn ToolHandler>>,
}

impl HandlerRegistry {
    /// Built-in tools only (no DB access needed).
    pub fn new() -> Self {
        let mut r = Self { handlers: HashMap::new() };
        r.register(Box::new(search_documents::SearchDocumentsHandler));
        r.register(Box::new(web_search::WebSearchHandler));
        r.register(Box::new(fetch_url::FetchUrlHandler));
        r.register(Box::new(browser::BrowserHandler));
        r.register(Box::new(run_python::RunPythonHandler));
        r.register(Box::new(write_file::WriteFileHandler));
        r.register(Box::new(chat_llm::ChatLlmHandler));
        r.register(Box::new(propose_memory::ProposeMemoryHandler));
        r.register(Box::new(desktop::DesktopHandler));
        r.register(Box::new(save_artifact::SaveArtifactHandler));
        r.register(Box::new(enqueue::EnqueueHandler));
        r.register(Box::new(read_file::ReadFileHandler));
        r.register(Box::new(recall::RecallHandler));
        r.register(Box::new(remember::RememberHandler));
        r.register(Box::new(git::GitHandler));
        r.register(Box::new(shell::ShellHandler));
        r
    }

    /// Built-in tools + dynamically loaded MCP tools from DB.
    pub fn with_mcp(conn: &Connection) -> Self {
        let mut r = Self::new();
        if let Ok(tools) = crate::mcp::load_all_tools(conn) {
            for tool in tools {
                if let Ok(servers) = crate::mcp::list_servers(conn) {
                    if let Some(srv) = servers.into_iter().find(|s| s.id == tool.server_id && s.enabled) {
                        r.register(Box::new(mcp_tool::McpToolHandler::new(srv, tool)));
                    }
                }
            }
        }
        r
    }

    fn register(&mut self, h: Box<dyn ToolHandler>) {
        self.handlers.insert(h.id().to_string(), h);
    }

    pub fn get(&self, id: &str) -> Option<&dyn ToolHandler> {
        self.handlers.get(id).map(|h| h.as_ref())
    }

    /// All registered tool IDs (built-in + MCP).
    pub fn all_ids(&self) -> Vec<&str> {
        self.handlers.keys().map(|s| s.as_str()).collect()
    }

    /// Sort a list of tool IDs by ascending ActionScale, unknowns last.
    pub fn sort_by_scale(&self, tools: &[String]) -> Vec<String> {
        let mut sorted = tools.to_vec();
        sorted.sort_by_key(|id| {
            self.get(id).map(|h| h.scale() as u8).unwrap_or(255)
        });
        sorted
    }
}
