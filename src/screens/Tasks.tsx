import React, { useEffect, useState, useCallback } from 'react';
import { useAppStore } from '../store';
import { TaskQueueItem, UserAgent } from '../types';
import * as api from '../api/tauri';

type StatusFilter = 'all' | 'pending' | 'running' | 'done' | 'failed' | 'cancelled';

const STATUS_COLOR: Record<string, string> = {
  pending:   'var(--text-muted)',
  running:   'var(--accent)',
  done:      'var(--success, #4ade80)',
  failed:    'var(--danger, #ef4444)',
  cancelled: 'var(--text-faint)',
};

const STATUS_ICON: Record<string, string> = {
  pending:   '○',
  running:   '◉',
  done:      '✓',
  failed:    '✕',
  cancelled: '⊘',
};

export default function Tasks() {
  const { workspaces, addToast } = useAppStore();
  const [tasks, setTasks] = useState<TaskQueueItem[]>([]);
  const [agents, setAgents] = useState<UserAgent[]>([]);
  const [filter, setFilter] = useState<StatusFilter>('all');
  const [expanded, setExpanded] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Enqueue form
  const [showForm, setShowForm] = useState(false);
  const [formAgent, setFormAgent] = useState('');
  const [formInput, setFormInput] = useState('');
  const [formPriority, setFormPriority] = useState(0);
  const [submitting, setSubmitting] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const status = filter === 'all' ? undefined : filter;
      const items = await api.listTasks(undefined, status, 200);
      setTasks(items);
    } catch (e) {
      addToast(String(e), 'error');
    }
  }, [filter]);

  useEffect(() => {
    setLoading(true);
    Promise.all([
      refresh(),
      api.listAgents().then(setAgents).catch(() => {}),
    ]).finally(() => setLoading(false));
  }, [filter]);

  // Auto-refresh every 5s when any task is pending or running
  useEffect(() => {
    const hasPendingOrRunning = tasks.some(
      (t) => t.status === 'pending' || t.status === 'running',
    );
    if (!hasPendingOrRunning) return;
    const id = setInterval(refresh, 5000);
    return () => clearInterval(id);
  }, [tasks, refresh]);

  const handleEnqueue = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formAgent || !formInput.trim()) return;
    setSubmitting(true);
    try {
      await api.enqueueTask(formAgent, formInput.trim(), formPriority || undefined);
      addToast('Task enqueued', 'success');
      setShowForm(false);
      setFormAgent('');
      setFormInput('');
      setFormPriority(0);
      await refresh();
    } catch (e) {
      addToast(String(e), 'error');
    } finally {
      setSubmitting(false);
    }
  };

  const handleCancel = async (taskId: string) => {
    try {
      await api.cancelTask(taskId);
      setTasks((prev) =>
        prev.map((t) => t.id === taskId ? { ...t, status: 'cancelled' } : t),
      );
    } catch (e) {
      addToast(String(e), 'error');
    }
  };

  const counts = tasks.reduce<Record<string, number>>((acc, t) => {
    acc[t.status] = (acc[t.status] ?? 0) + 1;
    return acc;
  }, {});

  const FILTERS: StatusFilter[] = ['all', 'pending', 'running', 'done', 'failed', 'cancelled'];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        padding: '12px 20px', borderBottom: '1px solid var(--border)',
        display: 'flex', alignItems: 'center', gap: 10,
        background: 'var(--bg-surface)',
      }}>
        <h1 style={{ fontSize: 15, fontWeight: 700, flex: 1 }}>📬 Task Queue</h1>

        <button className="btn btn-ghost" style={{ fontSize: 12 }} onClick={refresh}>
          Refresh
        </button>
        <button
          className="btn btn-primary"
          style={{ fontSize: 12 }}
          onClick={() => setShowForm((v) => !v)}
        >
          + Enqueue Task
        </button>
      </div>

      {/* Enqueue form */}
      {showForm && (
        <form
          onSubmit={handleEnqueue}
          style={{
            padding: '14px 20px', borderBottom: '1px solid var(--border)',
            background: 'var(--bg-elevated)',
            display: 'flex', flexDirection: 'column', gap: 10,
          }}
        >
          <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 2 }}>New Task</div>
          <div style={{ display: 'flex', gap: 10 }}>
            <div style={{ flex: 2 }}>
              <label style={{ fontSize: 11, color: 'var(--text-muted)' }}>Agent</label>
              <select
                className="form-input"
                value={formAgent}
                onChange={(e) => setFormAgent(e.target.value)}
                required
                style={{ width: '100%', marginTop: 4, fontSize: 12 }}
              >
                <option value="">— pick an agent —</option>
                {agents.map((a) => (
                  <option key={a.id} value={a.id}>{a.name}</option>
                ))}
              </select>
            </div>
            <div style={{ flex: 1 }}>
              <label style={{ fontSize: 11, color: 'var(--text-muted)' }}>Priority</label>
              <input
                className="form-input"
                type="number"
                value={formPriority}
                onChange={(e) => setFormPriority(Number(e.target.value))}
                min={0}
                max={100}
                style={{ width: '100%', marginTop: 4, fontSize: 12 }}
              />
            </div>
          </div>
          <div>
            <label style={{ fontSize: 11, color: 'var(--text-muted)' }}>Task input</label>
            <textarea
              className="form-input"
              value={formInput}
              onChange={(e) => setFormInput(e.target.value)}
              rows={3}
              required
              placeholder="Describe what the agent should do…"
              style={{ width: '100%', marginTop: 4, resize: 'vertical', fontSize: 12 }}
            />
          </div>
          <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
            <button
              type="button"
              className="btn btn-ghost"
              style={{ fontSize: 12 }}
              onClick={() => setShowForm(false)}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={submitting || !formAgent || !formInput.trim()}
              style={{ fontSize: 12 }}
            >
              {submitting ? 'Enqueueing…' : 'Enqueue'}
            </button>
          </div>
        </form>
      )}

      {/* Status filter tabs */}
      <div style={{
        display: 'flex', gap: 4, padding: '8px 16px',
        borderBottom: '1px solid var(--border)', background: 'var(--bg-surface)',
        flexWrap: 'wrap',
      }}>
        {FILTERS.map((f) => {
          const count = f === 'all' ? tasks.length : (counts[f] ?? 0);
          return (
            <button
              key={f}
              onClick={() => setFilter(f)}
              style={{
                fontSize: 11, padding: '3px 10px', borderRadius: 12,
                border: '1px solid var(--border)',
                background: filter === f ? 'var(--accent)' : 'transparent',
                color: filter === f ? '#fff' : 'var(--text-muted)',
                cursor: 'pointer',
              }}
            >
              {f === 'all' ? 'All' : f.charAt(0).toUpperCase() + f.slice(1)}
              {count > 0 && (
                <span style={{ marginLeft: 5, opacity: 0.8 }}>{count}</span>
              )}
            </button>
          );
        })}
      </div>

      {/* Task list */}
      <div style={{ flex: 1, overflow: 'auto', padding: '12px 16px' }}>
        {loading && tasks.length === 0 && (
          <div style={{ textAlign: 'center', padding: 40 }}>
            <div className="spinner" />
          </div>
        )}

        {!loading && tasks.length === 0 && (
          <div style={{
            textAlign: 'center', padding: 60,
            color: 'var(--text-faint)', fontSize: 13,
          }}>
            No tasks{filter !== 'all' ? ` with status "${filter}"` : ''}.<br />
            <span style={{ fontSize: 12, marginTop: 6, display: 'block' }}>
              Use "+ Enqueue Task" to add work, or agents can enqueue tasks using the <code>enqueue</code> tool.
            </span>
          </div>
        )}

        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {tasks.map((task) => {
            const isExpanded = expanded === task.id;
            const color = STATUS_COLOR[task.status] ?? 'var(--text-muted)';
            const icon  = STATUS_ICON[task.status] ?? '?';

            return (
              <div
                key={task.id}
                className="card"
                style={{ padding: '10px 14px', cursor: 'pointer' }}
                onClick={() => setExpanded(isExpanded ? null : task.id)}
              >
                {/* Top row */}
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <span style={{ color, fontSize: 16, width: 18, flexShrink: 0 }}>{icon}</span>

                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, flexWrap: 'wrap' }}>
                      <span style={{ fontWeight: 600, fontSize: 13 }}>{task.agent_name}</span>
                      {task.priority > 0 && (
                        <span style={{
                          fontSize: 10, padding: '1px 6px', borderRadius: 8,
                          background: 'rgba(99,102,241,0.15)', color: 'var(--accent)',
                        }}>
                          p{task.priority}
                        </span>
                      )}
                      <span style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                        {fmtTime(task.created_at)}
                      </span>
                    </div>
                    <div style={{
                      fontSize: 12, color: 'var(--text-muted)', marginTop: 2,
                      whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
                    }}>
                      {task.input.split('\n')[0]}
                    </div>
                  </div>

                  <div style={{ display: 'flex', gap: 6, flexShrink: 0, alignItems: 'center' }}>
                    {task.status === 'running' && (
                      <span style={{
                        fontSize: 10, color: 'var(--accent)',
                        animation: 'pulse 1.4s ease-in-out infinite',
                      }}>
                        running…
                      </span>
                    )}
                    {task.status === 'pending' && (
                      <button
                        className="btn btn-ghost"
                        style={{ fontSize: 11, padding: '2px 8px', color: 'var(--danger, #ef4444)' }}
                        onClick={(e) => { e.stopPropagation(); handleCancel(task.id); }}
                      >
                        Cancel
                      </button>
                    )}
                    <span style={{
                      fontSize: 11, padding: '2px 8px', borderRadius: 10,
                      border: `1px solid ${color}`,
                      color,
                    }}>
                      {task.status}
                    </span>
                  </div>
                </div>

                {/* Expanded detail */}
                {isExpanded && (
                  <div
                    style={{
                      marginTop: 10, paddingTop: 10,
                      borderTop: '1px solid var(--border)',
                      display: 'flex', flexDirection: 'column', gap: 8,
                      fontSize: 12,
                    }}
                    onClick={(e) => e.stopPropagation()}
                  >
                    {/* Full input */}
                    <div>
                      <div style={{ fontSize: 11, color: 'var(--text-faint)', marginBottom: 3 }}>Input</div>
                      <pre style={{
                        margin: 0, padding: '8px 10px',
                        background: 'var(--bg-elevated)',
                        border: '1px solid var(--border)',
                        borderRadius: 'var(--radius-sm)',
                        fontSize: 11, whiteSpace: 'pre-wrap', wordBreak: 'break-word',
                      }}>
                        {task.input}
                      </pre>
                    </div>

                    {/* Timestamps */}
                    <div style={{ display: 'flex', gap: 20, color: 'var(--text-faint)', fontSize: 11 }}>
                      {task.started_at && <span>Started: {fmtTime(task.started_at)}</span>}
                      {task.completed_at && <span>Completed: {fmtTime(task.completed_at)}</span>}
                      <span style={{ fontFamily: 'monospace', fontSize: 10 }}>id: {task.id.slice(0, 8)}</span>
                    </div>

                    {/* Output */}
                    {task.output && (
                      <div>
                        <div style={{ fontSize: 11, color: 'var(--success, #4ade80)', marginBottom: 3 }}>Output</div>
                        <pre style={{
                          margin: 0, padding: '8px 10px',
                          background: 'rgba(74,222,128,0.05)',
                          border: '1px solid rgba(74,222,128,0.2)',
                          borderRadius: 'var(--radius-sm)',
                          fontSize: 11, whiteSpace: 'pre-wrap', wordBreak: 'break-word',
                          maxHeight: 200, overflow: 'auto',
                        }}>
                          {task.output}
                        </pre>
                      </div>
                    )}

                    {/* Error */}
                    {task.error && (
                      <div>
                        <div style={{ fontSize: 11, color: 'var(--danger, #ef4444)', marginBottom: 3 }}>Error</div>
                        <pre style={{
                          margin: 0, padding: '8px 10px',
                          background: 'rgba(239,68,68,0.06)',
                          border: '1px solid rgba(239,68,68,0.25)',
                          borderRadius: 'var(--radius-sm)',
                          fontSize: 11, whiteSpace: 'pre-wrap', wordBreak: 'break-word',
                        }}>
                          {task.error}
                        </pre>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function fmtTime(iso: string): string {
  try {
    const d = new Date(iso);
    const now = Date.now();
    const diff = Math.floor((now - d.getTime()) / 1000);
    if (diff < 60)  return `${diff}s ago`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return d.toLocaleDateString();
  } catch {
    return iso;
  }
}
