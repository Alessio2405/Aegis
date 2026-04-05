import React, { useRef, useState } from 'react';
import { useAppStore } from '../store';
import { WinState, AppScreen } from '../types';

// Screen components
import Workspace from '../screens/Workspace';
import QA from '../screens/QA';
import Chat from '../screens/Chat';
import Agents from '../screens/Agents';
import MemoryInbox from '../screens/MemoryInbox';
import Permissions from '../screens/Permissions';
import Audit from '../screens/Audit';
import Settings from '../screens/Settings';
import IDE from '../screens/IDE';
import Browser from '../screens/Browser';
import Activity from '../screens/Activity';
import Tasks from '../screens/Tasks';

const TASKBAR_H = 44;
const MIN_W = 320;
const MIN_H = 220;

function renderScreen(screen: AppScreen) {
  switch (screen) {
    case 'workspace':   return <Workspace />;
    case 'qa':          return <QA />;
    case 'chat':        return <Chat />;
    case 'agents':      return <Agents />;
    case 'memory':      return <MemoryInbox />;
    case 'permissions': return <Permissions />;
    case 'audit':       return <Audit />;
    case 'settings':    return <Settings />;
    case 'ide':         return <IDE />;
    case 'browser':     return <Browser />;
    case 'activity':    return <Activity />;
    case 'tasks':       return <Tasks />;
  }
}

// ── Resize direction ──────────────────────────────────────────────────────────

type ResizeDir = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';

interface DragRef {
  pointerId: number;
  startX: number;
  startY: number;
  origX: number;
  origY: number;
}

interface ResizeRef {
  pointerId: number;
  dir: ResizeDir;
  startX: number;
  startY: number;
  origX: number;
  origY: number;
  origW: number;
  origH: number;
}

function applyResize(
  dir: ResizeDir,
  orig: { x: number; y: number; w: number; h: number },
  dx: number,
  dy: number,
) {
  let { x, y, w, h } = orig;

  if (dir.includes('n')) {
    const newH = Math.max(MIN_H, h - dy);
    y = y + h - newH;
    h = newH;
  }
  if (dir.includes('s')) h = Math.max(MIN_H, h + dy);
  if (dir.includes('w')) {
    const newW = Math.max(MIN_W, w - dx);
    x = x + w - newW;
    w = newW;
  }
  if (dir.includes('e')) w = Math.max(MIN_W, w + dx);

  // Clamp position
  x = Math.max(0, x);
  y = Math.max(0, y);

  return { x, y, w, h };
}

// ── Window component ──────────────────────────────────────────────────────────

