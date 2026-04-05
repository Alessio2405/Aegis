import { create } from 'zustand';
import {
  VaultStatus, Workspace, Screen, AppScreen, WinState, Toast, AppSettings, Memory, Permission
} from '../types';
import * as api from '../api/tauri';

// ── App definitions ─────────────────────────────────────────────────────────
// Exported so Desktop and Taskbar can share the same list.

export interface AppDef {
  screen: AppScreen;
  label: string;
  icon: string;
  w: number;
  h: number;
}

export const APP_DEFS: AppDef[] = [
  // ── Primary ──────────────────────────────────────────────────────────────
  { screen: 'chat',        label: 'Chat',        icon: '💬',  w: 520, h: 620 },
  { screen: 'agents',      label: 'Agents',      icon: '🤖',  w: 860, h: 620 },
  { screen: 'qa',          label: 'Ask',         icon: '🔍',  w: 660, h: 560 },
  { screen: 'workspace',   label: 'Workspace',   icon: '🗂',  w: 820, h: 560 },
  { screen: 'memory',      label: 'Memory',      icon: '🧠',  w: 720, h: 560 },
  { screen: 'activity',    label: 'Activity',    icon: '📡',  w: 860, h: 620 },
  // ── Tools ────────────────────────────────────────────────────────────────
  { screen: 'browser',     label: 'Browser',     icon: '🌐',  w: 1100, h: 720 },
  { screen: 'ide',         label: 'Python IDE',  icon: '🐍',  w: 900, h: 650 },
  { screen: 'tasks',       label: 'Task Queue',  icon: '📬',  w: 800, h: 600 },
  // ── Admin ────────────────────────────────────────────────────────────────
  { screen: 'settings',    label: 'Settings',    icon: '⚙',   w: 660, h: 700 },
  { screen: 'permissions', label: 'Permissions', icon: '🔐',  w: 660, h: 500 },
  { screen: 'audit',       label: 'Audit',       icon: '📋',  w: 760, h: 560 },
];

const TASKBAR_H = 44;

// ── Store interface ─────────────────────────────────────────────────────────

interface AppStore {
  vaultStatus: VaultStatus;
  setVaultStatus: (s: VaultStatus) => void;
  refreshVaultStatus: () => Promise<void>;

  // Legacy navigation — kept for Onboarding compatibility
  screen: Screen;
  setScreen: (s: Screen) => void;

  activeWorkspaceId: string | null;
  setActiveWorkspaceId: (id: string | null) => void;

  workspaces: Workspace[];
  refreshWorkspaces: () => Promise<void>;

  settings: AppSettings | null;
  refreshSettings: () => Promise<void>;
  saveSettings: (s: AppSettings) => Promise<void>;

  inboxCount: number;
  refreshInboxCount: () => Promise<void>;

  permissions: Permission[];
  refreshPermissions: () => Promise<void>;

  toasts: Toast[];
  addToast: (msg: string, type?: Toast['type']) => void;
  removeToast: (id: string) => void;

  // ── Desktop window manager ───────────────────────────────────────────────

  windows: WinState[];
  nextZ: number;
  /** Open an app window. If already open, focus/unminimize it. */
  openWindow: (screen: AppScreen) => void;
  closeWindow: (id: string) => void;
  focusWindow: (id: string) => void;
  minimizeWindow: (id: string) => void;
  /** Toggle maximize / restore. */
  maximizeWindow: (id: string) => void;
  moveWindow: (id: string, x: number, y: number) => void;
  resizeWindow: (id: string, w: number, h: number) => void;
  /** Set x, y, width, height atomically (needed for resize handles that move the origin). */
  placeWindow: (id: string, x: number, y: number, w: number, h: number) => void;
}

// ── Store implementation ────────────────────────────────────────────────────

