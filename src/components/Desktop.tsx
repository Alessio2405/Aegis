import React from 'react';
import { useAppStore, APP_DEFS } from '../store';
import { AppScreen, Toast } from '../types';
import Window from './Window';
import Taskbar from './Taskbar';

// ── Toast overlay ──────────────────────────────────────────────────────────

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

// ── Desktop icon ───────────────────────────────────────────────────────────

function DesktopIcon({ screen, label, icon, badge }: {
  screen: AppScreen; label: string; icon: string; badge?: number;
}) {
  const openWindow = useAppStore((s) => s.openWindow);
  return (
    <button
      className="desktop-icon"
      onClick={() => openWindow(screen)}
      title={`Open ${label}`}
    >
      <div className="desktop-icon-emoji">
        {icon}
        {badge ? <span className="desktop-icon-badge">{badge}</span> : null}
      </div>
      <div className="desktop-icon-label">{label}</div>
    </button>
  );
}

// ── Desktop ────────────────────────────────────────────────────────────────

export default function Desktop() {
  const { windows, toasts, inboxCount } = useAppStore();

  return (
    <div className="desktop">
      {/* Wallpaper grid */}
      <div className="desktop-bg" />

      {/* App icons — two-column grid on the left */}
      <div className="desktop-icons">
        {APP_DEFS.map((def) => (
          <DesktopIcon
            key={def.screen}
            screen={def.screen}
            label={def.label}
            icon={def.icon}
            badge={def.screen === 'memory' && inboxCount > 0 ? inboxCount : undefined}
          />
        ))}
      </div>

      {/* Open windows */}
      {windows.map((win) => (
        <Window key={win.id} win={win} />
      ))}

      {/* Taskbar — fixed at bottom */}
      <Taskbar />

      {/* Toast notifications */}
      <ToastList toasts={toasts} />
    </div>
  );
}
