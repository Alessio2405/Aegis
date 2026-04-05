//! Scale: Actuation — executes desktop automation directives from the final answer.
//!
//! Parses lines of the form:
//!   DESKTOP:TYPE:<text>     — type text via keyboard
//!   DESKTOP:KEY:<combo>     — press a key combination (e.g. ctrl+c)
//!   DESKTOP:OPEN:<target>   — open a file, URL, or application
//!
//! Only runs if `desktop_control_enabled` is set to "true" in settings.
//! Always runs last (ActionScale::Actuation = 4) so it acts on the final synthesised answer.

use rusqlite::params;

use crate::error::AppResult;
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct DesktopHandler;

impl ToolHandler for DesktopHandler {
    fn id(&self) -> &'static str { "desktop" }
    fn scale(&self) -> ActionScale { ActionScale::Actuation }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        // Check setting
        let enabled: bool = ctx.conn.query_row(
            "SELECT value FROM settings WHERE key = 'desktop_control_enabled'",
            params![],
            |r| r.get::<_, String>(0),
        ).ok().as_deref() == Some("true");

        if !enabled {
            return Ok(ToolOutput {
                answer:           None,
                context_fragment: None,
                step: AgentRunStep {
                    step_type:   "desktop".into(),
                    description: "Desktop automation disabled in settings".into(),
                    output:      String::new(),
                },
                extra_steps: vec![],
            });
        }

        let directives = parse_directives(ctx.current_answer);

        if directives.is_empty() {
            return Ok(ToolOutput {
                answer:           None,
                context_fragment: None,
                step: AgentRunStep {
                    step_type:   "desktop".into(),
                    description: "No DESKTOP: directives found in answer".into(),
                    output:      String::new(),
                },
                extra_steps: vec![],
            });
        }

        let mut results: Vec<String> = Vec::new();

        for (kind, value) in &directives {
            let outcome = match kind.as_str() {
                "TYPE" => crate::desktop::type_text(value)
                    .map(|_| format!("typed: {}", value.chars().take(60).collect::<String>()))
                    .unwrap_or_else(|e| format!("type error: {e}")),
                "KEY" => crate::desktop::key_combo(value)
                    .map(|_| format!("key: {value}"))
                    .unwrap_or_else(|e| format!("key error: {e}")),
                "OPEN" => crate::desktop::open_target(value)
                    .map(|_| format!("opened: {value}"))
                    .unwrap_or_else(|e| format!("open error: {e}")),
                other => format!("unknown directive: {other}"),
            };
            results.push(outcome);
        }

        let summary = results.join("\n");

        Ok(ToolOutput {
            answer:           None,  // Actuation doesn't change the answer
            context_fragment: None,
            step: AgentRunStep {
                step_type:   "desktop".into(),
                description: format!("Executed {} desktop directive(s)", directives.len()),
                output:      summary,
            },
            extra_steps: vec![],
        })
    }
}

// ── Directive parsing ──────────────────────────────────────────────────────

/// Parse `DESKTOP:KIND:value` lines from text.
/// Returns (kind, value) pairs.
fn parse_directives(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.starts_with("DESKTOP:") { return None; }
            let rest = &line["DESKTOP:".len()..];
            let colon = rest.find(':')?;
            let kind = rest[..colon].trim().to_uppercase();
            let value = rest[colon + 1..].trim().to_string();
            if kind.is_empty() || value.is_empty() { return None; }
            Some((kind, value))
        })
        .collect()
}
