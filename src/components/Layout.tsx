import React from 'react';
import { useAppStore } from '../store';
import { Screen, Toast } from '../types';
import * as api from '../api/tauri';

const NAV_ITEMS: { id: Screen; label: string; icon: string }[] = [
  { id: 'workspace',   label: 'Workspace',   icon: '🗂' },
  { id: 'qa',          label: 'Ask',         icon: '🔍' },
  { id: 'chat',        label: 'Chat',        icon: '💬' },
  { id: 'agents',      label: 'Agents',      icon: '🤖' },
  { id: 'memory',      label: 'Memory',      icon: '🧠' },
  { id: 'permissions', label: 'Permissions', icon: '🔐' },
  { id: 'audit',       label: 'Audit',       icon: '📋' },
  { id: 'settings',    label: 'Settings',    icon: '⚙' },
];

function ToastList({ toasts }: { toasts: Toast[] }) {
  const removeToast = useAppStore((s) => s.removeToast);
  return (
    <div className="toast-container">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`toast toast-${t.type}`}
          onClick={() => removeToast(t.id)}
          style={{ cursor: 'pointer' }}
        >
          {t.message}
        </div>
      ))}
    </div>
  );
}

export default function Layout({ children }: { children: React.ReactNode }) {
  const { screen, setScreen, vaultStatus, inboxCount, toasts, addToast } = useAppStore();

  const handleLock = async () => {
    await api.lockVault();
    setScreen('onboarding');
    addToast('Vault locked', 'info');
  };

  return (
    <div style={{ display: 'flex', height: '100vh', overflow: 'hidden' }}>
      {/* Sidebar */}
      <nav style={{
        width: 200,
        background: 'var(--bg-surface)',
        borderRight: '1px solid var(--border)',
        display: 'flex',
        flexDirection: 'column',
        padding: '16px 0',
        flexShrink: 0,
      }}>
        {/* Logo */}
        <div style={{ padding: '0 16px 16px', borderBottom: '1px solid var(--border)' }}>
          <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--accent)' }}>
            🔒 Agent OS
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 2 }}>
            {vaultStatus.unlocked ? '● Vault open' : '○ Vault locked'}
          </div>
        </div>

        {/* Nav items */}
        <div style={{ flex: 1, padding: '8px 8px', display: 'flex', flexDirection: 'column', gap: 2 }}>
          {NAV_ITEMS.map((item) => {
            const isActive = screen === item.id;
            const badge = item.id === 'memory' && inboxCount > 0 ? inboxCount : null;
            return (
              <button
                key={item.id}
                onClick={() => setScreen(item.id)}
                style={{
                  display: 'flex', alignItems: 'center', gap: 8,
                  padding: '7px 10px',
                  borderRadius: 'var(--radius)',
                  background: isActive ? 'var(--bg-elevated)' : 'transparent',
                  color: isActive ? 'var(--text)' : 'var(--text-muted)',
                  fontSize: 13, fontWeight: isActive ? 600 : 400,
                  transition: 'background 0.1s',
                  width: '100%', textAlign: 'left',
                }}
              >
                <span>{item.icon}</span>
                <span style={{ flex: 1 }}>{item.label}</span>
                {badge && <span className="badge">{badge}</span>}
              </button>
            );
          })}
        </div>

        {/* Lock button */}
        {vaultStatus.unlocked && (
          <div style={{ padding: '8px 8px', borderTop: '1px solid var(--border)' }}>
            <button
              onClick={handleLock}
              className="btn btn-ghost"
              style={{ width: '100%', justifyContent: 'center', fontSize: 12 }}
            >
              🔒 Lock Vault
            </button>
          </div>
        )}
      </nav>

      {/* Main content */}
      <main style={{ flex: 1, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
        {children}
      </main>

      <ToastList toasts={toasts} />
    </div>
  );
}
