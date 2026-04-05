//! Typed wrappers around Tauri invoke() calls.
//! All backend communication goes through this module.

import { invoke } from '@tauri-apps/api/tauri';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import {
  VaultStatus, Workspace, DocumentInfo, IngestStats,
  AgentAnswer, RetrievedChunk, Memory, Permission,
  AuditRecord, AppSettings, ChatTurn, ChatHistoryRow,
  UserAgent, AgentPipeline, AgentRun, AgentRunStep,
  PythonResult, PythonInfo, HandTemplate,
  Artifact, McpServerConfig, TaskQueueItem, ScratchpadEntry, WatchTrigger, SystemStatus,
} from '../types';

// ── Vault ──────────────────────────────────────────────────────────────────

export const getVaultStatus = (): Promise<VaultStatus> =>
  invoke('get_vault_status');

export const createVault = (password: string): Promise<void> =>
  invoke('create_vault', { password });

export const unlockVault = (password: string): Promise<void> =>
  invoke('unlock_vault', { password });

export const lockVault = (): Promise<void> =>
  invoke('lock_vault');

// ── Settings ───────────────────────────────────────────────────────────────

export const getSettings = (): Promise<AppSettings> =>
  invoke('get_settings');

export const updateSettings = (settings: AppSettings): Promise<void> =>
  invoke('update_settings', { settings });

// ── Workspaces ─────────────────────────────────────────────────────────────

export const addWorkspace = (name: string, path: string): Promise<Workspace> =>
  invoke('add_workspace', { name, path });

export const listWorkspaces = (): Promise<Workspace[]> =>
  invoke('list_workspaces');

export const listDocuments = (workspaceId: string): Promise<DocumentInfo[]> =>
  invoke('list_documents', { workspaceId });

export const ingestWorkspace = (workspaceId: string): Promise<IngestStats> =>
  invoke('ingest_workspace', { workspaceId });

// ── Q&A ───────────────────────────────────────────────────────────────────

export const askQuestion = (question: string, workspaceId?: string): Promise<AgentAnswer> =>
  invoke('ask_question', { question, workspaceId: workspaceId ?? null });

export const getChunkSource = (chunkId: string): Promise<RetrievedChunk> =>
  invoke('get_chunk_source', { chunkId });

// ── Memories ──────────────────────────────────────────────────────────────

export const listMemories = (
  status?: string,
  workspaceId?: string,
): Promise<Memory[]> =>
  invoke('list_memories', { status: status ?? null, workspaceId: workspaceId ?? null });

export const acceptMemory = (memoryId: string): Promise<void> =>
  invoke('accept_memory', { memoryId });

export const editMemory = (memoryId: string, content: string): Promise<void> =>
  invoke('edit_memory', { memoryId, content });

export const rejectMemory = (memoryId: string): Promise<void> =>
  invoke('reject_memory', { memoryId });

export const deleteMemory = (memoryId: string): Promise<void> =>
  invoke('delete_memory', { memoryId });

export const proposeMemory = (
  content: string,
  memoryType: string,
  workspaceId?: string,
): Promise<string> =>
  invoke('propose_memory', { content, memoryType, workspaceId: workspaceId ?? null });

// ── Permissions ───────────────────────────────────────────────────────────

export const listPermissions = (): Promise<Permission[]> =>
  invoke('list_permissions');

export const revokePermission = (permissionId: string): Promise<void> =>
  invoke('revoke_permission', { permissionId });

export const checkPermission = (capability: string, workspaceId?: string): Promise<boolean> =>
  invoke('check_permission', { capability, workspaceId: workspaceId ?? null });

// ── Audit ─────────────────────────────────────────────────────────────────

export const listAuditEvents = (filters: {
  actionType?: string;
  outcome?: string;
  workspaceId?: string;
  limit?: number;
}): Promise<AuditRecord[]> =>
  // Tauri v1 does not auto-convert camelCase — use snake_case to match Rust param names
  invoke('list_audit_events', {
    action_type: filters.actionType ?? null,
    outcome: filters.outcome ?? null,
    workspace_id: filters.workspaceId ?? null,
    limit: filters.limit ?? null,
  });

// ── Chat ──────────────────────────────────────────────────────────────────

export const sendChatMessage = (message: string, history: ChatTurn[]): Promise<string> =>
  invoke('chat_message', { message, history });

export const listChatHistory = (limit?: number): Promise<ChatHistoryRow[]> =>
  invoke('list_chat_history', { limit: limit ?? null });

