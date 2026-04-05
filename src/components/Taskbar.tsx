import React, { useEffect, useRef, useState } from 'react';
import { useAppStore, APP_DEFS } from '../store';
import * as api from '../api/tauri';

// ── Launcher (start-menu) popup ────────────────────────────────────────────

function Launcher({ onClose }: { onClose: () => void }) {
  const openWindow = useAppStore((s) => s.openWindow);
  const ref = useRef<HTMLDivElement>(null);

  // Close on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [onClose]);

  return (
    <div className="launcher-menu" ref={ref}>
      <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-muted)', padding: '4px 8px 8px', letterSpacing: '0.08em', textTransform: 'uppercase' }}>
        Apps
      </div>
      {APP_DEFS.map((def) => (
        <button
          key={def.screen}
          className="launcher-app-btn"
          onClick={() => { openWindow(def.screen); onClose(); }}
        >
          <span style={{ fontSize: 18 }}>{def.icon}</span>
          <span>{def.label}</span>
        </button>
      ))}
    </div>
  );
}

// ── Clock ──────────────────────────────────────────────────────────────────

function Clock() {
  const fmt = () =>
    new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  const [time, setTime] = useState(fmt);
  useEffect(() => {
    const id = setInterval(() => setTime(fmt()), 10_000);
    return () => clearInterval(id);
  }, []);
  return <span>{time}</span>;
}

// ── Taskbar ────────────────────────────────────────────────────────────────

export default function Taskbar() {
  const {
    windows, focusWindow, minimizeWindow,
    vaultStatus, inboxCount, addToast, refreshVaultStatus,
  } = useAppStore();

  const [launcherOpen, setLauncherOpen] = useState(false);

  const handleLock = async () => {
    await api.lockVault();
    addToast('Vault locked', 'info');
    await refreshVaultStatus(); // triggers App.tsx to switch to <Onboarding />
  };

  // Non-minimized windows in z-index order (lowest first, so we show them left-to-right by open order)
  const visibleWindows = [...windows].sort((a, b) => a.zIndex - b.zIndex);

  return (
    <div className="taskbar">
      {/* Launcher button */}
      <button
        className="taskbar-launcher"
        title="All apps"
        onClick={() => setLauncherOpen((v) => !v)}
      >
        🛡 Aegis
      </button>

      <div className="taskbar-sep" />

      {/* Running window buttons */}
      <div className="taskbar-apps">
        {visibleWindows.map((win) => {
          const isTop = win.zIndex === Math.max(...windows.map((w) => w.zIndex));
          return (
            <button
              key={win.id}
              className={`taskbar-btn${win.minimized ? ' minimized' : isTop ? ' active' : ''}`}
              title={win.title}
              onClick={() => {
                if (win.minimized) {
                  focusWindow(win.id);
                } else if (isTop) {
                  minimizeWindow(win.id);
                } else {
                  focusWindow(win.id);
                }
              }}
            >
              <span>{win.icon}</span>
              <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 90 }}>
                {win.title}
              </span>
            </button>
          );
        })}
      </div>

      {/* System tray */}
      <div className="taskbar-tray">
        {inboxCount > 0 && (
          <span
            style={{ color: 'var(--warning)', cursor: 'pointer', fontSize: 11 }}
            title={`${inboxCount} memory proposal${inboxCount > 1 ? 's' : ''}`}
            onClick={() => useAppStore.getState().openWindow('memory')}
          >
            🧠 {inboxCount}
          </span>
        )}

        <span
          style={{
            cursor: 'pointer',
            color: vaultStatus.unlocked ? 'var(--success)' : 'var(--danger)',
            fontSize: 12,
          }}
          title={vaultStatus.unlocked ? 'Vault unlocked — click to lock' : 'Vault locked'}
          onClick={vaultStatus.unlocked ? handleLock : undefined}
        >
          {vaultStatus.unlocked ? '🔓' : '🔒'}
        </span>

        <Clock />
      </div>

      {/* Launcher popup */}
      {launcherOpen && <Launcher onClose={() => setLauncherOpen(false)} />}
    </div>
  );
}
