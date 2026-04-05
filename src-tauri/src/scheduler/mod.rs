//! Agent scheduler — background thread that runs agents on a repeating interval.
//!
//! Schedule format: "every Nm" or "every Nh"
//!   "every 5m"  → every 5 minutes
//!   "every 1h"  → every hour
//!   "every 24h" → daily
//!
//! The scheduler wakes every 60 seconds, checks for due agents, and executes them.
//! It requires the vault to be unlocked (reads shared_key from BotSharedState).

use std::sync::Arc;
use std::time::Duration;
use std::sync::atomic::Ordering;

use serde::Serialize;

use crate::state::BotSharedState;
use crate::db;
use crate::connectors::LlmConfig;
use crate::crypto::CryptoContext;
use crate::agent::{registry, runner};

// ── Public API ──────────────────────────────────────────────────────────────

/// Start the scheduler background thread.
pub fn start(bot: Arc<BotSharedState>) {
    if bot.scheduler_running.compare_exchange(
        false, true, Ordering::SeqCst, Ordering::SeqCst,
    ).is_err() {
        return; // already running
    }

    std::thread::spawn(move || {
        run_loop(&bot);
        bot.scheduler_running.store(false, Ordering::SeqCst);
    });
}

/// Stop the scheduler. The background thread will exit on its next wake cycle.
pub fn stop(bot: &BotSharedState) {
    bot.scheduler_running.store(false, Ordering::SeqCst);
}

// ── Main loop ───────────────────────────────────────────────────────────────

