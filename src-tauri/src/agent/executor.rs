//! Executor — runs a Plan step-by-step, enforcing permissions at every step.
//! The executor NEVER runs arbitrary code. Every tool call is a typed enum variant.

use rusqlite::Connection;

use crate::crypto::CryptoContext;
use crate::error::{AppError, AppResult};
use crate::permissions::{self, Capability};
use crate::retrieval::{self, RetrievedChunk};
use crate::connectors::LlmConfig;
use super::{AgentAnswer, Citation, Plan, PlanStep};
use super::memory_writer;

pub fn execute(
    conn: &Connection,
    crypto: &CryptoContext,
    plan: &Plan,
    question: &str,
    workspace_id: Option<&str>,
    llm: &LlmConfig,
) -> AppResult<AgentAnswer> {
    let mut retrieved: Vec<RetrievedChunk> = Vec::new();
    let mut answer = String::new();
    let mut is_extractive = true;
    let mut proposed_memory_ids: Vec<String> = Vec::new();

    for step in &plan.steps {
        match step {
            // ── Step: RetrieveChunks ────────────────────────────────────
            PlanStep::RetrieveChunks { query, top_k, workspace_id: ws } => {
                // Check file.read permission
                permissions::require(
                    conn, &Capability::FileRead,
                    ws.as_deref().or(workspace_id),
                    "agent", Some("chunk"), None,
                )?;

                retrieved = retrieval::retrieve(
                    conn, crypto, query,
                    ws.as_deref().or(workspace_id),
                    *top_k,
                    Some(llm),
                )?;

                // Verifier: if no results, refuse politely
                if retrieved.is_empty() {
                    return Ok(AgentAnswer {
                        answer: "I don't have sources for that yet. \
                                 Please ingest some documents into a workspace first, \
                                 then try again.".to_string(),
                        citations: vec![],
                        is_extractive: true,
                        proposed_memories: vec![],
                    });
                }
            }

            // ── Step: SynthesizeAnswer ──────────────────────────────────
            PlanStep::SynthesizeAnswer { question: q } => {
                if retrieved.is_empty() {
                    // Shouldn't reach here (verifier above), but guard anyway
                    return Err(AppError::NoSources);
                }

                if llm.enabled {
                    match call_llm(llm, q, &retrieved) {
                        Ok(llm_answer) => {
                            answer = llm_answer;
                            is_extractive = false;
                        }
                        Err(e) => {
                            let _ = permissions::check(
                                conn, &Capability::ModelUse, workspace_id, "agent", None, None
                            );
                            answer = build_extractive_answer(q, &retrieved);
                            answer.push_str(&format!(
                                "\n\n*(Note: LLM was unreachable — {e}. Showing extractive summary.)*"
                            ));
                        }
                    }
                } else {
                    answer = build_extractive_answer(q, &retrieved);
                }
            }

            // ── Step: RunPython ─────────────────────────────────────────
            PlanStep::RunPython { code, description: _ } => {
                // 30-second hard timeout; error is non-fatal (surfaced in answer)
                match crate::python::run_code(code, 30) {
                    Ok(res) => {
                        let out = res.stdout.trim();
                        let err = res.stderr.trim();
                        if res.exit_code != 0 || !err.is_empty() {
                            answer.push_str(&format!(
                                "\n\n**Python output** (exit {}):\n```\n{}\n```",
                                res.exit_code,
                                if out.is_empty() { err } else { out }
                            ));
                        } else {
                            answer.push_str(&format!(
                                "\n\n**Python output**:\n```\n{out}\n```"
                            ));
                        }
                    }
                    Err(e) => {
                        answer.push_str(&format!(
                            "\n\n**Python execution failed**: {e}"
                        ));
                    }
                }
            }

            // ── Step: FetchUrl ──────────────────────────────────────────
            PlanStep::FetchUrl { url, description } => {
                match fetch_and_strip(url) {
                    Ok(text) => {
                        let preview: String = text.chars().take(2000).collect();
                        answer.push_str(&format!(
                            "\n\n**Fetched: {}**\n{preview}", description
                        ));
                    }
                    Err(e) => {
                        answer.push_str(&format!(
                            "\n\n**Fetch failed ({}): {}**", description, e
                        ));
                    }
                }
            }

            // ── Step: ProposeMemory ─────────────────────────────────────
            PlanStep::ProposeMemory { content, memory_type, source } => {
                // Only propose non-empty memories
                if content.is_empty() && answer.is_empty() {
                    continue;
                }
                let mem_content = if content.is_empty() {
                    // Auto-generate from the answer
                    format!("Q: {question}\nA: {}", &answer[..answer.len().min(300)])
                } else {
                    content.clone()
                };

                let id = memory_writer::propose(
                    conn, crypto,
                    &mem_content,
                    memory_type,
                    source,
                    workspace_id,
                )?;
                proposed_memory_ids.push(id);
            }
        }
    }

    let citations: Vec<Citation> = retrieved.iter().map(|c| Citation {
        chunk_id: c.chunk_id.clone(),
        doc_title: c.doc_title.clone(),
        snippet: c.preview.chars().take(240).collect(),
    }).collect();

    Ok(AgentAnswer {
        answer,
        citations,
        is_extractive,
        proposed_memories: proposed_memory_ids,
    })
}

// ── Extractive answer builder ─────────────────────────────────────────────

fn build_extractive_answer(question: &str, chunks: &[RetrievedChunk]) -> String {
    let mut out = format!(
        "**Extractive answer** (no AI model used) for: *{question}*\n\n"
    );
    out.push_str("Based on your documents:\n\n");
    for (i, chunk) in chunks.iter().enumerate() {
        let snippet: String = chunk.preview.chars().take(300).collect();
        out.push_str(&format!(
            "**[{}] {}** (chunk {})\n> {}\n\n",
            i + 1, chunk.doc_title, chunk.chunk_idx, snippet
        ));
    }
    out
}

// ── Web fetch helpers ─────────────────────────────────────────────────────

fn fetch_and_strip(url: &str) -> AppResult<String> {
    let body = reqwest::blocking::get(url)
        .map_err(|e| AppError::Connector(e.to_string()))?
        .text()
        .map_err(|e| AppError::Connector(e.to_string()))?;
    Ok(strip_html_tags(&body))
}

fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => { in_tag = true; }
            '>' => { in_tag = false; out.push(' '); }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── LLM dispatch ─────────────────────────────────────────────────────────

fn call_llm(
    llm: &LlmConfig,
    question: &str,
    chunks: &[RetrievedChunk],
) -> AppResult<String> {
    crate::connectors::llm_rag(llm, question, chunks)
}
