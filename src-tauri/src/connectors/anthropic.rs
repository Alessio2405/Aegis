//! Anthropic connector — calls api.anthropic.com using reqwest::blocking.
//! Supports Claude claude-opus-4-6, claude-sonnet-4-6, claude-haiku-4-5, and earlier Claude 3.x models.

use std::io::{BufRead, BufReader};
use serde::{Deserialize, Serialize};
use crate::error::{AppError, AppResult};
use crate::retrieval::RetrievedChunk;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 8192;

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct Message {
    role:    String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model:      String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system:     Option<String>,
    messages:   Vec<Message>,
    stream:     bool,
}

// ── Response types ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text:       Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    content: Vec<ContentBlock>,
}

// ── Stream types ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TextDelta {
    #[serde(rename = "type")]
    delta_type: String,
    text:       Option<String>,
}

#[derive(Deserialize)]
struct DeltaEvent {
    delta: Option<TextDelta>,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Q&A with RAG context.
pub fn rag(
    api_key: &str,
    model: &str,
    question: &str,
    chunks: &[RetrievedChunk],
) -> AppResult<String> {
    let context = build_context(chunks);
    let system = format!(
        "You are a helpful assistant. Answer the question using ONLY the provided context.\n\
         If the answer is not in the context, say \"I don't have enough information in the provided documents.\"\n\n\
         CONTEXT:\n{context}"
    );
    call(api_key, model, Some(system), vec![
        Message { role: "user".into(), content: question.to_string() },
    ], 60)
}

/// Multi-turn chat — `messages` is a slice of (role, content) pairs.
/// Anthropic does not allow "system" role in the messages array; system messages
/// are extracted and passed as the top-level `system` field instead.
pub fn chat_multi(
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
) -> AppResult<String> {
    let (system, msgs) = split_system(messages);
    if msgs.is_empty() {
        return Err(AppError::Connector("No user/assistant messages to send to Anthropic.".into()));
    }
    call(api_key, model, system, msgs, 120)
}

/// Streaming multi-turn chat — calls `on_token` for each text fragment.
pub fn chat_multi_stream(
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
    on_token: &dyn Fn(&str),
) -> AppResult<String> {
    if api_key.is_empty() {
        return Err(AppError::Connector(
            "Anthropic API key is not set. Add it in Settings → Model.".into()
        ));
    }

    let (system, msgs) = split_system(messages);
    if msgs.is_empty() {
        return Err(AppError::Connector("No user/assistant messages to send to Anthropic.".into()));
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let resp = client
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("Content-Type", "application/json")
        .json(&ChatRequest { model: model.to_string(), max_tokens: MAX_TOKENS, system, messages: msgs, stream: true })
        .send()
        .map_err(|e| AppError::Connector(format!("Anthropic unreachable: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(AppError::Connector(format!("Anthropic returned HTTP {status}: {body}")));
    }

    // Anthropic SSE: "event: content_block_delta" followed by "data: {...}"
    let reader = BufReader::new(resp);
    let mut full = String::new();
    let mut is_delta_event = false;

    for line in reader.lines() {
        let line = line.map_err(|e| AppError::Connector(format!("Stream read error: {e}")))?;
        if line.starts_with("event: ") {
            is_delta_event = line.trim_start_matches("event: ").trim() == "content_block_delta";
        } else if line.starts_with("data: ") && is_delta_event {
            let data = &line["data: ".len()..];
            if let Ok(ev) = serde_json::from_str::<DeltaEvent>(data) {
                if let Some(delta) = ev.delta {
                    if delta.delta_type == "text_delta" {
                        if let Some(text) = delta.text {
                            if !text.is_empty() {
                                on_token(&text);
                                full.push_str(&text);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(full)
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn call(
    api_key: &str,
    model: &str,
    system: Option<String>,
    messages: Vec<Message>,
    timeout_secs: u64,
) -> AppResult<String> {
    if api_key.is_empty() {
        return Err(AppError::Connector(
            "Anthropic API key is not set. Add it in Settings → Model.".into()
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let resp = client
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("Content-Type", "application/json")
        .json(&ChatRequest { model: model.to_string(), max_tokens: MAX_TOKENS, system, messages, stream: false })
        .send()
        .map_err(|e| AppError::Connector(format!("Anthropic unreachable: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(AppError::Connector(format!("Anthropic returned HTTP {status}: {body}")));
    }

    let body: ChatResponse = resp.json()
        .map_err(|e| AppError::Connector(format!("Anthropic response parse error: {e}")))?;

    body.content
        .into_iter()
        .find(|b| b.block_type == "text")
        .and_then(|b| b.text)
        .ok_or_else(|| AppError::Connector("Anthropic returned empty content".into()))
}

/// Split (role, content) pairs: extract system messages → top-level `system` param,
/// pass the rest as the messages array.
fn split_system(messages: &[(String, String)]) -> (Option<String>, Vec<Message>) {
    let mut system_parts: Vec<String> = Vec::new();
    let mut msgs: Vec<Message> = Vec::new();
    for (role, content) in messages {
        if role == "system" {
            system_parts.push(content.clone());
        } else {
            msgs.push(Message { role: role.clone(), content: content.clone() });
        }
    }
    let system = if system_parts.is_empty() { None } else { Some(system_parts.join("\n\n")) };
    (system, msgs)
}

fn build_context(chunks: &[RetrievedChunk]) -> String {
    chunks.iter().enumerate().map(|(i, c)| {
        format!("[{}] Source: {} (chunk {})\n{}", i + 1, c.doc_title, c.chunk_idx, c.preview)
    }).collect::<Vec<_>>().join("\n\n---\n\n")
}
