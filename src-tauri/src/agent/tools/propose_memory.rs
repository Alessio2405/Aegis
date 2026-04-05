//! Scale: Memory — proposes episodic and structured memories from the answer.
//! Always runs last so it has access to the final Synthesis output.

use crate::agent::memory_writer;
use crate::error::AppResult;
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct ProposeMemoryHandler;

impl ToolHandler for ProposeMemoryHandler {
    fn id(&self) -> &'static str { "propose_memory" }
    fn scale(&self) -> ActionScale { ActionScale::Memory }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let answer = ctx.current_answer;
        if answer.is_empty() {
            return Ok(ToolOutput {
                answer:           None,
                context_fragment: None,
                step: AgentRunStep {
                    step_type:   "propose_memory".into(),
                    description: "Skipped — no answer to remember".into(),
                    output:      String::new(),
                },
                extra_steps: vec![],
            });
        }

        let source = format!("agent:{}", ctx.agent.id);

        // 1. Propose a single episodic summary (first 300 chars of the answer)
        let preview: String = answer.chars().take(300).collect();
        let mem_id = memory_writer::propose(
            ctx.conn, ctx.crypto, &preview, "Episodic",
            &source, ctx.workspace_id,
        )?;

        let primary_step = AgentRunStep {
            step_type:   "propose_memory".into(),
            description: "Proposed episodic memory from agent output".into(),
            output:      mem_id,
        };

        // 2. Auto-extract structured memories (definitions, decisions)
        let candidates = memory_writer::extract_candidate_memories(answer);
        let mut extra_steps: Vec<AgentRunStep> = Vec::new();

        for (content, mem_type) in &candidates {
            let _ = memory_writer::propose(
                ctx.conn, ctx.crypto, content, mem_type,
                &source, ctx.workspace_id,
            );
        }

        if !candidates.is_empty() {
            extra_steps.push(AgentRunStep {
                step_type:   "propose_memory".into(),
                description: format!(
                    "Auto-extracted {} structured memory candidate(s)",
                    candidates.len()
                ),
                output: candidates
                    .iter()
                    .map(|(c, t)| format!("[{t}] {c}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            });
        }

        Ok(ToolOutput {
            answer:           None,   // Memory writes never change the answer
            context_fragment: None,
            step:             primary_step,
            extra_steps,
        })
    }
}
