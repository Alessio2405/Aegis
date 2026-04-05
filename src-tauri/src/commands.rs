//! All Tauri IPC commands — the bridge between the React frontend and Rust backend.
//! Every command validates inputs, checks permissions, and logs audit events.

use tauri::{Manager, State};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use std::path::PathBuf;

use rusqlite;

use crate::state::AppState;
use crate::error::{AppError, AppResult};
use crate::crypto::{CryptoContext, generate_salt};
use crate::db;
use crate::ingest;
use crate::retrieval;
use crate::permissions::{self, Capability, GrantType};
use crate::audit;
use crate::agent;
use crate::agent::memory_writer;
use crate::agent::{registry, runner};

// ── Vault commands ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct VaultStatus {
    pub unlocked: bool,
    pub exists: bool,
}

#[tauri::command]
pub fn get_vault_status(state: State<AppState>) -> VaultStatus {
    VaultStatus {
        unlocked: state.is_unlocked(),
        exists: state.salt_path.exists(),
    }
}

#[tauri::command]
pub fn create_vault(password: String, state: State<AppState>) -> AppResult<()> {
    if state.salt_path.exists() {
        return Err(AppError::VaultAlreadyExists);
    }
    if password.len() < 8 {
        return Err(AppError::InvalidInput("Password must be at least 8 characters".into()));
    }

    let salt = generate_salt();
    fs::write(&state.salt_path, &salt)
        .map_err(|e| AppError::Io(e))?;

    let crypto = CryptoContext::derive(&password, &salt)
        .map_err(|e| AppError::Crypto(e.to_string()))?;

    // Bootstrap default permissions
    let db = state.db.lock().unwrap();
    permissions::bootstrap_defaults(&db)?;

    audit::log(&db, audit::AuditEvent {
        actor: "user".into(),
        action_type: "vault_create".into(),
        capability: None,
        target_type: Some("vault".into()),
        target_id: None,
        workspace_id: None,
        outcome: "allowed".into(),
        details_json: None,
    })?;
    drop(db);

    // Share key bytes with the bot thread before moving crypto into the mutex
    let key_bytes = crypto.key_bytes();
    let mut ctx = state.crypto.lock().unwrap();
    *ctx = Some(crypto);
    drop(ctx);
    *state.bot.shared_key.lock().unwrap() = Some(key_bytes);
    Ok(())
}

#[tauri::command]
pub fn unlock_vault(password: String, state: State<AppState>) -> AppResult<()> {
    if !state.salt_path.exists() {
        return Err(AppError::NotFound("Vault does not exist. Create one first.".into()));
    }
    let salt = fs::read(&state.salt_path)?;
    let crypto = CryptoContext::derive(&password, &salt)?;

    // Verify the key works by attempting to read any existing encrypted memory
    {
        let db = state.db.lock().unwrap();
        let test: Option<Vec<u8>> = db.query_row(
            "SELECT content_ciphertext FROM memories WHERE content_ciphertext IS NOT NULL LIMIT 1",
            [],
            |r| r.get(0),
        ).ok();
        if let Some(blob) = test {
            crypto.decrypt(&blob)
                .map_err(|_| AppError::Crypto("Wrong password — decryption failed.".into()))?;
        }

        audit::log(&db, audit::AuditEvent {
            actor: "user".into(),
            action_type: "vault_unlock".into(),
            capability: None,
            target_type: Some("vault".into()),
            target_id: None,
            workspace_id: None,
            outcome: "allowed".into(),
            details_json: None,
        })?;
    }

    // Share key bytes with the bot thread
    let key_bytes = crypto.key_bytes();
    let mut ctx = state.crypto.lock().unwrap();
    *ctx = Some(crypto);
    drop(ctx);
    *state.bot.shared_key.lock().unwrap() = Some(key_bytes);
    Ok(())
}

#[tauri::command]
pub fn lock_vault(state: State<AppState>) -> AppResult<()> {
    // Clear bot key first (bot sees vault as locked immediately)
    *state.bot.shared_key.lock().unwrap() = None;
    let mut ctx = state.crypto.lock().unwrap();
    *ctx = None; // CryptoContext::drop() zeros the key
    Ok(())
}

// ── Settings commands ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub model_enabled: bool,
    pub model_provider: String,
    pub ollama_url: String,
    pub ollama_model: String,
    pub ollama_embed_model: String,
    pub embeddings_enabled: bool,
    // OpenAI
    pub openai_api_key: String,
    pub openai_model: String,
    // Telegram bot
    pub telegram_enabled: bool,
    pub telegram_token: String,
    pub telegram_chat_id: String,
    // Discord bot
    pub discord_enabled: bool,
    pub discord_token: String,
    pub discord_allowed_user_id: String,
    // WhatsApp bot
    pub whatsapp_enabled: bool,
    pub whatsapp_access_token: String,
    pub whatsapp_phone_number_id: String,
    pub whatsapp_allowed_number: String,
    pub whatsapp_verify_token: String,
    pub whatsapp_webhook_port: String,
    // Desktop automation
    pub desktop_control_enabled: bool,
    // Web search
    pub search_provider: String,
    pub brave_api_key: String,
    pub searxng_url: String,
    // Agent environment
    pub agent_work_dir: String,
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> AppResult<AppSettings> {
    let db = state.db.lock().unwrap();
    Ok(AppSettings {
        model_enabled:            db::get_setting(&db, "model_enabled")?.as_deref() == Some("true"),
        model_provider:           db::get_setting(&db, "model_provider")?.unwrap_or_else(|| "ollama".into()),
        ollama_url:               db::get_setting(&db, "ollama_url")?.unwrap_or_else(|| "http://localhost:11434".into()),
        ollama_model:             db::get_setting(&db, "ollama_model")?.unwrap_or_else(|| "llama3".into()),
        ollama_embed_model:       db::get_setting(&db, "ollama_embed_model")?.unwrap_or_else(|| "nomic-embed-text".into()),
        embeddings_enabled:       db::get_setting(&db, "embeddings_enabled")?.as_deref() == Some("true"),
        openai_api_key:           db::get_setting(&db, "openai_api_key")?.unwrap_or_default(),
        openai_model:             db::get_setting(&db, "openai_model")?.unwrap_or_else(|| "gpt-4o-mini".into()),
        telegram_enabled:         db::get_setting(&db, "telegram_enabled")?.as_deref() == Some("true"),
        telegram_token:           db::get_setting(&db, "telegram_token")?.unwrap_or_default(),
        telegram_chat_id:         db::get_setting(&db, "telegram_chat_id")?.unwrap_or_default(),
        discord_enabled:          db::get_setting(&db, "discord_enabled")?.as_deref() == Some("true"),
        discord_token:            db::get_setting(&db, "discord_token")?.unwrap_or_default(),
        discord_allowed_user_id:  db::get_setting(&db, "discord_allowed_user_id")?.unwrap_or_default(),
        whatsapp_enabled:         db::get_setting(&db, "whatsapp_enabled")?.as_deref() == Some("true"),
        whatsapp_access_token:    db::get_setting(&db, "whatsapp_access_token")?.unwrap_or_default(),
        whatsapp_phone_number_id: db::get_setting(&db, "whatsapp_phone_number_id")?.unwrap_or_default(),
        whatsapp_allowed_number:  db::get_setting(&db, "whatsapp_allowed_number")?.unwrap_or_default(),
        whatsapp_verify_token:    db::get_setting(&db, "whatsapp_verify_token")?.unwrap_or_default(),
        whatsapp_webhook_port:    db::get_setting(&db, "whatsapp_webhook_port")?.unwrap_or_else(|| "8765".into()),
        desktop_control_enabled:  db::get_setting(&db, "desktop_control_enabled")?.as_deref() == Some("true"),
        search_provider:          db::get_setting(&db, "search_provider")?.unwrap_or_else(|| "brave".into()),
        brave_api_key:            db::get_setting(&db, "brave_api_key")?.unwrap_or_default(),
        searxng_url:              db::get_setting(&db, "searxng_url")?.unwrap_or_else(|| "http://localhost:8888".into()),
        agent_work_dir:           db::get_setting(&db, "agent_work_dir")?.unwrap_or_default(),
    })
}