export const useAppStore = create<AppStore>((set, get) => ({
  vaultStatus: { unlocked: false, exists: false },
  setVaultStatus: (s) => set({ vaultStatus: s }),
  refreshVaultStatus: async () => {
    try {
      const s = await api.getVaultStatus();
      set({ vaultStatus: s });
      if (!s.exists || !s.unlocked) {
        set({ screen: 'onboarding' });
      }
    } catch (e) {
      console.error('Failed to get vault status', e);
    }
  },

  screen: 'onboarding',
  setScreen: (screen) => set({ screen }),

  activeWorkspaceId: null,
  setActiveWorkspaceId: (id) => set({ activeWorkspaceId: id }),

  workspaces: [],
  refreshWorkspaces: async () => {
    try {
      const ws = await api.listWorkspaces();
      set({ workspaces: ws });
      if (ws.length > 0 && !get().activeWorkspaceId) {
        set({ activeWorkspaceId: ws[0].id });
      }
    } catch (e) {
      console.error('Failed to list workspaces', e);
    }
  },

  settings: null,
  refreshSettings: async () => {
    try {
      const s = await api.getSettings();
      set({ settings: s });
    } catch (e) {
      console.error('Failed to get settings', e);
    }
  },
  saveSettings: async (s) => {
    await api.updateSettings(s);
    set({ settings: s });
    get().addToast('Settings saved', 'success');
  },

  inboxCount: 0,
  refreshInboxCount: async () => {
    try {
      const mems = await api.listMemories('proposed');
      set({ inboxCount: mems.length });
    } catch {
      set({ inboxCount: 0 });
    }
  },

  permissions: [],
  refreshPermissions: async () => {
    try {
      const p = await api.listPermissions();
      set({ permissions: p });
    } catch (e) {
      console.error('Failed to list permissions', e);
    }
  },

  toasts: [],
  addToast: (message, type = 'info') => {
    const id = Math.random().toString(36).slice(2);
    set((s) => ({ toasts: [...s.toasts, { id, message, type }] }));
    setTimeout(() => get().removeToast(id), 4000);
  },
  removeToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  // ── Window manager ─────────────────────────────────────────────────────

  windows: [],
  nextZ: 100,

  openWindow: (screen) => {
    const { windows, nextZ } = get();
    const def = APP_DEFS.find((a) => a.screen === screen)!;

    // If already open (even minimized), focus / unminimize
    const existing = windows.find((w) => w.screen === screen);
    if (existing) {
      set({
        windows: windows.map((w) =>
          w.id === existing.id ? { ...w, minimized: false, zIndex: nextZ } : w
        ),
        nextZ: nextZ + 1,
      });
      return;
    }

    // Cascade from center
    const open = windows.filter((w) => !w.minimized).length;
    const offset = (open % 10) * 28;
    const dw = window.innerWidth;
    const dh = window.innerHeight - TASKBAR_H;
    const x = Math.round(Math.max(20, Math.min((dw - def.w) / 2 + offset, dw - def.w - 20)));
    const y = Math.round(Math.max(20, Math.min((dh - def.h) / 2 + offset, dh - def.h - 20)));

    const win: WinState = {
      id: Math.random().toString(36).slice(2),
      screen,
      title: def.label,
      icon: def.icon,
      x, y,
      width: def.w,
      height: def.h,
      minimized: false,
      maximized: false,
      zIndex: nextZ,
      restoreRect: null,
    };
    set({ windows: [...windows, win], nextZ: nextZ + 1 });
  },

  closeWindow: (id) =>
    set((s) => ({ windows: s.windows.filter((w) => w.id !== id) })),

  focusWindow: (id) => {
    const { windows, nextZ } = get();
    const win = windows.find((w) => w.id === id);
    if (!win) return;
    // Always unminimize + bring to front, even if already at top z-index
    if (win.zIndex === nextZ - 1 && !win.minimized) return;
    set({
      windows: windows.map((w) => (w.id === id ? { ...w, minimized: false, zIndex: nextZ } : w)),
      nextZ: nextZ + 1,
    });
  },

  minimizeWindow: (id) =>
    set((s) => ({
      windows: s.windows.map((w) => (w.id === id ? { ...w, minimized: true } : w)),
    })),

  maximizeWindow: (id) => {
    const { windows } = get();
    const win = windows.find((w) => w.id === id);
    if (!win) return;
    if (win.maximized) {
      const r = win.restoreRect ?? { x: 80, y: 60, w: win.width, h: win.height };
      set({
        windows: windows.map((w) =>
          w.id === id
            ? { ...w, maximized: false, x: r.x, y: r.y, width: r.w, height: r.h, restoreRect: null }
            : w
        ),
      });
    } else {
      set({
        windows: windows.map((w) =>
          w.id === id
            ? { ...w, maximized: true, restoreRect: { x: w.x, y: w.y, w: w.width, h: w.height } }
            : w
        ),
      });
    }
  },

  moveWindow: (id, x, y) =>
    set((s) => ({ windows: s.windows.map((w) => (w.id === id ? { ...w, x, y } : w)) })),

  resizeWindow: (id, width, height) =>
    set((s) => ({ windows: s.windows.map((w) => (w.id === id ? { ...w, width, height } : w)) })),

  placeWindow: (id, x, y, width, height) =>
    set((s) => ({ windows: s.windows.map((w) => (w.id === id ? { ...w, x, y, width, height } : w)) })),
}));
