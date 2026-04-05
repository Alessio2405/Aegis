# Connectors

Connectors are optional integrations with external services. They are:
- **Off by default** — the app works fully offline without any connector
- **Feature-gated** in Rust (compile-time opt-in)
- **Permission-gated** at runtime (`model.use` capability required)
- **Clearly labelled** in the UI when a network call occurs

---

## Existing Connectors

### Ollama (Local LLM)

**Status**: Implemented (requires `--features ollama` build flag)
**Endpoint**: `http://localhost:11434` (configurable)
**Capabilities required**: `model.use`

Provides:
1. **Chat / synthesis** (`POST /api/generate`) — generates Q&A answers from retrieved chunks
2. **Embeddings** (`POST /api/embeddings`) — creates vector embeddings for semantic search

**Setup**:
```bash
# Install Ollama from https://ollama.ai
ollama pull llama3              # chat model
ollama pull nomic-embed-text    # embedding model (optional)
ollama serve                    # starts at localhost:11434
```

**Enable in app**: Settings → AI Model → toggle ON → save.

---

## Adding a New Connector

### Step 1: Create the connector module

```
src-tauri/src/connectors/my_connector.rs
```

```rust
// src-tauri/src/connectors/my_connector.rs
use crate::error::AppResult;
use crate::retrieval::RetrievedChunk;

/// Send a RAG prompt and receive an answer string.
pub fn chat(
    api_key: &str,
    model: &str,
    question: &str,
    chunks: &[RetrievedChunk],
) -> AppResult<String> {
    // Build context from chunks
    // Call external API (only when model.use permission is granted)
    // Return answer string
    todo!()
}
```

### Step 2: Register in `connectors/mod.rs`

```rust
// src-tauri/src/connectors/mod.rs
#[cfg(feature = "my_connector")]
pub mod my_connector;
```

### Step 3: Add feature flag in `Cargo.toml`

```toml
[features]
my_connector = ["reqwest"]  # or whatever HTTP client

[dependencies]
reqwest = { version = "0.11", optional = true, features = ["json"] }
```

### Step 4: Wire into the executor

In `agent/executor.rs`, add a branch in `call_model()`:

```rust
fn call_model(settings: &AppSettings, question: &str, chunks: &[RetrievedChunk]) -> AppResult<String> {
    match settings.model_provider.as_str() {
        "ollama" => connectors::ollama::chat(&settings.ollama_url, &settings.ollama_model, question, chunks),
        "my_connector" => connectors::my_connector::chat(&settings.api_key, &settings.model, question, chunks),
        _ => Err(AppError::Connector("Unknown provider".into())),
    }
}
```

### Step 5: Add UI configuration

In `src/screens/Settings.tsx`, add the new provider option and any required fields (API key, endpoint, etc.).

**Important**: API keys must be stored encrypted in the vault (use `crypto.encrypt_str()` before storing in settings).

### Step 6: Add a setting entry

```sql
-- In db/schema.rs
INSERT OR IGNORE INTO settings(key, value) VALUES ('my_connector_api_key_enc', '');
```

---

## Connector Constraints

All connectors **must**:
- Check `permissions::require(Capability::ModelUse, ...)` before making any external call
- Log the call attempt to the audit log (success or failure)
- Implement a timeout (≤60s for chat, ≤30s for embeddings)
- Fall back to extractive mode on failure (never crash the app)
- Never send passwords, vault keys, or unrelated data to external services

All connectors **must not**:
- Store raw API keys in plaintext in settings (use vault encryption)
- Make calls outside the explicitly configured endpoint
- Cache responses in plaintext on disk
- Bypass the capability permission check

---

## Planned Connectors (not implemented)

| Connector | Notes |
|-----------|-------|
| OpenAI-compatible | Any OpenAI API-compatible endpoint (Together, Groq, etc.) |
| Claude API | Anthropic's API (requires explicit user key setup) |
| Local llama.cpp server | Direct llama.cpp REST API |
| Whisper (local) | Audio transcription for audio file ingest |
| OCR connector | Image/PDF text extraction |
