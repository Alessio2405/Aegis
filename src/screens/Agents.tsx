import React, { useState, useEffect, useRef } from 'react';
import { useAppStore } from '../store';
import { UserAgent, AgentPipeline, AgentRun, AgentRunStep, AgentTool, HandTemplate, ScratchpadEntry, WatchTrigger } from '../types';
import * as api from '../api/tauri';

type Tab = 'agents' | 'pipelines' | 'runs' | 'scheduled' | 'triggers';

const ALL_TOOLS: { id: AgentTool; label: string; desc: string }[] = [
  { id: 'search_documents', label: 'Search Documents', desc: 'FTS search over vault before each run' },
  { id: 'web_search',       label: 'Web Search',       desc: 'Query Brave Search or SearXNG for live results (configure in Settings → Search)' },
  { id: 'fetch_url',        label: 'Fetch URLs',       desc: 'Plain HTTP fetch — fast, no JS execution' },
  { id: 'browser',          label: 'Browser',          desc: 'Headless Chrome/Edge — renders JS, works on SPAs (requires Chrome/Chromium)' },
  { id: 'run_python',       label: 'Run Python',       desc: 'Execute ```python...``` code blocks from the input (requires Python on PATH)' },
  { id: 'write_file',       label: 'Write File',       desc: 'Write text to a workspace file. First line of input = path, rest = content.' },
  { id: 'chat_llm',         label: 'Chat LLM',         desc: 'Send prompt to model (requires model enabled)' },
  { id: 'propose_memory',   label: 'Propose Memory',   desc: 'Auto-propose memory from the output' },
  { id: 'desktop',          label: 'Desktop Control',  desc: 'Type text / press keys / open files via DESKTOP: directives (requires desktop_control_enabled)' },
  { id: 'enqueue',          label: 'Enqueue Task',     desc: 'Drop a task into another agent\'s queue for async background execution' },
  { id: 'read_file',        label: 'Read File',        desc: 'Read a file\'s content (or list a directory). Path must be inside the agent working directory.' },
  { id: 'recall',           label: 'Recall',           desc: 'Read a value from this agent\'s persistent scratchpad. Key survives across scheduled runs.' },
  { id: 'remember',         label: 'Remember',         desc: 'Write a key-value pair to this agent\'s persistent scratchpad for use in future runs.' },
  { id: 'git',              label: 'Git',              desc: 'Run git commands (status, diff, log, add, commit, pull, push, clone) inside the agent working directory.' },
  { id: 'shell',            label: 'Shell',            desc: 'Run arbitrary shell commands (cmd on Windows, sh on Unix) inside the agent working directory. 60 s timeout.' },
];

