//! Telegram bot — long-poll loop using reqwest::blocking.
//!
//! Security model:
//!   - Only messages from the configured `allowed_chat_id` are processed.
//!   - Commands that require vault access return "Vault locked" if the vault is not open.
//!   - The bot never exposes raw key material over Telegram.
//!
//! The bot thread is entirely independent of the main Tauri event loop. It communicates
//! with the app only via `BotSharedState` (db_path, shared_key Arc<Mutex<…>>, running flag).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

use crate::state::BotSharedState;

// ── Telegram API types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TgApiResponse<T> {
    ok: bool,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct TgUpdate {
    update_id: i64,
    message: Option<TgMessage>,
}

#[derive(Debug, Deserialize)]
struct TgMessage {
    chat: TgChat,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TgChat {
    id: i64,
}

#[derive(Debug, Serialize)]
struct SendMessagePayload<'a> {
    chat_id: i64,
    text: &'a str,
    parse_mode: &'a str,
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Spawn the Telegram bot polling thread.
/// Safe to call multiple times — if already running, returns immediately.
pub fn start(token: String, allowed_chat_id: i64, bot: Arc<BotSharedState>) {
    // compare_exchange: only set running=true if it was false
    if bot.running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        eprintln!("[telegram] Bot is already running — ignoring start()");
        return;
    }

    std::thread::spawn(move || {
        eprintln!("[telegram] Bot thread started (chat_id={allowed_chat_id})");
        run_loop(&token, allowed_chat_id, &bot);
        bot.running.store(false, Ordering::SeqCst);
        eprintln!("[telegram] Bot thread exited.");
    });
}

/// Signal the bot polling thread to stop. It will exit on the next poll cycle (~5 s).
pub fn stop(bot: &BotSharedState) {
    bot.running.store(false, Ordering::SeqCst);
}

// ── Polling loop ───────────────────────────────────────────────────────────

fn run_loop(token: &str, allowed_chat_id: i64, bot: &BotSharedState) {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client");

    let api_base = format!("https://api.telegram.org/bot{token}");
    let mut offset: i64 = 0;

    while bot.running.load(Ordering::SeqCst) {
        let url = format!("{api_base}/getUpdates?timeout=5&offset={offset}");

        let resp = match client.get(&url).send() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[telegram] getUpdates network error: {e}");
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            }
        };

        let updates: TgApiResponse<Vec<TgUpdate>> = match resp.json() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[telegram] JSON parse error: {e}");
                continue;
            }
        };

        if !updates.ok {
            continue;
        }

        for update in updates.result.unwrap_or_default() {
            offset = update.update_id + 1;

            if let Some(msg) = update.message {
                // Security: ignore any chat that isn't the configured one
                if msg.chat.id != allowed_chat_id {
                    continue;
                }
                if let Some(text) = msg.text {
                    let reply = dispatch(text.trim(), bot);
                    send_message(&client, &api_base, allowed_chat_id, &reply);
                }
            }
        }
    }
}

fn send_message(client: &reqwest::blocking::Client, api_base: &str, chat_id: i64, text: &str) {
    // Split long messages so Telegram's 4096-char limit isn't exceeded
    let chunks: Vec<&str> = split_message(text);
    let url = format!("{api_base}/sendMessage");
    for chunk in chunks {
        let payload = SendMessagePayload { chat_id, text: chunk, parse_mode: "HTML" };
        if let Err(e) = client.post(&url).json(&payload).send() {
            eprintln!("[telegram] sendMessage error: {e}");
        }
    }
}

