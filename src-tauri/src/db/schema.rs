//! SQLite schema — all DDL lives here so it stays in one place.

pub const SCHEMA_SQL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- ── Workspaces ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS workspaces (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    path        TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL
);

-- ── Documents ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS documents (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    path          TEXT NOT NULL,
    title         TEXT NOT NULL,
    mtime         INTEGER,
    hash          TEXT,
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_documents_workspace ON documents(workspace_id);
CREATE INDEX IF NOT EXISTS idx_documents_path ON documents(path);

-- ── Chunks ─────────────────────────────────────────────────────────────────
-- ciphertext_text: encrypted full chunk text (XChaCha20-Poly1305)
-- plaintext_preview: first 240 chars, stored plaintext for FTS indexing only
CREATE TABLE IF NOT EXISTS chunks (
    id                TEXT PRIMARY KEY,
    document_id       TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    idx               INTEGER NOT NULL,
    start_char        INTEGER NOT NULL,
    end_char          INTEGER NOT NULL,
    ciphertext_text   BLOB,
    plaintext_preview TEXT,
    created_at        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chunks_document ON chunks(document_id);

-- ── FTS5 virtual table ─────────────────────────────────────────────────────
-- Indexes plaintext_preview (short excerpt) + document title.
-- Stores chunk rowid for fast lookup back to chunks.
CREATE VIRTUAL TABLE IF NOT EXISTS chunk_fts USING fts5(
    title,
    plaintext_preview,
    content='',
    tokenize='porter unicode61'
);

-- ── Embeddings (optional) ──────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS embeddings (
    chunk_id    TEXT PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    dim         INTEGER NOT NULL,
    vector_blob BLOB NOT NULL
);

-- ── Memories ───────────────────────────────────────────────────────────────
-- status: proposed | saved | rejected
-- type:   Profile | Project | Procedure | Episodic
CREATE TABLE IF NOT EXISTS memories (
    id                  TEXT PRIMARY KEY,
    workspace_id        TEXT,
    type                TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'proposed',
    content_ciphertext  BLOB,
    plaintext_preview   TEXT,
    source              TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);

-- ── Permissions ────────────────────────────────────────────────────────────
-- grant_type: once | always
-- capability: vault.read | vault.write | file.read | model.use
CREATE TABLE IF NOT EXISTS permissions (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT,
    capability    TEXT NOT NULL,
    scope_json    TEXT,
    grant_type    TEXT NOT NULL,
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_permissions_cap ON permissions(capability);

-- ── Audit events ───────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS audit_events (
    id            TEXT PRIMARY KEY,
    ts            TEXT NOT NULL,
    actor         TEXT NOT NULL,
    action_type   TEXT NOT NULL,
    capability    TEXT,
    target_type   TEXT,
    target_id     TEXT,
    workspace_id  TEXT,
    outcome       TEXT NOT NULL,
    details_json  TEXT
);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_events(ts DESC);
CREATE INDEX IF NOT EXISTS idx_audit_workspace ON audit_events(workspace_id);
CREATE INDEX IF NOT EXISTS idx_audit_outcome ON audit_events(outcome);

-- ── Settings ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- Default settings
INSERT OR IGNORE INTO settings(key, value) VALUES
    ('model_enabled', 'false'),
    ('model_provider', 'ollama'),
    ('ollama_url', 'http://localhost:11434'),
    ('ollama_model', 'llama3'),
    ('ollama_embed_model', 'nomic-embed-text'),
    ('embeddings_enabled', 'false'),
    ('telegram_enabled', 'false'),
    ('telegram_token', ''),
    ('telegram_chat_id', ''),
    ('desktop_control_enabled', 'false'),
    ('openai_api_key', ''),
    ('openai_model', 'gpt-4o-mini'),
    ('search_provider', 'brave'),
    ('brave_api_key', ''),
    ('searxng_url', 'http://localhost:8888'),
    ('agent_work_dir', '');

-- ── User-defined Agents ─────────────────────────────────────────────────────
-- tools_json: JSON array of tool names the agent is allowed to invoke
--   valid values: "search_documents" | "chat_llm" | "propose_memory"
-- schedule: optional interval string e.g. "every 1h", "every 24h" (NULL = on-demand only)
-- last_run_at: ISO8601 timestamp of last scheduled execution (NULL = never)
-- schedule_paused: 1 = scheduler skips this agent even if due
CREATE TABLE IF NOT EXISTS user_agents (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    description      TEXT NOT NULL DEFAULT '',
    system_prompt    TEXT NOT NULL DEFAULT '',
    tools_json       TEXT NOT NULL DEFAULT '[]',
    workspace_id     TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
    schedule         TEXT,
    last_run_at      TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    schedule_paused  INTEGER NOT NULL DEFAULT 0
);

-- ── Agent Pipelines ──────────────────────────────────────────────────────────
-- agent_ids_json: ordered JSON array of user_agent IDs; output of step N → input of step N+1
CREATE TABLE IF NOT EXISTS agent_pipelines (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    agent_ids_json TEXT NOT NULL DEFAULT '[]',
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

-- ── Agent Runs ───────────────────────────────────────────────────────────────
-- steps_json: array of {step_type, description, output} records for the full trace
CREATE TABLE IF NOT EXISTS agent_runs (
    id           TEXT PRIMARY KEY,
    agent_id     TEXT NOT NULL,
    pipeline_id  TEXT,
    input        TEXT NOT NULL,
    output       TEXT,
    status       TEXT NOT NULL DEFAULT 'completed',
    steps_json   TEXT NOT NULL DEFAULT '[]',
    workspace_id TEXT,
    created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_agent_runs_agent   ON agent_runs(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_runs_created ON agent_runs(created_at DESC);

-- ── Chat History ─────────────────────────────────────────────────────────────
-- Persists chat turns across sessions; content stored plaintext (vault access required to read)
CREATE TABLE IF NOT EXISTS chat_history (
    id          TEXT PRIMARY KEY,
    role        TEXT NOT NULL,      -- 'user' | 'assistant'
    content     TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chat_history_created ON chat_history(created_at DESC);

-- ── Artifacts ─────────────────────────────────────────────────────────────────
-- Persistent workspace: findings/summaries agents produce over time.
CREATE TABLE IF NOT EXISTS artifacts (
    id            TEXT PRIMARY KEY,
    agent_id      TEXT,
    agent_name    TEXT NOT NULL DEFAULT '',
    run_id        TEXT,
    title         TEXT NOT NULL,
    content       TEXT NOT NULL,
    artifact_type TEXT NOT NULL DEFAULT 'finding',
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artifacts_created ON artifacts(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_artifacts_agent   ON artifacts(agent_id);

-- ── MCP Servers ───────────────────────────────────────────────────────────────
-- MCP (Model Context Protocol) tool servers. tools_json caches the tool list.
CREATE TABLE IF NOT EXISTS mcp_servers (
    id         TEXT PRIMARY KEY,
    label      TEXT NOT NULL,
    command    TEXT NOT NULL,
    args_json  TEXT NOT NULL DEFAULT '[]',
    tools_json TEXT NOT NULL DEFAULT '[]',
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

-- ── Task Queue ────────────────────────────────────────────────────────────────
-- Human- or agent-submitted tasks the scheduler picks up and executes.
-- status: pending | running | done | failed | cancelled
-- priority: higher numbers are picked first (default 0)
CREATE TABLE IF NOT EXISTS task_queue (
    id           TEXT PRIMARY KEY,
    agent_id     TEXT NOT NULL,
    input        TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',
    priority     INTEGER NOT NULL DEFAULT 0,
    workspace_id TEXT,
    created_at   TEXT NOT NULL,
    started_at   TEXT,
    completed_at TEXT,
    output       TEXT,
    error        TEXT
);
CREATE INDEX IF NOT EXISTS idx_task_queue_status   ON task_queue(status);
CREATE INDEX IF NOT EXISTS idx_task_queue_agent    ON task_queue(agent_id);
CREATE INDEX IF NOT EXISTS idx_task_queue_priority ON task_queue(priority DESC, created_at ASC);

-- ── Agent Scratchpad ──────────────────────────────────────────────────────────
-- Per-agent persistent key-value store. Agents use recall/remember tools to
-- read/write state that persists across runs without going through the memory inbox.
CREATE TABLE IF NOT EXISTS agent_scratchpad (
    agent_id   TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (agent_id, key)
);
CREATE INDEX IF NOT EXISTS idx_scratchpad_agent ON agent_scratchpad(agent_id);

-- ── Watch Triggers ────────────────────────────────────────────────────────────
-- Fire an agent when files change inside a watched directory.
-- enabled: 0 = paused, 1 = active
-- recursive: 1 = watch all subdirectories
CREATE TABLE IF NOT EXISTS watch_triggers (
    id          TEXT PRIMARY KEY,
    agent_id    TEXT NOT NULL,
    watch_path  TEXT NOT NULL,
    recursive   INTEGER NOT NULL DEFAULT 1,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_watch_triggers_enabled ON watch_triggers(enabled);
"#;
