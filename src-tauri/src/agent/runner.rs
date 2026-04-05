//! Agent execution engine.
//!
//! Each user-defined agent runs its declared tools in [`ActionScale`] order:
//!
//!   Retrieval (search_documents)
//!     → External (fetch_url)
//!       → Synthesis (chat_llm)
//!         → Memory (propose_memory)
//!
//! Adding a new tool = implement [`ToolHandler`] + register in [`HandlerRegistry`].
//! No changes to this file required.
//!
//! For pipelines, output of step N → input of step N+1.

use rusqlite::Connection;

use crate::crypto::CryptoContext;
use crate::error::{AppError, AppResult};
use crate::connectors::LlmConfig;
use super::registry::{self, AgentRun, AgentRunStep, UserAgent};
use super::tools::{HandlerRegistry, RunCtx};
use super::react;

// ── Public entry points ───────────────────────────────────────────────────

/// Run a single user-defined agent and persist the run record.
/// `on_step` is called immediately after each step completes (for live UI streaming).
/// Pass `&|_| {}` when streaming is not needed (e.g. scheduler).
pub fn run_agent(
    conn:         &Connection,
    crypto:       &CryptoContext,
    agent_id:     &str,
    input:        &str,
    workspace_id: Option<&str>,
    llm:          &LlmConfig,
    on_step:      &dyn Fn(&AgentRunStep),
) -> AppResult<AgentRun> {
    let agent = registry::get_agent(conn, agent_id)?;

    let (output, steps) = if agent.react_mode {
        react::run_react(conn, crypto, &agent, input, workspace_id, llm, on_step)?
    } else {
        execute_agent_steps(conn, crypto, &agent, input, workspace_id, llm, on_step)?
    };

    registry::save_run(
        conn, agent_id, None, input, Some(&output), "completed",
        &steps, workspace_id,
    )
}

/// Run an ordered pipeline; output of each agent feeds as input to the next.
pub fn run_pipeline(
    conn:         &Connection,
    crypto:       &CryptoContext,
    pipeline_id:  &str,
    input:        &str,
    workspace_id: Option<&str>,
    llm:          &LlmConfig,
    on_step:      &dyn Fn(&AgentRunStep),
) -> AppResult<AgentRun> {
    let pipeline = registry::get_pipeline(conn, pipeline_id)?;
    if pipeline.agent_ids.is_empty() {
        return Err(AppError::InvalidInput("Pipeline has no agents".into()));
    }

    let mut current_input = input.to_string();
    let mut all_steps: Vec<AgentRunStep> = Vec::new();
    let mut final_output = String::new();

    for (i, agent_id) in pipeline.agent_ids.iter().enumerate() {
        let agent = registry::get_agent(conn, agent_id)?;

        let header = AgentRunStep {
            step_type:   "pipeline_step".into(),
            description: format!("Step {}/{}: \"{}\"", i + 1, pipeline.agent_ids.len(), agent.name),
            output:      String::new(),
        };
        on_step(&header);
        all_steps.push(header);

        let (output, steps) = if agent.react_mode {
            react::run_react(conn, crypto, &agent, &current_input, workspace_id, llm, on_step)?
        } else {
            execute_agent_steps(conn, crypto, &agent, &current_input, workspace_id, llm, on_step)?
        };

        all_steps.extend(steps);
        final_output  = output.clone();
        current_input = output;
    }

    registry::save_run(
        conn,
        &pipeline.agent_ids[0],
        Some(pipeline_id),
        input,
        Some(&final_output),
        "completed",
        &all_steps,
        workspace_id,
    )
}

// ── Core execution ────────────────────────────────────────────────────────

fn execute_agent_steps(
    conn:         &Connection,
    crypto:       &CryptoContext,
    agent:        &UserAgent,
    input:        &str,
    workspace_id: Option<&str>,
    llm:          &LlmConfig,
    on_step:      &dyn Fn(&AgentRunStep),
) -> AppResult<(String, Vec<AgentRunStep>)> {
    let registry = HandlerRegistry::new();

    // Sort the agent's declared tools by ActionScale.
    // This guarantees: Retrieval → External → Synthesis → Memory,
    // regardless of the order the user listed them.
    let sorted_tools = registry.sort_by_scale(&agent.tools);

    let mut context_parts:  Vec<String>       = Vec::new();
    let mut current_answer: String            = input.to_string();
    let mut answer_updated: bool              = false;
    let mut steps:          Vec<AgentRunStep> = Vec::new();

    for tool_id in &sorted_tools {
        let Some(handler) = registry.get(tool_id) else { continue };

        let ctx = RunCtx {
            conn,
            crypto,
            workspace_id,
            llm,
            agent,
            input,
            context_parts:  &context_parts,
            current_answer: &current_answer,
        };

        let output = handler.run(&ctx)?;

        on_step(&output.step);
        for s in &output.extra_steps { on_step(s); }
        steps.push(output.step);
        steps.extend(output.extra_steps);

        // Context fragments accumulate across scales (retrieval/fetch → injected into Synthesis)
        if let Some(fragment) = output.context_fragment {
            context_parts.push(fragment);
        }

        // Running answer is updated by Synthesis (LLM) and Memory-scale handlers
        if let Some(answer) = output.answer {
            current_answer = answer;
            answer_updated = true;
        }
    }

    // If no Synthesis handler produced an answer, fall back to accumulated context
    if !answer_updated {
        current_answer = if !context_parts.is_empty() {
            context_parts.join("\n\n")
        } else if sorted_tools.is_empty() {
            format!("Agent \"{}\" has no tools configured.", agent.name)
        } else {
            // Tools ran but none produced a final answer (e.g. write_file, save_artifact).
            // Enable ReAct mode so the LLM can decide which tool to call and how to format input.
            "Tools ran but produced no text output. \
             Enable ReAct mode on this agent so the LLM can reason about tool inputs and outputs."
                .into()
        };
    }

    Ok((current_answer, steps))
}
