// ── Core domain types (mirror Rust structs) ────────────────────────────────

export interface VaultStatus {
  unlocked: boolean;
  exists: boolean;
}

export interface Workspace {
  id: string;
  name: string;
  path: string;
  created_at: string;
}

export interface DocumentInfo {
  id: string;
  workspace_id: string;
  path: string;
  title: string;
  chunk_count: number;
  created_at: string;
}

export interface IngestStats {
  workspace_id: string;
  documents_added: number;
  chunks_added: number;
  skipped: number;
}

export interface Citation {
  chunk_id: string;
  doc_title: string;
  snippet: string; // ≤240 chars
}

export interface AgentAnswer {
  answer: string;
  citations: Citation[];
  is_extractive: boolean;
  proposed_memories: string[];
}

export interface RetrievedChunk {
  chunk_id: string;
  document_id: string;
  doc_title: string;
  doc_path: string;
  chunk_idx: number;
  start_char: number;
  end_char: number;
  preview: string;
  full_text?: string;
  score: number;
}

export interface Memory {
  id: string;
  workspace_id: string | null;
  memory_type: string;
  status: 'proposed' | 'saved' | 'rejected';
  content: string | null;
  plaintext_preview: string | null;
  source: string | null;
  created_at: string;
  updated_at: string;
}

export type MemoryStatus = 'proposed' | 'saved' | 'rejected';

export interface Permission {
  id: string;
  workspace_id: string | null;
  capability: string;
  scope_json: string | null;
  grant_type: 'once' | 'always';
  created_at: string;
}

export interface AuditRecord {
  id: string;
  ts: string;
  actor: string;
  action_type: string;
  capability: string | null;
  target_type: string | null;
  target_id: string | null;
  workspace_id: string | null;
  outcome: 'allowed' | 'denied';
  details_json: string | null;
}

export interface AppSettings {
  model_enabled: boolean;
  model_provider: string;
  ollama_url: string;
  ollama_model: string;
  ollama_embed_model: string;
  embeddings_enabled: boolean;
  // OpenAI
  openai_api_key: string;
  openai_model: string;
  // Anthropic
  anthropic_api_key: string;
  anthropic_model: string;
  // Telegram bot
  telegram_enabled: boolean;
  telegram_token: string;
  telegram_chat_id: string;
  // Discord bot
  discord_enabled: boolean;
  discord_token: string;
  discord_allowed_user_id: string;
  // WhatsApp bot
  whatsapp_enabled: boolean;
  whatsapp_access_token: string;
  whatsapp_phone_number_id: string;
  whatsapp_allowed_number: string;
  whatsapp_verify_token: string;
  whatsapp_webhook_port: string;
  // Desktop automation
  desktop_control_enabled: boolean;
  // Web search
  search_provider: string;
  brave_api_key: string;
  searxng_url: string;
  // Agent environment
  agent_work_dir: string;
}

// ── UI state types ─────────────────────────────────────────────────────────

// ── Agent types ────────────────────────────────────────────────────────────

export type AgentTool = 'search_documents' | 'web_search' | 'chat_llm' | 'propose_memory' | 'fetch_url' | 'browser' | 'run_python' | 'read_file' | 'write_file' | 'desktop' | 'enqueue' | 'recall' | 'remember' | 'git' | 'shell';

export interface UserAgent {
  id: string;
  name: string;
  description: string;
  system_prompt: string;
  tools: AgentTool[];
  workspace_id: string | null;
  schedule: string | null;
  last_run_at: string | null;
  created_at: string;
  updated_at: string;
  schedule_paused: boolean;
  react_mode: boolean;
}

export interface HandTemplate {
  id: string;
  name: string;
  icon: string;
  description: string;
  system_prompt: string;
  tools: string[];
  suggested_schedule: string;
}

export interface ScratchpadEntry {
  key: string;
  value: string;
  updated_at: string;
}

export interface WatchTrigger {
  id: string;
  agent_id: string;
  agent_name: string;
  watch_path: string;
  recursive: boolean;
  enabled: boolean;
  created_at: string;
}

export interface AgentRunSummary {
  id: string;
  agent_id: string;
  agent_name: string;
  status: string;
  step_count: number;
  output: string | null;
  created_at: string;
}

export interface SystemStatus {
  scheduler_running: boolean;
  watcher_running: boolean;
  bot_running: boolean;
  pending_tasks: number;
  running_tasks: number;
  active_triggers: number;
  scheduled_agents: number;
  recent_runs: AgentRunSummary[];
}

export interface TaskQueueItem {
  id: string;
  agent_id: string;
  agent_name: string;
  input: string;
  status: 'pending' | 'running' | 'done' | 'failed' | 'cancelled';
  priority: number;
  workspace_id: string | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  output: string | null;
  error: string | null;
}

export interface AgentPipeline {
  id: string;
  name: string;
  description: string;
  agent_ids: string[];
  created_at: string;
  updated_at: string;
}

export interface AgentRunStep {
  step_type: string;
  description: string;
  output: string;
}

export interface AgentRun {
  id: string;
  agent_id: string;
  pipeline_id: string | null;
  input: string;
  output: string | null;
  status: string;
  steps: AgentRunStep[];
  workspace_id: string | null;
  created_at: string;
}

export interface ChatTurn {
  role: 'user' | 'assistant' | 'system';
  content: string;
}

export interface ChatHistoryRow {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  created_at: string;
}

export interface Artifact {
  id: string;
  agent_id: string | null;
  agent_name: string;
  run_id: string | null;
  title: string;
  content: string;
  artifact_type: string;
  created_at: string;
}

export interface McpServerConfig {
  id: string;
  label: string;
  command: string;
  args: string[];
  enabled: boolean;
  created_at: string;
}

export interface McpTool {
  server_id: string;
  server_label: string;
  tool_id: string;
  name: string;
  description: string;
}

export interface PythonResult {
  stdout:      string;
  stderr:      string;
  exit_code:   number;
  duration_ms: number;
  timed_out:   boolean;
}

export interface PythonInfo {
  version:   string;
  path:      string;
  available: boolean;
}

export type Screen =
  | 'onboarding'
  | 'workspace'
  | 'qa'
  | 'chat'
  | 'agents'
  | 'memory'
  | 'permissions'
  | 'audit'
  | 'settings'
  | 'ide'
  | 'browser'
  | 'activity'
  | 'tasks';

/** All screens except onboarding — valid as a desktop window. */
export type AppScreen = Exclude<Screen, 'onboarding'>;

/** A single open window on the desktop. */
export interface WinState {
  id: string;
  screen: AppScreen;
  title: string;
  icon: string;
  x: number;
  y: number;
  width: number;
  height: number;
  minimized: boolean;
  maximized: boolean;
  zIndex: number;
  restoreRect: { x: number; y: number; w: number; h: number } | null;
}

export interface Toast {
  id: string;
  message: string;
  type: 'success' | 'error' | 'info';
}