#[tauri::command]
pub fn update_settings(settings: AppSettings, state: State<AppState>) -> AppResult<()> {
    let db = state.db.lock().unwrap();
    db::set_setting(&db, "model_enabled",           if settings.model_enabled { "true" } else { "false" })?;
    db::set_setting(&db, "model_provider",          &settings.model_provider)?;
    db::set_setting(&db, "ollama_url",              &settings.ollama_url)?;
    db::set_setting(&db, "ollama_model",            &settings.ollama_model)?;
    db::set_setting(&db, "ollama_embed_model",      &settings.ollama_embed_model)?;
    db::set_setting(&db, "embeddings_enabled",      if settings.embeddings_enabled { "true" } else { "false" })?;
    db::set_setting(&db, "openai_api_key",          &settings.openai_api_key)?;
    db::set_setting(&db, "openai_model",            &settings.openai_model)?;
    db::set_setting(&db, "telegram_enabled",         if settings.telegram_enabled { "true" } else { "false" })?;
    db::set_setting(&db, "telegram_token",           &settings.telegram_token)?;
    db::set_setting(&db, "telegram_chat_id",         &settings.telegram_chat_id)?;
    db::set_setting(&db, "discord_enabled",          if settings.discord_enabled { "true" } else { "false" })?;
    db::set_setting(&db, "discord_token",            &settings.discord_token)?;
    db::set_setting(&db, "discord_allowed_user_id",  &settings.discord_allowed_user_id)?;
    db::set_setting(&db, "whatsapp_enabled",         if settings.whatsapp_enabled { "true" } else { "false" })?;
    db::set_setting(&db, "whatsapp_access_token",    &settings.whatsapp_access_token)?;
    db::set_setting(&db, "whatsapp_phone_number_id", &settings.whatsapp_phone_number_id)?;
    db::set_setting(&db, "whatsapp_allowed_number",  &settings.whatsapp_allowed_number)?;
    db::set_setting(&db, "whatsapp_verify_token",    &settings.whatsapp_verify_token)?;
    db::set_setting(&db, "whatsapp_webhook_port",    &settings.whatsapp_webhook_port)?;
    db::set_setting(&db, "desktop_control_enabled", if settings.desktop_control_enabled { "true" } else { "false" })?;
    db::set_setting(&db, "search_provider",         &settings.search_provider)?;
    db::set_setting(&db, "brave_api_key",           &settings.brave_api_key)?;
    db::set_setting(&db, "searxng_url",             &settings.searxng_url)?;
    db::set_setting(&db, "agent_work_dir",          &settings.agent_work_dir)?;

    // If model.use is enabled, grant the capability; if disabled, revoke it
    if settings.model_enabled {
        let already: bool = db.query_row(
            "SELECT COUNT(*) > 0 FROM permissions WHERE capability='model.use'",
            [], |r| r.get(0),
        )?;
        if !already {
            permissions::grant(&db, &Capability::ModelUse, None, None, GrantType::Always)?;
        }
    } else {
        db.execute("DELETE FROM permissions WHERE capability='model.use'", [])?;
    }

    audit::log(&db, audit::AuditEvent {
        actor: "user".into(),
        action_type: "settings_update".into(),
        capability: None,
        target_type: Some("settings".into()),
        target_id: None,
        workspace_id: None,
        outcome: "allowed".into(),
        details_json: Some(serde_json::json!({
            "model_enabled": settings.model_enabled,
            "telegram_enabled": settings.telegram_enabled,
            "desktop_control_enabled": settings.desktop_control_enabled,
        }).to_string()),
    })?;
    Ok(())
}

// ── Workspace commands ────────────────────────────────────────────────────

#[tauri::command]
pub fn add_workspace(name: String, path: String, state: State<AppState>) -> AppResult<ingest::Workspace> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    permissions::require(&db, &Capability::VaultWrite, None, "user", Some("workspace"), None)?;
    let ws = ingest::create_workspace(&db, &name, &path)?;
    audit::log_write(&db, "user", "workspace", &ws.id, None)?;
    Ok(ws)
}

#[tauri::command]
pub fn list_workspaces(state: State<AppState>) -> AppResult<Vec<ingest::Workspace>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    ingest::list_workspaces(&db)
}

#[tauri::command]
pub fn list_documents(workspace_id: String, state: State<AppState>) -> AppResult<Vec<ingest::DocumentInfo>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    ingest::list_documents(&db, &workspace_id)
}

#[tauri::command]
pub fn ingest_workspace(workspace_id: String, state: State<AppState>) -> AppResult<ingest::IngestStats> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }

    // Phase 1: ingest — hold db + crypto simultaneously (required by ingest_workspace signature)
    let stats = {
        // Lock db first, then crypto — consistent lock order to avoid deadlock
        let db = state.db.lock().unwrap();
        permissions::require(&db, &Capability::FileRead, Some(&workspace_id), "user", Some("workspace"), Some(&workspace_id))?;
        let llm = load_llm_config(&db);
        let crypto_guard = state.crypto.lock().unwrap();
        let crypto = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;
        ingest::ingest_workspace(&db, crypto, &workspace_id, Some(&llm))?
        // Guards dropped here
    };

    // Phase 2: propose memories from ingested doc titles (simple heuristic)
    // Acquire locks again in same order
    {
        let db = state.db.lock().unwrap();
        let docs = ingest::list_documents(&db, &workspace_id)?;
        let candidates: Vec<(String, String, String)> = docs.iter().take(5).flat_map(|doc| {
            memory_writer::extract_candidate_memories(&doc.title)
                .into_iter()
                .map(|(content, mtype)| (content, mtype, doc.id.clone()))
        }).collect();

        if !candidates.is_empty() {
            let crypto_guard = state.crypto.lock().unwrap();
            let crypto = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;
            for (content, mtype, doc_id) in candidates {
                memory_writer::propose(&db, crypto, &content, &mtype, &format!("document:{doc_id}"), Some(&workspace_id))?;
            }
        }
    }

    Ok(stats)
}

