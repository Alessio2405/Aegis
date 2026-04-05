import React, { useState } from 'react';
import { useAppStore } from '../store';
import * as api from '../api/tauri';

type Mode = 'unlock' | 'create';

export default function Onboarding() {
  const { vaultStatus, refreshVaultStatus, refreshWorkspaces, refreshSettings, refreshInboxCount, setScreen, openWindow, addToast } = useAppStore();
  const [mode, setMode] = useState<Mode>(vaultStatus.exists ? 'unlock' : 'create');
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (mode === 'create') {
      if (password.length < 8) { setError('Password must be at least 8 characters.'); return; }
      if (password !== confirmPassword) { setError('Passwords do not match.'); return; }
    }

    setLoading(true);
    try {
      if (mode === 'create') {
        await api.createVault(password);
        addToast('Vault created successfully', 'success');
      } else {
        await api.unlockVault(password);
        addToast('Vault unlocked', 'success');
      }
      await refreshVaultStatus();
      await refreshWorkspaces();
      await refreshSettings();
      await refreshInboxCount();
      setScreen('workspace');
      openWindow('chat');
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
      setPassword('');
      setConfirmPassword('');
    }
  };

  return (
    <div style={{
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      height: '100vh', background: 'var(--bg)',
    }}>
      <div style={{ width: 400 }}>
        {/* Logo / header */}
        <div style={{ textAlign: 'center', marginBottom: 32 }}>
          <div style={{ fontSize: 48, marginBottom: 8 }}>🛡</div>
          <h1 style={{ fontSize: 24, fontWeight: 700, color: 'var(--text)', letterSpacing: '-0.5px' }}>Aegis</h1>
          <p style={{ color: 'var(--text-muted)', fontSize: 13, marginTop: 4 }}>
            Local-first · Encrypted · AI Agent OS
          </p>
        </div>

        <div className="card" style={{ padding: 28 }}>
          {/* Mode switcher — only shown if vault exists */}
          {vaultStatus.exists && (
            <div style={{ display: 'flex', gap: 8, marginBottom: 20 }}>
              <button
                className={`btn ${mode === 'unlock' ? 'btn-primary' : 'btn-ghost'}`}
                style={{ flex: 1, justifyContent: 'center' }}
                onClick={() => { setMode('unlock'); setError(''); }}
              >
                Unlock Vault
              </button>
              <button
                className={`btn ${mode === 'create' ? 'btn-primary' : 'btn-ghost'}`}
                style={{ flex: 1, justifyContent: 'center' }}
                onClick={() => { setMode('create'); setError(''); }}
              >
                New Vault
              </button>
            </div>
          )}

          <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
            <h2 style={{ fontSize: 16, fontWeight: 600 }}>
              {mode === 'create' ? '🔑 Create Vault' : '🔓 Unlock Vault'}
            </h2>

            {mode === 'create' && (
              <p style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.5 }}>
                Your vault password is used to derive the encryption key. It is never stored.
                <strong style={{ color: 'var(--warning)' }}> There is no recovery if forgotten.</strong>
              </p>
            )}

            <div className="form-group">
              <label className="form-label" htmlFor="password">
                {mode === 'create' ? 'Vault Password (min 8 chars)' : 'Vault Password'}
              </label>
              <input
                id="password"
                className="form-input"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter password…"
                autoFocus
                required
              />
            </div>

            {mode === 'create' && (
              <div className="form-group">
                <label className="form-label" htmlFor="confirm">Confirm Password</label>
                <input
                  id="confirm"
                  className="form-input"
                  type="password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  placeholder="Confirm password…"
                  required
                />
              </div>
            )}

            {error && (
              <div style={{
                padding: '8px 12px',
                background: 'rgba(248,113,113,0.1)',
                border: '1px solid var(--danger)',
                borderRadius: 'var(--radius-sm)',
                color: 'var(--danger)',
                fontSize: 12,
              }}>
                {error}
              </div>
            )}

            <button
              type="submit"
              className="btn btn-primary"
              disabled={loading}
              style={{ justifyContent: 'center' }}
            >
              {loading
                ? <><span className="spinner" style={{ width: 14, height: 14 }} /> Working…</>
                : mode === 'create' ? 'Create Vault' : 'Unlock Vault'
              }
            </button>
          </form>
        </div>

        <p style={{ textAlign: 'center', fontSize: 11, color: 'var(--text-faint)', marginTop: 16 }}>
          Local-first · No telemetry · All data stored on your device
        </p>
      </div>
    </div>
  );
}