fn run_loop(bot: &BotSharedState) {
    eprintln!("[scheduler] started");

    loop {
        // Sleep 60s in 1-second increments so we can exit quickly on stop()
        for _ in 0..60 {
            if !bot.scheduler_running.load(Ordering::SeqCst) {
                eprintln!("[scheduler] stopping");
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }

        // Require vault to be unlocked
        let key = match bot.shared_key.lock().unwrap().clone() {
            Some(k) => k,
            None => continue, // vault locked — skip this cycle silently
        };

        let crypto = CryptoContext::from_key_bytes(key);

        // Open a fresh DB connection (WAL allows concurrent reads)
        let conn = match db::open(&bot.db_path) {
            Ok(c) => c,
            Err(e) => { eprintln!("[scheduler] DB open error: {e}"); continue; }
        };

        // Load LLM settings
        let llm = load_llm_config(&conn);

        // Load agents that have a schedule
        let agents = match registry::list_scheduled_agents(&conn) {
            Ok(a) => a,
            Err(e) => { eprintln!("[scheduler] list_scheduled_agents error: {e}"); continue; }
        };

        let now = chrono::Utc::now();

        for agent in agents {
            let schedule = match &agent.schedule {
                Some(s) => s.clone(),
                None => continue,
            };

            if agent.schedule_paused {
                continue; // user paused this agent's schedule
            }

            if !is_due(&schedule, agent.last_run_at.as_deref()) {
                continue;
            }

            eprintln!("[scheduler] running agent \"{}\" (schedule: {})", agent.name, schedule);

            match runner::run_agent(
                &conn, &crypto, &agent.id, "(scheduled run)", None, &llm, &|_| {},
            ) {
                Ok(run) => {
                    eprintln!("[scheduler] agent \"{}\" completed run {}", agent.name, run.id);
                    let ts = now.to_rfc3339();
                    let _ = registry::update_last_run(&conn, &agent.id, &ts);
                    bot.notify(
                        &format!("✓ {}", agent.name),
                        &format!("{} step(s) completed", run.steps.len()),
                    );
                }
                Err(e) => {
                    let msg = format!("[scheduler] agent \"{}\" failed: {e}", agent.name);
                    eprintln!("{msg}");
                    send_telegram_alert(&conn, &msg);
                    bot.notify(
                        &format!("✗ {} failed", agent.name),
                        &e.to_string().chars().take(120).collect::<String>(),
                    );
                }
            }
        }

        // Drain the task queue — pick up pending tasks in priority order
        drain_task_queue(&conn, &crypto, &llm);
    }
}

// ── Task queue drainer ──────────────────────────────────────────────────────

fn drain_task_queue(conn: &rusqlite::Connection, crypto: &CryptoContext, llm: &LlmConfig) {
    // Claim up to 10 pending tasks per cycle
    let tasks: Vec<(String, String, String, Option<String>)> = {
        let mut stmt = match conn.prepare(
            "SELECT id, agent_id, input, workspace_id \
             FROM task_queue WHERE status='pending' \
             ORDER BY priority DESC, created_at ASC LIMIT 10",
        ) {
            Ok(s) => s,
            Err(_) => return,
        };
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    for (task_id, agent_id, input, workspace_id) in tasks {
        let now = chrono::Utc::now().to_rfc3339();
        let _ = conn.execute(
            "UPDATE task_queue SET status='running', started_at=?1 \
             WHERE id=?2 AND status='pending'",
            rusqlite::params![now, task_id],
        );

        eprintln!("[scheduler] task {task_id}: running agent {agent_id}");

        let ws = workspace_id.as_deref();
        match runner::run_agent(conn, crypto, &agent_id, &input, ws, llm, &|_| {}) {
            Ok(run) => {
                let done_at = chrono::Utc::now().to_rfc3339();
                let _ = conn.execute(
                    "UPDATE task_queue SET status='done', completed_at=?1, output=?2 WHERE id=?3",
                    rusqlite::params![done_at, run.output, task_id],
                );
                eprintln!("[scheduler] task {task_id}: done");
            }
            Err(e) => {
                let done_at = chrono::Utc::now().to_rfc3339();
                let err_str = e.to_string();
                let _ = conn.execute(
                    "UPDATE task_queue SET status='failed', completed_at=?1, error=?2 WHERE id=?3",
                    rusqlite::params![done_at, err_str, task_id],
                );
                eprintln!("[scheduler] task {task_id}: failed — {err_str}");
                send_telegram_alert(conn, &format!("[task queue] {task_id} failed: {err_str}"));
            }
        }
    }
}

// ── Schedule helpers ────────────────────────────────────────────────────────

/// Parse "every Nm" or "every Nh" into seconds. Returns None for unknown formats.
pub fn parse_interval_secs(schedule: &str) -> Option<u64> {
    let s = schedule.trim().to_lowercase();
    let rest = s.strip_prefix("every ")?;
    if let Some(n) = rest.strip_suffix('h') {
        n.trim().parse::<u64>().ok().map(|h| h * 3600)
    } else if let Some(n) = rest.strip_suffix('m') {
        n.trim().parse::<u64>().ok().map(|m| m * 60)
    } else {
        None
    }
}

/// Returns true if the agent is due based on its schedule and last_run_at.
fn is_due(schedule: &str, last_run_at: Option<&str>) -> bool {
    let interval_secs = match parse_interval_secs(schedule) {
        Some(s) => s as i64,
        None => return false,
    };
    match last_run_at {
        None => true, // never run → run immediately
        Some(ts) => {
            let last = match chrono::DateTime::parse_from_rfc3339(ts) {
                Ok(t) => t.with_timezone(&chrono::Utc),
                Err(_) => return true, // unparseable → run again
            };
            let elapsed = chrono::Utc::now()
                .signed_duration_since(last)
                .num_seconds();
            elapsed >= interval_secs
        }
    }
}

fn load_llm_config(conn: &rusqlite::Connection) -> LlmConfig {
    let get = |key: &str| db::get_setting(conn, key).ok().flatten().unwrap_or_default();
    let provider = get("model_provider");
    let embed_model = if provider == "openai" {
        "text-embedding-3-small".to_string()
    } else {
        let m = get("ollama_embed_model");
        if m.is_empty() { "nomic-embed-text".to_string() } else { m }
    };
    LlmConfig {
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

// ── Telegram helpers ─────────────────────────────────────────────────────────

/// Send a Telegram message if the bot is configured. Fails silently.
fn send_telegram_alert(conn: &rusqlite::Connection, text: &str) {
    let get = |key: &str| db::get_setting(conn, key).ok().flatten().unwrap_or_default();
    let enabled = get("telegram_enabled") == "true";
    if !enabled { return; }
    let token = get("telegram_token");
    let chat_id = get("telegram_chat_id");
    if token.is_empty() || chat_id.is_empty() { return; }

    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let body = serde_json::json!({ "chat_id": chat_id, "text": text });

    if let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        let _ = client.post(&url).json(&body).send();
    }
}

// ── Hands — pre-built agent templates ──────────────────────────────────────

/// A pre-built agent template ("Hand") that users can instantiate in one click.
#[derive(Debug, Clone, Serialize)]
pub struct HandTemplate {
    pub id:                 &'static str,
    pub name:               &'static str,
    pub icon:               &'static str,
    pub description:        &'static str,
    pub system_prompt:      &'static str,
    pub tools:              &'static [&'static str],
    pub suggested_schedule: &'static str,
}

pub const HANDS: &[HandTemplate] = &[
    HandTemplate {
        id:          "researcher",
        name:        "Researcher",
        icon:        "🔬",
        description: "Deep-dives into your knowledge base and synthesises comprehensive \
                      answers with citations. Proposes key findings as memories.",
        system_prompt: "You are an expert researcher with access to a curated knowledge base. \
                        Analyse the provided documents thoroughly, identify the most relevant \
                        information, synthesise a comprehensive answer with clear citations, \
                        and highlight any knowledge gaps or contradictions you find.",
        tools:              &["search_documents", "chat_llm", "propose_memory"],
        suggested_schedule: "every 24h",
    },
    HandTemplate {
        id:          "curator",
        name:        "Memory Curator",
        icon:        "🧠",
        description: "Reviews the workspace for insights worth remembering, then \
                      proposes structured memory entries for your review.",
        system_prompt: "You are a knowledge curator. Review the provided documents and \
                        identify the most important facts, decisions, and patterns worth \
                        retaining long-term. Write concise, actionable memory proposals \
                        (one key insight per proposal, max 200 words each).",
        tools:              &["search_documents", "chat_llm", "propose_memory"],
        suggested_schedule: "every 6h",
    },
    HandTemplate {
        id:          "briefing",
        name:        "Daily Briefing",
        icon:        "📰",
        description: "Generates a structured daily summary of your workspace — \
                      key topics, recent changes, and open questions.",
        system_prompt: "You are a chief of staff preparing a daily briefing. Search the \
                        knowledge base for the most recent and relevant information, then \
                        produce a structured report with sections: KEY TOPICS, NOTABLE \
                        CHANGES, OPEN QUESTIONS, and RECOMMENDED ACTIONS. Be concise.",
        tools:              &["search_documents", "chat_llm", "propose_memory"],
        suggested_schedule: "every 24h",
    },
    HandTemplate {
        id:          "scout",
        name:        "Lead Scout",
        icon:        "🎯",
        description: "Scans the workspace for opportunities, patterns, and \
                      actionable signals worth acting on.",
        system_prompt: "You are a strategic analyst. Scan the knowledge base for emerging \
                        patterns, opportunities, risks, and signals that require attention. \
                        Prioritise findings by impact and urgency. Format as a ranked list \
                        with a one-sentence rationale for each item.",
        tools:              &["search_documents", "chat_llm", "propose_memory"],
        suggested_schedule: "every 12h",
    },
    HandTemplate {
        id:          "distiller",
        name:        "Knowledge Distiller",
        icon:        "⚗️",
        description: "Extracts and distils the core concepts from documents into \
                      concise, permanent memory entries.",
        system_prompt: "You are a knowledge distiller. Extract the essential concepts, \
                        definitions, and rules from the provided documents. Produce clear, \
                        standalone summaries that would be useful out of context. Avoid \
                        redundancy — only propose genuinely new information.",
        tools:              &["search_documents", "chat_llm", "propose_memory"],
        suggested_schedule: "every 1h",
    },
    HandTemplate {
        id:          "manager",
        name:        "Manager",
        icon:        "🎯",
        description: "Orchestrator agent that breaks complex tasks into sub-tasks and \
                      delegates them to specialist agents, then merges their results.",
        system_prompt: "You are an orchestration manager. When given a complex task:\n\
                        1. Break it into focused sub-tasks, one per specialist agent.\n\
                        2. Use the delegate tool for each sub-task (JSON format preferred):\n\
                           Action: delegate\n\
                           Action Input: {\"agent\": \"Agent Name\", \"input\": \"specific sub-task\"}\n\
                        3. Collect each agent's output from the Observation.\n\
                        4. Synthesise all results into a coherent Final Answer.\n\
                        Rules: delegate only tasks that match an agent's specialty. \
                        Never delegate to yourself. If no specialist exists for a sub-task, \
                        handle it directly. Always produce a Final Answer that integrates \
                        all results.",
        tools:              &["delegate", "search_documents", "save_artifact", "propose_memory"],
        suggested_schedule: "",
    },
];
