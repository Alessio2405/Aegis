//! Planner — produces a JSON plan (steps array) for a given request.
//! For MVP, plans are generated deterministically (no LLM needed).
//! When a model is integrated, the LLM would produce the same JSON shape.

use super::{Plan, PlanStep, RequestKind};

/// Build an execution plan for `kind` and `question`.
pub fn plan(kind: &RequestKind, question: &str, workspace_id: Option<&str>) -> Plan {
    match kind {
        RequestKind::QAndA => Plan {
            kind: RequestKind::QAndA,
            steps: vec![
                PlanStep::RetrieveChunks {
                    query: question.to_string(),
                    top_k: 5,
                    workspace_id: workspace_id.map(str::to_string),
                },
                PlanStep::SynthesizeAnswer {
                    question: question.to_string(),
                },
                // Optionally propose a memory after answering (executor can skip if empty)
                PlanStep::ProposeMemory {
                    content: String::new(), // filled in by executor
                    memory_type: "Episodic".to_string(),
                    source: "Q&A session".to_string(),
                },
            ],
        },
        RequestKind::MemoryOps => Plan {
            kind: RequestKind::MemoryOps,
            steps: vec![
                PlanStep::ProposeMemory {
                    content: question.to_string(),
                    memory_type: "Episodic".to_string(),
                    source: "user statement".to_string(),
                },
            ],
        },
        // Ingest and Settings are handled directly by Tauri commands, not the planner
        _ => Plan {
            kind: kind.clone(),
            steps: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qa_plan_has_retrieve_and_synthesize() {
        let plan = plan(&RequestKind::QAndA, "What is X?", None);
        assert!(plan.steps.iter().any(|s| matches!(s, PlanStep::RetrieveChunks { .. })));
        assert!(plan.steps.iter().any(|s| matches!(s, PlanStep::SynthesizeAnswer { .. })));
    }

    #[test]
    fn test_memory_ops_plan() {
        let plan = plan(&RequestKind::MemoryOps, "Remember that X uses Y", None);
        assert!(plan.steps.iter().any(|s| matches!(s, PlanStep::ProposeMemory { .. })));
    }
}