// ── Q&A commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn ask_question(
    question: String,
    workspace_id: Option<String>,
    state: State<AppState>,
) -> AppResult<agent::AgentAnswer> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if question.trim().is_empty() {
        return Err(AppError::InvalidInput("Question cannot be empty".into()));
    }

    let db = state.db.lock().unwrap();
    permissions::require(&db, &Capability::VaultRead, workspace_id.as_deref(), "user", Some("chunk"), None)?;

    let llm = load_llm_config(&db);

    let crypto_guard = state.crypto.lock().unwrap();
    let crypto = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;

    agent::handle_question(&db, crypto, &question, workspace_id.as_deref(), &llm)
}

#[tauri::command]
pub fn get_chunk_source(chunk_id: String, state: State<AppState>) -> AppResult<retrieval::RetrievedChunk> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    permissions::require(&db, &Capability::VaultRead, None, "user", Some("chunk"), Some(&chunk_id))?;
    let crypto_guard = state.crypto.lock().unwrap();
    let crypto = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;
    retrieval::get_chunk_full_text(&db, crypto, &chunk_id)
}

// ── Memory commands ───────────────────────────────────────────────────────

#[tauri::command]
pub fn list_memories(
    status: Option<String>,
    workspace_id: Option<String>,
    state: State<AppState>,
) -> AppResult<Vec<memory_writer::Memory>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    permissions::require(&db, &Capability::VaultRead, workspace_id.as_deref(), "user", Some("memory"), None)?;
    let crypto_guard = state.crypto.lock().unwrap();
    let crypto = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;
    memory_writer::list(&db, crypto, status.as_deref(), workspace_id.as_deref())
}

#[tauri::command]
pub fn accept_memory(memory_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    permissions::require(&db, &Capability::VaultWrite, None, "user", Some("memory"), Some(&memory_id))?;
    memory_writer::accept(&db, &memory_id)?;
    audit::log_write(&db, "user", "memory", &memory_id, None)
}

#[tauri::command]
pub fn edit_memory(memory_id: String, content: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if content.trim().is_empty() {
        return Err(AppError::InvalidInput("Memory content cannot be empty".into()));
    }
    let db = state.db.lock().unwrap();
    permissions::require(&db, &Capability::VaultWrite, None, "user", Some("memory"), Some(&memory_id))?;
    let crypto_guard = state.crypto.lock().unwrap();
    let crypto = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;
    memory_writer::edit_and_accept(&db, crypto, &memory_id, &content)?;
    audit::log_write(&db, "user", "memory", &memory_id, None)
}

#[tauri::command]
pub fn reject_memory(memory_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    memory_writer::reject(&db, &memory_id)
}

#[tauri::command]
pub fn delete_memory(memory_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    permissions::require(&db, &Capability::VaultWrite, None, "user", Some("memory"), Some(&memory_id))?;
    memory_writer::delete(&db, &memory_id)?;
    audit::log(&db, audit::AuditEvent {
        actor: "user".into(),
        action_type: "delete".into(),
        capability: Some("vault.write".into()),
        target_type: Some("memory".into()),
        target_id: Some(memory_id),
        workspace_id: None,
        outcome: "allowed".into(),
        details_json: None,
    })
}

#[tauri::command]
pub fn propose_memory(
    content: String,
    memory_type: String,
    workspace_id: Option<String>,
    state: State<AppState>,
) -> AppResult<String> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    permissions::require(&db, &Capability::VaultWrite, workspace_id.as_deref(), "user", Some("memory"), None)?;
    let crypto_guard = state.crypto.lock().unwrap();
    let crypto = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;
    memory_writer::propose(&db, crypto, &content, &memory_type, "user_input", workspace_id.as_deref())
}

// ── Permissions commands ──────────────────────────────────────────────────

#[tauri::command]
pub fn list_permissions(state: State<AppState>) -> AppResult<Vec<permissions::Permission>> {
    let db = state.db.lock().unwrap();
    permissions::list_all(&db)
}

#[tauri::command]
pub fn revoke_permission(permission_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    permissions::revoke(&db, &permission_id)?;
    audit::log(&db, audit::AuditEvent {
        actor: "user".into(),
        action_type: "permission_revoke".into(),
        capability: None,
        target_type: Some("permission".into()),
        target_id: Some(permission_id),
        workspace_id: None,
        outcome: "allowed".into(),
        details_json: None,
    })
}

#[tauri::command]
pub fn check_permission(capability: String, workspace_id: Option<String>, state: State<AppState>) -> AppResult<bool> {
    let db = state.db.lock().unwrap();
    let cap = Capability::from_str(&capability)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown capability: {capability}")))?;
    permissions::check(&db, &cap, workspace_id.as_deref(), "user", None, None)
}

// ── Audit commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_audit_events(
    action_type: Option<String>,
    outcome: Option<String>,
    workspace_id: Option<String>,
    limit: Option<i64>,
    state: State<AppState>,
) -> AppResult<Vec<audit::AuditRecord>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    audit::query(&db, &audit::AuditFilter {
        action_type,
        outcome,
        workspace_id,
        limit,
    })
}