export default function Agents() {
  const { workspaces, settings, addToast } = useAppStore();
  const [tab, setTab] = useState<Tab>('agents');
  const [agents, setAgents] = useState<UserAgent[]>([]);
  const [pipelines, setPipelines] = useState<AgentPipeline[]>([]);
  const [runs, setRuns] = useState<AgentRun[]>([]);
  const [loading, setLoading] = useState(false);
  const [scheduledAgents, setScheduledAgents] = useState<UserAgent[]>([]);
  const [schedulerRunning, setSchedulerRunning] = useState(false);
  const [triggers, setTriggers] = useState<WatchTrigger[]>([]);
  const [watcherRunning, setWatcherRunning] = useState(false);

  // Form state
  const [agentForm, setAgentForm] = useState<AgentFormState | null>(null);
  const [pipelineForm, setPipelineForm] = useState<PipelineFormState | null>(null);
  const [runModal, setRunModal] = useState<RunModalState | null>(null);
  const [expandedRun, setExpandedRun] = useState<string | null>(null);
  const [hands, setHands] = useState<HandTemplate[]>([]);
  const [showHands, setShowHands] = useState(false);

  useEffect(() => { refresh(); }, [tab]);
  useEffect(() => {
    api.listHandTemplates().then(setHands).catch(() => {});
  }, []);

  const refreshTriggers = async () => {
    try {
      const [ts, running] = await Promise.all([api.listTriggers(), api.getWatcherStatus()]);
      setTriggers(ts);
      setWatcherRunning(running);
    } catch (_) {}
  };

  const refreshScheduled = async () => {
    try {
      const [agents, running] = await Promise.all([
        api.getScheduledAgents(),
        api.getSchedulerStatus(),
      ]);
      setScheduledAgents(agents);
      setSchedulerRunning(running);
    } catch (_) {}
  };

  const refresh = async () => {
    try {
      if (tab === 'agents')    setAgents(await api.listAgents());
      if (tab === 'pipelines') { setAgents(await api.listAgents()); setPipelines(await api.listPipelines()); }
      if (tab === 'runs')      setRuns(await api.listAgentRuns(undefined, 100));
      if (tab === 'scheduled') refreshScheduled();
      if (tab === 'triggers')  refreshTriggers();
    } catch (e) { addToast(String(e), 'error'); }
  };

  // Poll scheduler status every 30s when on scheduled tab
  useEffect(() => {
    if (tab !== 'scheduled') return;
    refreshScheduled();
    const id = setInterval(refreshScheduled, 30_000);
    return () => clearInterval(id);
  }, [tab]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        padding: '12px 20px', borderBottom: '1px solid var(--border)',
        display: 'flex', alignItems: 'center', gap: 12, background: 'var(--bg-surface)',
      }}>
        <h1 style={{ fontSize: 15, fontWeight: 700, flex: 1 }}>🤖 Agents</h1>
        {tab === 'agents' && (
          <div style={{ display: 'flex', gap: 6 }}>
            <button className="btn btn-ghost" style={{ fontSize: 12 }}
              onClick={() => setShowHands((v) => !v)}>
              🤲 Hands {showHands ? '▲' : '▼'}
            </button>
            <button className="btn btn-primary" style={{ fontSize: 12 }}
              onClick={() => setAgentForm({ mode: 'create', name: '', description: '', system_prompt: '', tools: [], workspace_id: null, schedule: '', react_mode: false })}>
              + New Agent
            </button>
          </div>
        )}
        {tab === 'pipelines' && (
          <button className="btn btn-primary" style={{ fontSize: 12 }}
            onClick={() => setPipelineForm({ name: '', description: '', agent_ids: [] })}>
            + New Pipeline
          </button>
        )}
      </div>

      {/* Hands panel */}
      {showHands && hands.length > 0 && (
        <div style={{
          padding: '12px 20px', borderBottom: '1px solid var(--border)',
          background: 'var(--bg-elevated)', display: 'flex', flexDirection: 'column', gap: 8,
        }}>
          <div style={{ fontSize: 11, color: 'var(--text-muted)', fontWeight: 600, marginBottom: 4 }}>
            PRE-BUILT HAND TEMPLATES — click to pre-fill the create form
          </div>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            {hands.map((h) => (
              <button
                key={h.id}
                type="button"
                className="btn btn-ghost"
                style={{ fontSize: 11, display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px' }}
                onClick={() => {
                  setAgentForm({
                    mode: 'create',
                    name: h.name,
                    description: h.description,
                    system_prompt: h.system_prompt,
                    tools: h.tools as AgentTool[],
                    workspace_id: null,
                    schedule: h.suggested_schedule,
                    react_mode: false,
                  });
                  setShowHands(false);
                }}
              >
                <span>{h.icon}</span>
                <div style={{ textAlign: 'left' }}>
                  <div style={{ fontWeight: 600 }}>{h.name}</div>
                  <div style={{ fontSize: 10, color: 'var(--text-faint)' }}>{h.suggested_schedule}</div>
                </div>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Tabs */}
      <div style={{ display: 'flex', borderBottom: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
        {(['agents', 'pipelines', 'runs', 'scheduled', 'triggers'] as Tab[]).map((t) => (
          <button key={t} onClick={() => setTab(t)} style={{
            padding: '8px 18px', fontSize: 12, fontWeight: tab === t ? 700 : 400,
            borderBottom: tab === t ? '2px solid var(--accent)' : '2px solid transparent',
            color: tab === t ? 'var(--text)' : 'var(--text-muted)',
          }}>
            {t === 'scheduled' ? '⏰ Scheduled' : t === 'triggers' ? '👁 Triggers' : t.charAt(0).toUpperCase() + t.slice(1)}
          </button>
        ))}
      </div>

      {/* Body */}
      <div style={{ flex: 1, overflow: 'auto', padding: 20 }}>
        {tab === 'agents' && (
          <AgentsTab agents={agents} onEdit={(a) => setAgentForm({
            mode: 'edit', id: a.id, name: a.name, description: a.description,
            system_prompt: a.system_prompt, tools: a.tools, workspace_id: a.workspace_id,
            schedule: a.schedule ?? '', react_mode: a.react_mode,
          })} onDelete={async (id) => {
            await api.deleteAgent(id); setAgents((prev) => prev.filter((a) => a.id !== id));
            addToast('Agent deleted', 'info');
          }} onRun={(a) => setRunModal({ type: 'agent', id: a.id, name: a.name, input: '', workspace_id: null })}
          onPause={async (id) => {
            await api.pauseAgentSchedule(id); setAgents(await api.listAgents());
            addToast('Schedule paused', 'info');
          }}
          onResume={async (id) => {
            await api.resumeAgentSchedule(id); setAgents(await api.listAgents());
            addToast('Schedule resumed', 'success');
          }}
          />
        )}
        {tab === 'pipelines' && (
          <PipelinesTab pipelines={pipelines} agents={agents}
            onDelete={async (id) => {
              await api.deletePipeline(id); setPipelines((prev) => prev.filter((p) => p.id !== id));
              addToast('Pipeline deleted', 'info');
            }}
            onRun={(p) => setRunModal({ type: 'pipeline', id: p.id, name: p.name, input: '', workspace_id: null })}
          />
        )}
        {tab === 'runs' && (
          <RunsTab runs={runs} agents={agents} pipelines={pipelines}
            expandedRun={expandedRun} onToggleExpand={(id) => setExpandedRun((prev) => prev === id ? null : id)}
            onReplay={async (run) => {
              try {
                addToast(`Replaying run for "${agents.find(a => a.id === run.agent_id)?.name ?? run.agent_id}"…`, 'info');
                await api.replayAgentRun(run.id);
                setRuns(await api.listAgentRuns(undefined, 100));
                addToast('Replay complete', 'success');
              } catch (e: any) {
                addToast(`Replay failed: ${e}`, 'error');
              }
            }}
          />
        )}
        {tab === 'triggers' && (
          <TriggersTab
            triggers={triggers}
            agents={agents.length ? agents : []}
            watcherRunning={watcherRunning}
            onAdd={async (agentId, watchPath, recursive) => {
              try {
                await api.addTrigger(agentId, watchPath, recursive);
                refreshTriggers();
                addToast('Trigger added', 'success');
              } catch (e) { addToast(String(e), 'error'); }
            }}
            onRemove={async (id) => {
              try {
                await api.removeTrigger(id);
                refreshTriggers();
                addToast('Trigger removed', 'info');
              } catch (e) { addToast(String(e), 'error'); }
            }}
            onToggle={async (id, enabled) => {
              try {
                await api.toggleTrigger(id, enabled);
                refreshTriggers();
              } catch (e) { addToast(String(e), 'error'); }
            }}
          />
        )}
        {tab === 'scheduled' && (
          <ScheduledTab
            agents={scheduledAgents}
            schedulerRunning={schedulerRunning}
            onTrigger={async (id, name) => {
              try {
                const run = await api.triggerAgentNow(id);
                addToast(`"${name}" completed — ${run.steps.length} step(s)`, 'success');
                refreshScheduled();
              } catch (e) { addToast(String(e), 'error'); }
            }}
            onPause={async (id) => {
              await api.pauseAgentSchedule(id); refreshScheduled();
              addToast('Schedule paused', 'info');
            }}
            onResume={async (id) => {
              await api.resumeAgentSchedule(id); refreshScheduled();
              addToast('Schedule resumed', 'success');
            }}
          />
        )}
      </div>

      {/* Agent create/edit form modal */}
      {agentForm && (
        <AgentFormModal
          form={agentForm}
          workspaces={workspaces}
          loading={loading}
          onChange={setAgentForm}
          onCancel={() => setAgentForm(null)}
          onSubmit={async () => {
            setLoading(true);
            try {
              if (agentForm.mode === 'create') {
                const a = await api.createAgent({
                  name: agentForm.name, description: agentForm.description,
                  system_prompt: agentForm.system_prompt, tools: agentForm.tools,
                  workspace_id: agentForm.workspace_id,
                  schedule: agentForm.schedule?.trim() || null,
                  react_mode: agentForm.react_mode,
                });
                setAgents((prev) => [a, ...prev]);
                addToast(`Agent "${a.name}" created`, 'success');
              } else {
                await api.updateAgent({
                  id: agentForm.id!, name: agentForm.name, description: agentForm.description,
                  system_prompt: agentForm.system_prompt, tools: agentForm.tools,
                  react_mode: agentForm.react_mode,
                });
                setAgents(await api.listAgents());
                addToast('Agent updated', 'success');
              }
              setAgentForm(null);
            } catch (e) { addToast(String(e), 'error'); }
            finally { setLoading(false); }
          }}
        />
      )}

      {/* Pipeline create form modal */}
      {pipelineForm && (
        <PipelineFormModal
          form={pipelineForm}
          agents={agents}
          loading={loading}
          onChange={setPipelineForm}
          onCancel={() => setPipelineForm(null)}
          onSubmit={async () => {
            setLoading(true);
            try {
              const p = await api.createPipeline({
                name: pipelineForm.name, description: pipelineForm.description,
                agent_ids: pipelineForm.agent_ids,
              });
              setPipelines((prev) => [p, ...prev]);
              addToast(`Pipeline "${p.name}" created`, 'success');
              setPipelineForm(null);
            } catch (e) { addToast(String(e), 'error'); }
            finally { setLoading(false); }
          }}
        />
      )}

      {/* Run modal */}
      {runModal && (
        <RunModal
          modal={runModal}
          workspaces={workspaces}
          loading={loading}
          onChange={setRunModal}
          onCancel={() => setRunModal(null)}
          onSubmit={async () => {
            setLoading(true);
            try {
              const run = runModal.type === 'agent'
                ? await api.runAgent(runModal.id, runModal.input, runModal.workspace_id ?? undefined)
                : await api.runPipeline(runModal.id, runModal.input, runModal.workspace_id ?? undefined);
              addToast(`Run completed — ${run.steps.length} step(s)`, 'success');
              setRunModal(null);
              if (tab === 'runs') setRuns(await api.listAgentRuns(undefined, 100));
            } catch (e) { addToast(String(e), 'error'); }
            finally { setLoading(false); }
          }}
        />
      )}
    </div>
  );
}

// ── Form state types ───────────────────────────────────────────────────────

interface AgentFormState {
  mode: 'create' | 'edit';
  id?: string;
  name: string;
  description: string;
  system_prompt: string;
  tools: AgentTool[];
  workspace_id: string | null;
  schedule: string;
  react_mode: boolean;
}

interface PipelineFormState {
  name: string;
  description: string;
  agent_ids: string[];
}

interface RunModalState {
  type: 'agent' | 'pipeline';
  id: string;
  name: string;
  input: string;
  workspace_id: string | null;
}

// ── Agents tab ─────────────────────────────────────────────────────────────

function AgentsTab({ agents, onEdit, onDelete, onRun, onPause, onResume }: {
  agents: UserAgent[];
  onEdit: (a: UserAgent) => void;
  onDelete: (id: string) => void;
  onRun: (a: UserAgent) => void;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
}) {
  const [scratchpadOpen, setScratchpadOpen] = useState<string | null>(null);
  const [scratchpadData, setScratchpadData] = useState<Record<string, ScratchpadEntry[]>>({});

  const toggleScratchpad = async (agentId: string) => {
    if (scratchpadOpen === agentId) {
      setScratchpadOpen(null);
      return;
    }
    try {
      const entries = await api.listScratchpad(agentId);
      setScratchpadData((prev) => ({ ...prev, [agentId]: entries }));
      setScratchpadOpen(agentId);
    } catch {}
  };

  const deleteScratchpadKey = async (agentId: string, key: string) => {
    try {
      await api.deleteScratchpadKey(agentId, key);
      setScratchpadData((prev) => ({
        ...prev,
        [agentId]: (prev[agentId] ?? []).filter((e) => e.key !== key),
      }));
    } catch {}
  };

  if (agents.length === 0) return (
    <div className="empty-state" style={{ marginTop: 60 }}>
      <div className="icon">🤖</div>
      <div style={{ fontWeight: 600 }}>No agents yet</div>
      <div style={{ fontSize: 12, color: 'var(--text-faint)' }}>
        Create an agent with a system prompt and a set of tools.
      </div>
    </div>
  );

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      {agents.map((a) => (
        <div key={a.id} className="card" style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 600, fontSize: 14 }}>{a.name}</div>
              {a.description && (
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>{a.description}</div>
              )}
            </div>
            <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
              <button className="btn btn-primary" style={{ fontSize: 11 }} onClick={() => onRun(a)}>▶ Run</button>
              {a.schedule && (
                a.schedule_paused
                  ? <button className="btn btn-ghost" style={{ fontSize: 11, color: 'var(--success)' }}
                      onClick={() => onResume(a.id)} title="Resume schedule">▶ Resume</button>
                  : <button className="btn btn-ghost" style={{ fontSize: 11 }}
                      onClick={() => onPause(a.id)} title="Pause schedule">⏸ Pause</button>
              )}
              <button className="btn btn-ghost" style={{ fontSize: 11 }} onClick={() => onEdit(a)}>Edit</button>
              <button className="btn btn-ghost" style={{ fontSize: 11, color: 'var(--danger)' }}
                onClick={() => { if (confirm(`Delete agent "${a.name}"?`)) onDelete(a.id); }}>
                Delete
              </button>
            </div>
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, alignItems: 'center' }}>
            {a.tools.length === 0 && <span className="tag" style={{ fontSize: 10, color: 'var(--text-faint)' }}>no tools</span>}
            {a.tools.map((t) => <span key={t} className="tag" style={{ fontSize: 10 }}>{t}</span>)}
            {a.react_mode && (
              <span className="tag" style={{ fontSize: 10, background: 'rgba(16,185,129,0.15)', color: 'var(--success)' }}>
                ⚡ ReAct
              </span>
            )}
            {a.schedule && (
              <span className="tag" style={{
                fontSize: 10,
                background: a.schedule_paused ? 'rgba(148,163,184,0.15)' : 'rgba(99,102,241,0.15)',
                color: a.schedule_paused ? 'var(--text-muted)' : 'var(--accent)',
              }}>
                {a.schedule_paused ? '⏸ ' : '⏰ '}{a.schedule}
              </span>
            )}
            {a.last_run_at && (
              <span style={{ fontSize: 10, color: 'var(--text-faint)' }}>
                last run {a.last_run_at.slice(0, 16)}
              </span>
            )}
            {a.schedule && !a.last_run_at && (
              <span style={{ fontSize: 10, color: 'var(--text-faint)' }}>never run</span>
            )}
          </div>
          {a.system_prompt && (
            <div style={{ fontSize: 11, color: 'var(--text-muted)', fontStyle: 'italic', maxHeight: 40, overflow: 'hidden' }}>
              "{a.system_prompt.slice(0, 120)}{a.system_prompt.length > 120 ? '…' : ''}"
            </div>
          )}

          {/* Scratchpad toggle — only shown for agents with recall/remember tools */}
          {(a.tools.includes('recall') || a.tools.includes('remember')) && (
            <div>
              <button
                className="btn btn-ghost"
                style={{ fontSize: 11, padding: '2px 8px' }}
                onClick={() => toggleScratchpad(a.id)}
              >
                {scratchpadOpen === a.id ? '▲ Scratchpad' : '▼ Scratchpad'}
              </button>

              {scratchpadOpen === a.id && (
                <div style={{
                  marginTop: 8, padding: '8px 10px',
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border)',
                  borderRadius: 'var(--radius-sm)',
                }}>
                  {(scratchpadData[a.id] ?? []).length === 0 ? (
                    <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>Scratchpad is empty.</div>
                  ) : (
                    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                      {(scratchpadData[a.id] ?? []).map((entry) => (
                        <div key={entry.key} style={{ display: 'flex', alignItems: 'baseline', gap: 6, fontSize: 11 }}>
                          <code style={{ color: 'var(--accent)', flexShrink: 0 }}>{entry.key}</code>
                          <span style={{ color: 'var(--border)' }}>=</span>
                          <span style={{ flex: 1, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                            {entry.value}
                          </span>
                          <span style={{ fontSize: 10, color: 'var(--text-faint)', flexShrink: 0 }}>
                            {entry.updated_at.slice(0, 16)}
                          </span>
                          <button
                            className="btn btn-ghost"
                            style={{ fontSize: 10, padding: '1px 6px', color: 'var(--danger)', flexShrink: 0 }}
                            onClick={() => deleteScratchpadKey(a.id, entry.key)}
                          >
                            ✕
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

// ── Pipelines tab ──────────────────────────────────────────────────────────

function PipelinesTab({ pipelines, agents, onDelete, onRun }: {
  pipelines: AgentPipeline[];
  agents: UserAgent[];
  onDelete: (id: string) => void;
  onRun: (p: AgentPipeline) => void;
}) {
  if (pipelines.length === 0) return (
    <div className="empty-state" style={{ marginTop: 60 }}>
      <div className="icon">⛓</div>
      <div style={{ fontWeight: 600 }}>No pipelines yet</div>
      <div style={{ fontSize: 12, color: 'var(--text-faint)' }}>
        Chain agents together — output of each step feeds into the next.
      </div>
    </div>
  );

  const agentName = (id: string) => agents.find((a) => a.id === id)?.name ?? id.slice(0, 8);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      {pipelines.map((p) => (
        <div key={p.id} className="card" style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 600, fontSize: 14 }}>{p.name}</div>
              {p.description && (
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>{p.description}</div>
              )}
            </div>
            <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
              <button className="btn btn-primary" style={{ fontSize: 11 }} onClick={() => onRun(p)}>▶ Run</button>
              <button className="btn btn-ghost" style={{ fontSize: 11, color: 'var(--danger)' }}
                onClick={() => { if (confirm(`Delete pipeline "${p.name}"?`)) onDelete(p.id); }}>
                Delete
              </button>
            </div>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
            {p.agent_ids.map((id, i) => (
              <React.Fragment key={id}>
                <span className="tag" style={{ fontSize: 10 }}>{agentName(id)}</span>
                {i < p.agent_ids.length - 1 && <span style={{ fontSize: 10, color: 'var(--text-faint)' }}>→</span>}
              </React.Fragment>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Runs tab ───────────────────────────────────────────────────────────────

function RunsTab({ runs, agents, pipelines, expandedRun, onToggleExpand, onReplay }: {
  runs: AgentRun[];
  agents: UserAgent[];
  pipelines: AgentPipeline[];
  expandedRun: string | null;
  onToggleExpand: (id: string) => void;
  onReplay: (run: AgentRun) => void;
}) {
  if (runs.length === 0) return (
    <div className="empty-state" style={{ marginTop: 60 }}>
      <div className="icon">📜</div>
      <div style={{ fontWeight: 600 }}>No runs yet</div>
      <div style={{ fontSize: 12, color: 'var(--text-faint)' }}>Run an agent or pipeline to see results here.</div>
    </div>
  );

  const agentName = (id: string) => agents.find((a) => a.id === id)?.name ?? id.slice(0, 8);
  const pipelineName = (id: string | null) => id ? (pipelines.find((p) => p.id === id)?.name ?? id.slice(0, 8)) : null;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {runs.map((run) => (
        <div key={run.id} className="card" style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}
            onClick={() => onToggleExpand(run.id)}>
            <span style={{ fontSize: 13 }}>{expandedRun === run.id ? '▾' : '▸'}</span>
            <div style={{ flex: 1 }}>
              <span style={{ fontSize: 12, fontWeight: 600 }}>
                {run.pipeline_id ? `Pipeline: ${pipelineName(run.pipeline_id)}` : `Agent: ${agentName(run.agent_id)}`}
              </span>
              <span style={{ fontSize: 11, color: 'var(--text-muted)', marginLeft: 8 }}>
                {run.input.slice(0, 80)}{run.input.length > 80 ? '…' : ''}
              </span>
            </div>
            <span style={{ fontSize: 10, color: 'var(--text-faint)' }}>{run.created_at.slice(0, 16)}</span>
            <span className="tag" style={{ fontSize: 10 }}>{run.steps.length} steps</span>
            {!run.pipeline_id && (
              <button
                className="btn-ghost"
                style={{ fontSize: 10, padding: '2px 6px' }}
                title="Re-run with same agent and input"
                onClick={(e) => { e.stopPropagation(); onReplay(run); }}
              >↺ Replay</button>
            )}
          </div>

          {expandedRun === run.id && (
            <div style={{ borderTop: '1px solid var(--border)', paddingTop: 8, display: 'flex', flexDirection: 'column', gap: 6 }}>
              <div style={{ fontSize: 11 }}>
                <span style={{ color: 'var(--text-muted)' }}>Input: </span>
                <span>{run.input}</span>
              </div>
              {run.output && (
                <div style={{ fontSize: 11, background: 'var(--bg-elevated)', padding: '8px 10px', borderRadius: 'var(--radius)', whiteSpace: 'pre-wrap' }}>
                  <div style={{ color: 'var(--text-muted)', marginBottom: 4, fontSize: 10 }}>Output</div>
                  {run.output}
                </div>
              )}
              {run.steps.map((step, i) => {
                const isDelegate = step.step_type === 'delegate';
                return (
                  <div key={i} style={{
                    fontSize: 11,
                    paddingLeft: isDelegate ? 16 : 12,
                    borderLeft: `2px solid ${isDelegate ? 'var(--accent)' : 'var(--border)'}`,
                    background: isDelegate ? 'var(--bg-elevated)' : undefined,
                    borderRadius: isDelegate ? 'var(--radius)' : undefined,
                    padding: isDelegate ? '4px 8px 4px 12px' : undefined,
                  }}>
                    <div style={{ color: isDelegate ? 'var(--accent)' : 'var(--text-muted)', fontWeight: isDelegate ? 700 : 600 }}>
                      {isDelegate ? '⇢ ' : ''}{step.step_type}: {step.description}
                    </div>
                    {step.output && (
                      <div style={{ color: 'var(--text-muted)', marginTop: 2, whiteSpace: 'pre-wrap', maxHeight: 100, overflow: 'auto' }}>
                        {step.output}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

// ── Scheduled tab ──────────────────────────────────────────────────────────

function ScheduledTab({ agents, schedulerRunning, onTrigger, onPause, onResume }: {
  agents: UserAgent[];
  schedulerRunning: boolean;
  onTrigger: (id: string, name: string) => void;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
}) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* Scheduler status banner */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '10px 14px', borderRadius: 'var(--radius-sm)',
        background: schedulerRunning ? 'rgba(74,222,128,0.08)' : 'rgba(148,163,184,0.08)',
        border: `1px solid ${schedulerRunning ? 'rgba(74,222,128,0.3)' : 'var(--border)'}`,
        fontSize: 13,
      }}>
        <span style={{ color: schedulerRunning ? 'var(--success)' : 'var(--text-muted)' }}>
          {schedulerRunning ? '● Scheduler running' : '○ Scheduler stopped'}
        </span>
        <span style={{ flex: 1, fontSize: 11, color: 'var(--text-faint)' }}>
          {schedulerRunning ? 'Wakes every 60 s to check for due agents.' : 'Scheduler is not running.'}
        </span>
      </div>

      {agents.length === 0 ? (
        <div className="empty-state" style={{ marginTop: 40 }}>
          <div className="icon">⏰</div>
          <div style={{ fontWeight: 600 }}>No scheduled agents</div>
          <div style={{ fontSize: 12, color: 'var(--text-faint)' }}>
            Set a schedule on an agent (e.g. "every 24h") to have it run automatically.
          </div>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {agents.map((a) => (
            <div key={a.id} className="card" style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                <div style={{ flex: 1 }}>
                  <div style={{ fontWeight: 600, fontSize: 14 }}>{a.name}</div>
                  {a.description && (
                    <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>{a.description}</div>
                  )}
                </div>
                <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
                  <button className="btn btn-primary" style={{ fontSize: 11 }}
                    onClick={() => onTrigger(a.id, a.name)}>
                    ▶ Run Now
                  </button>
                  {a.schedule_paused
                    ? <button className="btn btn-ghost" style={{ fontSize: 11, color: 'var(--success)' }}
                        onClick={() => onResume(a.id)}>▶ Resume</button>
                    : <button className="btn btn-ghost" style={{ fontSize: 11 }}
                        onClick={() => onPause(a.id)}>⏸ Pause</button>
                  }
                </div>
              </div>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, fontSize: 11 }}>
                <span className="tag" style={{
                  fontSize: 10,
                  background: a.schedule_paused ? 'rgba(148,163,184,0.15)' : 'rgba(99,102,241,0.15)',
                  color: a.schedule_paused ? 'var(--text-muted)' : 'var(--accent)',
                }}>
                  {a.schedule_paused ? '⏸ paused' : `⏰ ${a.schedule}`}
                </span>
                <span style={{ color: 'var(--text-faint)' }}>
                  {a.last_run_at
                    ? `last run ${a.last_run_at.slice(0, 16)}`
                    : 'never run'}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Triggers tab ───────────────────────────────────────────────────────────

function TriggersTab({ triggers, agents, watcherRunning, onAdd, onRemove, onToggle }: {
  triggers: WatchTrigger[];
  agents: UserAgent[];
  watcherRunning: boolean;
  onAdd: (agentId: string, watchPath: string, recursive: boolean) => void;
  onRemove: (id: string) => void;
  onToggle: (id: string, enabled: boolean) => void;
}) {
  const [agentId, setAgentId] = useState('');
  const [watchPath, setWatchPath] = useState('');
  const [recursive, setRecursive] = useState(true);

  const handleAdd = () => {
    if (!agentId || !watchPath.trim()) return;
    onAdd(agentId, watchPath.trim(), recursive);
    setWatchPath('');
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* Watcher status */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '10px 14px', borderRadius: 'var(--radius-sm)',
        background: watcherRunning ? 'rgba(74,222,128,0.08)' : 'rgba(148,163,184,0.08)',
        border: `1px solid ${watcherRunning ? 'rgba(74,222,128,0.3)' : 'var(--border)'}`,
        fontSize: 13,
      }}>
        <span style={{ color: watcherRunning ? 'var(--success)' : 'var(--text-muted)' }}>
          {watcherRunning ? '● Watcher running' : '○ Watcher stopped'}
        </span>
        <span style={{ flex: 1, fontSize: 11, color: 'var(--text-faint)' }}>
          Fires agents automatically when files change in watched directories.
        </span>
      </div>

      {/* Add trigger form */}
      <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
        <div style={{ fontWeight: 600, fontSize: 13 }}>Add Watch Trigger</div>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          <select
            className="form-input"
            value={agentId}
            onChange={(e) => setAgentId(e.target.value)}
            style={{ flex: '1 1 160px', minWidth: 0 }}
          >
            <option value="">— Select agent —</option>
            {agents.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
          </select>
          <input
            className="form-input"
            placeholder="Watch path (folder or file)"
            value={watchPath}
            onChange={(e) => setWatchPath(e.target.value)}
            style={{ flex: '2 1 220px', minWidth: 0 }}
          />
          <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, whiteSpace: 'nowrap' }}>
            <input type="checkbox" checked={recursive} onChange={(e) => setRecursive(e.target.checked)} />
            Recursive
          </label>
          <button
            className="btn btn-primary"
            style={{ fontSize: 12, whiteSpace: 'nowrap' }}
            disabled={!agentId || !watchPath.trim()}
            onClick={handleAdd}
          >
            + Add
          </button>
        </div>
        <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
          When any file inside the watch path changes the agent receives: <code>File changed: &lt;path&gt;</code>
        </div>
      </div>

      {/* Trigger list */}
      {triggers.length === 0 ? (
        <div className="empty-state" style={{ marginTop: 20 }}>
          <div className="icon">👁</div>
          <div style={{ fontWeight: 600 }}>No watch triggers</div>
          <div style={{ fontSize: 12, color: 'var(--text-faint)' }}>
            Add a trigger above to fire an agent when files change.
          </div>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {triggers.map((t) => (
            <div key={t.id} className="card" style={{
              display: 'flex', alignItems: 'center', gap: 10,
              opacity: t.enabled ? 1 : 0.55,
            }}>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontWeight: 600, fontSize: 13 }}>{t.agent_name}</div>
                <div style={{
                  fontSize: 11, color: 'var(--text-muted)', marginTop: 2,
                  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                }}>
                  {t.recursive ? '📁' : '📄'} {t.watch_path}
                </div>
              </div>
              <div style={{ display: 'flex', gap: 6, flexShrink: 0, alignItems: 'center' }}>
                <span className="tag" style={{ fontSize: 10 }}>{t.recursive ? 'recursive' : 'top-level'}</span>
                <button
                  className="btn btn-ghost"
                  style={{ fontSize: 11, color: t.enabled ? 'var(--text-muted)' : 'var(--success)' }}
                  onClick={() => onToggle(t.id, !t.enabled)}
                >
                  {t.enabled ? '⏸ Pause' : '▶ Enable'}
                </button>
                <button
                  className="btn btn-ghost"
                  style={{ fontSize: 11, color: 'var(--error)' }}
                  onClick={() => onRemove(t.id)}
                >
                  Delete
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Agent form modal ───────────────────────────────────────────────────────

function AgentFormModal({ form, workspaces, loading, onChange, onCancel, onSubmit }: {
  form: AgentFormState;
  workspaces: import('../types').Workspace[];
  loading: boolean;
  onChange: (f: AgentFormState) => void;
  onCancel: () => void;
  onSubmit: () => void;
}) {
  const toggleTool = (t: AgentTool) =>
    onChange({ ...form, tools: form.tools.includes(t) ? form.tools.filter((x) => x !== t) : [...form.tools, t] });

  return (
    <Modal title={form.mode === 'create' ? 'New Agent' : 'Edit Agent'} onClose={onCancel}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <label style={{ fontSize: 12 }}>
          <div style={{ marginBottom: 4, fontWeight: 600 }}>Name *</div>
          <input className="form-input" value={form.name} onChange={(e) => onChange({ ...form, name: e.target.value })} placeholder="e.g. Research Assistant" style={{ width: '100%' }} />
        </label>
        <label style={{ fontSize: 12 }}>
          <div style={{ marginBottom: 4, fontWeight: 600 }}>Description</div>
          <input className="form-input" value={form.description} onChange={(e) => onChange({ ...form, description: e.target.value })} placeholder="What this agent does…" style={{ width: '100%' }} />
        </label>
        <label style={{ fontSize: 12 }}>
          <div style={{ marginBottom: 4, fontWeight: 600 }}>System Prompt</div>
          <textarea className="form-input" value={form.system_prompt} rows={4}
            onChange={(e) => onChange({ ...form, system_prompt: e.target.value })}
            placeholder="You are a helpful assistant. Answer concisely using the provided context."
            style={{ width: '100%', resize: 'vertical' }} />
        </label>
        <div style={{ fontSize: 12 }}>
          <div style={{ marginBottom: 6, fontWeight: 600 }}>Tools</div>
          {ALL_TOOLS.map((t) => (
            <label key={t.id} style={{ display: 'flex', alignItems: 'flex-start', gap: 8, marginBottom: 8, cursor: 'pointer' }}>
              <input type="checkbox" checked={form.tools.includes(t.id)} onChange={() => toggleTool(t.id)} style={{ marginTop: 2 }} />
              <div>
                <div style={{ fontWeight: 500 }}>{t.label}</div>
                <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>{t.desc}</div>
              </div>
            </label>
          ))}
        </div>
        {workspaces.length > 0 && (
          <label style={{ fontSize: 12 }}>
            <div style={{ marginBottom: 4, fontWeight: 600 }}>Scope to workspace (optional)</div>
            <select className="form-input" value={form.workspace_id ?? ''} style={{ width: '100%' }}
              onChange={(e) => onChange({ ...form, workspace_id: e.target.value || null })}>
              <option value="">All workspaces</option>
              {workspaces.map((ws) => <option key={ws.id} value={ws.id}>{ws.name}</option>)}
            </select>
          </label>
        )}
        <label style={{ fontSize: 12 }}>
          <div style={{ marginBottom: 4, fontWeight: 600 }}>Schedule (optional)</div>
          <input
            className="form-input"
            value={form.schedule}
            onChange={(e) => onChange({ ...form, schedule: e.target.value })}
            placeholder="e.g. every 1h, every 24h, every 30m"
            style={{ width: '100%' }}
          />
          <div style={{ fontSize: 11, color: 'var(--text-faint)', marginTop: 3 }}>
            Leave empty for on-demand only. Scheduler runs every 60 s.
          </div>
        </label>
        <label style={{ fontSize: 12, display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
          <input type="checkbox" checked={form.react_mode} onChange={(e) => onChange({ ...form, react_mode: e.target.checked })} />
          <div>
            <div style={{ fontWeight: 600 }}>ReAct mode (Reasoning + Acting)</div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
              LLM drives tool selection iteratively (Thought → Action → Observe) instead of the static scale-order pipeline. Requires a model.
            </div>
          </div>
        </label>
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 4 }}>
          <button className="btn btn-ghost" onClick={onCancel}>Cancel</button>
          <button className="btn btn-primary" disabled={!form.name.trim() || loading} onClick={onSubmit}>
            {loading ? '…' : form.mode === 'create' ? 'Create Agent' : 'Save Changes'}
          </button>
        </div>
      </div>
    </Modal>
  );
}

// ── Pipeline form modal ────────────────────────────────────────────────────

function PipelineFormModal({ form, agents, loading, onChange, onCancel, onSubmit }: {
  form: PipelineFormState;
  agents: UserAgent[];
  loading: boolean;
  onChange: (f: PipelineFormState) => void;
  onCancel: () => void;
  onSubmit: () => void;
}) {
  const addAgent = (id: string) => {
    if (id && !form.agent_ids.includes(id)) onChange({ ...form, agent_ids: [...form.agent_ids, id] });
  };
  const removeAgent = (id: string) => onChange({ ...form, agent_ids: form.agent_ids.filter((a) => a !== id) });
  const moveUp = (i: number) => {
    if (i === 0) return;
    const ids = [...form.agent_ids];
    [ids[i - 1], ids[i]] = [ids[i], ids[i - 1]];
    onChange({ ...form, agent_ids: ids });
  };
  const moveDown = (i: number) => {
    if (i === form.agent_ids.length - 1) return;
    const ids = [...form.agent_ids];
    [ids[i], ids[i + 1]] = [ids[i + 1], ids[i]];
    onChange({ ...form, agent_ids: ids });
  };
  const agentName = (id: string) => agents.find((a) => a.id === id)?.name ?? id;

  return (
    <Modal title="New Pipeline" onClose={onCancel}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <label style={{ fontSize: 12 }}>
          <div style={{ marginBottom: 4, fontWeight: 600 }}>Name *</div>
          <input className="form-input" value={form.name} onChange={(e) => onChange({ ...form, name: e.target.value })} placeholder="e.g. Research + Summarize" style={{ width: '100%' }} />
        </label>
        <label style={{ fontSize: 12 }}>
          <div style={{ marginBottom: 4, fontWeight: 600 }}>Description</div>
          <input className="form-input" value={form.description} onChange={(e) => onChange({ ...form, description: e.target.value })} placeholder="What this pipeline does…" style={{ width: '100%' }} />
        </label>
        <div style={{ fontSize: 12 }}>
          <div style={{ marginBottom: 6, fontWeight: 600 }}>Agent Steps (in order)</div>
          {form.agent_ids.length === 0 && (
            <div style={{ fontSize: 11, color: 'var(--text-faint)', marginBottom: 8 }}>No agents added yet.</div>
          )}
          {form.agent_ids.map((id, i) => (
            <div key={id} style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
              <span style={{ fontSize: 11, color: 'var(--text-muted)', width: 20, textAlign: 'right' }}>{i + 1}.</span>
              <span style={{ flex: 1, fontSize: 12 }}>{agentName(id)}</span>
              <button className="btn btn-ghost" style={{ fontSize: 10, padding: '2px 6px' }} onClick={() => moveUp(i)} disabled={i === 0}>↑</button>
              <button className="btn btn-ghost" style={{ fontSize: 10, padding: '2px 6px' }} onClick={() => moveDown(i)} disabled={i === form.agent_ids.length - 1}>↓</button>
              <button className="btn btn-ghost" style={{ fontSize: 10, padding: '2px 6px', color: 'var(--danger)' }} onClick={() => removeAgent(id)}>✕</button>
            </div>
          ))}
          <select className="form-input" defaultValue="" style={{ width: '100%', marginTop: 4, fontSize: 12 }}
            onChange={(e) => { addAgent(e.target.value); e.target.value = ''; }}>
            <option value="" disabled>+ Add agent…</option>
            {agents.filter((a) => !form.agent_ids.includes(a.id)).map((a) => (
              <option key={a.id} value={a.id}>{a.name}</option>
            ))}
          </select>
        </div>
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 4 }}>
          <button className="btn btn-ghost" onClick={onCancel}>Cancel</button>
          <button className="btn btn-primary" disabled={!form.name.trim() || form.agent_ids.length === 0 || loading} onClick={onSubmit}>
            {loading ? '…' : 'Create Pipeline'}
          </button>
        </div>
      </div>
    </Modal>
  );
}

// ── Run modal ──────────────────────────────────────────────────────────────

function RunModal({ modal, workspaces, loading, onChange, onCancel, onSubmit }: {
  modal: RunModalState;
  workspaces: import('../types').Workspace[];
  loading: boolean;
  onChange: (m: RunModalState) => void;
  onCancel: () => void;
  onSubmit: () => void;
}) {
  const [liveSteps, setLiveSteps] = useState<AgentRunStep[]>([]);
  const stepsEndRef = useRef<HTMLDivElement>(null);

  // Subscribe to live step events while the run is in progress
  useEffect(() => {
    if (!loading) {
      setLiveSteps([]);
      return;
    }
    let unlisten: (() => void) | null = null;
    api.listenAgentStep((step) => {
      setLiveSteps((prev) => [...prev, step]);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [loading]);

  // Auto-scroll to latest step
  useEffect(() => {
    stepsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [liveSteps]);

  const stepTypeLabel: Record<string, string> = {
    react_think:   'Think',
    react_observe: 'Observe',
    search:        'Search',
    fetch:         'Fetch',
    llm:           'LLM',
    propose_memory:'Memory',
    pipeline_step: 'Pipeline',
    web_search:    'Web Search',
    write_file:    'Write File',
    run_python:    'Python',
    desktop:       'Desktop',
  };

  return (
    <Modal title={`Run: ${modal.name}`} onClose={onCancel}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        {!loading && liveSteps.length === 0 && (
          <>
            <label style={{ fontSize: 12 }}>
              <div style={{ marginBottom: 4, fontWeight: 600 }}>Input *</div>
              <textarea className="form-input" value={modal.input} rows={4} autoFocus
                onChange={(e) => onChange({ ...modal, input: e.target.value })}
                placeholder="What should the agent work on?"
                style={{ width: '100%', resize: 'vertical' }} />
            </label>
            {workspaces.length > 0 && (
              <label style={{ fontSize: 12 }}>
                <div style={{ marginBottom: 4, fontWeight: 600 }}>Workspace scope (optional)</div>
                <select className="form-input" value={modal.workspace_id ?? ''} style={{ width: '100%' }}
                  onChange={(e) => onChange({ ...modal, workspace_id: e.target.value || null })}>
                  <option value="">All workspaces</option>
                  {workspaces.map((ws) => <option key={ws.id} value={ws.id}>{ws.name}</option>)}
                </select>
              </label>
            )}
          </>
        )}

        {loading && (
          <div style={{
            background: 'var(--bg-raised)', borderRadius: 'var(--radius)',
            border: '1px solid var(--border)', padding: '8px 0',
            maxHeight: 300, overflowY: 'auto', fontSize: 12,
          }}>
            {liveSteps.length === 0 && (
              <div style={{ padding: '8px 12px', color: 'var(--text-muted)', display: 'flex', gap: 8, alignItems: 'center' }}>
                <span className="spinner" /> Starting…
              </div>
            )}
            {liveSteps.map((step, i) => (
              <div key={i} style={{
                padding: '6px 12px',
                borderBottom: i < liveSteps.length - 1 ? '1px solid var(--border)' : undefined,
              }}>
                <div style={{ display: 'flex', gap: 8, alignItems: 'baseline' }}>
                  <span style={{
                    fontSize: 10, fontWeight: 700, textTransform: 'uppercase',
                    color: step.step_type.startsWith('react_think') ? 'var(--accent)' :
                           step.step_type === 'react_observe' ? 'var(--text-muted)' : 'var(--text)',
                    minWidth: 64,
                  }}>
                    {stepTypeLabel[step.step_type] ?? step.step_type}
                  </span>
                  <span style={{ color: 'var(--text)', flex: 1 }}>{step.description}</span>
                </div>
                {step.output && (
                  <div style={{
                    marginTop: 3, marginLeft: 72, color: 'var(--text-muted)',
                    whiteSpace: 'pre-wrap', wordBreak: 'break-word',
                    maxHeight: 80, overflowY: 'auto',
                  }}>
                    {step.output}
                  </div>
                )}
              </div>
            ))}
            <div ref={stepsEndRef} />
          </div>
        )}

        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 4 }}>
          <button className="btn btn-ghost" onClick={onCancel} disabled={loading}>Cancel</button>
          <button className="btn btn-primary" disabled={!modal.input.trim() || loading} onClick={onSubmit}>
            {loading
              ? <span style={{ display: 'flex', gap: 6, alignItems: 'center' }}><span className="spinner" /> Running…</span>
              : '▶ Run'}
          </button>
        </div>
      </div>
    </Modal>
  );
}

// ── Shared modal wrapper ───────────────────────────────────────────────────

function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: React.ReactNode }) {
  return (
    <div style={{
      position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.6)',
      display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000,
    }} onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div style={{
        background: 'var(--bg-surface)', border: '1px solid var(--border)',
        borderRadius: 'var(--radius)', padding: 24, width: 480, maxWidth: '90vw',
        maxHeight: '85vh', overflow: 'auto',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', marginBottom: 16 }}>
          <h2 style={{ fontSize: 14, fontWeight: 700, flex: 1 }}>{title}</h2>
          <button className="btn btn-ghost" style={{ fontSize: 14, padding: '2px 8px' }} onClick={onClose}>✕</button>
        </div>
        {children}
      </div>
    </div>
  );
}
