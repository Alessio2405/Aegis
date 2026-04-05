//! Discord bot — WebSocket Gateway using tungstenite (blocking).
//!
//! Security model:
//!   - Only messages from the configured `allowed_user_id` are processed.
//!   - Commands that require vault access return "Vault locked" if the vault is not open.
//!   - The bot never exposes raw key material over Discord.
//!
//! Required Discord setup:
//!   1. Create a bot at https://discord.com/developers/applications
//!   2. Enable "Message Content Intent" under Bot → Privileged Gateway Intents
//!   3. Copy the bot token into Settings → Discord
//!   4. Add the bot to your server with scopes: bot + applications.commands
//!   5. Get your Discord user ID (Settings → Advanced → Developer Mode, then right-click your name)

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::Value;
use tungstenite::{connect, Message};

use crate::state::BotSharedState;

// Gateway intents: GUILDS | GUILD_MESSAGES | DIRECT_MESSAGES | MESSAGE_CONTENT (privileged)
const INTENTS: u32 = (1 << 0) | (1 << 9) | (1 << 12) | (1 << 15);
const GATEWAY_VERSION: u8 = 10;

// ── Public API ─────────────────────────────────────────────────────────────

/// Spawn the Discord Gateway bot thread.
/// Safe to call multiple times — if already running, returns immediately.
pub fn start(token: String, allowed_user_id: String, bot: Arc<BotSharedState>) {
    if bot
        .discord_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        eprintln!("[discord] Bot is already running — ignoring start()");
        return;
    }

    std::thread::spawn(move || {
        eprintln!("[discord] Bot thread started (allowed_user={allowed_user_id})");
        run_loop(&token, &allowed_user_id, &bot);
        bot.discord_running.store(false, Ordering::SeqCst);
        eprintln!("[discord] Bot thread exited.");
    });
}

/// Signal the Discord bot thread to stop. It will exit on the next Gateway read cycle.
pub fn stop(bot: &BotSharedState) {
    bot.discord_running.store(false, Ordering::SeqCst);
}

// ── Gateway URL ────────────────────────────────────────────────────────────

fn get_gateway_url(token: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    #[derive(serde::Deserialize)]
    struct GwResp { url: String }

    let resp: GwResp = client
        .get("https://discord.com/api/v10/gateway/bot")
        .header("Authorization", format!("Bot {token}"))
        .send()
        .map_err(|e| format!("HTTP error: {e}"))?
        .json()
        .map_err(|e| format!("JSON error: {e}"))?;

    Ok(format!("{}/?v={}&encoding=json", resp.url, GATEWAY_VERSION))
}

// ── Session outcome ────────────────────────────────────────────────────────

enum Session { Stop, Reconnect }

// ── Gateway loop ───────────────────────────────────────────────────────────