// ── Chat commands ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: String,    // "user" | "assistant" | "system"
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistoryRow {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[tauri::command]
pub fn chat_message(
    message: String,
    history: Vec<ChatTurn>,
    state: State<AppState>,
) -> AppResult<String> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if message.trim().is_empty() {
        return Err(AppError::InvalidInput("Message cannot be empty".into()));
    }

    let llm = {
        let db = state.db.lock().unwrap();
        permissions::require(&db, &Capability::ModelUse, None, "user", Some("chat"), None)?;

        let llm = load_llm_config(&db);
        if !llm.enabled {
            return Err(AppError::Connector(
                "LLM is disabled — enable a model provider in Settings first.".into(),
            ));
        }

        audit::log(&db, audit::AuditEvent {
            actor: "user".into(),
            action_type: "chat".into(),
            capability: Some("model.use".into()),
            target_type: Some("chat".into()),
            target_id: None,
            workspace_id: None,
            outcome: "allowed".into(),
            details_json: None,
        })?;

        llm
    };

    // Build full message list: history (capped at last 20 turns) + new user message
    const MAX_HISTORY_TURNS: usize = 20;
    let user_msg = message.clone();
    let mut msgs: Vec<(String, String)> = history
        .into_iter()
        .map(|t| (t.role, t.content))
        .collect();
    if msgs.len() > MAX_HISTORY_TURNS {
        let drop = msgs.len() - MAX_HISTORY_TURNS;
        msgs.drain(0..drop);
    }
    msgs.push(("user".to_string(), message));

    let reply = crate::connectors::llm_chat_multi(&llm, &msgs)?;

    // Persist both turns to chat_history
    {
        let db = state.db.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let _ = db.execute(
            "INSERT INTO chat_history(id, role, content, created_at) VALUES(?1,'user',?2,?3)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), user_msg, now],
        );
        let _ = db.execute(
            "INSERT INTO chat_history(id, role, content, created_at) VALUES(?1,'assistant',?2,?3)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), reply, now],
        );
    }

    Ok(reply)
}

/// Streaming version of chat_message. Emits `chat-token` events for each
/// text fragment, then returns () when complete. The frontend collects tokens
/// via event listener and gets the final save via `chat-done`.
#[tauri::command]
pub fn chat_message_stream(
    id: String,       // client-generated ID to match events to the right bubble
    message: String,
    history: Vec<ChatTurn>,
    state: State<AppState>,
    app_handle: tauri::AppHandle,
) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if message.trim().is_empty() {
        return Err(AppError::InvalidInput("Message cannot be empty".into()));
    }

    let (llm, user_msg) = {
        let db = state.db.lock().unwrap();
        permissions::require(&db, &Capability::ModelUse, None, "user", Some("chat"), None)?;
        let llm = load_llm_config(&db);
        if !llm.enabled {
            return Err(AppError::Connector(
                "LLM is disabled — enable a model provider in Settings first.".into(),
            ));
        }
        audit::log(&db, audit::AuditEvent {
            actor: "user".into(),
            action_type: "chat".into(),
            capability: Some("model.use".into()),
            target_type: Some("chat".into()),
            target_id: None,
            workspace_id: None,
            outcome: "allowed".into(),
            details_json: None,
        })?;
        (llm, message.clone())
    };

    const MAX_HISTORY_TURNS: usize = 20;
    let mut msgs: Vec<(String, String)> = history
        .into_iter()
        .map(|t| (t.role, t.content))
        .collect();
    if msgs.len() > MAX_HISTORY_TURNS {
        let drop = msgs.len() - MAX_HISTORY_TURNS;
        msgs.drain(0..drop);
    }
    msgs.push(("user".to_string(), message));

    let id_clone = id.clone();
    let handle = app_handle.clone();

    let reply = crate::connectors::llm_chat_multi_stream(&llm, &msgs, &move |token: &str| {
        let _ = handle.emit_all("chat-token", serde_json::json!({
            "id": id_clone,
            "token": token,
        }));
    })?;

    // Persist both turns
    {
        let db = state.db.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let _ = db.execute(
            "INSERT INTO chat_history(id, role, content, created_at) VALUES(?1,'user',?2,?3)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), user_msg, now],
        );
        let _ = db.execute(
            "INSERT INTO chat_history(id, role, content, created_at) VALUES(?1,'assistant',?2,?3)",
            rusqlite::params![uuid::Uuid::new_v4().to_string(), reply, now],
        );
    }

    Ok(())
}

#[tauri::command]
pub fn list_chat_history(limit: Option<i64>, state: State<AppState>) -> AppResult<Vec<ChatHistoryRow>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let n = limit.unwrap_or(50);
    let mut stmt = db.prepare(
        "SELECT id, role, content, created_at FROM chat_history ORDER BY created_at DESC LIMIT ?1",
    )?;
    let rows: Vec<ChatHistoryRow> = stmt.query_map(rusqlite::params![n], |r| {
        Ok(ChatHistoryRow {
            id: r.get(0)?,
            role: r.get(1)?,
            content: r.get(2)?,
            created_at: r.get(3)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    // Return chronological order (oldest first)
    Ok(rows.into_iter().rev().collect())
}

#[tauri::command]
pub fn clear_chat_history(state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    db.execute("DELETE FROM chat_history", [])?;
    Ok(())
}

// ── Agent registry commands ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateAgentInput {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub workspace_id: Option<String>,
    pub schedule: Option<String>,
    #[serde(default)]
    pub react_mode: bool,
}

#[tauri::command]
pub fn create_agent(input: CreateAgentInput, state: State<AppState>) -> AppResult<registry::UserAgent> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput("Agent name cannot be empty".into()));
    }
    let db = state.db.lock().unwrap();
    registry::create_agent(
        &db, &input.name, &input.description,
        &input.system_prompt, &input.tools,
        input.workspace_id.as_deref(),
        input.schedule.as_deref(),
        input.react_mode,
    )
}

#[tauri::command]
pub fn list_agents(state: State<AppState>) -> AppResult<Vec<registry::UserAgent>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    registry::list_agents(&db)
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentInput {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    #[serde(default)]
    pub react_mode: bool,
}

#[tauri::command]
pub fn update_agent(input: UpdateAgentInput, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput("Agent name cannot be empty".into()));
    }
    let db = state.db.lock().unwrap();
    registry::update_agent(&db, &input.id, &input.name, &input.description, &input.system_prompt, &input.tools, input.react_mode)
}

#[tauri::command]
pub fn delete_agent(agent_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    registry::delete_agent(&db, &agent_id)
}

#[tauri::command]
pub fn run_agent(
    agent_id: String,
    input: String,
    workspace_id: Option<String>,
    state: State<AppState>,
    app_handle: tauri::AppHandle,
) -> AppResult<registry::AgentRun> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if input.trim().is_empty() {
        return Err(AppError::InvalidInput("Input cannot be empty".into()));
    }
    let db = state.db.lock().unwrap();
    let llm          = load_llm_config(&db);
    let crypto_guard = state.crypto.lock().unwrap();
    let crypto       = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;

    runner::run_agent(&db, crypto, &agent_id, &input, workspace_id.as_deref(), &llm, &|step| {
        let _ = app_handle.emit_all("agent-step", step);
    })
}

// ── Pipeline commands ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreatePipelineInput {
    pub name: String,
    pub description: String,
    pub agent_ids: Vec<String>,
}

