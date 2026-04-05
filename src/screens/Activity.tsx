import React, { useEffect, useState } from 'react';
import { Artifact, SystemStatus } from '../types';
import * as api from '../api/tauri';

// ── Shared helpers ────────────────────────────────────────────────────────────

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString([], {
      month: 'short', day: 'numeric',
      hour: '2-digit', minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

// ── Artifact feed ─────────────────────────────────────────────────────────────

const TYPE_COLORS: Record<string, string> = {
  finding:   'var(--accent)',
  summary:   '#10b981',
  report:    '#f59e0b',
  research:  '#8b5cf6',
  note:      'var(--text-muted)',
};

function ArtifactCard({
  artifact, expanded, deleting, onToggle, onDelete,
}: {
  artifact: Artifact;
  expanded: boolean;
  deleting: boolean;
  onToggle: () => void;
  onDelete: () => void;
}) {
  const color = TYPE_COLORS[artifact.artifact_type] ?? TYPE_COLORS.note;
  const preview = artifact.content.slice(0, 200) + (artifact.content.length > 200 ? '…' : '');

  return (
    <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
      <div
        style={{ display: 'flex', alignItems: 'flex-start', gap: 10, padding: '12px 14px', cursor: 'pointer' }}
        onClick={onToggle}
      >
        <div style={{ width: 4, minHeight: 44, borderRadius: 2, background: color, flexShrink: 0, alignSelf: 'stretch' }} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
            <span style={{ fontSize: 10, fontWeight: 600, textTransform: 'uppercase', color, letterSpacing: '0.06em' }}>
              {artifact.artifact_type}
            </span>
            {artifact.agent_name && (
              <span style={{ fontSize: 10, color: 'var(--text-faint)' }}>by {artifact.agent_name}</span>
            )}
            <span style={{ fontSize: 10, color: 'var(--text-faint)', marginLeft: 'auto' }}>
              {formatTime(artifact.created_at)}
            </span>
          </div>
          <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 4 }}>{artifact.title}</div>
          {!expanded && (
            <div style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.5 }}>{preview}</div>
          )}
        </div>
        <span style={{ fontSize: 12, color: 'var(--text-faint)', flexShrink: 0 }}>{expanded ? '▲' : '▼'}</span>
      </div>
      {expanded && (
        <div style={{ borderTop: '1px solid var(--border)', padding: '12px 14px' }}>
          <pre style={{
            fontSize: 12, lineHeight: 1.6, whiteSpace: 'pre-wrap', wordBreak: 'break-word',
            color: 'var(--text)', margin: 0, maxHeight: 400, overflow: 'auto',
          }}>
            {artifact.content}
          </pre>
          <div style={{ marginTop: 12, display: 'flex', justifyContent: 'flex-end' }}>
            <button
              className="btn btn-ghost"
              style={{ fontSize: 12, color: 'var(--danger, #ef4444)' }}
              disabled={deleting}
              onClick={(e) => { e.stopPropagation(); onDelete(); }}
            >
              {deleting ? '…' : 'Delete'}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function ArtifactsTab() {
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
  const [loading, setLoading]   = useState(true);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);

  const load = () => {
    setLoading(true);
    api.listArtifacts(undefined, 100)
      .then(setArtifacts).catch(() => {}).finally(() => setLoading(false));
  };

  useEffect(() => { load(); }, []);

  const handleDelete = async (id: string) => {
    setDeleting(id);
    try {
      await api.deleteArtifact(id);
      setArtifacts((prev) => prev.filter((a) => a.id !== id));
      if (expanded === id) setExpanded(null);
    } catch { /* ignore */ } finally { setDeleting(null); }
  };

  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'flex-end', padding: '8px 20px 0' }}>
        <span style={{ fontSize: 12, color: 'var(--text-muted)', marginRight: 8 }}>
          {artifacts.length} artifact{artifacts.length !== 1 ? 's' : ''}
        </span>
        <button className="btn btn-ghost" style={{ fontSize: 12 }} onClick={load}>Refresh</button>
      </div>
      <div style={{ flex: 1, overflow: 'auto', padding: '12px 20px', display: 'flex', flexDirection: 'column', gap: 10 }}>
        {loading ? (
          <div style={{ textAlign: 'center', marginTop: 60 }}><div className="spinner" style={{ margin: '0 auto' }} /></div>
        ) : artifacts.length === 0 ? (
          <div className="empty-state" style={{ marginTop: 60 }}>
            <div className="icon">📡</div>
            <div style={{ fontWeight: 600 }}>No artifacts yet</div>
            <div style={{ fontSize: 12, color: 'var(--text-faint)', maxWidth: 340 }}>
              Artifacts are findings, summaries, and reports saved by agents using the{' '}
              <code>save_artifact</code> tool.
            </div>
          </div>
        ) : (
          artifacts.map((a) => (
            <ArtifactCard
              key={a.id} artifact={a}
              expanded={expanded === a.id} deleting={deleting === a.id}
              onToggle={() => setExpanded(expanded === a.id ? null : a.id)}
              onDelete={() => handleDelete(a.id)}
            />
          ))
        )}
      </div>
    </div>
  );
}

// ── System status dashboard ───────────────────────────────────────────────────

function StatusDot({ running, label }: { running: boolean; label: string }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
      <span style={{
        display: 'inline-block', width: 9, height: 9, borderRadius: '50%',
        background: running ? '#4ade80' : '#475569', flexShrink: 0,
      }} />
      <span style={{ fontSize: 13, color: running ? 'var(--text)' : 'var(--text-muted)' }}>{label}</span>
      <span style={{ fontSize: 11, color: running ? '#4ade80' : 'var(--text-faint)', marginLeft: 4 }}>
        {running ? 'running' : 'stopped'}
      </span>
    </div>
  );
}