export default function Window({ win }: { win: WinState }) {
  const {
    focusWindow, closeWindow, minimizeWindow, maximizeWindow,
    moveWindow, placeWindow,
  } = useAppStore();

  const dragRef   = useRef<DragRef | null>(null);
  const resizeRef = useRef<ResizeRef | null>(null);
  const [snapPreview, setSnapPreview] = useState(false);

  // ── Title-bar drag ──────────────────────────────────────────────────────

  const onTitleDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (win.maximized) return;
    if ((e.target as HTMLElement).closest('[data-wbtn]')) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = {
      pointerId: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      origX: win.x,
      origY: win.y,
    };
  };

  const onTitleMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = dragRef.current;
    if (!d || e.pointerId !== d.pointerId) return;

    const atTop = e.clientY <= 8;
    setSnapPreview(atTop);

    if (!atTop) {
      const maxX = window.innerWidth - win.width;
      const maxY = window.innerHeight - TASKBAR_H - 36;
      const nx = Math.max(0, Math.min(d.origX + e.clientX - d.startX, maxX));
      const ny = Math.max(0, Math.min(d.origY + e.clientY - d.startY, maxY));
      moveWindow(win.id, nx, ny);
    }
  };

  const onTitleUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.pointerId !== e.pointerId) return;
    if (snapPreview) maximizeWindow(win.id);
    dragRef.current = null;
    setSnapPreview(false);
  };

  const onTitleDblClick = () => maximizeWindow(win.id);

  // ── Resize handles ──────────────────────────────────────────────────────

  const onResizeDown = (dir: ResizeDir) => (e: React.PointerEvent<HTMLDivElement>) => {
    if (win.maximized) return;
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    resizeRef.current = {
      pointerId: e.pointerId,
      dir,
      startX: e.clientX,
      startY: e.clientY,
      origX: win.x,
      origY: win.y,
      origW: win.width,
      origH: win.height,
    };
  };

  const onResizeMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const r = resizeRef.current;
    if (!r || e.pointerId !== r.pointerId) return;
    const dx = e.clientX - r.startX;
    const dy = e.clientY - r.startY;
    const { x, y, w, h } = applyResize(r.dir, { x: r.origX, y: r.origY, w: r.origW, h: r.origH }, dx, dy);
    placeWindow(win.id, x, y, w, h);
  };

  const onResizeUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (resizeRef.current?.pointerId === e.pointerId) resizeRef.current = null;
  };

  // ── Window style ────────────────────────────────────────────────────────

  const style: React.CSSProperties = win.maximized
    ? {
        position: 'absolute',
        left: 0,
        top: 0,
        width: '100%',
        height: `calc(100% - ${TASKBAR_H}px)`,
        borderRadius: 0,
        border: 'none',
        zIndex: win.zIndex,
      }
    : {
        position: 'absolute',
        left: win.x,
        top: win.y,
        width: win.width,
        height: win.height,
        zIndex: win.zIndex,
      };

  if (win.minimized) return null;

  const DIRS: ResizeDir[] = ['n', 'ne', 'e', 'se', 's', 'sw', 'w', 'nw'];

  return (
    <>
      {/* Aero snap preview */}
      {snapPreview && (
        <div style={{
          position: 'fixed',
          inset: 0,
          bottom: TASKBAR_H,
          background: 'rgba(99,102,241,0.18)',
          border: '2px solid rgba(99,102,241,0.55)',
          borderRadius: 4,
          pointerEvents: 'none',
          zIndex: 99998,
          transition: 'opacity 0.1s',
        }} />
      )}

      <div
        className={`window${win.maximized ? ' window-maximized' : ''}`}
        style={style}
        onPointerDown={() => focusWindow(win.id)}
      >
        {/* Title bar */}
        <div
          className="window-titlebar"
          onPointerDown={onTitleDown}
          onPointerMove={onTitleMove}
          onPointerUp={onTitleUp}
          onDoubleClick={onTitleDblClick}
        >
          <span className="window-icon">{win.icon}</span>
          <span className="window-title">{win.title}</span>

          {/* Windows-style controls — right side */}
          <div className="win-btns" data-wbtn="true">
            <button
              className="win-btn win-btn-min"
              title="Minimize"
              data-wbtn="true"
              onClick={(e) => { e.stopPropagation(); minimizeWindow(win.id); }}
            >
              <span style={{ display: 'block', width: 10, height: 1, background: 'currentColor', marginTop: 6 }} />
            </button>
            <button
              className="win-btn win-btn-max"
              title={win.maximized ? 'Restore' : 'Maximize'}
              data-wbtn="true"
              onClick={(e) => { e.stopPropagation(); maximizeWindow(win.id); }}
            >
              {win.maximized
                ? <RestoreIcon />
                : <MaximizeIcon />}
            </button>
            <button
              className="win-btn win-btn-close"
              title="Close"
              data-wbtn="true"
              onClick={(e) => { e.stopPropagation(); closeWindow(win.id); }}
            >
              ✕
            </button>
          </div>
        </div>

        {/* Screen content */}
        <div className="window-content">
          {renderScreen(win.screen)}
        </div>

        {/* 8-directional resize handles — hidden when maximized */}
        {!win.maximized && DIRS.map((dir) => (
          <div
            key={dir}
            className={`window-resize-${dir}`}
            onPointerDown={onResizeDown(dir)}
            onPointerMove={onResizeMove}
            onPointerUp={onResizeUp}
          />
        ))}
      </div>
    </>
  );
}

// ── Maximize / Restore icons ──────────────────────────────────────────────────

function MaximizeIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" style={{ display: 'block' }}>
      <rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

function RestoreIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" style={{ display: 'block' }}>
      {/* back window */}
      <rect x="2" y="0" width="8" height="8" fill="none" stroke="currentColor" strokeWidth="1" />
      {/* front window */}
      <rect x="0" y="2" width="8" height="8" fill="var(--bg-surface, #1e2333)" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}
