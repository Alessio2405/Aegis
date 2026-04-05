//! Scale: Memory — writes a key-value pair to the agent's persistent scratchpad.
//!
//! Input formats:
//!   JSON: {"key": "last_url", "value": "https://example.com"}
//!   Bare: last_url\nhttps://example.com
//!         (first line = key, remaining lines = value)
//!
//! Upserts: existing keys are overwritten.
//! Use the `recall` tool to read values back in a later run.

use rusqlite::params;
use serde_json::Value;
use chrono::Utc;

use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct RememberHandler;

impl ToolHandler for RememberHandler {
    fn id(&self) -> &'static str { "remember" }
    fn scale(&self) -> ActionScale { ActionScale::Memory }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let (key, value) = parse_kv(ctx.input.trim()).ok_or_else(|| {
            AppError::InvalidInput(
                "remember: use JSON {\"key\":\"k\",\"value\":\"v\"} or two lines: key\\nvalue".into(),
            )
        })?;

        let now = Utc::now().to_rfc3339();
        ctx.conn.execute(
            "INSERT INTO agent_scratchpad(agent_id, key, value, updated_at) \
             VALUES(?1,?2,?3,?4) \
             ON CONFLICT(agent_id, key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
            params![ctx.agent.id, key, value, now],
        ).map_err(AppError::Database)?;

        Ok(ToolOutput {
            answer:           None,
            context_fragment: Some(format!("REMEMBERED: {key} = {value}")),
            step: AgentRunStep {
                step_type:   "remember".into(),
                description: format!("Stored '{key}' in scratchpad"),
                output:      format!("{key} = {value}"),
            },
            extra_steps: vec![],
        })
    }
}

fn parse_kv(raw: &str) -> Option<(String, String)> {
    if raw.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            let key   = v.get("key").and_then(|x| x.as_str()).map(str::to_string)?;
            let value = v.get("value").and_then(|x| x.as_str()).map(str::to_string)
                .unwrap_or_default();
            if !key.is_empty() { return Some((key, value)); }
        }
    }
    let (first, rest) = raw.split_once('\n')?;
    let key   = first.trim().to_string();
    let value = rest.trim().to_string();
    if !key.is_empty() { return Some((key, value)); }
    None
}
