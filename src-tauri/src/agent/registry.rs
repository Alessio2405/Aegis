//! CRUD for user-defined agents, pipelines, and run history.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppResult;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAgent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    /// Tool names this agent may invoke: "search_documents" | "chat_llm" | "propose_memory"
    pub tools: Vec<String>,
    pub workspace_id: Option<String>,
    /// Optional schedule interval e.g. "every 1h", "every 24h". None = on-demand only.
    pub schedule: Option<String>,
    /// ISO8601 timestamp of last scheduled run. None = never run.
    pub last_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// If true, the scheduler skips this agent even when it is due.
    pub schedule_paused: bool,
    /// If true, uses the ReAct (think→act→observe) loop instead of the static scale-order pipeline.
    pub react_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPipeline {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Ordered agent IDs — output of step N becomes input of step N+1
    pub agent_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunStep {
    pub step_type: String,   // "search" | "llm" | "propose_memory" | "pipeline_step"
    pub description: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: String,
    pub agent_id: String,
    pub pipeline_id: Option<String>,
    pub input: String,
    pub output: Option<String>,
    pub status: String,
    pub steps: Vec<AgentRunStep>,
    pub workspace_id: Option<String>,
    pub created_at: String,
}

// ── Agent CRUD ────────────────────────────────────────────────────────────

pub fn create_agent(
    conn: &Connection,
    name: &str,
    description: &str,
    system_prompt: &str,
    tools: &[String],
    workspace_id: Option<&str>,
    schedule: Option<&str>,
    react_mode: bool,
) -> AppResult<UserAgent> {
    let id = Uuid::new_v4().to_string();
    let tools_json = serde_json::to_string(tools).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO user_agents
            (id, name, description, system_prompt, tools_json, workspace_id, schedule, react_mode, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'), datetime('now'))",
        params![id, name, description, system_prompt, tools_json, workspace_id, schedule, react_mode as i64],
    )?;
    get_agent(conn, &id)
}

pub fn get_agent(conn: &Connection, id: &str) -> AppResult<UserAgent> {
    let agent = conn.query_row(
        "SELECT id, name, description, system_prompt, tools_json, workspace_id,
                schedule, last_run_at, created_at, updated_at, schedule_paused, react_mode
         FROM user_agents WHERE id = ?1",
        params![id],
        row_to_agent,
    )?;
    Ok(agent)
}

pub fn list_agents(conn: &Connection) -> AppResult<Vec<UserAgent>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, system_prompt, tools_json, workspace_id,
                schedule, last_run_at, created_at, updated_at, schedule_paused, react_mode
         FROM user_agents ORDER BY created_at DESC",
    )?;
    let agents = stmt
        .query_map([], row_to_agent)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(agents)
}

/// List only agents that have a non-null schedule (for the scheduler loop).
pub fn list_scheduled_agents(conn: &Connection) -> AppResult<Vec<UserAgent>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, system_prompt, tools_json, workspace_id,
                schedule, last_run_at, created_at, updated_at, schedule_paused, react_mode
         FROM user_agents WHERE schedule IS NOT NULL ORDER BY last_run_at ASC NULLS FIRST",
    )?;
    let agents = stmt
        .query_map([], row_to_agent)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(agents)
}

pub fn update_agent(
    conn: &Connection,
    id: &str,
    name: &str,
    description: &str,
    system_prompt: &str,
    tools: &[String],
    react_mode: bool,
) -> AppResult<()> {
    let tools_json = serde_json::to_string(tools).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "UPDATE user_agents
         SET name=?2, description=?3, system_prompt=?4, tools_json=?5, react_mode=?6, updated_at=datetime('now')
         WHERE id=?1",
        params![id, name, description, system_prompt, tools_json, react_mode as i64],
    )?;
    Ok(())
}

/// Stamp `last_run_at` after a successful scheduled execution.
pub fn update_last_run(conn: &Connection, id: &str, ts: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE user_agents SET last_run_at=?2 WHERE id=?1",
        params![id, ts],
    )?;
    Ok(())
}

pub fn delete_agent(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM user_agents WHERE id=?1", params![id])?;
    Ok(())
}

fn row_to_agent(row: &rusqlite::Row) -> rusqlite::Result<UserAgent> {
    let tools_json: String = row.get(4)?;
    let tools: Vec<String> = serde_json::from_str(&tools_json).unwrap_or_default();
    let paused_int: i64 = row.get(10).unwrap_or(0);
    let react_int:  i64 = row.get(11).unwrap_or(0);
    Ok(UserAgent {
        id:              row.get(0)?,
        name:            row.get(1)?,
        description:     row.get(2)?,
        system_prompt:   row.get(3)?,
        tools,
        workspace_id:    row.get(5)?,
        schedule:        row.get(6)?,
        last_run_at:     row.get(7)?,
        created_at:      row.get(8)?,
        updated_at:      row.get(9)?,
        schedule_paused: paused_int != 0,
        react_mode:      react_int  != 0,
    })
}

