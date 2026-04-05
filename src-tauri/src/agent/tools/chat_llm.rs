//! Scale: Synthesis — LLM inference with injected context and saved memories.

use crate::permissions::{self, Capability};
use crate::agent::memory_writer;
use crate::error::AppResult;
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct ChatLlmHandler;

impl ToolHandler for ChatLlmHandler {
    fn id(&self) -> &'static str { "chat_llm" }
    fn scale(&self) -> ActionScale { ActionScale::Synthesis }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        permissions::require(
            ctx.conn, &Capability::ModelUse, ctx.workspace_id,
            "agent", Some("chat"), None,
        )?;

        if !ctx.llm.enabled {
            // No model — fall back to the accumulated context as the answer
            let fallback = if ctx.context_parts.is_empty() {
                ctx.input.to_string()
            } else {
                ctx.context_parts.join("\n\n")
            };
            return Ok(ToolOutput {
                answer:           Some(fallback),
                context_fragment: None,
                step: AgentRunStep {
                    step_type:   "llm".into(),
                    description: "LLM disabled — returning extractive answer".into(),
                    output:      "Model not enabled.".into(),
                },
                extra_steps: vec![],
            });
        }

        // ── Memory injection ──────────────────────────────────────────────
        let memory_prefix = {
            let mems = memory_writer::list(ctx.conn, ctx.crypto, Some("saved"), ctx.workspace_id)
                .unwrap_or_default();
            let lines: Vec<String> = mems.iter()
                .take(10)
                .filter_map(|m| m.content.as_deref().map(str::to_string))
                .collect();
            if lines.is_empty() {
                String::new()
            } else {
                format!("LONG-TERM MEMORY:\n{}\n\n", lines.join("\n"))
            }
        };

        // ── System prompt ─────────────────────────────────────────────────
        let system = if ctx.agent.system_prompt.is_empty() {
            format!("{}You are a helpful assistant.", memory_prefix)
        } else {
            format!("{}{}", memory_prefix, ctx.agent.system_prompt)
        };

        // ── Context block from earlier scales ─────────────────────────────
        // Cap each part at 2000 chars and total block at 6000 chars to stay within token budget.
        const MAX_PART_CHARS: usize = 2000;
        const MAX_CONTEXT_CHARS: usize = 6000;
        let context_block = if ctx.context_parts.is_empty() {
            String::new()
        } else {
            let capped_parts: Vec<String> = ctx.context_parts.iter().map(|p| {
                if p.len() > MAX_PART_CHARS {
                    format!("{}…", &p[..MAX_PART_CHARS])
                } else {
                    p.clone()
                }
            }).collect();
            let joined = capped_parts.join("\n\n");
            let truncated = if joined.len() > MAX_CONTEXT_CHARS {
                format!("{}…", &joined[..MAX_CONTEXT_CHARS])
            } else {
                joined
            };
            format!("\n\n{truncated}\n\n")
        };

        let prompt = format!(
            "{system}{context}USER INPUT: {input}\n\nRESPONSE:",
            context = context_block,
            input   = ctx.input,
        );

        let reply = crate::connectors::llm_chat_multi(
            ctx.llm,
            &[("user".to_string(), prompt)],
        )?;

        let provider_label = if ctx.llm.provider == "openai" {
            ctx.llm.openai_model.clone()
        } else {
            ctx.llm.ollama_model.clone()
        };

        Ok(ToolOutput {
            answer:           Some(reply.clone()),
            context_fragment: None,
            step: AgentRunStep {
                step_type:   "llm".into(),
                description: format!("LLM response ({}:{})", ctx.llm.provider, provider_label),
                output:      reply.chars().take(500).collect(),
            },
            extra_steps: vec![],
        })
    }
}