#[tauri::command]
pub fn create_pipeline(input: CreatePipelineInput, state: State<AppState>) -> AppResult<registry::AgentPipeline> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if input.name.trim().is_empty() {
        return Err(AppError::InvalidInput("Pipeline name cannot be empty".into()));
    }
    if input.agent_ids.is_empty() {
        return Err(AppError::InvalidInput("Pipeline must contain at least one agent".into()));
    }
    let db = state.db.lock().unwrap();
    registry::create_pipeline(&db, &input.name, &input.description, &input.agent_ids)
}

#[tauri::command]
pub fn list_pipelines(state: State<AppState>) -> AppResult<Vec<registry::AgentPipeline>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    registry::list_pipelines(&db)
}

#[tauri::command]
pub fn delete_pipeline(pipeline_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    registry::delete_pipeline(&db, &pipeline_id)
}

#[tauri::command]
pub fn run_pipeline(
    pipeline_id: String,
    input: String,
    workspace_id: Option<String>,
    state: State<AppState>,
    app_handle: tauri::AppHandle,
) -> AppResult<registry::AgentRun> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    if input.trim().is_empty() {
        return Err(AppError::InvalidInput("Input cannot be empty".into()));
    }
    let db = state.db.lock().unwrap();
    let llm          = load_llm_config(&db);
    let crypto_guard = state.crypto.lock().unwrap();
    let crypto       = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;

    runner::run_pipeline(&db, crypto, &pipeline_id, &input, workspace_id.as_deref(), &llm, &|step| {
        let _ = app_handle.emit_all("agent-step", step);
    })
}

#[tauri::command]
pub fn list_agent_runs(
    agent_id: Option<String>,
    limit: Option<i64>,
    state: State<AppState>,
) -> AppResult<Vec<registry::AgentRun>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    registry::list_runs(&db, agent_id.as_deref(), limit.unwrap_or(50))
}

// ── File output commands ──────────────────────────────────────────────────

/// Generate a file (txt / md / pdf) from `content` and return its absolute path.
/// `output_dir`: optional path; defaults to the user's Downloads folder.
#[tauri::command]
pub fn generate_file(
    content: String,
    filename: String,
    format: String,
    output_dir: Option<String>,
    state: State<AppState>,
) -> AppResult<String> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }

    let dir = resolve_output_dir(output_dir)?;
    let path = crate::fileout::generate(&content, &filename, &format, &dir)?;
    Ok(path.to_string_lossy().to_string())
}

/// Open a file or URL with the OS default application.
#[tauri::command]
pub fn open_file(path: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    crate::desktop::open_target(&path)
}

fn resolve_output_dir(output_dir: Option<String>) -> AppResult<PathBuf> {
    if let Some(d) = output_dir {
        if !d.is_empty() {
            return Ok(PathBuf::from(d));
        }
    }
    // Default: user's Downloads folder
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    Ok(PathBuf::from(home).join("Downloads"))
}

// ── Desktop automation commands ───────────────────────────────────────────

fn require_desktop(state: &State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    if db::get_setting(&db, "desktop_control_enabled")?.as_deref() != Some("true") {
        return Err(AppError::PermissionDenied(
            "Desktop control is disabled. Enable it in Settings → Desktop Control.".into()
        ));
    }
    Ok(())
}

/// Type a string of text via the system keyboard.
#[tauri::command]
pub fn desktop_type(text: String, state: State<AppState>) -> AppResult<()> {
    require_desktop(&state)?;
    crate::desktop::type_text(&text)
}

/// Press a key combination, e.g. "ctrl+c", "win+r", "ctrl+shift+t".
#[tauri::command]
pub fn desktop_key(combo: String, state: State<AppState>) -> AppResult<()> {
    require_desktop(&state)?;
    crate::desktop::key_combo(&combo)
}

/// Open a file, folder, URL, or application using the OS default handler.
#[tauri::command]
pub fn desktop_open(target: String, state: State<AppState>) -> AppResult<()> {
    require_desktop(&state)?;
    crate::desktop::open_target(&target)
}

// ── Telegram bot commands ─────────────────────────────────────────────────

/// Start the Telegram bot. Reads token + chat_id from settings.
#[tauri::command]
pub fn start_telegram_bot(state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }

    let (token, chat_id_str) = {
        let db = state.db.lock().unwrap();
        let token = db::get_setting(&db, "telegram_token")?.unwrap_or_default();
        let chat_id = db::get_setting(&db, "telegram_chat_id")?.unwrap_or_default();
        (token, chat_id)
    };

    if token.is_empty() {
        return Err(AppError::InvalidInput(
            "Telegram bot token is not configured. Add it in Settings → Telegram.".into()
        ));
    }

    let chat_id: i64 = chat_id_str
        .parse()
        .map_err(|_| AppError::InvalidInput(
            "Telegram chat ID must be a number. Check Settings → Telegram.".into()
        ))?;

    crate::telegram::start(token, chat_id, Arc::clone(&state.bot));
    Ok(())
}

/// Stop the Telegram bot.
#[tauri::command]
pub fn stop_telegram_bot(state: State<AppState>) -> AppResult<()> {
    crate::telegram::stop(&state.bot);
    Ok(())
}

/// Returns true if the Telegram bot polling thread is running.
#[tauri::command]
pub fn get_telegram_status(state: State<AppState>) -> bool {
    state.bot.is_running()
}

// ── Discord bot commands ──────────────────────────────────────────────────

/// Start the Discord Gateway bot. Reads token + allowed_user_id from settings.
#[tauri::command]
pub fn start_discord_bot(state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }

    let (token, allowed_user_id) = {
        let db = state.db.lock().unwrap();
        let token = db::get_setting(&db, "discord_token")?.unwrap_or_default();
        let uid   = db::get_setting(&db, "discord_allowed_user_id")?.unwrap_or_default();
        (token, uid)
    };

    if token.is_empty() {
        return Err(AppError::InvalidInput(
            "Discord bot token is not configured. Add it in Settings → Discord.".into()
        ));
    }
    if allowed_user_id.is_empty() {
        return Err(AppError::InvalidInput(
            "Discord allowed user ID is not configured. Add it in Settings → Discord.".into()
        ));
    }

    crate::discord::start(token, allowed_user_id, Arc::clone(&state.bot));
    Ok(())
}

/// Stop the Discord Gateway bot.
#[tauri::command]
pub fn stop_discord_bot(state: State<AppState>) -> AppResult<()> {
    crate::discord::stop(&state.bot);
    Ok(())
}

/// Returns true if the Discord bot thread is running.
#[tauri::command]
pub fn get_discord_status(state: State<AppState>) -> bool {
    state.bot.discord_is_running()
}

// ── WhatsApp bot commands ─────────────────────────────────────────────────

