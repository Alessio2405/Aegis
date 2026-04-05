import React, { useEffect, useState } from 'react';
import { useAppStore } from '../store';
import { DocumentInfo, IngestStats } from '../types';
import * as api from '../api/tauri';

export default function Workspace() {
  const { workspaces, activeWorkspaceId, setActiveWorkspaceId, refreshWorkspaces, addToast, refreshInboxCount } = useAppStore();
  const [documents, setDocuments] = useState<DocumentInfo[]>([]);
  const [ingestStats, setIngestStats] = useState<IngestStats | null>(null);
  const [ingesting, setIngesting] = useState(false);
  const [addingWorkspace, setAddingWorkspace] = useState(false);
  const [newWsName, setNewWsName] = useState('');
  const [newWsPath, setNewWsPath] = useState('');
  const [showAddForm, setShowAddForm] = useState(false);
  const [error, setError] = useState('');

  const activeWs = workspaces.find((w) => w.id === activeWorkspaceId);

  useEffect(() => {
    if (activeWorkspaceId) {
      api.listDocuments(activeWorkspaceId)
        .then(setDocuments)
        .catch(() => setDocuments([]));
    }
  }, [activeWorkspaceId]);

  const handlePickFolder = async () => {
    const path = await api.openFolderDialog();
    if (path) {
      setNewWsPath(path);
      if (!newWsName) {
        // Auto-fill name from folder basename
        const parts = path.replace(/\\/g, '/').split('/');
        setNewWsName(parts[parts.length - 1] || 'My Workspace');
      }
    }
  };

  const handleAddWorkspace = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    if (!newWsPath) { setError('Please select a folder.'); return; }
    if (!newWsName) { setError('Please enter a workspace name.'); return; }
    setAddingWorkspace(true);
    try {
      const ws = await api.addWorkspace(newWsName, newWsPath);
      addToast(`Workspace "${ws.name}" added`, 'success');
      await refreshWorkspaces();
      setActiveWorkspaceId(ws.id);
      setShowAddForm(false);
      setNewWsName('');
      setNewWsPath('');
      // Auto-ingest
      await handleIngest(ws.id);
    } catch (err) {
      setError(String(err));
    } finally {
      setAddingWorkspace(false);
    }
  };

  const handleIngest = async (wsId?: string) => {
    const id = wsId ?? activeWorkspaceId;
    if (!id) return;
    setIngesting(true);
    setIngestStats(null);
    try {
      const stats = await api.ingestWorkspace(id);
      setIngestStats(stats);
      addToast(`Ingested ${stats.documents_added} docs, ${stats.chunks_added} chunks`, 'success');
      const docs = await api.listDocuments(id);
      setDocuments(docs);
      await refreshInboxCount();
    } catch (err) {
      addToast(String(err), 'error');
    } finally {
      setIngesting(false);
    }
  };

  return (
    <div style={{ display: 'flex', height: '100%', overflow: 'hidden' }}>
      {/* Workspace list panel */}
      <div style={{
        width: 220,
        background: 'var(--bg-surface)',
        borderRight: '1px solid var(--border)',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
      }}>
        <div style={{ padding: '12px 12px 8px', borderBottom: '1px solid var(--border)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>Workspaces</span>
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => setShowAddForm(true)}
            title="Add workspace"
            style={{ padding: '3px 6px' }}
          >
            +
          </button>
        </div>
        <div style={{ flex: 1, overflow: 'auto', padding: 8 }}>
          {workspaces.length === 0 ? (
            <div className="empty-state" style={{ padding: 24 }}>
              <div className="icon">🗂</div>
              <div style={{ fontSize: 12 }}>No workspaces yet</div>
              <button className="btn btn-primary btn-sm" onClick={() => setShowAddForm(true)}>
                Add folder
              </button>
            </div>
          ) : (
            workspaces.map((ws) => (
              <button
                key={ws.id}
                onClick={() => setActiveWorkspaceId(ws.id)}
                style={{
                  display: 'block', width: '100%', textAlign: 'left',
                  padding: '8px 10px',
                  borderRadius: 'var(--radius)',
                  background: activeWorkspaceId === ws.id ? 'var(--bg-elevated)' : 'transparent',
                  color: activeWorkspaceId === ws.id ? 'var(--text)' : 'var(--text-muted)',
                  fontSize: 12,
                  marginBottom: 2,
                }}
              >
                <div style={{ fontWeight: 600, marginBottom: 2 }}>{ws.name}</div>
                <div style={{ fontSize: 10, color: 'var(--text-faint)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {ws.path}
                </div>
              </button>
            ))
          )}
        </div>
      </div>

      {/* Main panel */}
      <div style={{ flex: 1, overflow: 'auto', padding: 24, display: 'flex', flexDirection: 'column', gap: 16 }}>
        {showAddForm ? (
          <div className="card" style={{ maxWidth: 520 }}>
            <h2 style={{ marginBottom: 16, fontSize: 15, fontWeight: 600 }}>Add Workspace Folder</h2>
            <form onSubmit={handleAddWorkspace} style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <div className="form-group">
                <label className="form-label">Workspace Name</label>
                <input
                  className="form-input"
                  value={newWsName}
                  onChange={(e) => setNewWsName(e.target.value)}
                  placeholder="My Project"
                />
              </div>
              <div className="form-group">
                <label className="form-label">Folder Path</label>
                <div style={{ display: 'flex', gap: 8 }}>
                  <input
                    className="form-input"
                    value={newWsPath}
                    onChange={(e) => setNewWsPath(e.target.value)}
                    placeholder="/path/to/folder"
                    style={{ flex: 1 }}
                  />
                  <button type="button" className="btn btn-ghost btn-sm" onClick={handlePickFolder}>
                    Browse…
                  </button>
                </div>
              </div>
              {error && <div style={{ color: 'var(--danger)', fontSize: 12 }}>{error}</div>}
              <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
                <button type="button" className="btn btn-ghost" onClick={() => { setShowAddForm(false); setError(''); }}>
                  Cancel
                </button>
                <button type="submit" className="btn btn-primary" disabled={addingWorkspace}>
                  {addingWorkspace ? 'Adding…' : 'Add & Ingest'}
                </button>
              </div>
            </form>
          </div>
        ) : activeWs ? (
          <>
            {/* Workspace header */}
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
              <div>
                <h1 style={{ fontSize: 18, fontWeight: 700 }}>{activeWs.name}</h1>
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2, fontFamily: 'var(--font-mono)' }}>
                  {activeWs.path}
                </div>
              </div>
              <button
                className="btn btn-primary"
                onClick={() => handleIngest()}
                disabled={ingesting}
              >
                {ingesting ? <><span className="spinner" style={{ width: 14, height: 14 }} /> Ingesting…</> : '⟳ Re-ingest'}
              </button>
            </div>

            {/* Ingest stats */}
            {ingestStats && (
              <div className="card" style={{ display: 'flex', gap: 24, padding: '12px 16px' }}>
                <Stat label="Docs added" value={ingestStats.documents_added} />
                <Stat label="Chunks created" value={ingestStats.chunks_added} />
                <Stat label="Skipped (unchanged)" value={ingestStats.skipped} />
              </div>
            )}

            {/* Documents list */}
            <div>
              <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 10 }}>
                Documents ({documents.length})
              </div>
              {documents.length === 0 ? (
                <div className="empty-state">
                  <div className="icon">📄</div>
                  <div>No documents ingested yet.</div>
                  <button className="btn btn-primary btn-sm" onClick={() => handleIngest()} disabled={ingesting}>
                    Ingest folder
                  </button>
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                  {documents.map((doc) => (
                    <div key={doc.id} className="card" style={{ padding: '10px 14px', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div style={{ fontWeight: 600, fontSize: 13 }}>📄 {doc.title}</div>
                        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 2, fontFamily: 'var(--font-mono)' }}>
                          {doc.path}
                        </div>
                      </div>
                      <span className="tag tag-blue">{doc.chunk_count} chunks</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="empty-state" style={{ marginTop: 80 }}>
            <div className="icon">🗂</div>
            <div>Select or add a workspace to get started.</div>
            <button className="btn btn-primary" onClick={() => setShowAddForm(true)}>
              Add Workspace Folder
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--accent)' }}>{value}</div>
      <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>{label}</div>
    </div>
  );
}
