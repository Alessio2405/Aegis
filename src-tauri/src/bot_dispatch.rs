//! Shared command dispatcher for Discord and WhatsApp bots.
//! Returns plain-text strings suitable for both platforms.
//! (Telegram keeps its own HTML-formatted handlers in telegram/mod.rs.)

use crate::state::BotSharedState;

// ── Public dispatch entry point ────────────────────────────────────────────

/// Parse and execute a bot command. Returns a plain-text reply.
pub fn dispatch(text: &str, bot: &BotSharedState) -> String {
    let mut parts = text.splitn(3, char::is_whitespace);
    let raw_cmd = parts.next().unwrap_or("").to_lowercase();
    // Strip leading '/' and any '@BotName' suffix (Discord/Telegram group chats add it)
    let raw_cmd = raw_cmd.trim_start_matches('/');
    let cmd = raw_cmd.split('@').next().unwrap_or("");

    let arg1 = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("").trim();

    match cmd {
        "start" | "help" => help_text(),
        "status"         => status(bot),
        "locked"         => locked_status(bot),
        "agents"         => list_agents(bot),
        "inbox"          => list_inbox(bot),
        "run" => {
            if arg1.is_empty() {
                "Usage: /run <agent_name> <input>".into()
            } else if rest.is_empty() {
                "Usage: /run <agent_name> <input>\nInput cannot be empty.".into()
            } else {
                run_agent(bot, arg1, rest)
            }
        }
        "approve" => {
            if arg1.is_empty() {
                "Usage: /approve <memory_id_prefix>".into()
            } else {
                approve_memory(bot, arg1)
            }
        }
        "reject" => {
            if arg1.is_empty() {
                "Usage: /reject <memory_id_prefix>".into()
            } else {
                reject_memory(bot, arg1)
            }
        }
        _ => format!(
            "Unknown command: {}\n\nType /help for available commands.",
            text
        ),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn open_db(bot: &BotSharedState) -> Result<rusqlite::Connection, String> {
    rusqlite::Connection::open(&bot.db_path).map_err(|e| format!("DB error: {e}"))
}

pub fn locked_status(bot: &BotSharedState) -> String {
    let key = bot.shared_key.lock().unwrap();
    if key.is_some() {
        "Vault: UNLOCKED".into()
    } else {
        "Vault: LOCKED\n\nUnlock it from the desktop app first.".into()
    }
}

// ── Command handlers ───────────────────────────────────────────────────────

fn help_text() -> String {
    "Aegis Agent OS\n\n\
     /status          — App & vault status\n\
     /agents          — List available agents\n\
     /run <name> <input> — Run an agent\n\
     /inbox           — Pending memory proposals\n\
     /approve <id>    — Accept a memory proposal\n\
     /reject <id>     — Reject a memory proposal\n\
     /locked          — Check vault lock state\n\
     /help            — This message"
        .into()
}

fn status(bot: &BotSharedState) -> String {
    let vault_state = {
        let key = bot.shared_key.lock().unwrap();
        if key.is_some() { "UNLOCKED" } else { "LOCKED" }
    };
    format!("Aegis Agent OS\nVault: {vault_state}\nBot: RUNNING")
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
        Ok(agents) if agents.is_empty() => {
            "No agents defined yet.\n\nCreate one from the desktop app → Agents tab.".into()
        }
        Ok(agents) => {
            let mut out = format!("Agents ({})\n\n", agents.len());
            for a in &agents {
                let tools = if a.tools.is_empty() {
                    "none".to_string()
                } else {
                    a.tools.join(", ")
                };
                out.push_str(&format!(
                    "• {} — {}\n  Tools: {}\n\n",
                    a.name, a.description, tools
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
        Ok(mems) if mems.is_empty() => "No pending memory proposals.".into(),
        Ok(mems) => {
            let count = mems.len();
            let mut out = format!("Memory proposals ({})\n\n", count);
            for m in mems.iter().take(10) {
                let preview = m.plaintext_preview.as_deref().unwrap_or("(encrypted)");
                let preview = if preview.len() > 100 { &preview[..100] } else { preview };
                let short_id = if m.id.len() >= 8 { &m.id[..8] } else { &m.id };
                out.push_str(&format!(
                    "[{}] [{}]\n{}\n\n",
                    short_id, m.memory_type, preview
                ));
            }
            if count > 10 {
                out.push_str(&format!("… and {} more\n\n", count - 10));
            }
            out.push_str("Use /approve <id> or /reject <id>");
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
    .map_err(|_| format!("No proposed memory found with ID prefix '{partial}'"))
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
        Ok(_) => format!("Memory {partial_id} accepted."),
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
        Ok(_) => format!("Memory {partial_id} rejected."),
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
            "Agent '{}' not found.\n\nUse /agents to see available agents.",
            agent_name
        ),
    };

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
            let output = if output.len() > 3000 {
                format!("{}…\n\n(truncated — full output saved in Run history)", &output[..3000])
            } else {
                output
            };
            format!("Agent: {}\nInput: {}\n\n{}", agent.name, input, output)
        }
        Err(e) => format!("Error running agent: {e}"),
    }
}