/// Pause or resume the schedule for an agent without changing its schedule string.
pub fn set_schedule_paused(conn: &Connection, id: &str, paused: bool) -> AppResult<()> {
    conn.execute(
        "UPDATE user_agents SET schedule_paused=?2, updated_at=datetime('now') WHERE id=?1",
        params![id, paused as i64],
    )?;
    Ok(())
}

// ── Pipeline CRUD ─────────────────────────────────────────────────────────

pub fn create_pipeline(
    conn: &Connection,
    name: &str,
    description: &str,
    agent_ids: &[String],
) -> AppResult<AgentPipeline> {
    let id = Uuid::new_v4().to_string();
    let ids_json = serde_json::to_string(agent_ids).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO agent_pipelines (id, name, description, agent_ids_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))",
        params![id, name, description, ids_json],
    )?;
    get_pipeline(conn, &id)
}

pub fn get_pipeline(conn: &Connection, id: &str) -> AppResult<AgentPipeline> {
    let pipeline = conn.query_row(
        "SELECT id, name, description, agent_ids_json, created_at, updated_at
         FROM agent_pipelines WHERE id=?1",
        params![id],
        row_to_pipeline,
    )?;
    Ok(pipeline)
}

pub fn list_pipelines(conn: &Connection) -> AppResult<Vec<AgentPipeline>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, agent_ids_json, created_at, updated_at
         FROM agent_pipelines ORDER BY created_at DESC",
    )?;
    let pipelines = stmt
        .query_map([], row_to_pipeline)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(pipelines)
}

pub fn delete_pipeline(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM agent_pipelines WHERE id=?1", params![id])?;
    Ok(())
}

fn row_to_pipeline(row: &rusqlite::Row) -> rusqlite::Result<AgentPipeline> {
    let ids_json: String = row.get(3)?;
    let agent_ids: Vec<String> = serde_json::from_str(&ids_json).unwrap_or_default();
    Ok(AgentPipeline {
        id:          row.get(0)?,
        name:        row.get(1)?,
        description: row.get(2)?,
        agent_ids,
        created_at:  row.get(4)?,
        updated_at:  row.get(5)?,
    })
}

// ── Run records ───────────────────────────────────────────────────────────

pub fn save_run(
    conn: &Connection,
    agent_id: &str,
    pipeline_id: Option<&str>,
    input: &str,
    output: Option<&str>,
    status: &str,
    steps: &[AgentRunStep],
    workspace_id: Option<&str>,
) -> AppResult<AgentRun> {
    let id = Uuid::new_v4().to_string();
    let steps_json = serde_json::to_string(steps).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO agent_runs (id, agent_id, pipeline_id, input, output, status, steps_json, workspace_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
        params![id, agent_id, pipeline_id, input, output, status, steps_json, workspace_id],
    )?;
    Ok(AgentRun {
        id,
        agent_id: agent_id.to_string(),
        pipeline_id: pipeline_id.map(|s| s.to_string()),
        input: input.to_string(),
        output: output.map(|s| s.to_string()),
        status: status.to_string(),
        steps: steps.to_vec(),
        workspace_id: workspace_id.map(|s| s.to_string()),
        created_at: String::new(), // DB-generated; caller gets it from a fresh query if needed
    })
}

pub fn list_runs(
    conn: &Connection,
    agent_id: Option<&str>,
    limit: i64,
) -> AppResult<Vec<AgentRun>> {
    let runs = if let Some(aid) = agent_id {
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, pipeline_id, input, output, status, steps_json, workspace_id, created_at
             FROM agent_runs WHERE agent_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![aid, limit], row_to_run)?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, pipeline_id, input, output, status, steps_json, workspace_id, created_at
             FROM agent_runs ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], row_to_run)?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    Ok(runs)
}

fn row_to_run(row: &rusqlite::Row) -> rusqlite::Result<AgentRun> {
    let steps_json: String = row.get(6)?;
    let steps: Vec<AgentRunStep> = serde_json::from_str(&steps_json).unwrap_or_default();
    Ok(AgentRun {
        id:          row.get(0)?,
        agent_id:    row.get(1)?,
        pipeline_id: row.get(2)?,
        input:       row.get(3)?,
        output:      row.get(4)?,
        status:      row.get(5)?,
        steps,
        workspace_id: row.get(7)?,
        created_at:   row.get(8)?,
    })
}