fn split_message(text: &str) -> Vec<&str> {
    const LIMIT: usize = 4000;
    if text.len() <= LIMIT {
        return vec![text];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = (start + LIMIT).min(text.len());
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}

// ── Command dispatcher ─────────────────────────────────────────────────────

fn dispatch(text: &str, bot: &BotSharedState) -> String {
    // Split on whitespace, up to 3 parts: cmd arg1 rest…
    let mut parts = text.splitn(3, char::is_whitespace);
    let raw_cmd = parts.next().unwrap_or("").to_lowercase();
    // Strip @BotName suffix (Telegram adds it in group chats)
    let cmd = raw_cmd.split('@').next().unwrap_or("");

    let arg1 = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("").trim();

    match cmd {
        "/start" | "/help" => help_text(),
        "/status"          => status(bot),
        "/locked"          => locked_status(bot),
        "/agents"          => list_agents(bot),
        "/inbox"           => list_inbox(bot),
        "/run" => {
            if arg1.is_empty() {
                "Usage: /run &lt;agent_name&gt; &lt;input&gt;".into()
            } else if rest.is_empty() {
                "Usage: /run &lt;agent_name&gt; &lt;input&gt;\nInput cannot be empty.".into()
            } else {
                run_agent(bot, arg1, rest)
            }
        }
        "/approve" => {
            if arg1.is_empty() {
                "Usage: /approve &lt;memory_id_prefix&gt;".into()
            } else {
                approve_memory(bot, arg1)
            }
        }
        "/reject" => {
            if arg1.is_empty() {
                "Usage: /reject &lt;memory_id_prefix&gt;".into()
            } else {
                reject_memory(bot, arg1)
            }
        }
        _ => format!(
            "Unknown command: <code>{}</code>\n\nType /help for available commands.",
            html_escape(text)
        ),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ── Command handlers ───────────────────────────────────────────────────────

fn help_text() -> String {
    "<b>Alessio Doria Agent Coordinator OS</b>\n\n\
     /status — App &amp; vault status\n\
     /agents — List available agents\n\
     /run &lt;name&gt; &lt;input&gt; — Run an agent\n\
     /inbox — Pending memory proposals\n\
     /approve &lt;id&gt; — Accept a memory proposal\n\
     /reject &lt;id&gt; — Reject a memory proposal\n\
     /locked — Check vault lock state\n\
     /help — This message"
        .into()
}

fn locked_status(bot: &BotSharedState) -> String {
    let key = bot.shared_key.lock().unwrap();
    if key.is_some() {
        "Vault: <b>UNLOCKED</b> 🔓".into()
    } else {
        "Vault: <b>LOCKED</b> 🔒\n\nUnlock it from the desktop app first.".into()
    }
}

fn status(bot: &BotSharedState) -> String {
    let vault_state = {
        let key = bot.shared_key.lock().unwrap();
        if key.is_some() { "UNLOCKED 🔓" } else { "LOCKED 🔒" }
    };
    format!(
        "🖥️ <b>Alessio Doria Agent Coordinator OS</b>\nVault: <b>{vault_state}</b>\nBot: RUNNING ✅"
    )
}

fn open_db(bot: &BotSharedState) -> Result<rusqlite::Connection, String> {
    rusqlite::Connection::open(&bot.db_path)
        .map_err(|e| format!("DB error: {e}"))
}

fn list_agents(bot: &BotSharedState) -> String {
    {
        let key = bot.shared_key.lock().unwrap();
        if key.is_none() {
            return locked_status(bot);
        }
    }
    let conn = match open_db(bot) {
        Ok(c) => c,
        Err(e) => return e,
    };
    match crate::agent::registry::list_agents(&conn) {
        Ok(agents) if agents.is_empty() => "No agents defined yet.\n\nCreate one from the desktop app → Agents tab.".into(),
        Ok(agents) => {
            let mut out = format!("🤖 <b>Agents ({})</b>\n\n", agents.len());
            for a in &agents {
                let tools = if a.tools.is_empty() {
                    "none".to_string()
                } else {
                    a.tools.join(", ")
                };
                out.push_str(&format!(
                    "• <b>{}</b> — {}\n  <i>Tools: {}</i>\n\n",
                    html_escape(&a.name),
                    html_escape(&a.description),
                    html_escape(&tools),
                ));
            }
            out.trim_end().to_string()
        }
        Err(e) => format!("Error listing agents: {e}"),
    }
}

fn list_inbox(bot: &BotSharedState) -> String {
    let raw_key = {
        let guard = bot.shared_key.lock().unwrap();
        match *guard {
            Some(k) => k,
            None => return locked_status(bot),
        }
    };
    let conn = match open_db(bot) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let crypto = crate::crypto::CryptoContext::from_key_bytes(raw_key);
    match crate::agent::memory_writer::list(&conn, &crypto, Some("proposed"), None) {
        Ok(mems) if mems.is_empty() => "📭 No pending memory proposals.".into(),
        Ok(mems) => {
            let count = mems.len();
            let mut out = format!("📬 <b>Memory proposals ({count})</b>\n\n");
            for m in mems.iter().take(10) {
                let preview = m.plaintext_preview.as_deref().unwrap_or("(encrypted)");
                let preview = if preview.len() > 100 { &preview[..100] } else { preview };
                // Show the first 8 chars of the UUID as shorthand
                let short_id = if m.id.len() >= 8 { &m.id[..8] } else { &m.id };
                out.push_str(&format!(
                    "<code>{short_id}</code> [{}]\n{}\n\n",
                    html_escape(&m.memory_type),
                    html_escape(preview),
                ));
            }
            if count > 10 {
                out.push_str(&format!("<i>… and {} more</i>\n\n", count - 10));
            }
            out.push_str("Use <code>/approve &lt;id&gt;</code> or <code>/reject &lt;id&gt;</code>");
            out
        }
        Err(e) => format!("Error listing inbox: {e}"),
    }
}

fn resolve_memory_id(conn: &rusqlite::Connection, partial: &str) -> Result<String, String> {
    let pattern = format!("{partial}%");
    conn.query_row(
        "SELECT id FROM memories WHERE id LIKE ?1 AND status='proposed' LIMIT 1",
        rusqlite::params![pattern],
        |r| r.get::<_, String>(0),
    )
    .map_err(|_| format!("No proposed memory found with ID prefix '<code>{partial}</code>'"))
}

fn approve_memory(bot: &BotSharedState, partial_id: &str) -> String {
    {
        let key = bot.shared_key.lock().unwrap();
        if key.is_none() {
            return locked_status(bot);
        }
    }
    let conn = match open_db(bot) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let full_id = match resolve_memory_id(&conn, partial_id) {
        Ok(id) => id,
        Err(e) => return e,
    };
    match crate::agent::memory_writer::accept(&conn, &full_id) {
        Ok(_) => format!("✅ Memory <code>{partial_id}</code> accepted."),
        Err(e) => format!("Error accepting memory: {e}"),
    }
}

fn reject_memory(bot: &BotSharedState, partial_id: &str) -> String {
    {
        let key = bot.shared_key.lock().unwrap();
        if key.is_none() {
            return locked_status(bot);
        }
    }
    let conn = match open_db(bot) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let full_id = match resolve_memory_id(&conn, partial_id) {
        Ok(id) => id,
        Err(e) => return e,
    };
    match crate::agent::memory_writer::reject(&conn, &full_id) {
        Ok(_) => format!("❌ Memory <code>{partial_id}</code> rejected."),
        Err(e) => format!("Error rejecting memory: {e}"),
    }
}

fn run_agent(bot: &BotSharedState, agent_name: &str, input: &str) -> String {
    let raw_key = {
        let guard = bot.shared_key.lock().unwrap();
        match *guard {
            Some(k) => k,
            None => return locked_status(bot),
        }
    };
    let conn = match open_db(bot) {
        Ok(c) => c,
        Err(e) => return e,
    };

    // Find agent by name (case-insensitive)
    let agents = match crate::agent::registry::list_agents(&conn) {
        Ok(a) => a,
        Err(e) => return format!("Error listing agents: {e}"),
    };
    let agent = match agents
        .iter()
        .find(|a| a.name.to_lowercase() == agent_name.to_lowercase())
    {
        Some(a) => a.clone(),
        None => return format!(
            "Agent '<b>{}</b>' not found.\n\nUse /agents to see available agents.",
            html_escape(agent_name)
        ),
    };

    // Load LLM config from DB
    let get_s = |key: &str| crate::db::get_setting(&conn, key).ok().flatten().unwrap_or_default();
    let provider = get_s("model_provider");
    let embed_model = if provider == "openai" {
        "text-embedding-3-small".to_string()
    } else {
        let m = get_s("ollama_embed_model");
        if m.is_empty() { "nomic-embed-text".to_string() } else { m }
    };
    let llm = crate::connectors::LlmConfig {
        enabled:            get_s("model_enabled") == "true",
        provider,
        ollama_url:         get_s("ollama_url"),
        ollama_model:       get_s("ollama_model"),
        embeddings_enabled: get_s("embeddings_enabled") == "true",
        embed_model,
        openai_api_key:     get_s("openai_api_key"),
        openai_model:       get_s("openai_model"),
        anthropic_api_key:  get_s("anthropic_api_key"),
        anthropic_model:    get_s("anthropic_model"),
    };

    let crypto = crate::crypto::CryptoContext::from_key_bytes(raw_key);

    match crate::agent::runner::run_agent(
        &conn, &crypto, &agent.id, input, None, &llm, &|_| {},
    ) {
        Ok(run) => {
            let output = run.output.unwrap_or_else(|| "No output produced.".into());
            // Truncate very long outputs
            let output = if output.len() > 3000 {
                format!("{}…\n\n<i>(truncated — full output saved in Run history)</i>", &output[..3000])
            } else {
                output
            };
            format!(
                "✅ <b>Agent: {}</b>\n<b>Input:</b> {}\n\n{}",
                html_escape(&agent.name),
                html_escape(input),
                html_escape(&output),
            )
        }
        Err(e) => format!("Error running agent: {e}"),
    }
}
