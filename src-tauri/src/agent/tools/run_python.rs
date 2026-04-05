//! Scale: External — executes Python code blocks extracted from the agent input.
//!
//! Extracts fenced ```python ... ``` blocks from `ctx.input`, runs each via
//! `crate::python::run_code`, and returns combined stdout as the answer.
//! If no code blocks are found, the step is skipped gracefully.

use crate::error::AppResult;
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct RunPythonHandler;

impl ToolHandler for RunPythonHandler {
    fn id(&self) -> &'static str { "run_python" }
    fn scale(&self) -> ActionScale { ActionScale::External }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        let blocks = extract_python_blocks(ctx.input);

        if blocks.is_empty() {
            return Ok(ToolOutput {
                answer:           None,
                context_fragment: None,
                step: AgentRunStep {
                    step_type:   "run_python".into(),
                    description: "No Python code blocks found in input".into(),
                    output:      String::new(),
                },
                extra_steps: vec![],
            });
        }

        let mut outputs: Vec<String> = Vec::new();

        for (i, code) in blocks.iter().enumerate() {
            match crate::python::run_code(code, 30) {
                Ok(result) => {
                    let label = if blocks.len() > 1 {
                        format!("=== Block {} ===\n", i + 1)
                    } else {
                        String::new()
                    };
                    if result.timed_out {
                        outputs.push(format!("{label}[timed out after 30s]"));
                    } else if !result.stdout.is_empty() {
                        outputs.push(format!("{label}{}", result.stdout.trim()));
                    } else if !result.stderr.is_empty() {
                        outputs.push(format!("{label}[stderr] {}", result.stderr.trim().chars().take(500).collect::<String>()));
                    } else {
                        outputs.push(format!("{label}[exit code {}]", result.exit_code));
                    }
                }
                Err(e) => outputs.push(format!("=== Block {} ===\n[error: {e}]", i + 1)),
            }
        }

        let combined = outputs.join("\n\n");
        let preview: String = combined.chars().take(500).collect();

        Ok(ToolOutput {
            answer:           Some(combined.clone()),
            context_fragment: None,
            step: AgentRunStep {
                step_type:   "run_python".into(),
                description: format!("Executed {} Python block(s)", blocks.len()),
                output:      preview,
            },
            extra_steps: vec![],
        })
    }
}

// ── Code block extraction ──────────────────────────────────────────────────

/// Extract all ```python ... ``` fenced code blocks from text.
fn extract_python_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut remaining = text;

    while let Some(open) = remaining.find("```python") {
        let after_open = &remaining[open + 9..]; // skip "```python"
        // Skip any trailing whitespace / newline on the opening fence
        let code_start = after_open
            .find('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let code_region = &after_open[code_start..];

        if let Some(close) = code_region.find("```") {
            let code = code_region[..close].to_string();
            if !code.trim().is_empty() {
                blocks.push(code);
            }
            remaining = &code_region[close + 3..];
        } else {
            // No closing fence — treat rest as code
            let code = code_region.to_string();
            if !code.trim().is_empty() {
                blocks.push(code);
            }
            break;
        }
    }

    blocks
}
