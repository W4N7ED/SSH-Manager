import React, { useState, useCallback, useEffect, useMemo, useRef } from 'react';
import { Connection, ConnectionType, GroupFilter, Tab, Quadrant } from './types';
import GroupRail from './components/GroupRail';
import TerminalTab, { TerminalTabHandle } from './components/TerminalTab';
import FileBrowser from './components/FileBrowser';
import ConnectionForm from './components/ConnectionForm';
import ConnectionList from './components/ConnectionList';
import PasswordPrompt from './components/PasswordPrompt';
import TiledLayout from './components/TiledLayout';
import SetupWizard from './components/SetupWizard';
import CommandPalette from './components/CommandPalette';
import StatusBar from './components/StatusBar';
import Toolbox from './components/Toolbox';
import UnifiedBar from './components/UnifiedBar';
import {
  TerminalThemeConfig, loadTerminalTheme, TERMINAL_THEME_KEY,
} from './terminal-themes';

declare global {
  interface Window {
    electronAPI: {
      appGetVersion: () => Promise<string>;
      clipboardCopy: (p: { text: string; autoClear: boolean; delaySeconds: number }) => Promise<{ autoClear: boolean; delaySeconds?: number; privateMode: boolean }>;
      windowMinimize: () => void;
      windowMaximize: () => void;
      windowClose: () => void;
      cryptoGetSetupStatus: () => Promise<{ initialized: boolean; portable: boolean }>;
      cryptoGenerateMnemonic: () => Promise<string>;
      cryptoCompleteSetup: (mnemonic: string) => Promise<{ success: boolean; error?: string }>;
      cryptoSkipSetup: () => Promise<{ success: boolean; error?: string }>;
      cryptoVerifyMnemonic: (mnemonic: string) => Promise<{ valid: boolean }>;
      connectionsGetAll: () => Promise<Connection[]>;
      connectionsSave: (c: Connection[]) => Promise<boolean>;
      connectionsExport: (d: { connections: Connection[]; groups: string[] }) => Promise<{ success: boolean; canceled?: boolean; path?: string; error?: string }>;
      connectionsImport: () => Promise<{ success: boolean; canceled?: boolean; requiresMnemonic?: boolean; payload?: unknown; connections?: Connection[]; groups?: string[]; error?: string }>;
      connectionsImportDecrypt: (p: { mnemonic: string; payload: unknown }) => Promise<{ success: boolean; connections?: Connection[]; groups?: string[]; error?: string }>;
      sshConnect: (p: { tabId: string; connection: Connection; cols: number; rows: number }) => Promise<{ success: boolean }>;
      sshWrite: (p: { tabId: string; data: string }) => void;
      sshResize: (p: { tabId: string; cols: number; rows: number }) => void;
      sshDisconnect: (tabId: string) => void;
      sshOpenInPowerShell: (c: Connection) => void;
      sshOpenInCmd: (c: Connection) => void;
      sshForgetHostKey: (p: { host: string; port: number }) => Promise<{ success: boolean }>;
      sshGetHostKeyFingerprint: (p: { host: string; port: number }) => Promise<string | null>;
      sshGenerateKey: (p: { name: string; type: 'ed25519' | 'rsa' }) => Promise<{ success: boolean; privateKeyPath?: string; publicKey?: string; error?: string }>;
      onSshData: (tabId: string, cb: (data: string | Uint8Array) => void) => () => void;
      onSshClose: (tabId: string, cb: () => void) => () => void;
      sftpConnect: (p: { tabId: string; connection: Connection }) => Promise<{ success: boolean; error?: string }>;
      sftpList: (p: { tabId: string; remotePath: string }) => Promise<{ success: boolean; list?: FileEntry[]; error?: string }>;
      sftpMkdir: (p: { tabId: string; remotePath: string }) => Promise<{ success: boolean; error?: string }>;
      sftpDelete: (p: { tabId: string; remotePath: string; isDir: boolean }) => Promise<{ success: boolean; error?: string }>;
      sftpDownload: (p: { tabId: string; remotePath: string }) => Promise<{ success: boolean; error?: string }>;
      sftpUpload: (p: { tabId: string; remotePath: string }) => Promise<{ success: boolean; error?: string }>;
      sftpDisconnect: (tabId: string) => void;
      ftpConnect: (p: { tabId: string; connection: Connection }) => Promise<{ success: boolean; error?: string }>;
      ftpList: (p: { tabId: string; remotePath: string }) => Promise<{ success: boolean; list?: FileEntry[]; error?: string }>;
      ftpMkdir: (p: { tabId: string; remotePath: string }) => Promise<{ success: boolean; error?: string }>;
      ftpDelete: (p: { tabId: string; remotePath: string; isDir: boolean }) => Promise<{ success: boolean; error?: string }>;
      ftpDownload: (p: { tabId: string; remotePath: string }) => Promise<{ success: boolean; error?: string }>;
      ftpUpload: (p: { tabId: string; remotePath: string }) => Promise<{ success: boolean; error?: string }>;
      ftpDisconnect: (tabId: string) => void;
      pickKeyFile: () => Promise<string | null>;
    };
  }
  interface FileEntry {
    name: string;
    type: string;
    size: number;
    modifyTime?: number;
    /** SFTP: permissions already formatted as rwx triplets. */
    rights?: { user: string; group: string; other: string };
    /** FTP: permissions as octal-style bitfields. */
    permissions?: { user: number; group: number; world: number };
  }
}

