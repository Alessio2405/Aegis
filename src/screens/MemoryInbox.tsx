import React, { useEffect, useState } from 'react';
import { useAppStore } from '../store';
import { Memory, MemoryStatus } from '../types';
import * as api from '../api/tauri';

const STATUS_TABS: { key: MemoryStatus | 'all'; label: string }[] = [
  { key: 'proposed', label: '📥 Inbox' },
  { key: 'saved',    label: '✅ Saved' },
  { key: 'rejected', label: '🗑 Rejected' },
  { key: 'all',      label: 'All' },
];

const MEMORY_TYPES = ['Profile', 'Project', 'Procedure', 'Episodic'];

export default function MemoryInbox() {
  const { refreshInboxCount, addToast, workspaces, activeWorkspaceId } = useAppStore();
  const [activeTab, setActiveTab] = useState<MemoryStatus | 'all'>('proposed');
  const [memories, setMemories] = useState<Memory[]>([]);
  const [loading, setLoading] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editContent, setEditContent] = useState('');
  const [showPropose, setShowPropose] = useState(false);
  const [proposeContent, setProposeContent] = useState('');
  const [proposeType, setProposeType] = useState('Project');
  const [proposeWs, setProposeWs] = useState('');

  const load = async () => {
    setLoading(true);
    try {
      const status = activeTab === 'all' ? undefined : activeTab;
      const mems = await api.listMemories(status);
      setMemories(mems);
    } catch (e) {
      addToast(String(e), 'error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { load(); }, [activeTab]);

  const handleAccept = async (id: string) => {
    try {
      await api.acceptMemory(id);
      addToast('Memory saved', 'success');
      await load();
      await refreshInboxCount();
    } catch (e) { addToast(String(e), 'error'); }
  };

  const handleReject = async (id: string) => {
    try {
      await api.rejectMemory(id);
      addToast('Memory rejected', 'info');
      await load();
      await refreshInboxCount();
    } catch (e) { addToast(String(e), 'error'); }
  };

  const handleDelete = async (id: string) => {
    if (!confirm('Permanently delete this memory?')) return;
    try {
      await api.deleteMemory(id);
      addToast('Memory deleted', 'info');
      await load();
      await refreshInboxCount();
    } catch (e) { addToast(String(e), 'error'); }
  };

  const handleStartEdit = (mem: Memory) => {
    setEditingId(mem.id);
    setEditContent(mem.content ?? mem.plaintext_preview ?? '');
  };

  const handleSaveEdit = async (id: string) => {
    if (!editContent.trim()) { addToast('Content cannot be empty', 'error'); return; }
    try {
      await api.editMemory(id, editContent);
      addToast('Memory edited & saved', 'success');
      setEditingId(null);
      await load();
      await refreshInboxCount();
    } catch (e) { addToast(String(e), 'error'); }
  };

  const handlePropose = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!proposeContent.trim()) return;
    try {
      await api.proposeMemory(proposeContent, proposeType, proposeWs || undefined);
      addToast('Memory sent to inbox', 'success');
      setProposeContent('');
      setShowPropose(false);
      if (activeTab === 'proposed') await load();
      await refreshInboxCount();
    } catch (e) { addToast(String(e), 'error'); }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        padding: '12px 20px',
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-surface)',
        display: 'flex', alignItems: 'center', gap: 12,
      }}>
        <h1 style={{ fontSize: 15, fontWeight: 700, flex: 1 }}>🧠 Memory Inbox</h1>
        <button
          className="btn btn-primary btn-sm"
          onClick={() => setShowPropose(!showPropose)}
        >
          + Add Memory
        </button>
      </div>

      {/* Propose form */}
      {showPropose && (
        <div style={{ padding: '16px 20px', borderBottom: '1px solid var(--border)', background: 'var(--bg-elevated)' }}>
          <form onSubmit={handlePropose} style={{ display: 'flex', flexDirection: 'column', gap: 10, maxWidth: 600 }}>
            <div style={{ display: 'flex', gap: 8 }}>
              <select
                value={proposeType}
                onChange={(e) => setProposeType(e.target.value)}
                style={{ fontSize: 12, padding: '5px 8px' }}
              >
                {MEMORY_TYPES.map((t) => <option key={t}>{t}</option>)}
              </select>
              <select
                value={proposeWs}
                onChange={(e) => setProposeWs(e.target.value)}
                style={{ fontSize: 12, padding: '5px 8px' }}
              >
                <option value="">Global scope</option>
                {workspaces.map((ws) => <option key={ws.id} value={ws.id}>{ws.name}</option>)}
              </select>
            </div>
            <textarea
              className="form-input"
              value={proposeContent}
              onChange={(e) => setProposeContent(e.target.value)}
              placeholder="Enter memory content…"
              rows={3}
              style={{ resize: 'vertical' }}
            />
            <div style={{ display: 'flex', gap: 8 }}>
              <button type="submit" className="btn btn-primary btn-sm" disabled={!proposeContent.trim()}>
                Send to Inbox
              </button>
              <button type="button" className="btn btn-ghost btn-sm" onClick={() => setShowPropose(false)}>
                Cancel
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Tabs */}
      <div style={{
        display: 'flex', gap: 4, padding: '8px 20px',
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-surface)',
      }}>
        {STATUS_TABS.map((tab) => (
          <button
            key={tab.key}
            className={`btn btn-sm ${activeTab === tab.key ? 'btn-primary' : 'btn-ghost'}`}
            onClick={() => setActiveTab(tab.key)}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Memory list */}
      <div style={{ flex: 1, overflow: 'auto', padding: 20 }}>
        {loading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: 40 }}>
            <div className="spinner" />
          </div>
        ) : memories.length === 0 ? (
          <div className="empty-state">
            <div className="icon">🧠</div>
            <div>No memories in this view.</div>
            {activeTab === 'proposed' && (
              <div style={{ fontSize: 12, color: 'var(--text-faint)' }}>
                Ask questions to generate memory proposals, or add one manually.
              </div>
            )}
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {memories.map((mem) => (
              <MemoryCard
                key={mem.id}
                memory={mem}
                isEditing={editingId === mem.id}
                editContent={editContent}
                onEditContent={setEditContent}
                onAccept={() => handleAccept(mem.id)}
                onReject={() => handleReject(mem.id)}
                onDelete={() => handleDelete(mem.id)}
                onStartEdit={() => handleStartEdit(mem)}
                onSaveEdit={() => handleSaveEdit(mem.id)}
                onCancelEdit={() => setEditingId(null)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Memory card component ─────────────────────────────────────────────────

interface MemoryCardProps {
  memory: Memory;
  isEditing: boolean;
  editContent: string;
  onEditContent: (v: string) => void;
  onAccept: () => void;
  onReject: () => void;
  onDelete: () => void;
  onStartEdit: () => void;
  onSaveEdit: () => void;
  onCancelEdit: () => void;
}

function MemoryCard({
  memory, isEditing, editContent, onEditContent,
  onAccept, onReject, onDelete, onStartEdit, onSaveEdit, onCancelEdit,
}: MemoryCardProps) {
  const statusTag = {
    proposed: 'tag-yellow',
    saved: 'tag-green',
    rejected: 'tag-gray',
  }[memory.status] ?? 'tag-gray';

  const displayContent = memory.content ?? memory.plaintext_preview ?? '(encrypted)';

  return (
    <div className="card">
      {/* Top row */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 8 }}>
        <div style={{ display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap' }}>
          <span className={`tag ${statusTag}`}>{memory.status}</span>
          <span className="tag tag-blue">{memory.memory_type}</span>
          {memory.workspace_id && (
            <span className="tag tag-gray" style={{ fontSize: 10 }}>workspace-scoped</span>
          )}
        </div>
        <div style={{ fontSize: 10, color: 'var(--text-faint)' }}>
          {new Date(memory.created_at).toLocaleString()}
        </div>
      </div>

      {/* Content */}
      {isEditing ? (
        <textarea
          className="form-input"
          value={editContent}
          onChange={(e) => onEditContent(e.target.value)}
          rows={4}
          style={{ width: '100%', resize: 'vertical', marginBottom: 8, fontSize: 13 }}
          autoFocus
        />
      ) : (
        <p style={{ fontSize: 13, lineHeight: 1.6, color: 'var(--text)', marginBottom: 8 }}>
          {displayContent}
        </p>
      )}

      {/* Source */}
      {memory.source && (
        <div style={{ fontSize: 11, color: 'var(--text-faint)', marginBottom: 10 }}>
          Source: {memory.source}
        </div>
      )}

      {/* Actions */}
      <div style={{ display: 'flex', gap: 6 }}>
        {isEditing ? (
          <>
            <button className="btn btn-success btn-sm" onClick={onSaveEdit}>Save & Accept</button>
            <button className="btn btn-ghost btn-sm" onClick={onCancelEdit}>Cancel</button>
          </>
        ) : (
          <>
            {memory.status === 'proposed' && (
              <>
                <button className="btn btn-success btn-sm" onClick={onAccept}>✓ Accept</button>
                <button className="btn btn-ghost btn-sm" onClick={onStartEdit}>✎ Edit</button>
                <button className="btn btn-ghost btn-sm" style={{ color: 'var(--danger)' }} onClick={onReject}>✗ Reject</button>
              </>
            )}
            {memory.status === 'saved' && (
              <>
                <button className="btn btn-ghost btn-sm" onClick={onStartEdit}>✎ Edit</button>
                <button className="btn btn-ghost btn-sm" style={{ color: 'var(--danger)' }} onClick={onDelete}>Delete</button>
              </>
            )}
            {memory.status === 'rejected' && (
              <button className="btn btn-ghost btn-sm" style={{ color: 'var(--danger)' }} onClick={onDelete}>Delete permanently</button>
            )}
          </>
        )}
      </div>
    </div>
  );
}
