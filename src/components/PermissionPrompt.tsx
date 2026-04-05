import React from 'react';

interface Props {
  capability: string;
  message: string;
  onAllow: () => void;
  onDeny: () => void;
}

export default function PermissionPrompt({ capability, message, onAllow, onDeny }: Props) {
  return (
    <div style={{
      position: 'fixed', inset: 0,
      background: 'rgba(0,0,0,0.7)',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      zIndex: 2000,
    }}>
      <div className="card" style={{ width: 420, padding: 24 }}>
        <div style={{ fontSize: 18, marginBottom: 4 }}>🔐 Permission Required</div>
        <div style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 16 }}>
          {message}
        </div>
        <div style={{ marginBottom: 16, padding: '8px 12px', background: 'var(--bg-elevated)', borderRadius: 'var(--radius-sm)' }}>
          <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Capability: </span>
          <code style={{ color: 'var(--warning)' }}>{capability}</code>
        </div>
        <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
          <button className="btn btn-ghost" onClick={onDeny}>Deny</button>
          <button className="btn btn-primary" onClick={onAllow}>Allow</button>
        </div>
      </div>
    </div>
  );
}