export const clearChatHistory = (): Promise<void> =>
  invoke('clear_chat_history');

/** Streaming chat — emits `chat-token` events while running, returns when done. */
export const chatMessageStream = (
  id: string,
  message: string,
  history: ChatTurn[],
): Promise<void> =>
  invoke('chat_message_stream', { id, message, history });

/** Subscribe to streaming chat tokens. Returns unlisten fn. */
export const listenChatToken = (
  cb: (evt: { id: string; token: string }) => void,
): Promise<UnlistenFn> =>
  listen<{ id: string; token: string }>('chat-token', (e) => cb(e.payload));

// ── Agents ────────────────────────────────────────────────────────────────

export const createAgent = (input: {
  name: string; description: string; system_prompt: string;
  tools: string[]; workspace_id?: string | null; schedule?: string | null;
  react_mode?: boolean;
}): Promise<UserAgent> =>
  invoke('create_agent', { input });

export const listAgents = (): Promise<UserAgent[]> =>
  invoke('list_agents');

export const updateAgent = (input: {
  id: string; name: string; description: string;
  system_prompt: string; tools: string[]; react_mode?: boolean;
}): Promise<void> =>
  invoke('update_agent', { input });

export const deleteAgent = (agentId: string): Promise<void> =>
  invoke('delete_agent', { agentId });

export const runAgent = (
  agentId: string, input: string, workspaceId?: string,
): Promise<AgentRun> =>
  invoke('run_agent', { agentId, input, workspaceId: workspaceId ?? null });

/** Subscribe to live step events emitted during an agent run.
 *  Returns an unlisten function — call it when the run completes. */
export const listenAgentStep = (
  cb: (step: AgentRunStep) => void,
): Promise<UnlistenFn> =>
  listen<AgentRunStep>('agent-step', (event) => cb(event.payload));

// ── Pipelines ─────────────────────────────────────────────────────────────

export const createPipeline = (input: {
  name: string; description: string; agent_ids: string[];
}): Promise<AgentPipeline> =>
  invoke('create_pipeline', { input });

export const listPipelines = (): Promise<AgentPipeline[]> =>
  invoke('list_pipelines');

export const deletePipeline = (pipelineId: string): Promise<void> =>
  invoke('delete_pipeline', { pipelineId });

export const runPipeline = (
  pipelineId: string, input: string, workspaceId?: string,
): Promise<AgentRun> =>
  invoke('run_pipeline', { pipelineId, input, workspaceId: workspaceId ?? null });

export const listAgentRuns = (agentId?: string, limit?: number): Promise<AgentRun[]> =>
  invoke('list_agent_runs', { agentId: agentId ?? null, limit: limit ?? null });

export const replayAgentRun = (runId: string): Promise<AgentRun> =>
  invoke('replay_agent_run', { runId });

// ── File output ────────────────────────────────────────────────────────────

export const generateFile = (
  content: string,
  filename: string,
  format: 'txt' | 'md' | 'pdf',
  outputDir?: string,
): Promise<string> =>
  invoke('generate_file', {
    content,
    filename,
    format,
    output_dir: outputDir ?? null,
  });

export const openFile = (path: string): Promise<void> =>
  invoke('open_file', { path });

// ── Desktop automation ─────────────────────────────────────────────────────

export const desktopType = (text: string): Promise<void> =>
  invoke('desktop_type', { text });

export const desktopKey = (combo: string): Promise<void> =>
  invoke('desktop_key', { combo });

export const desktopOpen = (target: string): Promise<void> =>
  invoke('desktop_open', { target });

// ── Telegram bot ───────────────────────────────────────────────────────────

export const startTelegramBot = (): Promise<void> =>
  invoke('start_telegram_bot');

export const stopTelegramBot = (): Promise<void> =>
  invoke('stop_telegram_bot');

export const getTelegramStatus = (): Promise<boolean> =>
  invoke('get_telegram_status');

// ── Discord bot ────────────────────────────────────────────────────────────

export const startDiscordBot = (): Promise<void> =>
  invoke('start_discord_bot');

export const stopDiscordBot = (): Promise<void> =>
  invoke('stop_discord_bot');

export const getDiscordStatus = (): Promise<boolean> =>
  invoke('get_discord_status');

// ── WhatsApp bot ───────────────────────────────────────────────────────────

export const startWhatsAppBot = (): Promise<void> =>
  invoke('start_whatsapp_bot');

export const stopWhatsAppBot = (): Promise<void> =>
  invoke('stop_whatsapp_bot');

