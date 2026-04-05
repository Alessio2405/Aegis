//! save_artifact — writes a finding/summary/report to the persistent artifact workspace.
//!
//! Input format (plain text):
//!   title: <one-line title>
//!   type: finding | summary | report | note | code   (optional, defaults to "finding")
//!   ---
//!   <content goes here, can be multiple lines>

use rusqlite::params;
use uuid::Uuid;

use crate::error::AppResult;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};
use crate::agent::registry::AgentRunStep;

pub struct SaveArtifactHandler;

impl ToolHandler for SaveArtifactHandler {
    fn id(&self) -> &'static str { "save_artifact" }
    fn scale(&self) -> ActionScale { ActionScale::Memory }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let (title, artifact_type, content) = parse_artifact_input(ctx.input);

        let id  = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let agent_name = &ctx.agent.name;
        let agent_id   = &ctx.agent.id;

        ctx.conn.execute(
            "INSERT INTO artifacts(id, agent_id, agent_name, title, content, artifact_type, created_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![id, agent_id, agent_name, title, content, artifact_type, now],
        )?;

        Ok(ToolOutput {
            answer:           None,
            context_fragment: None,
            step: AgentRunStep {
                step_type:   "save_artifact".into(),
                description: format!("Saved artifact: {title}"),
                output:      format!("[{artifact_type}] {title}"),
            },
            extra_steps: vec![],
        })
    }
}

fn parse_artifact_input(raw: &str) -> (String, String, String) {
    let mut title = String::new();
    let mut artifact_type = "finding".to_string();
    let mut content_lines: Vec<&str> = Vec::new();
    let mut in_content = false;

    for line in raw.lines() {
        if in_content {
            content_lines.push(line);
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("title:") {
            title = line["title:".len()..].trim().to_string();
        } else if lower.starts_with("type:") {
            artifact_type = line["type:".len()..].trim().to_lowercase();
        } else if line.trim() == "---" {
            in_content = true;
        } else if title.is_empty() {
            // First non-empty line as title if no "title:" header
            let t = line.trim();
            if !t.is_empty() { title = t.to_string(); }
        }
    }

    // If no "---" separator, treat everything after title line as content
    if !in_content {
        content_lines = raw.lines()
            .filter(|l| !l.to_ascii_lowercase().starts_with("title:")
                     && !l.to_ascii_lowercase().starts_with("type:"))
            .collect();
    }

    if title.is_empty() { title = "Untitled artifact".to_string(); }
    let content = content_lines.join("\n").trim().to_string();
    let content = if content.is_empty() { raw.trim().to_string() } else { content };

    (title, artifact_type, content)
}