/// Start the WhatsApp webhook server. Reads config from settings.
#[tauri::command]
pub fn start_whatsapp_bot(state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }

    let (access_token, phone_number_id, allowed_number, verify_token, port_str) = {
        let db = state.db.lock().unwrap();
        let t  = db::get_setting(&db, "whatsapp_access_token")?.unwrap_or_default();
        let p  = db::get_setting(&db, "whatsapp_phone_number_id")?.unwrap_or_default();
        let n  = db::get_setting(&db, "whatsapp_allowed_number")?.unwrap_or_default();
        let v  = db::get_setting(&db, "whatsapp_verify_token")?.unwrap_or_default();
        let po = db::get_setting(&db, "whatsapp_webhook_port")?.unwrap_or_else(|| "8765".into());
        (t, p, n, v, po)
    };

    if access_token.is_empty() {
        return Err(AppError::InvalidInput(
            "WhatsApp access token is not configured. Add it in Settings → WhatsApp.".into()
        ));
    }
    if phone_number_id.is_empty() {
        return Err(AppError::InvalidInput(
            "WhatsApp Phone Number ID is not configured. Add it in Settings → WhatsApp.".into()
        ));
    }
    if allowed_number.is_empty() {
        return Err(AppError::InvalidInput(
            "WhatsApp allowed number is not configured. Add it in Settings → WhatsApp.".into()
        ));
    }

    let port: u16 = port_str.parse().unwrap_or(8765);

    crate::whatsapp::start(
        access_token, phone_number_id, allowed_number,
        verify_token, port, Arc::clone(&state.bot),
    );
    Ok(())
}

/// Stop the WhatsApp webhook server.
#[tauri::command]
pub fn stop_whatsapp_bot(state: State<AppState>) -> AppResult<()> {
    crate::whatsapp::stop(&state.bot);
    Ok(())
}

/// Returns true if the WhatsApp webhook server thread is running.
#[tauri::command]
pub fn get_whatsapp_status(state: State<AppState>) -> bool {
    state.bot.whatsapp_is_running()
}

// ── Python IDE commands ───────────────────────────────────────────────────

/// Detect the system Python interpreter.
/// Returns `PythonInfo { available: false }` if Python is not installed.
#[tauri::command]
pub fn get_python_info() -> crate::python::PythonInfo {
    crate::python::get_info()
}

/// Execute Python `code` in a sandboxed subprocess.
/// `timeout_secs` defaults to 30 if omitted.
#[tauri::command]
pub fn run_python(
    code: String,
    timeout_secs: Option<u64>,
) -> AppResult<crate::python::PythonResult> {
    let timeout = timeout_secs.unwrap_or(30).max(1).min(300);
    crate::python::run_code(&code, timeout)
}

// ── Scheduler / Hands commands ────────────────────────────────────────────

/// Return all pre-built Hand templates (hardcoded library).
#[tauri::command]
pub fn list_hand_templates() -> Vec<crate::scheduler::HandTemplate> {
    crate::scheduler::HANDS.to_vec()
}

/// Return agents that have a non-null schedule.
#[tauri::command]
pub fn get_scheduled_agents(state: State<AppState>) -> AppResult<Vec<registry::UserAgent>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    registry::list_scheduled_agents(&db)
}

/// Returns true if the background scheduler thread is running.
#[tauri::command]
pub fn get_scheduler_status(state: State<AppState>) -> bool {
    state.bot.scheduler_is_running()
}

/// Immediately execute an agent regardless of its schedule.
#[tauri::command]
pub fn trigger_agent_now(
    agent_id: String,
    state: State<AppState>,
) -> AppResult<registry::AgentRun> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let llm          = load_llm_config(&db);
    let crypto_guard = state.crypto.lock().unwrap();
    let crypto       = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;
    let run = runner::run_agent(&db, crypto, &agent_id, "(manual trigger)", None, &llm, &|_| {})?;
    // Update last_run_at so the scheduler counts this as a run
    let ts = chrono::Utc::now().to_rfc3339();
    let _ = registry::update_last_run(&db, &agent_id, &ts);
    Ok(run)
}

/// Pause an agent's schedule (scheduler will skip it until resumed).
#[tauri::command]
pub fn pause_agent_schedule(agent_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    registry::set_schedule_paused(&db, &agent_id, true)
}

/// Resume a previously paused agent schedule.
#[tauri::command]
pub fn resume_agent_schedule(agent_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    registry::set_schedule_paused(&db, &agent_id, false)
}

/// Re-run a past agent run with the same agent and input.
#[tauri::command]
pub fn replay_agent_run(
    run_id: String,
    state: State<AppState>,
    app_handle: tauri::AppHandle,
) -> AppResult<registry::AgentRun> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();

    // Load the original run to get agent_id + input
    let orig = db.query_row(
        "SELECT agent_id, input, workspace_id FROM agent_runs WHERE id=?1",
        rusqlite::params![run_id],
        |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        )),
    ).map_err(|_| AppError::NotFound(format!("Run '{run_id}' not found")))?;

    let (agent_id, input, workspace_id) = orig;
    let llm          = load_llm_config(&db);
    let crypto_guard = state.crypto.lock().unwrap();
    let crypto       = crypto_guard.as_ref().ok_or(AppError::VaultLocked)?;

    runner::run_agent(&db, crypto, &agent_id, &input, workspace_id.as_deref(), &llm, &|step| {
        let _ = app_handle.emit_all("agent-step", step);
    })
}

// ── LLM config helper ─────────────────────────────────────────────────────

/// Read all LLM settings from DB into a unified `LlmConfig`.
fn load_llm_config(conn: &rusqlite::Connection) -> crate::connectors::LlmConfig {
    let get = |key: &str| db::get_setting(conn, key).ok().flatten().unwrap_or_default();
    let provider = get("model_provider");
    let embed_model = if provider == "openai" {
        "text-embedding-3-small".to_string()
    } else {
        let m = get("ollama_embed_model");
        if m.is_empty() { "nomic-embed-text".to_string() } else { m }
    };
    crate::connectors::LlmConfig {
        enabled:            get("model_enabled") == "true",
        provider,
        ollama_url:         get("ollama_url"),
        ollama_model:       get("ollama_model"),
        embeddings_enabled: get("embeddings_enabled") == "true",
        embed_model,
        openai_api_key:     get("openai_api_key"),
        openai_model:       get("openai_model"),
    }
}

// ── File dialog helper ────────────────────────────────────────────────────

#[tauri::command]
pub fn open_folder_dialog() -> Option<String> {
    tauri::api::dialog::blocking::FileDialogBuilder::new()
        .set_title("Select Workspace Folder")
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string())
}

// ── Artifacts ─────────────────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ArtifactRow {
    pub id:            String,
    pub agent_id:      Option<String>,
    pub agent_name:    String,
    pub title:         String,
    pub content:       String,
    pub artifact_type: String,
    pub created_at:    String,
}

