//! Ollama connector — only compiled when `--features ollama` is set.
//! Calls http://localhost:11434 (or configured URL) for chat + embeddings.

#![cfg(feature = "ollama")]

use std::io::{BufRead, BufReader};
use serde::{Deserialize, Serialize};
use crate::error::{AppError, AppResult};
use crate::retrieval::RetrievedChunk;

// ── Chat ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

/// Call Ollama /api/generate with a RAG prompt.
pub fn chat(
    url: &str,
    model: &str,
    question: &str,
    chunks: &[RetrievedChunk],
) -> AppResult<String> {
    let context = build_context(chunks);
    let prompt = format!(
        "You are a helpful assistant. Answer the question using ONLY the provided context.\n\
         If the answer is not in the context, say \"I don't have enough information in the provided documents.\"\n\n\
         CONTEXT:\n{context}\n\n\
         QUESTION: {question}\n\nANSWER:"
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let resp = client
        .post(format!("{url}/api/generate"))
        .json(&OllamaRequest {
            model: model.to_string(),
            prompt,
            stream: false,
        })
        .send()
        .map_err(|e| AppError::Connector(format!("Ollama unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Connector(format!("Ollama returned status {}", resp.status())));
    }

    let body: OllamaResponse = resp.json()
        .map_err(|e| AppError::Connector(format!("Ollama response parse error: {e}")))?;

    Ok(body.response)
}

// ── Embeddings ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

/// Get embedding vector for `text` from Ollama.
pub fn embed(url: &str, model: &str, text: &str) -> AppResult<Vec<f32>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let resp = client
        .post(format!("{url}/api/embeddings"))
        .json(&EmbedRequest {
            model: model.to_string(),
            prompt: text.to_string(),
        })
        .send()
        .map_err(|e| AppError::Connector(format!("Ollama embed unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Connector(format!("Ollama embed returned {}", resp.status())));
    }

    let body: EmbedResponse = resp.json()
        .map_err(|e| AppError::Connector(format!("Embed parse error: {e}")))?;

    Ok(body.embedding)
}

// ── Multi-turn Chat ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatMessage,
}

/// Call Ollama /api/chat with a full conversation history.
/// `messages` is a slice of (role, content) pairs in order.
pub fn chat_multi(url: &str, model: &str, messages: &[(String, String)]) -> AppResult<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let msgs: Vec<OllamaChatMessage> = messages.iter().map(|(role, content)| OllamaChatMessage {
        role: role.clone(),
        content: content.clone(),
    }).collect();

    let resp = client
        .post(format!("{url}/api/chat"))
        .json(&OllamaChatRequest { model: model.to_string(), messages: msgs, stream: false })
        .send()
        .map_err(|e| AppError::Connector(format!("Ollama unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Connector(format!("Ollama returned status {}", resp.status())));
    }

    let body: OllamaChatResponse = resp.json()
        .map_err(|e| AppError::Connector(format!("Ollama chat response parse error: {e}")))?;

    Ok(body.message.content)
}

/// Streaming multi-turn chat — calls `on_token` for each text fragment and
/// returns the fully accumulated response.
pub fn chat_multi_stream(
    url: &str,
    model: &str,
    messages: &[(String, String)],
    on_token: &dyn Fn(&str),
) -> AppResult<String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let msgs: Vec<OllamaChatMessage> = messages.iter().map(|(role, content)| OllamaChatMessage {
        role: role.clone(), content: content.clone(),
    }).collect();

    let resp = client
        .post(format!("{url}/api/chat"))
        .json(&OllamaChatRequest { model: model.to_string(), messages: msgs, stream: true })
        .send()
        .map_err(|e| AppError::Connector(format!("Ollama unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Connector(format!("Ollama returned status {}", resp.status())));
    }

    // Ollama streams NDJSON: one JSON object per line
    let reader = BufReader::new(resp);
    let mut full = String::new();

    for line in reader.lines() {
        let line = line.map_err(|e| AppError::Connector(format!("Stream read error: {e}")))?;
        let line = line.trim().to_string();
        if line.is_empty() { continue; }
        if let Ok(chunk) = serde_json::from_str::<OllamaStreamChunk>(&line) {
            if !chunk.message.content.is_empty() {
                on_token(&chunk.message.content);
                full.push_str(&chunk.message.content);
            }
            if chunk.done { break; }
        }
    }

    Ok(full)
}

#[derive(Deserialize)]
struct OllamaStreamChunk {
    message: OllamaChatMessage,
    done: bool,
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn build_context(chunks: &[RetrievedChunk]) -> String {
    chunks.iter().enumerate().map(|(i, c)| {
        format!("[{}] Source: {} (chunk {})\n{}", i + 1, c.doc_title, c.chunk_idx, c.preview)
    }).collect::<Vec<_>>().join("\n\n---\n\n")
}
