//! Router — classifies incoming user input into a request kind.
//! Uses simple heuristic keyword matching (no LLM required).

use super::RequestKind;

/// Classify `input` into a RequestKind using keyword heuristics.
pub fn classify(input: &str) -> RequestKind {
    let lower = input.to_lowercase();

    // Q&A patterns
    let qa_signals = [
        "what", "who", "when", "where", "why", "how", "explain", "describe",
        "tell me", "summarize", "list", "find", "search",
    ];

    // Memory patterns
    let memory_signals = [
        "remember", "note that", "keep in mind", "recall", "memory",
        "don't forget", "add to memory",
    ];

    // Ingest patterns
    let ingest_signals = [
        "ingest", "index", "add folder", "scan", "import",
    ];

    // Settings patterns
    let settings_signals = [
        "enable model", "disable model", "settings", "configure", "set up ollama",
    ];

    for sig in &memory_signals {
        if lower.contains(sig) { return RequestKind::MemoryOps; }
    }
    for sig in &ingest_signals {
        if lower.contains(sig) { return RequestKind::Ingest; }
    }
    for sig in &settings_signals {
        if lower.contains(sig) { return RequestKind::Settings; }
    }
    for sig in &qa_signals {
        if lower.starts_with(sig) || lower.contains(sig) {
            return RequestKind::QAndA;
        }
    }

    // Default: treat as Q&A
    RequestKind::QAndA
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_qa() {
        assert_eq!(classify("What is Project Alpha?"), RequestKind::QAndA);
        assert_eq!(classify("How does the auth system work?"), RequestKind::QAndA);
        assert_eq!(classify("Explain the architecture"), RequestKind::QAndA);
    }

    #[test]
    fn test_classify_memory() {
        assert_eq!(classify("Remember that we use Rust for backend"), RequestKind::MemoryOps);
        assert_eq!(classify("Note that Alice owns the auth module"), RequestKind::MemoryOps);
    }

    #[test]
    fn test_classify_ingest() {
        assert_eq!(classify("Ingest my docs folder"), RequestKind::Ingest);
    }

    #[test]
    fn test_classify_settings() {
        assert_eq!(classify("Enable model for this session"), RequestKind::Settings);
    }
}