#[tauri::command]
pub fn list_artifacts(
    agent_id: Option<String>,
    limit: Option<i64>,
    state: State<AppState>,
) -> AppResult<Vec<ArtifactRow>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let n = limit.unwrap_or(100);
    let rows = if let Some(aid) = &agent_id {
        let mut s = db.prepare(
            "SELECT id, agent_id, agent_name, title, content, artifact_type, created_at \
             FROM artifacts WHERE agent_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let x = s.query_map(rusqlite::params![aid, n], artifact_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        x
    } else {
        let mut s = db.prepare(
            "SELECT id, agent_id, agent_name, title, content, artifact_type, created_at \
             FROM artifacts ORDER BY created_at DESC LIMIT ?1",
        )?;
        let x = s.query_map(rusqlite::params![n], artifact_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        x
    };
    Ok(rows)
}

fn artifact_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactRow> {
    Ok(ArtifactRow {
        id:            r.get(0)?,
        agent_id:      r.get(1)?,
        agent_name:    r.get(2)?,
        title:         r.get(3)?,
        content:       r.get(4)?,
        artifact_type: r.get(5)?,
        created_at:    r.get(6)?,
    })
}

#[tauri::command]
pub fn delete_artifact(artifact_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    db.execute("DELETE FROM artifacts WHERE id=?1", rusqlite::params![artifact_id])?;
    Ok(())
}

// ── MCP Servers ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_mcp_servers(state: State<AppState>) -> AppResult<Vec<crate::mcp::McpServerConfig>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    crate::mcp::list_servers(&db)
}

#[tauri::command]
pub fn add_mcp_server(
    label: String,
    command: String,
    args: Vec<String>,
    state: State<AppState>,
) -> AppResult<crate::mcp::McpServerConfig> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let cfg = crate::mcp::add_server(&db, &label, &command, &args)?;
    // Test connection + cache tool list
    match crate::mcp::list_tools_from_server(&cfg) {
        Ok(tools) => { crate::mcp::cache_tools(&db, &cfg.id, &tools)?; }
        Err(e) => {
            // Don't fail — server added but tool list is empty; user can refresh later
            eprintln!("MCP: could not list tools for '{label}': {e}");
        }
    }
    Ok(cfg)
}

#[tauri::command]
pub fn remove_mcp_server(server_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    crate::mcp::remove_server(&db, &server_id)
}

#[tauri::command]
pub fn refresh_mcp_tools(server_id: String, state: State<AppState>) -> AppResult<Vec<crate::mcp::McpTool>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let servers = crate::mcp::list_servers(&db)?;
    let cfg = servers.into_iter().find(|s| s.id == server_id)
        .ok_or_else(|| AppError::NotFound(format!("MCP server {server_id}")))?;
    let tools = crate::mcp::list_tools_from_server(&cfg)?;
    crate::mcp::cache_tools(&db, &server_id, &tools)?;
    Ok(tools)
}

// ── Task Queue ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskQueueItem {
    pub id:           String,
    pub agent_id:     String,
    pub agent_name:   String,
    pub input:        String,
    pub status:       String,         // pending | running | done | failed | cancelled
    pub priority:     i64,
    pub workspace_id: Option<String>,
    pub created_at:   String,
    pub started_at:   Option<String>,
    pub completed_at: Option<String>,
    pub output:       Option<String>,
    pub error:        Option<String>,
}

fn task_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TaskQueueItem> {
    Ok(TaskQueueItem {
        id:           r.get(0)?,
        agent_id:     r.get(1)?,
        agent_name:   r.get(2)?,
        input:        r.get(3)?,
        status:       r.get(4)?,
        priority:     r.get(5)?,
        workspace_id: r.get(6)?,
        created_at:   r.get(7)?,
        started_at:   r.get(8)?,
        completed_at: r.get(9)?,
        output:       r.get(10)?,
        error:        r.get(11)?,
    })
}

const TASK_SELECT: &str =
    "SELECT tq.id, tq.agent_id, COALESCE(ua.name, tq.agent_id), tq.input, \
            tq.status, tq.priority, tq.workspace_id, tq.created_at, \
            tq.started_at, tq.completed_at, tq.output, tq.error \
     FROM task_queue tq \
     LEFT JOIN user_agents ua ON ua.id = tq.agent_id";

#[tauri::command]
pub fn enqueue_task(
    agent_id:    String,
    input:       String,
    priority:    Option<i64>,
    workspace_id: Option<String>,
    state: State<AppState>,
) -> AppResult<TaskQueueItem> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let prio = priority.unwrap_or(0);
    db.execute(
        "INSERT INTO task_queue(id, agent_id, input, status, priority, workspace_id, created_at) \
         VALUES(?1,?2,?3,'pending',?4,?5,?6)",
        rusqlite::params![id, agent_id, input, prio, workspace_id, now],
    )?;
    let mut stmt = db.prepare(&format!("{TASK_SELECT} WHERE tq.id=?1"))?;
    let item = stmt.query_row(rusqlite::params![id], task_from_row)?;
    Ok(item)
}

#[tauri::command]
pub fn list_tasks(
    agent_id: Option<String>,
    status:   Option<String>,
    limit:    Option<i64>,
    state: State<AppState>,
) -> AppResult<Vec<TaskQueueItem>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let n = limit.unwrap_or(200);
    let sql = match (&agent_id, &status) {
        (Some(_), Some(_)) =>
            format!("{TASK_SELECT} WHERE tq.agent_id=?1 AND tq.status=?2 ORDER BY tq.created_at DESC LIMIT ?3"),
        (Some(_), None) =>
            format!("{TASK_SELECT} WHERE tq.agent_id=?1 ORDER BY tq.created_at DESC LIMIT ?2"),
        (None, Some(_)) =>
            format!("{TASK_SELECT} WHERE tq.status=?1 ORDER BY tq.created_at DESC LIMIT ?2"),
        (None, None) =>
            format!("{TASK_SELECT} ORDER BY tq.created_at DESC LIMIT ?1"),
    };
    let mut stmt = db.prepare(&sql)?;
    let rows = match (&agent_id, &status) {
        (Some(a), Some(s)) =>
            stmt.query_map(rusqlite::params![a, s, n], task_from_row)?,
        (Some(a), None) =>
            stmt.query_map(rusqlite::params![a, n], task_from_row)?,
        (None, Some(s)) =>
            stmt.query_map(rusqlite::params![s, n], task_from_row)?,
        (None, None) =>
            stmt.query_map(rusqlite::params![n], task_from_row)?,
    };
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
pub fn cancel_task(task_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    db.execute(
        "UPDATE task_queue SET status='cancelled', completed_at=?1 \
         WHERE id=?2 AND status='pending'",
        rusqlite::params![now, task_id],
    )?;
    Ok(())
}

