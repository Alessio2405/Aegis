//! Scale: Memory — lets an agent drop a task into the queue for another agent.
//!
//! Input formats:
//!   JSON: {"agent":"name or id","input":"task description","priority":5}
//!   Bare: first line = agent name or ID, rest = task description
//!
//! The scheduler will pick up the pending task within one tick (≤60s).

use rusqlite::params;
use serde_json::Value;
use uuid::Uuid;
use chrono::Utc;

use crate::error::{AppError, AppResult};
use crate::agent::registry::{AgentRunStep, self};
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct EnqueueHandler;

impl ToolHandler for EnqueueHandler {
    fn id(&self) -> &'static str { "enqueue" }
    fn scale(&self) -> ActionScale { ActionScale::Memory }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let raw = ctx.input.trim();

        let (agent_ref, task, priority) = parse_enqueue_input(raw).ok_or_else(|| {
            AppError::InvalidInput(
                "enqueue: provide JSON {\"agent\":\"name\",\"input\":\"task\"} or two lines: agent_name\\ntask".into()
            )
        })?;

        // Resolve agent by ID, then by name
        let agent = registry::get_agent(ctx.conn, &agent_ref).or_else(|_| {
            registry::list_agents(ctx.conn).and_then(|agents| {
                agents
                    .into_iter()
                    .find(|a| a.name.to_lowercase().contains(&agent_ref.to_lowercase()))
                    .ok_or_else(|| AppError::NotFound(format!("No agent matching '{agent_ref}'")))
            })
        })?;

        let id  = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        ctx.conn.execute(
            "INSERT INTO task_queue(id, agent_id, input, status, priority, workspace_id, created_at) \
             VALUES(?1,?2,?3,'pending',?4,?5,?6)",
            params![id, agent.id, task, priority, ctx.workspace_id, now],
        ).map_err(AppError::Database)?;

        let msg = format!(
            "Task enqueued for agent \"{}\" (id: {}) with priority {}. Task id: {id}",
            agent.name, agent.id, priority
        );

        Ok(ToolOutput {
            answer:           Some(msg.clone()),
            context_fragment: Some(format!("ENQUEUED: {msg}")),
            step: AgentRunStep {
                step_type:   "enqueue".into(),
                description: format!("Enqueued task for \"{}\"", agent.name),
                output:      id,
            },
            extra_steps: vec![],
        })
    }
}

fn parse_enqueue_input(raw: &str) -> Option<(String, String, i64)> {
    // JSON format
    if raw.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            let agent    = v.get("agent").and_then(|x| x.as_str()).map(str::to_string)?;
            let input    = v.get("input").and_then(|x| x.as_str()).map(str::to_string)?;
            let priority = v.get("priority").and_then(|x| x.as_i64()).unwrap_or(0);
            if !agent.is_empty() && !input.is_empty() {
                return Some((agent, input, priority));
            }
        }
    }

    // Bare format: first line = agent, rest = task
    let (first, rest) = raw.split_once('\n')?;
    let agent_ref = first.trim().to_string();
    let task      = rest.trim().to_string();
    if !agent_ref.is_empty() && !task.is_empty() {
        return Some((agent_ref, task, 0));
    }

    None
}
