//! Connectors — integrations with LLM providers.
//!
//! All calls go through `llm_rag()` / `llm_chat_multi()` which route to the
//! correct provider based on `LlmConfig.provider`.
//!
//! Supported providers:
//!   "ollama"  — local Ollama instance (requires `--features ollama`)
//!   "openai"  — OpenAI cloud API (api key required, always compiled)

pub mod openai;

#[cfg(feature = "ollama")]
pub mod ollama;

use serde::{Deserialize, Serialize};
use crate::error::{AppError, AppResult};
use crate::retrieval::RetrievedChunk;

// ── LLM configuration ───────────────────────────────────────────────────────

/// All LLM settings collected into one struct so callers don't need to pass
/// individual url/model/key arguments.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    /// Whether model calls are enabled at all.
    pub enabled: bool,
    /// "ollama" or "openai"
    pub provider: String,
    // Ollama
    pub ollama_url: String,
    pub ollama_model: String,
    // OpenAI
    pub openai_api_key: String,
    pub openai_model: String,
    // Embeddings
    pub embeddings_enabled: bool,
    /// Model to use for embeddings ("nomic-embed-text" for Ollama, "text-embedding-3-small" for OpenAI).
    pub embed_model: String,
}

// ── Unified dispatch ────────────────────────────────────────────────────────

/// Q&A with RAG context — routes to Ollama or OpenAI based on config.
pub fn llm_rag(
    config: &LlmConfig,
    question: &str,
    chunks: &[RetrievedChunk],
) -> AppResult<String> {
    if !config.enabled {
        return Err(AppError::Connector(
            "Model calls are disabled. Enable them in Settings → Model.".into()
        ));
    }
    match config.provider.as_str() {
        "openai" => openai::rag(&config.openai_api_key, &config.openai_model, question, chunks),
        _ => llm_rag_ollama(config, question, chunks),
    }
}

/// Multi-turn chat — routes to Ollama or OpenAI based on config.
pub fn llm_chat_multi(
    config: &LlmConfig,
    messages: &[(String, String)],
) -> AppResult<String> {
    if !config.enabled {
        return Err(AppError::Connector(
            "Model calls are disabled. Enable them in Settings → Model.".into()
        ));
    }
    match config.provider.as_str() {
        "openai" => openai::chat_multi(&config.openai_api_key, &config.openai_model, messages),
        _ => llm_chat_multi_ollama(config, messages),
    }
}

/// Streaming multi-turn chat — calls `on_token` for each token fragment as it
/// arrives, returns full accumulated text when done.
pub fn llm_chat_multi_stream(
    config: &LlmConfig,
    messages: &[(String, String)],
    on_token: &dyn Fn(&str),
) -> AppResult<String> {
    if !config.enabled {
        return Err(AppError::Connector(
            "Model calls are disabled. Enable them in Settings → Model.".into()
        ));
    }
    match config.provider.as_str() {
        "openai" => openai::chat_multi_stream(&config.openai_api_key, &config.openai_model, messages, on_token),
        _ => llm_chat_multi_stream_ollama(config, messages, on_token),
    }
}

/// Generate an embedding vector — routes to Ollama or OpenAI based on config.
pub fn llm_embed(config: &LlmConfig, text: &str) -> AppResult<Vec<f32>> {
    match config.provider.as_str() {
        "openai" => openai::embed(&config.openai_api_key, &config.embed_model, text),
        _ => llm_embed_ollama(config, text),
    }
}

// ── Ollama fallbacks (feature-gated) ────────────────────────────────────────

#[cfg(feature = "ollama")]
fn llm_rag_ollama(config: &LlmConfig, question: &str, chunks: &[RetrievedChunk]) -> AppResult<String> {
    ollama::chat(&config.ollama_url, &config.ollama_model, question, chunks)
}

#[cfg(not(feature = "ollama"))]
fn llm_rag_ollama(_: &LlmConfig, _: &str, _: &[RetrievedChunk]) -> AppResult<String> {
    Err(AppError::Connector(
        "Ollama feature not compiled. Use --features ollama or switch to OpenAI.".into()
    ))
}

#[cfg(feature = "ollama")]
fn llm_chat_multi_ollama(config: &LlmConfig, messages: &[(String, String)]) -> AppResult<String> {
    ollama::chat_multi(&config.ollama_url, &config.ollama_model, messages)
}

#[cfg(not(feature = "ollama"))]
fn llm_chat_multi_ollama(_: &LlmConfig, _: &[(String, String)]) -> AppResult<String> {
    Err(AppError::Connector(
        "Ollama feature not compiled. Use --features ollama or switch to OpenAI.".into()
    ))
}

#[cfg(feature = "ollama")]
fn llm_chat_multi_stream_ollama(config: &LlmConfig, messages: &[(String, String)], on_token: &dyn Fn(&str)) -> AppResult<String> {
    ollama::chat_multi_stream(&config.ollama_url, &config.ollama_model, messages, on_token)
}

#[cfg(not(feature = "ollama"))]
fn llm_chat_multi_stream_ollama(_: &LlmConfig, _: &[(String, String)], _: &dyn Fn(&str)) -> AppResult<String> {
    Err(AppError::Connector(
        "Ollama feature not compiled. Use --features ollama or switch to OpenAI.".into()
    ))
}

#[cfg(feature = "ollama")]
fn llm_embed_ollama(config: &LlmConfig, text: &str) -> AppResult<Vec<f32>> {
    ollama::embed(&config.ollama_url, &config.embed_model, text)
}

#[cfg(not(feature = "ollama"))]
fn llm_embed_ollama(_: &LlmConfig, _: &str) -> AppResult<Vec<f32>> {
    Err(AppError::Connector(
        "Ollama feature not compiled. Use --features ollama or switch to OpenAI.".into()
    ))
}

/// Stub kept for any legacy call sites (will be removed once all callers migrated).
pub mod stub {
    use crate::error::{AppError, AppResult};
    use crate::retrieval::RetrievedChunk;

    #[allow(dead_code)]
    pub fn chat_stub(_question: &str, _chunks: &[RetrievedChunk]) -> AppResult<String> {
        Err(AppError::Connector(
            "No model connector enabled. Enable 'ollama' feature or configure a provider.".into()
        ))
    }
}
