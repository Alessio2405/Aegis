import React, { useEffect, useState } from 'react';
import { RetrievedChunk } from '../types';
import * as api from '../api/tauri';

interface Props {
  chunkId: string | null;
  onClose: () => void;
}

export default function SourceViewer({ chunkId, onClose }: Props) {
  const [chunk, setChunk] = useState<RetrievedChunk | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!chunkId) { setChunk(null); return; }
    setLoading(true);
    setError(null);
    api.getChunkSource(chunkId)
      .then(setChunk)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [chunkId]);

  if (!chunkId) return null;

  return (
    <div style={{
      position: 'fixed', inset: 0,
      background: 'rgba(0,0,0,0.7)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      zIndex: 1000,
    }} onClick={onClose}>
      <div
        style={{
          background: 'var(--bg-surface)',
          border: '1px solid var(--border)',
          borderRadius: 'var(--radius)',
          width: '90%', maxWidth: 720,
          maxHeight: '80vh',
          display: 'flex', flexDirection: 'column',
          overflow: 'hidden',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div style={{
          padding: '12px 16px',
          borderBottom: '1px solid var(--border)',
          display: 'flex', justifyContent: 'space-between', alignItems: 'center',
        }}>
          <div>
            <div style={{ fontWeight: 700, fontSize: 14 }}>
              {chunk ? chunk.doc_title : 'Loading source…'}
            </div>
            {chunk && (
              <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 2, fontFamily: 'var(--font-mono)' }}>
                {chunk.doc_path} · chars {chunk.start_char}–{chunk.end_char} · chunk #{chunk.chunk_idx}
              </div>
            )}
          </div>
          <button onClick={onClose} style={{ color: 'var(--text-muted)', fontSize: 18, lineHeight: 1 }}>×</button>
        </div>

        {/* Body */}
        <div style={{ flex: 1, overflow: 'auto', padding: 16 }}>
          {loading && <div className="spinner" />}
          {error && <div style={{ color: 'var(--danger)' }}>{error}</div>}
          {chunk && (
            <>
              {chunk.full_text ? (
                <pre style={{ whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.7, fontSize: 13 }}>
                  {chunk.full_text}
                </pre>
              ) : (
                <div style={{ color: 'var(--text-muted)', fontStyle: 'italic' }}>
                  Full text not available (vault may need to be unlocked).
                </div>
              )}
            </>
          )}
        </div>

        {/* Footer */}
        <div style={{
          padding: '8px 16px',
          borderTop: '1px solid var(--border)',
          display: 'flex', justifyContent: 'flex-end',
        }}>
          <button className="btn btn-ghost btn-sm" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
