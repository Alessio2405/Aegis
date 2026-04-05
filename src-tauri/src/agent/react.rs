//! ReAct (Reasoning + Acting) agent loop.
//!
//! Unlike the static scale-order pipeline, ReAct lets the LLM drive tool selection:
//!
//!   1. Think  — LLM reasons about what to do next and picks a tool
//!   2. Act    — the chosen tool is dispatched via HandlerRegistry
//!   3. Observe — tool output is fed back to the LLM as context
//!   4. Repeat until "Final Answer:" is found or MAX_ITERATIONS is reached
//!
//! Special built-in actions (not in HandlerRegistry):
//!   delegate      — run another agent as a sub-task (recursive, depth-limited)
//!   save_artifact — write a finding/summary to the persistent artifact workspace
//!
//! Format the LLM must follow:
//!   Thought: <reasoning>
//!   Action: <tool_id>
//!   Action Input: <input for the tool>
//!
//! Or, when ready to answer:
//!   Thought: <reasoning>
//!   Final Answer: <answer text>

use rusqlite::Connection;

use crate::crypto::CryptoContext;
use crate::connectors::{self, LlmConfig};
use crate::error::{AppError, AppResult};
use super::registry::{self, AgentRunStep, UserAgent};
use super::tools::{HandlerRegistry, RunCtx};

const MAX_ITERATIONS: usize  = 20;
const MAX_DELEGATE_DEPTH: u8 = 3;

// ── Public entry point ────────────────────────────────────────────────────────

/// Run an agent using the ReAct loop. Returns `(final_answer, steps)`.
pub fn run_react(
    conn:         &Connection,
    crypto:       &CryptoContext,
    agent:        &UserAgent,
    input:        &str,
    workspace_id: Option<&str>,
    llm:          &LlmConfig,
    on_step:      &dyn Fn(&AgentRunStep),
) -> AppResult<(String, Vec<AgentRunStep>)> {
    run_react_inner(conn, crypto, agent, input, workspace_id, llm, on_step, 0)
}

// ── Internal loop ─────────────────────────────────────────────────────────────

fn run_react_inner(
    conn:         &Connection,
    crypto:       &CryptoContext,
    agent:        &UserAgent,
    input:        &str,
    workspace_id: Option<&str>,
    llm:          &LlmConfig,
    on_step:      &dyn Fn(&AgentRunStep),
    depth:        u8,
) -> AppResult<(String, Vec<AgentRunStep>)> {
    if !llm.enabled {
        return Err(AppError::Connector(
            "ReAct mode requires a model. Enable one in Settings → Model.".into()
        ));
    }

    let registry  = HandlerRegistry::with_mcp(conn);
    let tool_list = build_tool_list(agent, &registry, conn);
    let tool_ids: Vec<&str> = agent.tools.iter().map(|s| s.as_str()).collect();
    let system    = build_system_prompt(agent, &tool_list, &tool_ids, conn, depth);

    let mut history: Vec<(String, String)> = vec![
        ("system".into(), system),
        ("user".into(),   format!("Task: {input}")),
    ];

    let mut steps:        Vec<AgentRunStep> = Vec::new();
    let mut final_answer: Option<String>    = None;

    for iteration in 0..MAX_ITERATIONS {
        let llm_response = connectors::llm_chat_multi(llm, &history)?;

        match parse_react_response(&llm_response) {
            // ── Final answer ──────────────────────────────────────────────────
            ReactResponse::FinalAnswer(answer) => {
                let step = AgentRunStep {
                    step_type:   "react_think".into(),
                    description: format!("Step {}: Final Answer", iteration + 1),
                    output:      extract_thought(&llm_response),
                };
                on_step(&step);
                steps.push(step);
                final_answer = Some(answer);
                break;
            }

            // ── Tool call ─────────────────────────────────────────────────────
            ReactResponse::ToolCall { tool, input: tool_input } => {
                let thought = extract_thought(&llm_response);
                let think_step = AgentRunStep {
                    step_type:   "react_think".into(),
                    description: format!(
                        "Step {}: {} → {}",
                        iteration + 1,
                        thought.chars().take(80).collect::<String>(),
                        tool
                    ),
                    output: thought,
                };
                on_step(&think_step);
                steps.push(think_step);

                history.push(("assistant".into(), llm_response.clone()));

                // ── Special: delegate ──────────────────────────────────────────
                let observation = if tool == "delegate" {
                    handle_delegate(
                        conn, crypto, &tool_input, workspace_id, llm, on_step, depth, &mut steps,
                        &agent.id,
                    )
                }
                // ── Regular tool from registry ─────────────────────────────────
                else {
                    match registry.get(&tool) {
                        Some(handler) => {
                            let ctx = RunCtx {
                                conn,
                                crypto,
                                workspace_id,
                                llm,
                                agent,
                                input:          &tool_input,
                                context_parts:  &[],
                                current_answer: &tool_input,
                            };
                            match handler.run(&ctx) {
                                Ok(out) => {
                                    let obs = out.context_fragment
                                        .or(out.answer)
                                        .unwrap_or_else(|| out.step.output.clone());
                                    on_step(&out.step);
                                    for s in &out.extra_steps { on_step(s); }
                                    steps.push(out.step);
                                    steps.extend(out.extra_steps);
                                    obs
                                }
                                Err(e) => format!("[tool error: {e}]"),
                            }
                        }
                        None => format!(
                            "[unknown tool '{}'. Valid tools: {}]",
                            tool,
                            tool_ids.join(", ")
                        ),
                    }
                };

                let obs_trimmed: String = observation.chars().take(2000).collect();
                let obs_step = AgentRunStep {
                    step_type:   "react_observe".into(),
                    description: format!("Observation from {tool}"),
                    output:      obs_trimmed.chars().take(300).collect(),
                };
                on_step(&obs_step);
                steps.push(obs_step);

                history.push(("user".into(), format!("Observation: {obs_trimmed}")));
            }

            // ── Unstructured ─────────────────────────────────────────────────
            ReactResponse::Unknown => {
                let step = AgentRunStep {
                    step_type:   "react_think".into(),
                    description: format!("Step {}: unstructured (treated as final)", iteration + 1),
                    output:      llm_response.chars().take(300).collect(),
                };
                on_step(&step);
                steps.push(step);
                final_answer = Some(llm_response);
                break;
            }
        }
    }

    let answer = final_answer.unwrap_or_else(|| {
        "Max iterations reached without a final answer.".to_string()
    });

    Ok((answer, steps))
}

