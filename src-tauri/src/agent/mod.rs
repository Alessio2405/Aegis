//! Agent runtime — Router → Planner → Executor → Verifier → Memory Writer
//!
//! Design constraints:
//!   - The LLM (if enabled) can only produce structured JSON plans.
//!   - The Executor NEVER runs arbitrary code.
//!   - Every tool call checks permissions first.
//!   - If retrieval returns 0 chunks, the agent responds with a polite refusal.

pub mod router;
pub mod planner;
pub mod executor;
pub mod memory_writer;
pub mod registry;
pub mod runner;
pub mod react;
pub mod tools;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::crypto::CryptoContext;
use crate::connectors::LlmConfig;
use crate::error::AppResult;

// ── Shared plan types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    QAndA,
    Ingest,
    MemoryOps,
    Settings,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum PlanStep {
    RetrieveChunks  { query: String, top_k: usize, workspace_id: Option<String> },
    SynthesizeAnswer { question: String },
    ProposeMemory   { content: String, memory_type: String, source: String },
    /// Run a Python code snippet and surface the output to the next step.
    RunPython       { code: String, description: String },
    /// Fetch a URL, strip HTML tags, and inject the text as context.
    FetchUrl        { url: String, description: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub kind: RequestKind,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub chunk_id: String,
    pub doc_title: String,
    pub snippet: String,    // ≤240 chars plaintext preview
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAnswer {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub is_extractive: bool,  // true = no LLM used
    pub proposed_memories: Vec<String>, // IDs of proposed memories
}

// ── Top-level entry point ─────────────────────────────────────────────────

/// Handle a user question end-to-end.
pub fn handle_question(
    conn: &Connection,
    crypto: &CryptoContext,
    question: &str,
    workspace_id: Option<&str>,
    llm: &LlmConfig,
) -> AppResult<AgentAnswer> {
    // 1. Route
    let kind = router::classify(question);

    // 2. Plan
    let plan = planner::plan(&kind, question, workspace_id);

    // 3. Execute
    executor::execute(conn, crypto, &plan, question, workspace_id, llm)
}
