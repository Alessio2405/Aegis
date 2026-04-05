//! Dynamic MCP tool handler — wraps one tool from one MCP server.
//! Tool IDs are leaked to produce &'static str (bounded: at most dozens of tools).

use crate::error::AppResult;
use crate::mcp::{self, McpServerConfig, McpTool};
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};
use crate::agent::registry::AgentRunStep;

pub struct McpToolHandler {
    pub server:      McpServerConfig,
    pub tool:        McpTool,
    /// &'static str produced by leaking a Box — fine for small, bounded counts.
    id_static: &'static str,
    desc_static: &'static str,
}

impl McpToolHandler {
    pub fn new(server: McpServerConfig, tool: McpTool) -> Self {
        let id_static:   &'static str = Box::leak(tool.tool_id.clone().into_boxed_str());
        let desc_static: &'static str = Box::leak(
            format!("{} (MCP server: {})", tool.description, server.label).into_boxed_str()
        );
        Self { server, tool, id_static, desc_static }
    }

    pub fn description(&self) -> &'static str { self.desc_static }
}

impl ToolHandler for McpToolHandler {
    fn id(&self)    -> &'static str { self.id_static }
    fn scale(&self) -> ActionScale  { ActionScale::External }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        // Try to parse Action Input as JSON; fall back to {"input": raw_text}
        let args: serde_json::Value = serde_json::from_str(ctx.input)
            .unwrap_or_else(|_| serde_json::json!({"input": ctx.input}));

        let output = mcp::call_tool(&self.server, &self.tool.name, args)?;

        Ok(ToolOutput {
            answer:           None,
            context_fragment: Some(output.clone()),
            step: AgentRunStep {
                step_type:   "mcp_tool".into(),
                description: format!("MCP {}:{}", self.server.label, self.tool.name),
                output:      output.chars().take(300).collect(),
            },
            extra_steps: vec![],
        })
    }
}
