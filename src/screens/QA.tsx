import React, { useState, useRef, useEffect } from 'react';
import { useAppStore } from '../store';
import { AgentAnswer } from '../types';
import CitationList from '../components/CitationList';
import SourceViewer from '../components/SourceViewer';
import * as api from '../api/tauri';

interface QAEntry {
  id: string;
  question: string;
  answer: AgentAnswer | null;
  loading: boolean;
  error: string | null;
}

export default function QA() {
  const { workspaces, activeWorkspaceId, addToast, refreshInboxCount } = useAppStore();
  const [entries, setEntries] = useState<QAEntry[]>([]);
  const [question, setQuestion] = useState('');
  const [selectedWs, setSelectedWs] = useState<string>('');
  const [sourceChunkId, setSourceChunkId] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (activeWorkspaceId) setSelectedWs(activeWorkspaceId);
  }, [activeWorkspaceId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [entries]);

  const handleAsk = async (e: React.FormEvent) => {
    e.preventDefault();
    const q = question.trim();
    if (!q) return;

    const entryId = Math.random().toString(36).slice(2);
    setEntries((prev) => [...prev, { id: entryId, question: q, answer: null, loading: true, error: null }]);
    setQuestion('');

    try {
      const answer = await api.askQuestion(q, selectedWs || undefined);
      setEntries((prev) => prev.map((e) =>
        e.id === entryId ? { ...e, answer, loading: false } : e
      ));
      if (answer.proposed_memories.length > 0) {
        await refreshInboxCount();
        addToast(`${answer.proposed_memories.length} memory proposal(s) sent to inbox`, 'info');
      }
    } catch (err) {
      setEntries((prev) => prev.map((e) =>
        e.id === entryId ? { ...e, loading: false, error: String(err) } : e
      ));
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        padding: '12px 20px',
        borderBottom: '1px solid var(--border)',
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        background: 'var(--bg-surface)',
      }}>
        <h1 style={{ fontSize: 15, fontWeight: 700, flex: 1 }}>🔍 Ask Questions</h1>
        {workspaces.length > 0 && (
          <select
            value={selectedWs}
            onChange={(e) => setSelectedWs(e.target.value)}
            style={{ fontSize: 12, padding: '4px 8px' }}
          >
            <option value="">All workspaces</option>
            {workspaces.map((ws) => (
              <option key={ws.id} value={ws.id}>{ws.name}</option>
            ))}
          </select>
        )}
      </div>

      {/* Conversation */}
      <div style={{ flex: 1, overflow: 'auto', padding: '20px', display: 'flex', flexDirection: 'column', gap: 24 }}>
        {entries.length === 0 ? (
          <div className="empty-state" style={{ marginTop: 60 }}>
            <div className="icon">💬</div>
            <div style={{ fontWeight: 600 }}>Ask anything about your documents</div>
            <div style={{ fontSize: 12, color: 'var(--text-faint)', maxWidth: 360 }}>
              Every answer includes citations. You can view the full source chunk for any citation.
              Ingest a workspace folder first if you haven't yet.
            </div>
          </div>
        ) : (
          entries.map((entry) => <QACard key={entry.id} entry={entry} onOpenSource={setSourceChunkId} />)
        )}
        <div ref={bottomRef} />
      </div>

      {/* Input form */}
      <div style={{
        padding: '12px 20px',
        borderTop: '1px solid var(--border)',
        background: 'var(--bg-surface)',
      }}>
        <form onSubmit={handleAsk} style={{ display: 'flex', gap: 10 }}>
          <input
            className="form-input"
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            placeholder="Ask a question about your documents…"
            style={{ flex: 1, fontSize: 13 }}
            autoFocus
          />
          <button
            type="submit"
            className="btn btn-primary"
            disabled={!question.trim()}
          >
            Ask ↵
          </button>
        </form>
        <div style={{ fontSize: 11, color: 'var(--text-faint)', marginTop: 6 }}>
          Answers include citations. No network calls unless AI model is enabled in Settings.
        </div>
      </div>

      <SourceViewer chunkId={sourceChunkId} onClose={() => setSourceChunkId(null)} />
    </div>
  );
}

function QACard({ entry, onOpenSource }: { entry: QAEntry; onOpenSource: (id: string) => void }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      {/* Question bubble */}
      <div style={{ alignSelf: 'flex-end', maxWidth: '70%' }}>
        <div style={{
          background: 'var(--accent)',
          color: '#fff',
          borderRadius: '12px 12px 4px 12px',
          padding: '10px 14px',
          fontSize: 13,
        }}>
          {entry.question}
        </div>
      </div>

      {/* Answer */}
      <div style={{ maxWidth: '90%' }}>
        {entry.loading && (
          <div style={{ display: 'flex', gap: 8, alignItems: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
            <div className="spinner" /> Searching your documents…
          </div>
        )}
        {entry.error && (
          <div className="card" style={{ borderColor: 'var(--danger)', color: 'var(--danger)', fontSize: 13 }}>
            Error: {entry.error}
          </div>
        )}
        {entry.answer && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div className="card">
              {entry.answer.is_extractive && (
                <div style={{ marginBottom: 8 }}>
                  <span className="tag tag-yellow">Extractive (no AI model)</span>
                </div>
              )}
              <div style={{ fontSize: 13, lineHeight: 1.7, whiteSpace: 'pre-wrap' }}>
                {entry.answer.answer}
              </div>
            </div>
            {entry.answer.citations.length > 0 && (
              <CitationList citations={entry.answer.citations} onOpenSource={onOpenSource} />
            )}
            {entry.answer.citations.length === 0 && !entry.answer.answer.includes("don't have sources") && (
              <div style={{ fontSize: 11, color: 'var(--text-faint)' }}>
                No citations — answer was generated without source chunks.
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