// ── Delegate handler ──────────────────────────────────────────────────────────

fn handle_delegate(
    conn:              &Connection,
    crypto:            &CryptoContext,
    tool_input:        &str,
    workspace_id:      Option<&str>,
    llm:               &LlmConfig,
    on_step:           &dyn Fn(&AgentRunStep),
    depth:             u8,
    steps:             &mut Vec<AgentRunStep>,
    calling_agent_id:  &str,
) -> String {
    if depth >= MAX_DELEGATE_DEPTH {
        return "[Max delegation depth reached — cannot delegate further]".into();
    }

    // Parse: first line = agent name or ID, rest = task
    let (agent_ref, task) = parse_delegate_input(tool_input);
    if agent_ref.is_empty() || task.is_empty() {
        return "[delegate: provide agent name/ID on line 1 and task on line 2+]".into();
    }

    // Resolve agent by ID, fallback to name search
    let agent_result = registry::get_agent(conn, &agent_ref).or_else(|_| {
        registry::list_agents(conn).and_then(|agents| {
            agents
                .into_iter()
                .find(|a| a.name.to_lowercase().contains(&agent_ref.to_lowercase()))
                .ok_or_else(|| crate::error::AppError::NotFound(
                    format!("No agent found matching '{agent_ref}'")
                ))
        })
    });

    match agent_result {
        Err(e) => format!("[delegate error: {e}]"),
        Ok(sub_agent) => {
            // Prevent delegating to self — would create an infinite loop
            if sub_agent.id == calling_agent_id {
                return "[delegate error: cannot delegate to yourself — give a Final Answer instead]".into();
            }
            let delegate_step = AgentRunStep {
                step_type:   "delegate".into(),
                description: format!("Delegating to \"{}\" (depth {})", sub_agent.name, depth + 1),
                output:      task.chars().take(120).collect(),
            };
            on_step(&delegate_step);
            steps.push(delegate_step);

            match run_react_inner(conn, crypto, &sub_agent, &task, workspace_id, llm, on_step, depth + 1) {
                Ok((output, sub_steps)) => {
                    steps.extend(sub_steps);
                    output
                }
                Err(e) => format!("[sub-agent error: {e}]"),
            }
        }
    }
}