fn run_loop(token: &str, allowed_user_id: &str, bot: &BotSharedState) {
    let ws_url = match get_gateway_url(token) {
        Ok(u) => u,
        Err(e) => { eprintln!("[discord] Cannot get gateway URL: {e}"); return; }
    };

    loop {
        if !bot.discord_running.load(Ordering::SeqCst) { break; }
        match run_session(&ws_url, token, allowed_user_id, bot) {
            Session::Stop => break,
            Session::Reconnect => {
                eprintln!("[discord] Reconnecting in 5 s…");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

fn run_session(ws_url: &str, token: &str, allowed_user_id: &str, bot: &BotSharedState) -> Session {
    let (mut socket, _) = match connect(ws_url) {
        Ok(v) => v,
        Err(e) => { eprintln!("[discord] WebSocket connect error: {e}"); return Session::Reconnect; }
    };

    // Short read timeout so the loop can tick heartbeats and check the running flag.
    set_read_timeout(&socket, Some(Duration::from_millis(500)));

    let mut heartbeat_ms: u64 = 41_250;
    let mut last_hb = Instant::now();
    let mut last_seq: Option<i64> = None;
    let mut identified = false;

    loop {
        if !bot.discord_running.load(Ordering::SeqCst) {
            let _ = socket.close(None);
            return Session::Stop;
        }

        // Send heartbeat if due
        if identified {
            let elapsed = last_hb.elapsed().as_millis() as u64;
            if elapsed >= heartbeat_ms {
                let hb = serde_json::json!({"op": 1, "d": last_seq});
                if socket.send(Message::Text(hb.to_string())).is_err() {
                    return Session::Reconnect;
                }
                last_hb = Instant::now();
            }
        }

        let msg = match socket.read() {
            Ok(m) => m,
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(tungstenite::Error::ConnectionClosed)
            | Err(tungstenite::Error::AlreadyClosed) => {
                eprintln!("[discord] Connection closed, reconnecting");
                return Session::Reconnect;
            }
            Err(e) => {
                eprintln!("[discord] Read error: {e}");
                return Session::Reconnect;
            }
        };

        let text = match msg {
            Message::Text(t) => t,
            Message::Ping(d) => { let _ = socket.send(Message::Pong(d)); continue; }
            Message::Close(_) => { eprintln!("[discord] Server closed connection"); return Session::Reconnect; }
            _ => continue,
        };

        let payload: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => { eprintln!("[discord] JSON parse error: {e}"); continue; }
        };

        // Track sequence number
        if let Some(s) = payload["s"].as_i64() {
            last_seq = Some(s);
        }

        match payload["op"].as_u64().unwrap_or(255) {
            10 => {
                // Hello — start heartbeating, then identify
                heartbeat_ms = payload["d"]["heartbeat_interval"].as_u64().unwrap_or(41_250);
                let hb = serde_json::json!({"op": 1, "d": null});
                let _ = socket.send(Message::Text(hb.to_string()));
                last_hb = Instant::now();

                let identify = serde_json::json!({
                    "op": 2,
                    "d": {
                        "token": token,
                        "intents": INTENTS,
                        "properties": { "os": "windows", "browser": "aegis", "device": "aegis" }
                    }
                });
                if socket.send(Message::Text(identify.to_string())).is_err() {
                    return Session::Reconnect;
                }
                identified = true;
            }
            11 => {} // HeartbeatAck
            1 => {
                // Server requests immediate heartbeat
                let hb = serde_json::json!({"op": 1, "d": last_seq});
                let _ = socket.send(Message::Text(hb.to_string()));
                last_hb = Instant::now();
            }
            7 => { eprintln!("[discord] Server requested reconnect"); return Session::Reconnect; }
            9 => {
                eprintln!("[discord] Invalid session");
                std::thread::sleep(Duration::from_secs(5));
                return Session::Reconnect;
            }
            0 => {
                let t = payload["t"].as_str().unwrap_or("");
                match t {
                    "READY" => eprintln!("[discord] Bot ready"),
                    "MESSAGE_CREATE" => {
                        handle_message(&payload["d"], allowed_user_id, token, bot);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Session::Stop
}

// ── Message handling ───────────────────────────────────────────────────────

fn handle_message(d: &Value, allowed_user_id: &str, token: &str, bot: &BotSharedState) {
    // Skip bot messages (including ourselves)
    if d["author"]["bot"].as_bool().unwrap_or(false) { return; }

    let author_id = d["author"]["id"].as_str().unwrap_or("");
    if author_id != allowed_user_id { return; }

    let content = d["content"].as_str().unwrap_or("").trim();
    let channel_id = d["channel_id"].as_str().unwrap_or("");
    if content.is_empty() || channel_id.is_empty() { return; }

    let reply = crate::bot_dispatch::dispatch(content, bot);
    send_message(token, channel_id, &reply);
}

fn send_message(token: &str, channel_id: &str, text: &str) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => { eprintln!("[discord] HTTP client build error: {e}"); return; }
    };

    for chunk in split_message(text, 1900) {
        let url = format!("https://discord.com/api/v10/channels/{channel_id}/messages");
        if let Err(e) = client
            .post(&url)
            .header("Authorization", format!("Bot {token}"))
            .json(&serde_json::json!({ "content": chunk }))
            .send()
        {
            eprintln!("[discord] sendMessage error: {e}");
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn split_message(text: &str, limit: usize) -> Vec<String> {
    if text.len() <= limit {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + limit).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        chunks.push(text[start..end].to_string());
        start = end;
    }
    chunks
}

/// Set a read timeout on the underlying TCP stream inside the WebSocket.
fn set_read_timeout(
    socket: &tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    dur: Option<Duration>,
) {
    use tungstenite::stream::MaybeTlsStream;
    let result = match socket.get_ref() {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(dur),
        MaybeTlsStream::NativeTls(s) => s.get_ref().set_read_timeout(dur),
        _ => return,
    };
    if let Err(e) = result {
        eprintln!("[discord] set_read_timeout error: {e}");
    }
}