const GROUPS_KEY         = 'sshmanager-groups';
const GROUP_COLORS_KEY   = 'sshmanager-group-colors';
const RAIL_COLLAPSED_KEY = 'sshmanager-rail-collapsed';

const QUADRANTS: Quadrant[] = ['topLeft', 'topRight', 'bottomLeft', 'bottomRight'];

let tabCounter = 0;
// Includes a timestamp so a session id can never collide with one left over in
// a previously-saved store (defense in depth alongside persistConnections).
const newTabId  = () => 'tab-' + Date.now().toString(36) + '-' + (++tabCounter);
const newConnId = () => 'conn-' + Date.now() + '-' + Math.random().toString(36).slice(2, 7);

// Active tab sessions live in the same `connections` array with a transient
// 'tab-*' id. They must NEVER be written to disk: persisting them, combined with
// tabCounter resetting on restart, causes id collisions where opening a saved
// connection resolves to a stale ghost session. Always strip them before saving.
const isSessionId = (id: string) => id.startsWith('tab-');
const persistConnections = (list: Connection[]) =>
  window.electronAPI.connectionsSave(list.filter(c => !isSessionId(c.id)));

const loadGroups       = (): string[]               => { try { return JSON.parse(localStorage.getItem(GROUPS_KEY)       ?? '[]'); } catch { return []; } };
const loadGroupColors  = (): Record<string, string> => { try { return JSON.parse(localStorage.getItem(GROUP_COLORS_KEY) ?? '{}'); } catch { return {}; } };
const loadRailCollapsed = (): boolean               => localStorage.getItem(RAIL_COLLAPSED_KEY) === 'true';

const saveGroupsLS      = (g: string[])               => localStorage.setItem(GROUPS_KEY,       JSON.stringify(g));
const saveGroupColorsLS = (c: Record<string, string>) => localStorage.setItem(GROUP_COLORS_KEY, JSON.stringify(c));

function needsPasswordPrompt(conn: Connection): boolean {
  if (conn.privateKeyPath) return false;
  if (conn.password) return false;
  return true;
}

interface ContextMenu { x: number; y: number; tabId: string; }
interface MnemonicImportState { payload: unknown }

