import React from 'react';
import { Citation } from '../types';

interface Props {
  citations: Citation[];
  onOpenSource: (chunkId: string) => void;
}

export default function CitationList({ citations, onOpenSource }: Props) {
  if (citations.length === 0) return null;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>
        Citations ({citations.length})
      </div>
      {citations.map((c, i) => (
        <div key={c.chunk_id} className="card" style={{ padding: '10px 12px' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8 }}>
            <div style={{ flex: 1 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                <span style={{
                  background: 'var(--accent)', color: '#fff',
                  borderRadius: '50%', width: 18, height: 18,
                  display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                  fontSize: 10, fontWeight: 700, flexShrink: 0,
                }}>
                  {i + 1}
                </span>
                <span style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)' }}>
                  {c.doc_title}
                </span>
                <span style={{ fontSize: 10, color: 'var(--text-faint)', fontFamily: 'var(--font-mono)' }}>
                  #{c.chunk_id.slice(0, 8)}
                </span>
              </div>
              <p style={{
                fontSize: 12, color: 'var(--text-muted)',
                lineHeight: 1.5,
                fontStyle: 'italic',
                borderLeft: '2px solid var(--border)',
                paddingLeft: 8,
              }}>
                {c.snippet}
              </p>
            </div>
            <button
              onClick={() => onOpenSource(c.chunk_id)}
              className="btn btn-ghost btn-sm"
              style={{ flexShrink: 0, fontSize: 11 }}
              title="Open full source chunk"
            >
              Open source ↗
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