#[tauri::command]
pub fn get_task(task_id: String, state: State<AppState>) -> AppResult<TaskQueueItem> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare(&format!("{TASK_SELECT} WHERE tq.id=?1"))?;
    let item = stmt.query_row(rusqlite::params![task_id], task_from_row)
        .map_err(|_| AppError::NotFound(format!("Task {task_id}")))?;
    Ok(item)
}

// ── Agent Scratchpad ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ScratchpadEntry {
    pub key:        String,
    pub value:      String,
    pub updated_at: String,
}

#[tauri::command]
pub fn list_scratchpad(agent_id: String, state: State<AppState>) -> AppResult<Vec<ScratchpadEntry>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare(
        "SELECT key, value, updated_at FROM agent_scratchpad \
         WHERE agent_id=?1 ORDER BY key",
    )?;
    let entries = stmt
        .query_map(rusqlite::params![agent_id], |r| Ok(ScratchpadEntry {
            key:        r.get(0)?,
            value:      r.get(1)?,
            updated_at: r.get(2)?,
        }))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(entries)
}

#[tauri::command]
pub fn delete_scratchpad_key(
    agent_id: String,
    key: String,
    state: State<AppState>,
) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    db.execute(
        "DELETE FROM agent_scratchpad WHERE agent_id=?1 AND key=?2",
        rusqlite::params![agent_id, key],
    )?;
    Ok(())
}

// ── Watch Triggers ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatchTriggerRow {
    pub id:         String,
    pub agent_id:   String,
    pub agent_name: String,
    pub watch_path: String,
    pub recursive:  bool,
    pub enabled:    bool,
    pub created_at: String,
}

fn trigger_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<WatchTriggerRow> {
    Ok(WatchTriggerRow {
        id:         r.get(0)?,
        agent_id:   r.get(1)?,
        agent_name: r.get(2)?,
        watch_path: r.get(3)?,
        recursive:  r.get::<_, i32>(4)? != 0,
        enabled:    r.get::<_, i32>(5)? != 0,
        created_at: r.get(6)?,
    })
}

const TRIGGER_SELECT: &str =
    "SELECT wt.id, wt.agent_id, COALESCE(ua.name, wt.agent_id), wt.watch_path, \
            wt.recursive, wt.enabled, wt.created_at \
     FROM watch_triggers wt \
     LEFT JOIN user_agents ua ON ua.id = wt.agent_id";

#[tauri::command]
pub fn add_trigger(
    agent_id:   String,
    watch_path: String,
    recursive:  Option<bool>,
    state: State<AppState>,
) -> AppResult<WatchTriggerRow> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db  = state.db.lock().unwrap();
    let id  = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let rec = if recursive.unwrap_or(true) { 1i32 } else { 0 };
    db.execute(
        "INSERT INTO watch_triggers(id, agent_id, watch_path, recursive, enabled, created_at) \
         VALUES(?1,?2,?3,?4,1,?5)",
        rusqlite::params![id, agent_id, watch_path, rec, now],
    )?;
    let mut stmt = db.prepare(&format!("{TRIGGER_SELECT} WHERE wt.id=?1"))?;
    let t = stmt.query_row(rusqlite::params![id], trigger_from_row)?;
    Ok(t)
}

#[tauri::command]
pub fn list_triggers(state: State<AppState>) -> AppResult<Vec<WatchTriggerRow>> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare(&format!("{TRIGGER_SELECT} ORDER BY wt.created_at DESC"))?;
    let rows = stmt
        .query_map([], trigger_from_row)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

#[tauri::command]
pub fn remove_trigger(trigger_id: String, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();
    db.execute("DELETE FROM watch_triggers WHERE id=?1", rusqlite::params![trigger_id])?;
    Ok(())
}

#[tauri::command]
pub fn toggle_trigger(trigger_id: String, enabled: bool, state: State<AppState>) -> AppResult<()> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db  = state.db.lock().unwrap();
    let val = if enabled { 1i32 } else { 0 };
    db.execute(
        "UPDATE watch_triggers SET enabled=?1 WHERE id=?2",
        rusqlite::params![val, trigger_id],
    )?;
    Ok(())
}

#[tauri::command]
pub fn get_watcher_status(state: State<AppState>) -> bool {
    state.bot.watcher_is_running()
}

// ── System Status (Activity dashboard) ───────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AgentRunSummary {
    pub id:         String,
    pub agent_id:   String,
    pub agent_name: String,
    pub status:     String,
    pub step_count: usize,
    pub output:     Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct SystemStatus {
    pub scheduler_running:  bool,
    pub watcher_running:    bool,
    pub bot_running:        bool,
    pub pending_tasks:      i64,
    pub running_tasks:      i64,
    pub active_triggers:    i64,
    pub scheduled_agents:   i64,
    pub recent_runs:        Vec<AgentRunSummary>,
}

#[tauri::command]
pub fn get_system_status(state: State<AppState>) -> AppResult<SystemStatus> {
    if !state.is_unlocked() { return Err(AppError::VaultLocked); }
    let db = state.db.lock().unwrap();

    let pending_tasks: i64 = db.query_row(
        "SELECT COUNT(*) FROM task_queue WHERE status='pending'", [], |r| r.get(0)
    ).unwrap_or(0);
    let running_tasks: i64 = db.query_row(
        "SELECT COUNT(*) FROM task_queue WHERE status='running'", [], |r| r.get(0)
    ).unwrap_or(0);
    let active_triggers: i64 = db.query_row(
        "SELECT COUNT(*) FROM watch_triggers WHERE enabled=1", [], |r| r.get(0)
    ).unwrap_or(0);
    let scheduled_agents: i64 = db.query_row(
        "SELECT COUNT(*) FROM user_agents WHERE schedule IS NOT NULL AND schedule != '' AND schedule_paused=0",
        [], |r| r.get(0)
    ).unwrap_or(0);

    let recent_runs = {
        let runs = agent::registry::list_runs(&db, None, 15).unwrap_or_default();
        runs.into_iter().map(|r| {
            let agent_name = db
                .query_row("SELECT name FROM user_agents WHERE id=?1", rusqlite::params![r.agent_id], |row| row.get::<_, String>(0))
                .unwrap_or_else(|_| r.agent_id.clone());
            AgentRunSummary {
                id:         r.id,
                agent_id:   r.agent_id,
                agent_name,
                status:     r.status,
                step_count: r.steps.len(),
                output:     r.output,
                created_at: r.created_at,
            }
        }).collect()
    };

    Ok(SystemStatus {
        scheduler_running: state.bot.scheduler_is_running(),
        watcher_running:   state.bot.watcher_is_running(),
        bot_running:       state.bot.is_running(),
        pending_tasks,
        running_tasks,
        active_triggers,
        scheduled_agents,
        recent_runs,
    })
}
