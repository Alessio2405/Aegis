//! Scale: Retrieval — local FTS lookup, no network, no LLM.

use crate::permissions::{self, Capability};
use crate::retrieval;
use crate::error::AppResult;
use crate::agent::registry::AgentRunStep;
use super::{ActionScale, RunCtx, ToolHandler, ToolOutput};

pub struct SearchDocumentsHandler;

impl ToolHandler for SearchDocumentsHandler {
    fn id(&self) -> &'static str { "search_documents" }
    fn scale(&self) -> ActionScale { ActionScale::Retrieval }

    fn run(&self, ctx: &RunCtx<'_>) -> AppResult<ToolOutput> {
        permissions::require(
            ctx.conn, &Capability::FileRead, ctx.workspace_id,
            "agent", Some("chunk"), None,
        )?;

        let chunks = retrieval::retrieve(ctx.conn, ctx.crypto, ctx.input, ctx.workspace_id, 5, Some(ctx.llm))?;

        let summary = if chunks.is_empty() {
            "No matching documents found.".to_string()
        } else {
            chunks.iter().enumerate().map(|(i, c)| {
                format!(
                    "[{}] {} — {}",
                    i + 1,
                    c.doc_title,
                    c.preview.chars().take(200).collect::<String>()
                )
            }).collect::<Vec<_>>().join("\n\n")
        };

        Ok(ToolOutput {
            answer:           None,   // Retrieval never overwrites the answer
            context_fragment: Some(format!("CONTEXT FROM DOCUMENTS:\n{summary}")),
            step: AgentRunStep {
                step_type:   "search".into(),
                description: format!(
                    "Searched documents for: \"{}\"",
                    ctx.input.chars().take(80).collect::<String>()
                ),
                output: summary,
            },
            extra_steps: vec![],
        })
    }
}
