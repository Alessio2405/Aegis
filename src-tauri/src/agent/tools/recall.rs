//! Scale: Retrieval — reads a key (or all keys) from the agent's persistent scratchpad.
//!
//! Input: a key name to look up, or "all" / "*" to dump every entry.
//!
//! The scratchpad is private to each agent and persists across runs.

use rusqlite::params;

use crate::error::{AppError, AppResult};
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct RecallHandler;

impl ToolHandler for RecallHandler {
    fn id(&self) -> &'static str { "recall" }
    fn scale(&self) -> ActionScale { ActionScale::Retrieval }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let key = ctx.input.trim();

        if key.is_empty() {
            return Err(AppError::InvalidInput(
                "recall: provide a key name, or 'all' to list every entry".into(),
            ));
        }

        // List all entries
        if matches!(key, "all" | "*" | "list") {
            let mut stmt = ctx.conn
                .prepare("SELECT key, value FROM agent_scratchpad WHERE agent_id=?1 ORDER BY key")
                .map_err(AppError::Database)?;
            let entries: Vec<(String, String)> = stmt
                .query_map(params![ctx.agent.id], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(AppError::Database)?
                .filter_map(|r| r.ok())
                .collect();

            let content = if entries.is_empty() {
                "Scratchpad is empty.".to_string()
            } else {
                entries.iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };

            return Ok(ToolOutput {
                answer:           None,
                context_fragment: Some(format!("SCRATCHPAD ({} entries):\n{content}", entries.len())),
                step: AgentRunStep {
                    step_type:   "recall".into(),
                    description: format!("Listed {} scratchpad entries", entries.len()),
                    output:      content,
                },
                extra_steps: vec![],
            });
        }

        // Single key lookup
        let value: Option<String> = ctx.conn
            .query_row(
                "SELECT value FROM agent_scratchpad WHERE agent_id=?1 AND key=?2",
                params![ctx.agent.id, key],
                |r| r.get(0),
            )
            .ok();

        let content = match &value {
            Some(v) => format!("{key} = {v}"),
            None    => format!("Key '{key}' not found in scratchpad."),
        };

        Ok(ToolOutput {
            answer:           None,
            context_fragment: Some(format!("SCRATCHPAD: {content}")),
            step: AgentRunStep {
                step_type:   "recall".into(),
                description: format!("Recalled '{key}'"),
                output:      content,
            },
            extra_steps: vec![],
        })
    }
}