export const getWhatsAppStatus = (): Promise<boolean> =>
  invoke('get_whatsapp_status');

// ── Python IDE ─────────────────────────────────────────────────────────────

export const getPythonInfo = (): Promise<PythonInfo> =>
  invoke('get_python_info');

export const runPython = (code: string, timeoutSecs?: number): Promise<PythonResult> =>
  invoke('run_python', { code, timeout_secs: timeoutSecs ?? null });

// ── Scheduler / Hands ──────────────────────────────────────────────────────

export const listHandTemplates = (): Promise<HandTemplate[]> =>
  invoke('list_hand_templates');

export const getScheduledAgents = (): Promise<UserAgent[]> =>
  invoke('get_scheduled_agents');

export const getSchedulerStatus = (): Promise<boolean> =>
  invoke('get_scheduler_status');

export const triggerAgentNow = (agentId: string): Promise<AgentRun> =>
  invoke('trigger_agent_now', { agentId });

export const pauseAgentSchedule = (agentId: string): Promise<void> =>
  invoke('pause_agent_schedule', { agentId });

export const resumeAgentSchedule = (agentId: string): Promise<void> =>
  invoke('resume_agent_schedule', { agentId });

// ── Utils ─────────────────────────────────────────────────────────────────

export const openFolderDialog = (): Promise<string | null> =>
  invoke('open_folder_dialog');

// ── Artifacts ─────────────────────────────────────────────────────────────

export const listArtifacts = (agentId?: string, limit?: number): Promise<Artifact[]> =>
  invoke('list_artifacts', { agent_id: agentId ?? null, limit: limit ?? null });

export const deleteArtifact = (artifactId: string): Promise<void> =>
  invoke('delete_artifact', { artifact_id: artifactId });

// ── MCP servers ───────────────────────────────────────────────────────────

export const listMcpServers = (): Promise<McpServerConfig[]> =>
  invoke('list_mcp_servers');

export const addMcpServer = (
  label: string,
  command: string,
  args: string[],
): Promise<McpServerConfig> =>
  invoke('add_mcp_server', { label, command, args });

export const removeMcpServer = (serverId: string): Promise<void> =>
  invoke('remove_mcp_server', { server_id: serverId });

export const refreshMcpTools = (serverId: string): Promise<McpServerConfig> =>
  invoke('refresh_mcp_tools', { server_id: serverId });

// ── Task queue ─────────────────────────────────────────────────────────────

export const enqueueTask = (
  agentId: string,
  input: string,
  priority?: number,
  workspaceId?: string,
): Promise<TaskQueueItem> =>
  invoke('enqueue_task', {
    agent_id:     agentId,
    input,
    priority:     priority ?? null,
    workspace_id: workspaceId ?? null,
  });

export const listTasks = (
  agentId?: string,
  status?: string,
  limit?: number,
): Promise<TaskQueueItem[]> =>
  invoke('list_tasks', {
    agent_id: agentId ?? null,
    status:   status  ?? null,
    limit:    limit   ?? null,
  });

export const cancelTask = (taskId: string): Promise<void> =>
  invoke('cancel_task', { task_id: taskId });

export const getTask = (taskId: string): Promise<TaskQueueItem> =>
  invoke('get_task', { task_id: taskId });

// ── Scratchpad ─────────────────────────────────────────────────────────────

export const listScratchpad = (agentId: string): Promise<ScratchpadEntry[]> =>
  invoke('list_scratchpad', { agent_id: agentId });

export const deleteScratchpadKey = (agentId: string, key: string): Promise<void> =>
  invoke('delete_scratchpad_key', { agent_id: agentId, key });

// ── Watch Triggers ──────────────────────────────────────────────────────────

export const listTriggers = (): Promise<WatchTrigger[]> =>
  invoke('list_triggers');

export const addTrigger = (agentId: string, watchPath: string, recursive?: boolean): Promise<WatchTrigger> =>
  invoke('add_trigger', { agent_id: agentId, watch_path: watchPath, recursive });

export const removeTrigger = (triggerId: string): Promise<void> =>
  invoke('remove_trigger', { trigger_id: triggerId });

export const toggleTrigger = (triggerId: string, enabled: boolean): Promise<void> =>
  invoke('toggle_trigger', { trigger_id: triggerId, enabled });

export const getWatcherStatus = (): Promise<boolean> =>
  invoke('get_watcher_status');

// ── Activity dashboard ──────────────────────────────────────────────────────

export const getSystemStatus = (): Promise<SystemStatus> =>
  invoke('get_system_status');
