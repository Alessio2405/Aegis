import React, { useEffect, useState } from 'react';
import { useAppStore } from '../store';
import { AuditRecord } from '../types';
import * as api from '../api/tauri';

const ACTION_TYPES = ['', 'read', 'write', 'ingest', 'permission_check', 'vault_unlock', 'vault_create', 'settings_update', 'delete', 'permission_revoke'];
const OUTCOMES = ['', 'allowed', 'denied'];

export default function Audit() {
  const { workspaces, addToast } = useAppStore();
  const [records, setRecords] = useState<AuditRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [filters, setFilters] = useState({
    actionType: '',
    outcome: '',
    workspaceId: '',
    limit: 200,
  });

  const load = async () => {
    setLoading(true);
    try {
      const events = await api.listAuditEvents({
        actionType: filters.actionType || undefined,
        outcome: filters.outcome || undefined,
        workspaceId: filters.workspaceId || undefined,
        limit: filters.limit,
      });
      setRecords(events);
    } catch (e) {
      addToast(String(e), 'error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { load(); }, [filters]);

  const outcomeClass = (o: string) => o === 'allowed' ? 'tag-green' : 'tag-red';
  const actionClass = (a: string) => {
    if (a === 'read') return 'tag-blue';
    if (a === 'write') return 'tag-yellow';
    if (a === 'ingest') return 'tag-blue';
    if (a.includes('vault')) return 'tag-gray';
    return 'tag-gray';
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        padding: '12px 20px',
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-surface)',
      }}>
        <h1 style={{ fontSize: 15, fontWeight: 700, marginBottom: 10 }}>📋 Audit Timeline</h1>

        {/* Filters */}
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          <select
            value={filters.actionType}
            onChange={(e) => setFilters((f) => ({ ...f, actionType: e.target.value }))}
            style={{ fontSize: 12, padding: '4px 8px' }}
          >
            <option value="">All action types</option>
            {ACTION_TYPES.filter(Boolean).map((t) => <option key={t} value={t}>{t}</option>)}
          </select>

          <select
            value={filters.outcome}
            onChange={(e) => setFilters((f) => ({ ...f, outcome: e.target.value }))}
            style={{ fontSize: 12, padding: '4px 8px' }}
          >
            <option value="">All outcomes</option>
            {OUTCOMES.filter(Boolean).map((o) => <option key={o} value={o}>{o}</option>)}
          </select>

          <select
            value={filters.workspaceId}
            onChange={(e) => setFilters((f) => ({ ...f, workspaceId: e.target.value }))}
            style={{ fontSize: 12, padding: '4px 8px' }}
          >
            <option value="">All workspaces</option>
            {workspaces.map((ws) => <option key={ws.id} value={ws.id}>{ws.name}</option>)}
          </select>

          <select
            value={filters.limit}
            onChange={(e) => setFilters((f) => ({ ...f, limit: Number(e.target.value) }))}
            style={{ fontSize: 12, padding: '4px 8px' }}
          >
            <option value={50}>Last 50</option>
            <option value={200}>Last 200</option>
            <option value={500}>Last 500</option>
          </select>

          <button className="btn btn-ghost btn-sm" onClick={load}>↺ Refresh</button>

          <div style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--text-muted)', alignSelf: 'center' }}>
            {records.length} events
          </div>
        </div>
      </div>

      {/* Timeline */}
      <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
        {loading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: 40 }}>
            <div className="spinner" />
          </div>
        ) : records.length === 0 ? (
          <div className="empty-state">
            <div className="icon">📋</div>
            <div>No audit events match the current filters.</div>
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {records.map((r) => (
              <AuditRow key={r.id} record={r}
                outcomeClass={outcomeClass(r.outcome)}
                actionClass={actionClass(r.action_type)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function AuditRow({ record: r, outcomeClass, actionClass }: {
  record: AuditRecord;
  outcomeClass: string;
  actionClass: string;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div
      className="card"
      style={{ padding: '8px 12px', cursor: 'pointer' }}
      onClick={() => setExpanded((e) => !e)}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
        {/* Timestamp */}
        <span style={{ fontSize: 11, color: 'var(--text-faint)', fontFamily: 'var(--font-mono)', flexShrink: 0 }}>
          {new Date(r.ts).toLocaleTimeString()} {new Date(r.ts).toLocaleDateString()}
        </span>

        {/* Outcome */}
        <span className={`tag ${outcomeClass}`} style={{ fontSize: 10, flexShrink: 0 }}>
          {r.outcome}
        </span>

        {/* Action type */}
        <span className={`tag ${actionClass}`} style={{ fontSize: 10, flexShrink: 0 }}>
          {r.action_type}
        </span>

        {/* Actor */}
        <span style={{ fontSize: 11, color: 'var(--text-muted)', flexShrink: 0 }}>
          {r.actor}
        </span>

        {/* Capability */}
        {r.capability && (
          <code style={{ fontSize: 11, color: 'var(--text-muted)' }}>{r.capability}</code>
        )}

        {/* Target */}
        {r.target_type && (
          <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>
            → {r.target_type}
            {r.target_id && <span style={{ color: 'var(--text-faint)' }}> #{r.target_id.slice(0, 8)}</span>}
          </span>
        )}

        {r.workspace_id && (
          <span style={{ fontSize: 10, color: 'var(--text-faint)' }}>
            ws:{r.workspace_id.slice(0, 8)}
          </span>
        )}

        <span style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--text-faint)' }}>
          {expanded ? '▲' : '▼'}
        </span>
      </div>

      {expanded && r.details_json && (
        <pre style={{ marginTop: 8, fontSize: 11, color: 'var(--text-muted)' }}>
          {JSON.stringify(JSON.parse(r.details_json), null, 2)}
        </pre>
      )}
    </div>
  );
}
