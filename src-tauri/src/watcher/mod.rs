//! File watcher — fires agents when watched directories (or files) change.
//!
//! Uses the `notify` v6 cross-platform watcher.
//! Each enabled `watch_triggers` row maps a filesystem path to a user agent.
//!
//! When a meaningful change event arrives the associated agent receives:
//!   "File changed: <absolute-path>"
//! as its input and runs in a fresh thread (non-blocking).
//!
//! Debounce: the same trigger won't fire again within DEBOUNCE_SECS.
//! Trigger list is refreshed from DB every REFRESH_SECS.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::collections::HashMap;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::params;

use crate::state::BotSharedState;
use crate::db;
use crate::crypto::CryptoContext;
use crate::agent::runner;

const DEBOUNCE_SECS: u64 = 5;
const REFRESH_SECS:  u64 = 30;

// ── Internal trigger record ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Trigger {
    id:         String,
    agent_id:   String,
    watch_path: String,
    recursive:  bool,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Start the file-watcher background thread. No-op if already running.
pub fn start(bot: Arc<BotSharedState>) {
    if bot.watcher_running.compare_exchange(
        false, true, Ordering::SeqCst, Ordering::SeqCst,
    ).is_err() {
        return; // already running
    }

    std::thread::spawn(move || {
        run_loop(&bot);
        bot.watcher_running.store(false, Ordering::SeqCst);
    });
}

/// Signal the watcher thread to exit on its next iteration.
pub fn stop(bot: &BotSharedState) {
    bot.watcher_running.store(false, Ordering::SeqCst);
}

// ── Main loop ────────────────────────────────────────────────────────────────

fn run_loop(bot: &BotSharedState) {
    eprintln!("[watcher] started");

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let tx2 = tx.clone();

    let mut os_watcher: RecommendedWatcher = match notify::recommended_watcher(move |res| {
        tx2.send(res).ok();
    }) {
        Ok(w) => w,
        Err(e) => { eprintln!("[watcher] failed to create OS watcher: {e}"); return; }
    };

    let mut triggers: Vec<Trigger> = vec![];
    let mut last_refresh: Option<Instant> = None;   // None → force immediate load
    let mut debounce: HashMap<String, Instant> = HashMap::new();

    while bot.watcher_running.load(Ordering::SeqCst) {
        // Reload trigger list from DB periodically
        let need_refresh = last_refresh
            .map(|t| t.elapsed().as_secs() >= REFRESH_SECS)
            .unwrap_or(true);

        if need_refresh {
            let new_triggers = load_triggers(bot);

            // Unwatch every previously-watched path
            for t in &triggers {
                let _ = os_watcher.unwatch(std::path::Path::new(&t.watch_path));
            }

            triggers = new_triggers;

            // Watch each new path
            for t in &triggers {
                let path = std::path::Path::new(&t.watch_path);
                let mode = if t.recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                if let Err(e) = os_watcher.watch(path, mode) {
                    eprintln!("[watcher] cannot watch '{}': {e}", t.watch_path);
                }
            }

            last_refresh = Some(Instant::now());
            eprintln!("[watcher] watching {} trigger(s)", triggers.len());
        }

        // Receive one event (or timeout after 1 s so we can check running flag)
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(event)) => {
                // Ignore pure read-access events
                if matches!(event.kind, notify::EventKind::Access(_)) {
                    continue;
                }

                for path in &event.paths {
                    for trigger in &triggers {
                        // Does this event fall inside the trigger's watch path?
                        if !path.starts_with(std::path::Path::new(&trigger.watch_path)) {
                            continue;
                        }

                        // Debounce: skip if fired too recently
                        let now = Instant::now();
                        if let Some(&last) = debounce.get(&trigger.id) {
                            if now.duration_since(last).as_secs() < DEBOUNCE_SECS {
                                continue;
                            }
                        }
                        debounce.insert(trigger.id.clone(), now);

                        eprintln!(
                            "[watcher] trigger '{}' → agent '{}' ({})",
                            trigger.id, trigger.agent_id, path.display()
                        );

                        fire_agent(bot, trigger.clone(), path.display().to_string());
                    }
                }
            }
            Ok(Err(e))                           => eprintln!("[watcher] event error: {e}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {} // normal, loop again
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    eprintln!("[watcher] stopped");
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn load_triggers(bot: &BotSharedState) -> Vec<Trigger> {
    let conn = match db::open(&bot.db_path) {
        Ok(c) => c,
        Err(e) => { eprintln!("[watcher] db open error: {e}"); return vec![]; }
    };
    let mut stmt = match conn.prepare(
        "SELECT id, agent_id, watch_path, recursive FROM watch_triggers WHERE enabled=1",
    ) {
        Ok(s) => s,
        Err(e) => { eprintln!("[watcher] prepare error: {e}"); return vec![]; }
    };
    stmt.query_map(params![], |r| {
        Ok(Trigger {
            id:         r.get(0)?,
            agent_id:   r.get(1)?,
            watch_path: r.get(2)?,
            recursive:  r.get::<_, i32>(3)? != 0,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// Spawn a thread to run the agent so the watcher loop stays unblocked.
fn fire_agent(bot: &BotSharedState, trigger: Trigger, changed_path: String) {
    let key = match bot.shared_key.lock().unwrap().clone() {
        Some(k) => k,
        None => {
            eprintln!("[watcher] vault locked — skipping trigger '{}'", trigger.id);
            return;
        }
    };

    let db_path    = bot.db_path.clone();
    let app_handle = bot.app_handle.clone();

    std::thread::spawn(move || {
        let conn = match db::open(&db_path) {
            Ok(c) => c,
            Err(e) => { eprintln!("[watcher] db error: {e}"); return; }
        };

        let crypto = CryptoContext::from_key_bytes(key);
        let llm    = load_llm_config(&conn);
        let input  = format!("File changed: {changed_path}");

        // Resolve agent name for notification
        let agent_name = conn
            .query_row("SELECT name FROM user_agents WHERE id=?1", params![trigger.agent_id], |r| r.get::<_, String>(0))
            .unwrap_or_else(|_| trigger.agent_id.clone());

        match runner::run_agent(
            &conn, &crypto, &trigger.agent_id, &input, None, &llm, &|_| {},
        ) {
            Ok(run) => {
                let id = &app_handle.config().tauri.bundle.identifier;
                let _ = tauri::api::notification::Notification::new(id)
                    .title(&format!("👁 {agent_name}"))
                    .body(&format!("File trigger fired — {} step(s)", run.steps.len()))
                    .show();
            }
            Err(e) => {
                eprintln!("[watcher] agent '{}' error: {e}", trigger.agent_id);
                let id = &app_handle.config().tauri.bundle.identifier;
                let _ = tauri::api::notification::Notification::new(id)
                    .title(&format!("✗ {agent_name} failed"))
                    .body(&e.to_string().chars().take(120).collect::<String>())
                    .show();
            }
        }
    });
}

/// Build LlmConfig from the settings table (same logic as scheduler).
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
        anthropic_api_key:  get("anthropic_api_key"),
        anthropic_model:    get("anthropic_model"),
    }
}
