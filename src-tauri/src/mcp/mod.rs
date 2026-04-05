//! MCP (Model Context Protocol) client — stdio transport.
//!
//! Each call spawns a fresh subprocess, performs the JSON-RPC handshake,
//! executes the request, and kills the process. Stateless; no persistent connections.
//!
//! Tool IDs exposed to agents: `mcp_{safe_label}_{tool_name}`
//! (spaces/hyphens in label → underscores, lowercased)

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub server_id: String,
    pub server_label: String,
    /// Stable ID used in the HandlerRegistry: `mcp_{safe_label}_{name}`
    pub tool_id: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

// ── DB helpers ────────────────────────────────────────────────────────────────

pub fn add_server(
    conn: &Connection,
    label: &str,
    command: &str,
    args: &[String],
) -> AppResult<McpServerConfig> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let args_json = serde_json::to_string(args).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO mcp_servers(id, label, command, args_json, tools_json, enabled, created_at) \
         VALUES(?1,?2,?3,?4,'[]',1,?5)",
        params![id, label, command, args_json, now],
    )?;
    Ok(McpServerConfig {
        id,
        label: label.to_string(),
        command: command.to_string(),
        args: args.to_vec(),
        enabled: true,
        created_at: now,
    })
}

pub fn cache_tools(conn: &Connection, server_id: &str, tools: &[McpTool]) -> AppResult<()> {
    let json = serde_json::to_string(tools).unwrap_or_else(|_| "[]".into());
    conn.execute("UPDATE mcp_servers SET tools_json=?1 WHERE id=?2", params![json, server_id])?;
    Ok(())
}

pub fn remove_server(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM mcp_servers WHERE id=?1", params![id])?;
    Ok(())
}

pub fn list_servers(conn: &Connection) -> AppResult<Vec<McpServerConfig>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, command, args_json, enabled, created_at \
         FROM mcp_servers ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let args_json: String = r.get(3)?;
            Ok(McpServerConfig {
                id:          r.get(0)?,
                label:       r.get(1)?,
                command:     r.get(2)?,
                args:        serde_json::from_str(&args_json).unwrap_or_default(),
                enabled:     r.get::<_, i64>(4)? != 0,
                created_at:  r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Load cached tools for all enabled MCP servers.
pub fn load_all_tools(conn: &Connection) -> AppResult<Vec<McpTool>> {
    let mut stmt = conn.prepare(
        "SELECT id, tools_json FROM mcp_servers WHERE enabled=1",
    )?;
    let mut result = Vec::new();
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (_, tools_json) = row?;
        if let Ok(tools) = serde_json::from_str::<Vec<McpTool>>(&tools_json) {
            result.extend(tools);
        }
    }
    Ok(result)
}

// ── MCP protocol ──────────────────────────────────────────────────────────────

pub fn safe_label(label: &str) -> String {
    label
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

/// Spawn server, run initialize + tools/list, return tool list.
pub fn list_tools_from_server(cfg: &McpServerConfig) -> AppResult<Vec<McpTool>> {
    let mut parts = cfg.command.split_whitespace();
    let prog = parts.next().unwrap_or(&cfg.command);
    let mut cmd = Command::new(prog);
    for p in parts { cmd.arg(p); }
    for a in &cfg.args { cmd.arg(a); }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = cmd.spawn()
        .map_err(|e| AppError::Connector(format!("MCP spawn '{}': {e}", cfg.command)))?;

    let mut stdin  = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    // 1. Initialize
    let init = json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{
            "protocolVersion":"2024-11-05",
            "capabilities":{},
            "clientInfo":{"name":"local-first-agent-os","version":"0.1.0"}
        }
    });
    writeln!(stdin, "{init}").ok();
    read_response(&mut lines, 1)?;

    // 2. Initialized notification
    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).ok();

    // 3. List tools
    let req = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
    writeln!(stdin, "{req}").ok();
    let resp = read_response(&mut lines, 2)?;
    child.kill().ok();

    let arr = resp["result"]["tools"]
        .as_array()
        .ok_or_else(|| AppError::Connector("MCP: no tools array".into()))?;

    let safe = safe_label(&cfg.label);
    let tools = arr.iter().map(|t| {
        let name = t["name"].as_str().unwrap_or("tool").to_string();
        McpTool {
            server_id:    cfg.id.clone(),
            server_label: cfg.label.clone(),
            tool_id:      format!("mcp_{safe}_{name}"),
            name:         name.clone(),
            description:  t["description"].as_str().unwrap_or("MCP tool").to_string(),
            input_schema: t.get("inputSchema").cloned().unwrap_or(Value::Null),
        }
    }).collect();

    Ok(tools)
}

/// Spawn server, call a specific tool, return text output.
pub fn call_tool(cfg: &McpServerConfig, tool_name: &str, args: Value) -> AppResult<String> {
    let mut parts = cfg.command.split_whitespace();
    let prog = parts.next().unwrap_or(&cfg.command);
    let mut cmd = Command::new(prog);
    for p in parts { cmd.arg(p); }
    for a in &cfg.args { cmd.arg(a); }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = cmd.spawn()
        .map_err(|e| AppError::Connector(format!("MCP spawn '{}': {e}", cfg.command)))?;

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();

    let init = json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{
            "protocolVersion":"2024-11-05",
            "capabilities":{},
            "clientInfo":{"name":"local-first-agent-os","version":"0.1.0"}
        }
    });
    writeln!(stdin, "{init}").ok();
    read_response(&mut lines, 1)?;
    writeln!(stdin, r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#).ok();

    let req = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
                     "params":{"name":tool_name,"arguments":args}});
    writeln!(stdin, "{req}").ok();
    let resp = read_response(&mut lines, 2)?;
    child.kill().ok();

    // Extract text content
    let content = &resp["result"]["content"];
    if let Some(arr) = content.as_array() {
        let text: String = arr
            .iter()
            .filter_map(|c| {
                if c["type"].as_str() == Some("text") { c["text"].as_str().map(|s| s.to_string()) }
                else { None }
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(if text.is_empty() { resp["result"].to_string() } else { text })
    } else if let Some(s) = content.as_str() {
        Ok(s.to_string())
    } else {
        Ok(resp["result"].to_string())
    }
}

// ── Internal ──────────────────────────────────────────────────────────────────

fn read_response(
    lines: &mut impl Iterator<Item = std::io::Result<String>>,
    expected_id: u64,
) -> AppResult<Value> {
    for _ in 0..200 {
        let line = match lines.next() {
            Some(Ok(l)) => l,
            Some(Err(e)) => return Err(AppError::Connector(format!("MCP read: {e}"))),
            None => return Err(AppError::Connector("MCP: process closed stdout".into())),
        };
        let line = line.trim();
        if line.is_empty() { continue; }
        let val: Value = serde_json::from_str(line)
            .map_err(|e| AppError::Connector(format!("MCP JSON parse: {e} — raw: {line}")))?;
        // Skip notifications (no "id" field)
        if val.get("id").is_none() { continue; }
        if val["id"].as_u64() == Some(expected_id) {
            if let Some(err) = val.get("error") {
                return Err(AppError::Connector(format!("MCP error: {err}")));
            }
            return Ok(val);
        }
    }
    Err(AppError::Connector("MCP: response timeout (too many lines)".into()))
}
