//! OpenAI connector — calls api.openai.com using reqwest::blocking.
//! No feature flag required; works whenever an API key is configured.

use std::io::{BufRead, BufReader};
use serde::{Deserialize, Serialize};
use crate::error::{AppError, AppResult};
use crate::retrieval::RetrievedChunk;

// ── Request / response types ────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatMessage {
    role:    String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model:    String,
    messages: Vec<ChatMessage>,
    stream:   bool,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

const API_URL: &str = "https://api.openai.com/v1/chat/completions";
const EMBED_URL: &str = "https://api.openai.com/v1/embeddings";

// ── Public API ──────────────────────────────────────────────────────────────

/// Q&A with RAG context — builds a system prompt from retrieved chunks and
/// sends a single user message with the question.
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
    let messages = vec![
        ChatMessage { role: "system".into(), content: system },
        ChatMessage { role: "user".into(),   content: question.to_string() },
    ];
    call(api_key, model, messages, 60)
}

/// Multi-turn chat — `messages` is a slice of (role, content) pairs.
pub fn chat_multi(
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
) -> AppResult<String> {
    let msgs = messages.iter().map(|(role, content)| ChatMessage {
        role: role.clone(), content: content.clone(),
    }).collect();
    call(api_key, model, msgs, 120)
}

// ── Embeddings ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: String,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

/// Generate an embedding vector for `text` using the OpenAI embeddings API.
pub fn embed(api_key: &str, model: &str, text: &str) -> AppResult<Vec<f32>> {
    if api_key.is_empty() {
        return Err(AppError::Connector(
            "OpenAI API key is not set. Add it in Settings → Model.".into()
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let resp = client
        .post(EMBED_URL)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&EmbedRequest { model: model.to_string(), input: text.to_string() })
        .send()
        .map_err(|e| AppError::Connector(format!("OpenAI embed unreachable: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(AppError::Connector(format!(
            "OpenAI embed returned HTTP {status}: {body}"
        )));
    }

    let body: EmbedResponse = resp.json()
        .map_err(|e| AppError::Connector(format!("OpenAI embed parse error: {e}")))?;

    body.data
        .into_iter()
        .next()
        .map(|d| d.embedding)
        .ok_or_else(|| AppError::Connector("OpenAI embed returned empty data".into()))
}

/// Streaming multi-turn chat — calls `on_token` for each text fragment as it
/// arrives, and returns the fully accumulated response when done.
pub fn chat_multi_stream(
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
    on_token: &dyn Fn(&str),
) -> AppResult<String> {
    if api_key.is_empty() {
        return Err(AppError::Connector(
            "OpenAI API key is not set. Add it in Settings → Model.".into()
        ));
    }

    let msgs: Vec<ChatMessage> = messages.iter().map(|(role, content)| ChatMessage {
        role: role.clone(), content: content.clone(),
    }).collect();

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let resp = client
        .post(API_URL)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&ChatRequest { model: model.to_string(), messages: msgs, stream: true })
        .send()
        .map_err(|e| AppError::Connector(format!("OpenAI unreachable: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(AppError::Connector(format!("OpenAI returned HTTP {status}: {body}")));
    }

    // Parse SSE: each line is either empty, "data: [DONE]", or "data: <json>"
    let reader = BufReader::new(resp);
    let mut full = String::new();

    for line in reader.lines() {
        let line = line.map_err(|e| AppError::Connector(format!("Stream read error: {e}")))?;
        if !line.starts_with("data: ") { continue; }
        let data = line["data: ".len()..].trim();
        if data == "[DONE]" { break; }
        if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
            if let Some(choice) = chunk.choices.into_iter().next() {
                if let Some(token) = choice.delta.content {
                    if !token.is_empty() {
                        on_token(&token);
                        full.push_str(&token);
                    }
                }
            }
        }
    }

    Ok(full)
}

// ── Stream types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn call(
    api_key: &str,
    model: &str,
    messages: Vec<ChatMessage>,
    timeout_secs: u64,
) -> AppResult<String> {
    if api_key.is_empty() {
        return Err(AppError::Connector(
            "OpenAI API key is not set. Add it in Settings → Model.".into()
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let resp = client
        .post(API_URL)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&ChatRequest { model: model.to_string(), messages, stream: false })
        .send()
        .map_err(|e| AppError::Connector(format!("OpenAI unreachable: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(AppError::Connector(format!(
            "OpenAI returned HTTP {status}: {body}"
        )));
    }

    let body: ChatResponse = resp.json()
        .map_err(|e| AppError::Connector(format!("OpenAI response parse error: {e}")))?;

    body.choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| AppError::Connector("OpenAI returned empty choices".into()))
}

fn build_context(chunks: &[RetrievedChunk]) -> String {
    chunks.iter().enumerate().map(|(i, c)| {
        format!("[{}] Source: {} (chunk {})\n{}", i + 1, c.doc_title, c.chunk_idx, c.preview)
    }).collect::<Vec<_>>().join("\n\n---\n\n")
}