function StatBadge({ label, value, color }: { label: string; value: number; color?: string }) {
  return (
    <div className="card" style={{ textAlign: 'center', padding: '12px 18px', minWidth: 110 }}>
      <div style={{ fontSize: 22, fontWeight: 700, color: color ?? 'var(--text)' }}>{value}</div>
      <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 2 }}>{label}</div>
    </div>
  );
}

function runStatusColor(status: string): string {
  switch (status) {
    case 'success': return '#4ade80';
    case 'failed':  return '#f87171';
    case 'running': return '#60a5fa';
    default:        return 'var(--text-muted)';
  }
}

function SystemTab() {
  const [status, setStatus]         = useState<SystemStatus | null>(null);
  const [error, setError]           = useState<string | null>(null);
  const [lastRefresh, setLastRefresh] = useState<Date>(new Date());

  const refresh = async () => {
    try {
      setStatus(await api.getSystemStatus());
      setLastRefresh(new Date());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 10_000);
    return () => clearInterval(id);
  }, []);

  return (
    <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'flex-end', marginBottom: 14, gap: 10 }}>
        <span style={{ fontSize: 11, color: 'var(--text-faint)' }}>
          Updated {lastRefresh.toLocaleTimeString()}
        </span>
        <button className="btn btn-ghost" style={{ fontSize: 12 }} onClick={refresh}>Refresh</button>
      </div>

      {error && (
        <div style={{
          background: 'rgba(239,68,68,0.1)', border: '1px solid rgba(239,68,68,0.3)',
          borderRadius: 8, padding: 12, marginBottom: 16, color: '#fca5a5', fontSize: 13,
        }}>
          {error}
        </div>
      )}

      {!status && !error && <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>Loading…</p>}

      {status && (
        <>
          {/* Services + counts */}
          <div className="card" style={{ marginBottom: 16, display: 'grid', gridTemplateColumns: '1fr auto', gap: 20, alignItems: 'start' }}>
            <div>
              <div style={{ fontSize: 11, color: 'var(--text-faint)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 10 }}>
                Services
              </div>
              <StatusDot running={status.scheduler_running} label="Scheduler" />
              <StatusDot running={status.watcher_running}   label="File Watcher" />
              <StatusDot running={status.bot_running}       label="Telegram Bot" />
            </div>
            <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', justifyContent: 'flex-end' }}>
              <StatBadge label="Scheduled"    value={status.scheduled_agents} color="#a78bfa" />
              <StatBadge label="Triggers"     value={status.active_triggers}  color="#38bdf8" />
              <StatBadge label="Pending"      value={status.pending_tasks}    color="#fbbf24" />
              <StatBadge label="Running"      value={status.running_tasks}    color="#60a5fa" />
            </div>
          </div>

          {/* Recent runs */}
          <div className="card">
            <div style={{ fontSize: 11, color: 'var(--text-faint)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 12 }}>
              Recent Agent Runs
            </div>
            {status.recent_runs.length === 0 ? (
              <p style={{ color: 'var(--text-faint)', fontSize: 13, margin: 0 }}>No runs yet.</p>
            ) : (
              <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
                <thead>
                  <tr style={{ color: 'var(--text-faint)', textAlign: 'left', borderBottom: '1px solid var(--border)' }}>
                    {['Agent', 'Status', 'Steps', 'When', 'Output'].map(h => (
                      <th key={h} style={{ padding: '3px 8px 8px 0', fontWeight: 500 }}>{h}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {status.recent_runs.map(run => (
                    <tr key={run.id} style={{ borderBottom: '1px solid var(--border)' }}>
                      <td style={{ padding: '6px 8px 6px 0', color: 'var(--text)' }}>{run.agent_name}</td>
                      <td style={{ padding: '6px 8px 6px 0' }}>
                        <span style={{ color: runStatusColor(run.status), fontWeight: 500 }}>{run.status}</span>
                      </td>
                      <td style={{ padding: '6px 8px 6px 0', color: 'var(--text-muted)' }}>{run.step_count}</td>
                      <td style={{ padding: '6px 8px 6px 0', color: 'var(--text-faint)', whiteSpace: 'nowrap' }}>
                        {formatTime(run.created_at)}
                      </td>
                      <td
                        style={{ padding: '6px 0', color: 'var(--text-muted)', maxWidth: 260, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
                        title={run.output ?? ''}
                      >
                        {run.output ?? '—'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </>
      )}
    </div>
  );
}

// ── Root component ────────────────────────────────────────────────────────────

type Tab = 'artifacts' | 'system';

export default function Activity() {
  const [tab, setTab] = useState<Tab>('system');

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        padding: '12px 20px 0',
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-surface)',
      }}>
        <h1 style={{ fontSize: 15, fontWeight: 700, margin: '0 0 10px' }}>Activity</h1>
        <div style={{ display: 'flex', gap: 4 }}>
          {(['system', 'artifacts'] as Tab[]).map(t => (
            <button
              key={t}
              onClick={() => setTab(t)}
              style={{
                background: 'none', border: 'none', cursor: 'pointer',
                padding: '5px 12px', fontSize: 13, borderRadius: '6px 6px 0 0',
                color: tab === t ? 'var(--accent)' : 'var(--text-muted)',
                borderBottom: tab === t ? '2px solid var(--accent)' : '2px solid transparent',
                fontWeight: tab === t ? 600 : 400,
              }}
            >
              {t === 'system' ? 'System Status' : 'Artifacts'}
            </button>
          ))}
        </div>
      </div>

      {tab === 'artifacts' ? <ArtifactsTab /> : <SystemTab />}
    </div>
  );
}