export default function App() {
  // ── Setup / encryption ────────────────────────────────────
  const [setupReady, setSetupReady] = useState<boolean | null>(null); // null = loading

  // ── Main state ────────────────────────────────────────────
  const [connections, setConnections]   = useState<Connection[]>([]);
  const [groups, setGroups]             = useState<string[]>(loadGroups);
  const [groupColors, setGroupColors]   = useState<Record<string, string>>(loadGroupColors);
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [editingConn, setEditingConn] = useState<Connection | null>(null);
  const [formDefaultGroup, setFormDefaultGroup] = useState<string | undefined>();
  const [toast, setToast] = useState<{ msg: string; ok: boolean } | null>(null);
  const [pwPrompt, setPwPrompt] = useState<{ conn: Connection; isEphemeral: boolean } | null>(null);
  const [isPortable, setIsPortable] = useState(false);
  const [appVersion, setAppVersion] = useState<string | null>(null);

  // ── Dashboard filters ─────────────────────────────────────
  const [typeFilter, setTypeFilter]     = useState<'all' | ConnectionType>('all');
  const [search, setSearch]             = useState('');
  const [groupFilter, setGroupFilter]   = useState<GroupFilter>({ kind: 'all' });
  const [railCollapsed, setRailCollapsed] = useState<boolean>(loadRailCollapsed);
  /** Ce que montre l'espace permanent, quand aucune session n'est au premier plan. */
  const [dashboardView, setDashboardView] = useState<'connections' | 'toolbox'>('connections');
  const [paletteOpen, setPaletteOpen]   = useState(false);
  const [terminalTheme, setTerminalTheme] = useState<TerminalThemeConfig>(loadTerminalTheme);

  const handleThemeChange = useCallback((theme: TerminalThemeConfig) => {
    setTerminalTheme(theme);
    localStorage.setItem(TERMINAL_THEME_KEY, JSON.stringify(theme));
  }, []);

  // ── Import mnemonic prompt ────────────────────────────────
  const [mnemonicImport, setMnemonicImport] = useState<MnemonicImportState | null>(null);
  const [mnemonicInput, setMnemonicInput] = useState('');
  const [mnemonicError, setMnemonicError] = useState('');

  // ── Tiling ───────────────────────────────────────────────
  const [tiledTabs, setTiledTabs] = useState<Partial<Record<Quadrant, string>>>({});
  const [draggingTabId, setDraggingTabId] = useState<string | null>(null);
  const isTiledMode = Object.keys(tiledTabs).length > 0;

  // ── Sync input ────────────────────────────────────────────
  const [syncedTabIds, setSyncedTabIds] = useState<Set<string>>(new Set());
  const tabRefsMap = useRef<Map<string, React.RefObject<TerminalTabHandle>>>(new Map());

  // ── Tab origin tracking (for retry) ──────────────────────
  const tabOrigins = useRef<Map<string, Connection>>(new Map());

  // ── Context menu ─────────────────────────────────────────
  const [contextMenu, setContextMenu] = useState<ContextMenu | null>(null);

  const getTabRef = (tabId: string) => {
    if (!tabRefsMap.current.has(tabId)) {
      tabRefsMap.current.set(tabId, React.createRef<TerminalTabHandle>());
    }
    return tabRefsMap.current.get(tabId)!;
  };

  // ── Init: check encryption setup ─────────────────────────
  useEffect(() => {
    window.electronAPI.cryptoGetSetupStatus().then(({ initialized, portable }) => {
      setIsPortable(portable);
      setSetupReady(initialized);
      if (initialized) loadConnections();
    });
    window.electronAPI.appGetVersion().then(setAppVersion);
    const closeMenu = () => setContextMenu(null);
    document.addEventListener('click', closeMenu);
    return () => document.removeEventListener('click', closeMenu);
  }, []);

  const loadConnections = () => {
    window.electronAPI.connectionsGetAll().then(loaded => {
      // Repair stores polluted by an earlier bug that persisted tab sessions:
      // drop any leftover 'tab-*' entries and rewrite the cleaned list to disk.
      const clean = loaded.filter((c: Connection) => !isSessionId(c.id));
      if (clean.length !== loaded.length) {
        window.electronAPI.connectionsSave(clean);
      }
      setConnections(clean);
      const stored = loadGroups();
      const fromConns = clean.map((c: Connection) => c.group).filter(Boolean) as string[];
      const merged = Array.from(new Set([...stored, ...fromConns]));
      setGroups(merged);
      saveGroupsLS(merged);
    });
  };

  const handleSetupComplete = () => {
    setSetupReady(true);
    loadConnections();
  };

  const showToast = (msg: string, ok = true) => {
    setToast({ msg, ok });
    setTimeout(() => setToast(null), 3500);
  };

  // ── Groups ───────────────────────────────────────────────
  const handleCreateGroup = useCallback((name: string) => {
    setGroups(prev => {
      if (prev.includes(name)) return prev;
      const next = [...prev, name];
      saveGroupsLS(next);
      return next;
    });
  }, []);

  const handleRenameGroup = useCallback((oldName: string, newName: string) => {
    setGroups(prev => {
      if (prev.includes(newName)) return prev;
      const next = prev.map(g => g === oldName ? newName : g);
      saveGroupsLS(next);
      return next;
    });
    // Migrate the color to the new name
    setGroupColors(prev => {
      if (!prev[oldName]) return prev;
      const next = { ...prev, [newName]: prev[oldName] };
      delete next[oldName];
      saveGroupColorsLS(next);
      return next;
    });
    setGroupFilter(prev =>
      prev.kind === 'group' && prev.name === oldName ? { kind: 'group', name: newName } : prev);
    setConnections(prev => {
      const next = prev.map(c => c.group === oldName ? { ...c, group: newName } : c);
      persistConnections(next);
      return next;
    });
  }, []);

  const handleSetGroupColor = useCallback((groupName: string, color: string) => {
    setGroupColors(prev => {
      const next = { ...prev };
      if (color) next[groupName] = color;
      else delete next[groupName];
      saveGroupColorsLS(next);
      return next;
    });
  }, []);

  const handleDeleteGroup = useCallback((name: string) => {
    setGroups(prev => { const next = prev.filter(g => g !== name); saveGroupsLS(next); return next; });
    setGroupColors(prev => { const next = { ...prev }; delete next[name]; saveGroupColorsLS(next); return next; });
    setConnections(prev => {
      const next = prev.map(c => c.group === name ? { ...c, group: undefined } : c);
      persistConnections(next);
      return next;
    });
  }, []);

  // ── Connections CRUD ──────────────────────────────────────
  const handleSaveConnection = useCallback((conn: Connection) => {
    if (conn.group) {
      setGroups(prev => {
        if (prev.includes(conn.group!)) return prev;
        const next = [...prev, conn.group!];
        saveGroupsLS(next);
        return next;
      });
    }
    setConnections(prev => {
      const exists = prev.find(c => c.id === conn.id);
      const next = exists ? prev.map(c => c.id === conn.id ? conn : c) : [...prev, conn];
      persistConnections(next);
      return next;
    });
    setFormOpen(false);
    setEditingConn(null);
    setFormDefaultGroup(undefined);
  }, []);

  const handleDeleteConnection = useCallback((id: string) => {
    setConnections(prev => {
      const removed = prev.find(c => c.id === id);
      // Forget the pinned SSH host key so a future server with the same address
      // can re-pin cleanly (TOFU reset).
      if (removed && (removed.type === 'ssh' || removed.type === 'sftp')) {
        window.electronAPI.sshForgetHostKey({ host: removed.host, port: removed.port || 22 });
      }
      const next = prev.filter(c => c.id !== id);
      persistConnections(next);
      return next;
    });
  }, []);

  const handleToggleFavorite = useCallback((id: string) => {
    setConnections(prev => {
      const next = prev.map(c => c.id === id ? { ...c, favorite: !c.favorite } : c);
      persistConnections(next);
      return next;
    });
  }, []);

  const handleSetConnectionColor = useCallback((id: string, color: string) => {
    setConnections(prev => {
      const next = prev.map(c => c.id === id ? { ...c, color: color || undefined } : c);
      persistConnections(next);
      return next;
    });
  }, []);

  // ── Export ────────────────────────────────────────────────
  const handleExport = useCallback(async () => {
    const result = await window.electronAPI.connectionsExport({ connections, groups });
    if (result.success) showToast('Exporté : ' + result.path);
    else if (!result.canceled) showToast(result.error ?? 'Erreur export', false);
  }, [connections, groups]);

  // ── Import (with optional mnemonic decrypt) ───────────────
  const handleImport = useCallback(async () => {
    const result = await window.electronAPI.connectionsImport();
    if (!result.success) {
      if (result.requiresMnemonic) {
        setMnemonicImport({ payload: result.payload });
        return;
      }
      if (!result.canceled) showToast(result.error ?? 'Erreur import', false);
      return;
    }
    doImport(result.connections ?? [], result.groups ?? []);
  }, []);

  const handleMnemonicImportConfirm = useCallback(async () => {
    if (!mnemonicImport) return;
    const result = await window.electronAPI.connectionsImportDecrypt({
      mnemonic: mnemonicInput.trim(),
      payload: mnemonicImport.payload,
    });
    if (!result.success) {
      setMnemonicError(result.error ?? 'Erreur de déchiffrement');
      return;
    }
    setMnemonicImport(null);
    setMnemonicInput('');
    setMnemonicError('');
    doImport(result.connections ?? [], result.groups ?? []);
  }, [mnemonicImport, mnemonicInput]);

  const doImport = (incoming: Connection[], incomingGroups: string[]) => {
    setConnections(prev => {
      const existingIds = new Set(prev.map(c => c.id));
      const toAdd = incoming.filter(c => !existingIds.has(c.id));
      const next = [...prev, ...toAdd];
      persistConnections(next);
      showToast(`${toAdd.length} connexion(s) importée(s)`);
      return next;
    });
    setGroups(prev => {
      const merged = Array.from(new Set([...prev, ...incomingGroups]));
      saveGroupsLS(merged);
      return merged;
    });
  };

  // ── Open tab ──────────────────────────────────────────────
  const openTab = useCallback((conn: Connection, isEphemeral = false) => {
    const tabId = newTabId();
    const isFile = conn.type === 'ftp' || conn.type === 'sftp';
    const sessionConn: Connection = { ...conn, id: tabId };
    const openedAt = Date.now();
    tabOrigins.current.set(tabId, conn);

    setConnections(prev => {
      // Stamp the saved entry so the list can sort and display "last access".
      const stamped = !isEphemeral && !isSessionId(conn.id)
        ? prev.map(c => c.id === conn.id ? { ...c, lastUsedAt: openedAt } : c)
        : prev;
      const next = [...stamped, sessionConn];
      if (stamped !== prev) persistConnections(next);
      return next;
    });

    setTabs(prev => [...prev, {
      id: tabId, connectionId: tabId,
      connectionName: conn.name || conn.host,
      type: isFile ? 'filebrowser' : 'terminal',
      connectionType: conn.type, status: 'connecting',
    }]);
    setActiveTabId(tabId);
  }, []);

  const handleConnect = useCallback((conn: Connection) => {
    if (needsPasswordPrompt(conn)) setPwPrompt({ conn, isEphemeral: false });
    else openTab(conn);
  }, [openTab]);

  /** Opens a saved connection over another protocol (SSH terminal ⇄ SFTP files). */
  const handleOpenAs = useCallback((conn: Connection, type: ConnectionType) => {
    handleConnect({ ...conn, type });
  }, [handleConnect]);

  const handleQuickConnect = useCallback((conn: Connection) => {
    if (needsPasswordPrompt(conn)) setPwPrompt({ conn, isEphemeral: true });
    else openTab(conn, true);
  }, [openTab]);

  const handlePwConfirm = useCallback((password: string, save: boolean) => {
    if (!pwPrompt) return;
    const { conn, isEphemeral } = pwPrompt;
    const connWithPw: Connection = { ...conn, password };
    if (save && !isEphemeral && conn.id && !isSessionId(conn.id)) {
      setConnections(prev => {
        const exists = prev.find(c => c.id === conn.id);
        if (!exists) return prev;
        const next = prev.map(c => c.id === conn.id ? { ...c, password } : c);
        persistConnections(next);
        return next;
      });
    }
    setPwPrompt(null);
    openTab(connWithPw, isEphemeral);
  }, [pwPrompt, openTab]);

  // ── Retry (after auth failure) ────────────────────────────
  const handleRetry = useCallback((tabId: string) => {
    const origin = tabOrigins.current.get(tabId);
    handleCloseTab(tabId);
    if (origin) {
      const connWithoutPw = { ...origin, password: undefined };
      setPwPrompt({ conn: connWithoutPw, isEphemeral: !origin.id || isSessionId(origin.id) });
    }
  }, []);

  const handleSaveQuickConn = useCallback((conn: Connection) => {
    handleSaveConnection({ ...conn, id: newConnId() });
  }, [handleSaveConnection]);

  // ── Tab status / close ────────────────────────────────────
  const handleTabStatusChange = useCallback((tabId: string, status: Tab['status'], error?: string) => {
    setTabs(prev => prev.map(t => t.id === tabId ? { ...t, status, error } : t));
  }, []);

  const handleCloseTab = useCallback((tabId: string) => {
    tabOrigins.current.delete(tabId);
    tabRefsMap.current.delete(tabId);
    setConnections(prev => prev.filter(c => c.id !== tabId));
    setTiledTabs(prev => {
      const next = { ...prev };
      (Object.keys(next) as Quadrant[]).forEach(q => { if (next[q] === tabId) delete next[q]; });
      return next;
    });
    setSyncedTabIds(prev => { const next = new Set(prev); next.delete(tabId); return next; });
    setTabs(prev => {
      const remaining = prev.filter(t => t.id !== tabId);
      if (activeTabId === tabId)
        setActiveTabId(remaining.length > 0 ? remaining[remaining.length - 1].id : null);
      return remaining;
    });
  }, [activeTabId]);

  // ── Tiling ────────────────────────────────────────────────
  const handleDropToQuadrant = useCallback((tabId: string, quadrant: Quadrant) => {
    setTiledTabs(prev => {
      const cleaned: Partial<Record<Quadrant, string>> = {};
      (Object.keys(prev) as Quadrant[]).forEach(q => { if (prev[q] !== tabId) cleaned[q] = prev[q]; });
      cleaned[quadrant] = tabId;
      return cleaned;
    });
    setDraggingTabId(null);
  }, []);

  const handleRemoveFromTile = useCallback((quadrant: Quadrant) => {
    setTiledTabs(prev => { const next = { ...prev }; delete next[quadrant]; return next; });
  }, []);

  /** Ctrl+M — spreads the first open sessions over the quadrants, or restores tabs. */
  const handleToggleTiled = useCallback(() => {
    setTiledTabs(prev => {
      if (Object.keys(prev).length > 0) return {};
      const next: Partial<Record<Quadrant, string>> = {};
      tabs.slice(0, QUADRANTS.length).forEach((tab, index) => { next[QUADRANTS[index]] = tab.id; });
      return next;
    });
    if (tabs.length > 0 && activeTabId === null) setActiveTabId(tabs[0].id);
  }, [tabs, activeTabId]);

  // ── Sync ──────────────────────────────────────────────────
  const handleToggleSync = useCallback((tabId: string) => {
    setSyncedTabIds(prev => { const next = new Set(prev); if (next.has(tabId)) next.delete(tabId); else next.add(tabId); return next; });
    setContextMenu(null);
  }, []);

  const handleSyncInput = useCallback((sourceTabId: string, data: string) => {
    setSyncedTabIds(current => {
      current.forEach(tabId => {
        if (tabId !== sourceTabId) tabRefsMap.current.get(tabId)?.current?.injectData(data);
      });
      return current;
    });
  }, []);

  // ── Context menu ──────────────────────────────────────────
  const handleTabContextMenu = useCallback((e: React.MouseEvent, tabId: string) => {
    e.preventDefault(); e.stopPropagation();
    setContextMenu({ x: e.clientX, y: e.clientY, tabId });
  }, []);

  // ── Drag ──────────────────────────────────────────────────
  const handleTabDragStart = useCallback((e: React.DragEvent, tabId: string) => {
    e.dataTransfer.setData('tabId', tabId);
    e.dataTransfer.effectAllowed = 'move';
    setDraggingTabId(tabId);
  }, []);

  const handleTabDragEnd = useCallback(() => setDraggingTabId(null), []);

  const openNewForm = useCallback((defaultGroup?: string) => {
    setEditingConn(null);
    setFormDefaultGroup(defaultGroup);
    setFormOpen(true);
  }, []);
  const openEditForm = useCallback((conn: Connection) => {
    setEditingConn(conn);
    setFormDefaultGroup(undefined);
    setFormOpen(true);
  }, []);

  const toggleRail = useCallback(() => {
    setRailCollapsed(prev => {
      localStorage.setItem(RAIL_COLLAPSED_KEY, String(!prev));
      return !prev;
    });
  }, []);

  const savedConnections = useMemo(
    () => connections.filter(c => !isSessionId(c.id)),
    [connections],
  );
  const tiledTabIds = new Set(Object.values(tiledTabs));
  const activeTab = tabs.find(t => t.id === activeTabId) ?? null;
  const activeSessionConn = activeTab ? connections.find(c => c.id === activeTab.connectionId) : undefined;
  const activeTerminalTabId = activeTab?.type === 'terminal' ? activeTab.id : null;

  /** The saved connections shown in the list, narrowed by rail, type and search. */
  const visibleConnections = useMemo(() => {
    const query = search.trim().toLowerCase();
    return savedConnections.filter(conn => {
      if (typeFilter !== 'all' && conn.type !== typeFilter) return false;

      if (groupFilter.kind === 'favorites' && !conn.favorite) return false;
      if (groupFilter.kind === 'group' && conn.group !== groupFilter.name) return false;
      if (groupFilter.kind === 'ungrouped' && conn.group && groups.includes(conn.group)) return false;

      if (query) {
        const haystack = `${conn.name} ${conn.host} ${conn.username}`.toLowerCase();
        if (!haystack.includes(query)) return false;
      }
      return true;
    });
  }, [savedConnections, typeFilter, groupFilter, groups, search]);

  // ── Global shortcuts ─────────────────────────────────────
  // Captured on the window so xterm.js never sees these combinations first.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey) return;
      const key = e.key.toLowerCase();

      if (key === 'k')                    { e.preventDefault(); e.stopPropagation(); setPaletteOpen(open => !open); }
      else if (key === 'n' && !e.shiftKey) { e.preventDefault(); e.stopPropagation(); openNewForm(); }
      else if (key === 'm' && !e.shiftKey) { e.preventDefault(); e.stopPropagation(); handleToggleTiled(); }
      else if (key === 's' && e.shiftKey && activeTerminalTabId) {
        e.preventDefault(); e.stopPropagation();
        handleToggleSync(activeTerminalTabId);
      }
    };
    window.addEventListener('keydown', handler, true);
    return () => window.removeEventListener('keydown', handler, true);
  }, [openNewForm, handleToggleTiled, handleToggleSync, activeTerminalTabId]);

  // ── Loading ───────────────────────────────────────────────
  if (setupReady === null) {
    return (
      <div className="app">
        <div className="setup-overlay">
          <div style={{ color: 'var(--text-muted)', fontSize: 14 }}>Chargement…</div>
        </div>
      </div>
    );
  }

  const titlebar = (
    <div className="titlebar" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}>
      <div className="titlebar-title">
        <span className="titlebar-icon">⬡</span> SSH Manager
      </div>
      <div className="titlebar-controls" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
        <button className="titlebar-btn" onClick={() => window.electronAPI.windowMinimize()}>─</button>
        <button className="titlebar-btn" onClick={() => window.electronAPI.windowMaximize()}>□</button>
        <button className="titlebar-btn close" onClick={() => window.electronAPI.windowClose()}>✕</button>
      </div>
    </div>
  );

  // ── First launch: setup wizard ────────────────────────────
  if (!setupReady) {
    return (
      <div className="app">
        {titlebar}
        <SetupWizard onComplete={handleSetupComplete} portable={isPortable} />
      </div>
    );
  }

  return (
    <div className="app">
      {titlebar}

      <div className="main-layout">
        <UnifiedBar
          groups={groups}
          currentFilter={typeFilter}
          onFilterChange={setTypeFilter}
          search={search}
          onSearchChange={setSearch}
          onConnect={handleQuickConnect}
          onSaveConnection={handleSaveQuickConn}
          onNewConnection={() => openNewForm()}
          onOpenPalette={() => setPaletteOpen(true)}
          onToggleRail={toggleRail}
          railCollapsed={railCollapsed}
        />

        <div className="tab-bar">
          {/* Permanent tabs: always visible, even with no open sessions. */}
          <div
            className={`tab tab-home ${activeTabId === null && dashboardView === 'connections' ? 'active' : ''}`}
            onClick={() => { setActiveTabId(null); setDashboardView('connections'); }}
            title="Tableau de bord"
          >
            <span className="tab-icon">⌂</span>
            <span className="tab-name">Accueil</span>
          </div>
          <div
            className={`tab tab-home ${activeTabId === null && dashboardView === 'toolbox' ? 'active' : ''}`}
            onClick={() => { setActiveTabId(null); setDashboardView('toolbox'); }}
            title="Boîte à outils"
          >
            <span className="tab-icon">⚒</span>
            <span className="tab-name">Boîte à outils</span>
          </div>
          {tabs.map(tab => {
            const inTile = tiledTabIds.has(tab.id);
            const isSynced = syncedTabIds.has(tab.id);
            return (
              <div key={tab.id}
                className={`tab ${tab.id === activeTabId ? 'active' : ''} ${inTile ? 'tab-tiled' : ''} ${isSynced ? 'tab-synced' : ''}`}
                draggable onDragStart={e => handleTabDragStart(e, tab.id)} onDragEnd={handleTabDragEnd}
                onClick={() => setActiveTabId(tab.id)}
                onContextMenu={e => handleTabContextMenu(e, tab.id)}
              >
                <span className={`tab-status status-${tab.status}`} />
                <span className="tab-name">{tab.connectionName}</span>
                {tab.connectionType !== 'ssh' && <span className="tab-proto">{tab.connectionType.toUpperCase()}</span>}
                {isSynced && <span className="tab-sync-dot" title="Saisie synchronisée">⟳</span>}
                {inTile && <span className="tab-tile-dot" title="Mode mosaïque">⊡</span>}
                <button className="tab-close" onClick={e => { e.stopPropagation(); handleCloseTab(tab.id); }}>✕</button>
              </div>
            );
          })}
          {draggingTabId && <div className="tab-bar-drag-hint">⊕ Déposez dans un coin de l'écran</div>}
        </div>

        <div className="workspace">
          {/* Le rail ne trie que des connexions : il ne suit ni les sessions
              ouvertes ni la boîte à outils. */}
          {!railCollapsed && activeTabId === null && dashboardView === 'connections' && (
            <GroupRail
              connections={savedConnections}
              groups={groups}
              groupColors={groupColors}
              selection={groupFilter}
              onSelect={setGroupFilter}
              onCreateGroup={handleCreateGroup}
              onRenameGroup={handleRenameGroup}
              onDeleteGroup={handleDeleteGroup}
              onSetGroupColor={handleSetGroupColor}
              onImport={handleImport}
              onExport={handleExport}
            />
          )}

          <div className="tab-content">
            {draggingTabId && (
              <div className="drop-zones-overlay">
                {QUADRANTS.map(quadrant => (
                  <div key={quadrant} className={`drop-zone drop-zone-${quadrant}`}
                    onDragOver={e => { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; }}
                    onDrop={e => { e.preventDefault(); const id = e.dataTransfer.getData('tabId'); if (id) handleDropToQuadrant(id, quadrant); }}>
                    <div className="drop-zone-label">
                      {quadrant === 'topLeft' && '↖ Haut gauche'}{quadrant === 'topRight' && '↗ Haut droite'}
                      {quadrant === 'bottomLeft' && '↙ Bas gauche'}{quadrant === 'bottomRight' && '↘ Bas droite'}
                    </div>
                  </div>
                ))}
              </div>
            )}

            {activeTabId === null && dashboardView === 'toolbox' && <Toolbox />}

            {activeTabId === null && dashboardView === 'connections' && (
              <ConnectionList
                connections={visibleConnections}
                groupColors={groupColors}
                search={search}
                onConnect={handleConnect}
                onOpenAs={handleOpenAs}
                onNew={() => openNewForm(groupFilter.kind === 'group' ? groupFilter.name : undefined)}
                onEdit={openEditForm}
                onDelete={handleDeleteConnection}
                onToggleFavorite={handleToggleFavorite}
                onSetConnectionColor={handleSetConnectionColor}
              />
            )}

            {isTiledMode && (
              // Kept mounted but hidden while the dashboard is displayed, so the
              // tiled terminal sessions stay alive.
              <div
                className="tiled-layout-host"
                style={{ display: activeTabId === null ? 'none' : 'flex', flex: 1, minHeight: 0 }}
              >
                <TiledLayout
                  tiledTabs={tiledTabs} tabs={tabs} connections={connections}
                  syncedTabIds={syncedTabIds} tabRefs={tabRefsMap.current}
                  draggingTabId={draggingTabId} onStatusChange={handleTabStatusChange}
                  onCloseTab={handleCloseTab} onDropToQuadrant={handleDropToQuadrant}
                  onRemoveFromTile={handleRemoveFromTile} onSyncInput={handleSyncInput}
                  terminalTheme={terminalTheme} onThemeChange={handleThemeChange}
                />
              </div>
            )}

            {tabs.map(tab => {
              if (tiledTabIds.has(tab.id)) return null;
              const conn = connections.find(c => c.id === tab.connectionId);
              if (!conn) return null;
              const tabRef = getTabRef(tab.id);
              const isSynced = syncedTabIds.has(tab.id);
              return (
                <div key={tab.id} className="tab-panel" style={{ display: tab.id === activeTabId ? 'flex' : 'none' }}>
                  {tab.type === 'terminal' ? (
                    <TerminalTab ref={tabRef} tab={tab} connection={conn}
                      terminalTheme={terminalTheme}
                      isActive={tab.id === activeTabId}
                      onStatusChange={handleTabStatusChange} onClose={() => handleCloseTab(tab.id)}
                      onRetry={() => handleRetry(tab.id)}
                      onSyncInput={isSynced ? (data) => handleSyncInput(tab.id, data) : undefined}
                      onThemeChange={handleThemeChange}
                      synced={isSynced}
                    />
                  ) : (
                    <FileBrowser tab={tab} connection={conn}
                      onStatusChange={handleTabStatusChange} onClose={() => handleCloseTab(tab.id)} />
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </div>

      <StatusBar
        tabs={tabs}
        activeTab={activeTab}
        activeTarget={activeSessionConn
          ? `${activeSessionConn.username}@${activeSessionConn.host}:${activeSessionConn.port || (activeSessionConn.type === 'ftp' ? 21 : 22)}`
          : null}
        syncedCount={syncedTabIds.size}
        vaultEncrypted={!isPortable}
        version={appVersion}
      />

      {paletteOpen && (
        <CommandPalette
          connections={savedConnections}
          canSyncInput={activeTerminalTabId !== null}
          tiled={isTiledMode}
          onConnect={handleConnect}
          onOpenAs={handleOpenAs}
          onNewConnection={() => openNewForm()}
          onOpenToolbox={() => { setActiveTabId(null); setDashboardView('toolbox'); }}
          onToggleTiled={handleToggleTiled}
          onToggleSync={() => { if (activeTerminalTabId) handleToggleSync(activeTerminalTabId); }}
          onThemeChange={handleThemeChange}
          onClose={() => setPaletteOpen(false)}
        />
      )}

      {toast && <div className={`toast ${toast.ok ? 'toast-ok' : 'toast-err'}`}>{toast.msg}</div>}

      {/* Context menu */}
      {contextMenu && (
        <div className="tab-context-menu" style={{ left: contextMenu.x, top: contextMenu.y }} onClick={e => e.stopPropagation()}>
          {(() => {
            const tab = tabs.find(t => t.id === contextMenu.tabId);
            if (!tab || tab.type !== 'terminal') return null;
            const isSynced = syncedTabIds.has(contextMenu.tabId);
            const inTile = tiledTabIds.has(contextMenu.tabId);
            return (
              <>
                <div className="ctx-menu-title">{tab.connectionName}</div>
                <button className={`ctx-menu-item ${isSynced ? 'ctx-active' : ''}`} onClick={() => handleToggleSync(contextMenu.tabId)}>
                  <span>⟳</span>{isSynced ? 'Désynchroniser la saisie' : 'Synchroniser la saisie'}
                </button>
                {inTile && (
                  <button className="ctx-menu-item" onClick={() => {
                    const q = (Object.keys(tiledTabs) as Quadrant[]).find(k => tiledTabs[k] === contextMenu.tabId);
                    if (q) handleRemoveFromTile(q);
                    setContextMenu(null);
                  }}><span>⊡</span> Retirer de la mosaïque</button>
                )}
                <hr className="ctx-menu-sep" />
                <button className="ctx-menu-item ctx-danger" onClick={() => { handleCloseTab(contextMenu.tabId); setContextMenu(null); }}>
                  <span>✕</span> Fermer l'onglet
                </button>
              </>
            );
          })()}
        </div>
      )}

      {/* Mnemonic import prompt */}
      {mnemonicImport && (
        <div className="modal-overlay" onClick={() => { setMnemonicImport(null); setMnemonicInput(''); setMnemonicError(''); }}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h2>Fichier chiffré</h2>
              <button className="btn-icon" onClick={() => { setMnemonicImport(null); setMnemonicInput(''); setMnemonicError(''); }}>✕</button>
            </div>
            <div className="modal-form">
              <p style={{ color: 'var(--text-muted)', fontSize: 13, lineHeight: 1.6 }}>
                Ce fichier a été chiffré avec une phrase de récupération. Entrez les 12 mots pour le déchiffrer.
              </p>
              <div className="form-group">
                <label>Phrase de récupération</label>
                <input type="text" value={mnemonicInput} onChange={e => { setMnemonicInput(e.target.value); setMnemonicError(''); }}
                  placeholder="mot1 mot2 mot3 … mot12"
                  className={mnemonicError ? 'input-error' : ''}
                  onKeyDown={e => { if (e.key === 'Enter') handleMnemonicImportConfirm(); }}
                  autoFocus />
                {mnemonicError && <span className="field-error">{mnemonicError}</span>}
              </div>
              <div className="modal-actions">
                <button className="btn-secondary" onClick={() => { setMnemonicImport(null); setMnemonicInput(''); setMnemonicError(''); }}>Annuler</button>
                <button className="btn-primary" onClick={handleMnemonicImportConfirm} disabled={!mnemonicInput.trim()}>
                  Déchiffrer et importer
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {pwPrompt && (
        <PasswordPrompt
          connection={pwPrompt.conn}
          groupColor={groupColors[pwPrompt.conn.group ?? ''] || undefined}
          onConfirm={handlePwConfirm}
          onCancel={() => setPwPrompt(null)}
        />
      )}

      {formOpen && (
        <ConnectionForm connection={editingConn} newId={newConnId()} groups={groups} defaultGroup={formDefaultGroup}
          onSave={handleSaveConnection} onDelete={handleDeleteConnection}
          onClose={() => { setFormOpen(false); setEditingConn(null); setFormDefaultGroup(undefined); }} />
      )}
    </div>
  );
}
