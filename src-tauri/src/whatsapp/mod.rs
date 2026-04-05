//! WhatsApp bot — Meta Cloud API (outgoing) + embedded webhook server (incoming).
//!
//! Architecture:
//!   - Incoming: tiny_http server on 0.0.0.0:{port}, receives Meta webhook POSTs
//!   - Outgoing: reqwest POST to graph.facebook.com
//!
//! Required setup:
//!   1. Create a Meta App at https://developers.facebook.com
//!   2. Add the "WhatsApp" product; get a Phone Number ID and temporary access token
//!   3. Under Webhooks, point to https://<your-public-host>:<port>/webhook
//!      Use ngrok (`ngrok http 8765`) or Cloudflare Tunnel to expose the local port
//!   4. Set the Verify Token to match `whatsapp_verify_token` in Settings
//!   5. Subscribe to the `messages` webhook field
//!   6. Your allowed phone number must match `whatsapp_allowed_number` (digits only, e.g. 15551234567)

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::Value;

use crate::state::BotSharedState;

// ── Public API ─────────────────────────────────────────────────────────────

pub fn start(
    access_token: String,
    phone_number_id: String,
    allowed_number: String,
    verify_token: String,
    port: u16,
    bot: Arc<BotSharedState>,
) {
    if bot
        .whatsapp_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        eprintln!("[whatsapp] Bot is already running — ignoring start()");
        return;
    }

    std::thread::spawn(move || {
        eprintln!("[whatsapp] Webhook server starting on port {port}");
        run_server(access_token, phone_number_id, allowed_number, verify_token, port, &bot);
        bot.whatsapp_running.store(false, Ordering::SeqCst);
        eprintln!("[whatsapp] Webhook server stopped.");
    });
}

pub fn stop(bot: &BotSharedState) {
    bot.whatsapp_running.store(false, Ordering::SeqCst);
}

// ── Webhook server ─────────────────────────────────────────────────────────

fn run_server(
    token: String,
    phone_number_id: String,
    allowed_number: String,
    verify_token: String,
    port: u16,
    bot: &BotSharedState,
) {
    let server = match tiny_http::Server::http(format!("0.0.0.0:{port}")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[whatsapp] Failed to bind port {port}: {e}");
            return;
        }
    };

    while bot.whatsapp_running.load(Ordering::SeqCst) {
        let request = match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(r)) => r,
            Ok(None) => continue, // timeout — check running flag and loop
            Err(e) => { eprintln!("[whatsapp] Server recv error: {e}"); continue; }
        };

        let method = request.method().as_str().to_uppercase();
        let url = request.url().to_string();

        match method.as_str() {
            "GET" => handle_verification(request, &url, &verify_token),
            "POST" => handle_incoming(request, &token, &phone_number_id, &allowed_number, bot),
            _ => {
                let _ = request.respond(
                    tiny_http::Response::from_string("Method Not Allowed").with_status_code(405),
                );
            }
        }
    }
}

// ── Webhook verification (GET) ─────────────────────────────────────────────

fn handle_verification(request: tiny_http::Request, url: &str, verify_token: &str) {
    let mode      = query_param(url, "hub.mode");
    let challenge = query_param(url, "hub.challenge");
    let token     = query_param(url, "hub.verify_token");

    if mode.as_deref() == Some("subscribe") && token.as_deref() == Some(verify_token) {
        let body = challenge.unwrap_or_default();
        let _ = request.respond(tiny_http::Response::from_string(body));
    } else {
        let _ = request.respond(
            tiny_http::Response::from_string("Forbidden").with_status_code(403),
        );
    }
}

// ── Incoming message (POST) ────────────────────────────────────────────────

fn handle_incoming(
    mut request: tiny_http::Request,
    token: &str,
    phone_number_id: &str,
    allowed_number: &str,
    bot: &BotSharedState,
) {
    // Read body before responding (respond() consumes the request)
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        let _ = request.respond(tiny_http::Response::from_string("OK"));
        return;
    }

    // Acknowledge immediately so Meta doesn't retry
    let _ = request.respond(tiny_http::Response::from_string("OK"));

    let payload: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => { eprintln!("[whatsapp] JSON parse error: {e}"); return; }
    };

    // Walk entry → changes → value → messages
    let empty = vec![];
    let entries = payload["entry"].as_array().unwrap_or(&empty);
    for entry in entries {
        let changes = entry["changes"].as_array().unwrap_or(&empty);
        for change in changes {
            let value = &change["value"];
            let msgs = value["messages"].as_array().unwrap_or(&empty);
            for msg in msgs {
                process_message(msg, token, phone_number_id, allowed_number, bot);
            }
        }
    }
}

fn process_message(
    msg: &Value,
    token: &str,
    phone_number_id: &str,
    allowed_number: &str,
    bot: &BotSharedState,
) {
    // Only handle text messages
    if msg["type"].as_str() != Some("text") { return; }

    let from = msg["from"].as_str().unwrap_or("");
    // Normalize: strip leading '+' for comparison
    if strip_plus(from) != strip_plus(allowed_number) { return; }

    let text = msg["text"]["body"].as_str().unwrap_or("").trim();
    if text.is_empty() { return; }

    let reply = crate::bot_dispatch::dispatch(text, bot);

    // Split long replies to stay within WhatsApp's 4096-char limit
    for chunk in split_message(&reply, 4000) {
        send_message(token, phone_number_id, allowed_number, &chunk);
    }
}

// ── Outgoing message ───────────────────────────────────────────────────────

fn send_message(token: &str, phone_number_id: &str, to: &str, text: &str) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => { eprintln!("[whatsapp] HTTP client build error: {e}"); return; }
    };

    let url = format!(
        "https://graph.facebook.com/v19.0/{phone_number_id}/messages"
    );
    let payload = serde_json::json!({
        "messaging_product": "whatsapp",
        "to": strip_plus(to),
        "type": "text",
        "text": { "body": text }
    });

    if let Err(e) = client
        .post(&url)
        .bearer_auth(token)
        .json(&payload)
        .send()
    {
        eprintln!("[whatsapp] sendMessage error: {e}");
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Extract a single query-string parameter from a URL path like `/webhook?foo=bar&baz=qux`.
fn query_param(url: &str, key: &str) -> Option<String> {
    let qs = url.splitn(2, '?').nth(1)?;
    for pair in qs.split('&') {
        let mut kv = pair.splitn(2, '=');
        if kv.next()? == key {
            return Some(kv.next().unwrap_or("").to_string());
        }
    }
    None
}

fn strip_plus(s: &str) -> &str {
    s.trim_start_matches('+')
}

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