fn parse_delegate_input(raw: &str) -> (String, String) {
    // JSON format: {"agent": "name", "input": "task description"}
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let agent = v.get("agent").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            let input = v.get("input").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            if !agent.is_empty() && !input.is_empty() {
                return (agent, input);
            }
        }
    }
    // Bare format: first line = agent name/id, rest = task
    let mut lines = raw.lines();
    let agent_ref = lines.next().unwrap_or("").trim().to_string();
    let task = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    (agent_ref, task)
}

// ── Parsing ───────────────────────────────────────────────────────────────────

enum ReactResponse {
    FinalAnswer(String),
    ToolCall { tool: String, input: String },
    Unknown,
}

fn parse_react_response(text: &str) -> ReactResponse {
    if let Some(pos) = text.find("Final Answer:") {
        let answer = text[pos + "Final Answer:".len()..].trim().to_string();
        return ReactResponse::FinalAnswer(answer);
    }

    let tool  = extract_line(text, "Action:");
    let input = extract_line(text, "Action Input:");

    if let (Some(t), Some(i)) = (tool, input) {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return ReactResponse::ToolCall { tool: t, input: i.trim().to_string() };
        }
    }

    ReactResponse::Unknown
}

fn extract_line(text: &str, prefix: &str) -> Option<String> {
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with(prefix) {
            let first = trimmed[prefix.len()..].trim().to_string();
            let mut rest = vec![first];
            for next_line in lines.by_ref() {
                let nl = next_line.trim();
                let is_key = nl.starts_with("Thought:")
                    || nl.starts_with("Action:")
                    || nl.starts_with("Action Input:")
                    || nl.starts_with("Final Answer:")
                    || nl.starts_with("Observation:");
                if is_key { break; }
                rest.push(nl.to_string());
            }
            let value = rest.join("\n").trim().to_string();
            if !value.is_empty() { return Some(value); }
        }
    }
    None
}

fn extract_thought(text: &str) -> String {
    extract_line(text, "Thought:")
        .unwrap_or_else(|| text.lines().next().unwrap_or("").trim().to_string())
}

// ── Prompt builders ───────────────────────────────────────────────────────────

fn build_tool_list(agent: &UserAgent, registry: &HandlerRegistry, conn: &Connection) -> String {
    let mut lines: Vec<String> = agent.tools.iter().filter_map(|id| {
        registry.get(id).map(|h| {
            let desc = h.description_hint().unwrap_or_else(|| match h.id() {
                "search_documents" => "Search the knowledge base. Input: a search query string.",
                "fetch_url"        => "Fetch a web page (plain HTTP). Input: a full URL.",
                "browser"          => "Fetch a web page with JS rendering. Input: a full URL.",
                "web_search"       => "Search the web. Input: a search query string.",
                "run_python"       => "Run Python code. Input: the Python code (no fences).",
                "write_file"       => "Write text to a file. Input MUST be valid JSON with the EXACT workspace path from above: {\"path\":\"/your/workspace/dir/filename.md\",\"content\":\"file text here\"}",
                "propose_memory"   => "Save a fact to long-term memory. Input: the fact.",
                "save_artifact"    => "Save a finding to the persistent workspace. Input:\n  title: <one-line title>\n  type: finding|summary|report|note|code\n  ---\n  <content>",
                "read_file"        => "Read a file's content (or list a directory). Input: JSON {\"path\":\"...\"} or bare path. Relative paths are resolved from the agent working directory (e.g. src-tauri/src/lib.rs).",
                "recall"           => "Read a value from this agent's persistent scratchpad. Input: a key name, or 'all' to list every stored key.",
                "remember"         => "Write a value to this agent's persistent scratchpad (survives across runs). Input JSON: {\"key\":\"k\",\"value\":\"v\"}",
                "enqueue"          => "Drop a task into another agent's queue for async execution. Input JSON: {\"agent\":\"name or id\",\"input\":\"task description\",\"priority\":0}",
                "git"              => "Run a git command in the working directory. Input JSON: {\"action\":\"status|diff|log|add|commit|pull|push|clone|show\",\"repo\":\"/abs/path\",\"message\":\"commit msg\",\"files\":[\"file.md\"],\"url\":\"https://...clone-url\"}. 'repo' defaults to agent working directory. 'message' required for commit; 'url' required for clone; 'files' optional for add (defaults to '.').",
                "shell"            => "Run a shell command in the agent working directory. Input: bare command WITHOUT quotes (e.g. dir src-tauri\\src) or JSON {\"cmd\":\"...\",\"cwd\":\"/optional/subdir\"}. On Windows use 'dir' not 'ls'. Timeout: 60 s. Output capped at 8 KB.",
                "desktop"          => "Control the desktop. Input: DESKTOP:TYPE:<text> | DESKTOP:KEY:<combo> | DESKTOP:OPEN:<target>.",
                _                  => "A specialized tool. Input: relevant parameters.",
            });
            format!("- {}: {}", h.id(), desc)
        })
    }).collect();

    // Always include delegate for react-mode agents
    let agent_list = build_agent_list(conn);
    lines.push(format!(
        "- delegate: Run another agent as a sub-task. Input: line 1 = agent name or ID, line 2+ = task description.\n  Available agents:\n{agent_list}"
    ));

    lines.join("\n")
}

fn build_workspace_info(conn: &Connection) -> String {
    // Prefer the configured agent working directory if set
    let work_dir: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key='agent_work_dir'", [], |r| r.get(0))
        .ok()
        .filter(|s: &String| !s.trim().is_empty());

    if let Some(dir) = work_dir {
        return format!(
            "\nAgent working directory: {dir}\n\
             ALL file operations (write_file) MUST use paths inside this directory.\n\
             Example: {dir}/output.md"
        );
    }

    // Fall back to workspace directories
    let mut stmt = match conn.prepare("SELECT path FROM workspaces") {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let paths: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    if paths.is_empty() {
        "\nWorking directory: none configured — set one in Settings → Agent Working Directory before using write_file.".into()
    } else {
        format!(
            "\nAllowed directories for write_file (paths must start with one of these):\n{}",
            paths.iter().map(|p| format!("  • {p}")).collect::<Vec<_>>().join("\n")
        )
    }
}

fn build_agent_list(conn: &Connection) -> String {
    match registry::list_agents(conn) {
        Ok(agents) if !agents.is_empty() => agents
            .iter()
            .map(|a| format!("    • {} (id: {})", a.name, a.id))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => "    (no other agents configured)".into(),
    }
}

fn build_system_prompt(
    agent: &UserAgent,
    tool_list: &str,
    tool_ids: &[&str],
    conn: &Connection,
    depth: u8,
) -> String {
    let role = if agent.system_prompt.is_empty() {
        "You are a helpful autonomous agent.".to_string()
    } else {
        agent.system_prompt.clone()
    };
    let all_ids = {
        let mut ids = tool_ids.to_vec();
        ids.push("delegate");
        ids.join(", ")
    };
    let depth_note = if depth > 0 {
        format!("\n(You are running as a sub-agent at depth {depth}.)")
    } else {
        String::new()
    };
    let workspace_info = build_workspace_info(conn);
    format!(
        "{role}{depth_note}{workspace_info}\n\n\
         You have access to these tools:\n{tool_list}\n\n\
         To use a tool, respond in EXACTLY this format (no extra text before Thought:):\n\
         Thought: <your reasoning about what to do next>\n\
         Action: <tool_id>\n\
         Action Input: <input for the tool>\n\n\
         When you have gathered enough information to fully answer, respond:\n\
         Thought: <your reasoning>\n\
         Final Answer: <your complete, well-formatted answer>\n\n\
         Rules:\n\
         - Always start with Thought:\n\
         - Action must be exactly one of: {all_ids}\n\
         - Never fabricate Observations — only use what the tool returns\n\
         - One Action per response only\n\
         - Be concise in Action Input\n\
         - If a tool fails, give a Final Answer explaining why rather than retrying the same thing or delegating\n\
         - NEVER claim to have written a file or saved an artifact in your Final Answer unless you actually called the tool and got a success Observation. If you have not yet called save_artifact or write_file, do it before giving a Final Answer.\n\
         - When the task asks you to save, summarize, or produce a report: you MUST call save_artifact as a dedicated Action BEFORE your Final Answer. Do NOT include the content only in your Final Answer text — call the tool.\n\
         - Never use delegate when you can do the action yourself. save_artifact, write_file, and propose_memory are your own tools — call them directly."
    )
}
