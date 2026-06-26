import React, { useState, useCallback, useEffect, useRef } from 'react';
import { Connection, Tab, Quadrant } from './types';
import Sidebar from './components/Sidebar';
import TerminalTab, { TerminalTabHandle } from './components/TerminalTab';
import FileBrowser from './components/FileBrowser';
import ConnectionForm from './components/ConnectionForm';
import PasswordPrompt from './components/PasswordPrompt';
import TiledLayout from './components/TiledLayout';
import SetupWizard from './components/SetupWizard';
import ConnectionGrid from './components/ConnectionGrid';
import UnifiedBar, { loadTileSize, TileSize } from './components/UnifiedBar';
import {
  TerminalThemeConfig, loadTerminalTheme, TERMINAL_THEME_KEY,
} from './terminal-themes';

declare global {
  interface Window {
    electronAPI: {
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
      sshConnect: (p: { tabId: string; connection: Connection }) => Promise<{ success: boolean }>;
      sshWrite: (p: { tabId: string; data: string }) => void;
      sshResize: (p: { tabId: string; cols: number; rows: number }) => void;
      sshDisconnect: (tabId: string) => void;
      sshOpenInPowerShell: (c: Connection) => void;
      sshOpenInCmd: (c: Connection) => void;
      sshForgetHostKey: (p: { host: string; port: number }) => Promise<{ success: boolean }>;
      onSshData: (tabId: string, cb: (data: string) => void) => () => void;
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
  interface FileEntry { name: string; type: string; size: number; modifyTime?: number; }
}

const GROUPS_KEY        = 'sshmanager-groups';
const GROUP_COLORS_KEY  = 'sshmanager-group-colors';
const RECENTS_KEY       = 'sshmanager-recents';
const MAX_RECENTS       = 15;

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

const loadGroups      = (): string[]                => { try { return JSON.parse(localStorage.getItem(GROUPS_KEY)       ?? '[]');  } catch { return []; } };
const loadGroupColors = (): Record<string, string>  => { try { return JSON.parse(localStorage.getItem(GROUP_COLORS_KEY) ?? '{}');  } catch { return {}; } };
const loadRecents     = (): string[]                => { try { return JSON.parse(localStorage.getItem(RECENTS_KEY)      ?? '[]');  } catch { return []; } };

const saveGroupsLS      = (g: string[])                  => localStorage.setItem(GROUPS_KEY,       JSON.stringify(g));
const saveGroupColorsLS = (c: Record<string, string>)    => localStorage.setItem(GROUP_COLORS_KEY, JSON.stringify(c));
const pushRecent    = (id: string) => {
  const prev    = loadRecents().filter(r => r !== id);
  const updated = [id, ...prev].slice(0, MAX_RECENTS);
  localStorage.setItem(RECENTS_KEY, JSON.stringify(updated));
};

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
  const [recentIds, setRecentIds]       = useState<string[]>(loadRecents);
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [editingConn, setEditingConn] = useState<Connection | null>(null);
  const [formDefaultGroup, setFormDefaultGroup] = useState<string | undefined>();
  const [toast, setToast] = useState<{ msg: string; ok: boolean } | null>(null);
  const [pwPrompt, setPwPrompt] = useState<{ conn: Connection; isEphemeral: boolean } | null>(null);
  const [isPortable, setIsPortable] = useState(false);

  // ── Unified bar state ─────────────────────────────────────
  const [gridFilter, setGridFilter] = useState<'all' | 'ssh' | 'sftp' | 'ftp'>('all');
  const [gridSearch, setGridSearch] = useState('');
  const [tileSize, setTileSize]           = useState<TileSize>(loadTileSize);
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
    // Migrate the collapsed state to the new name
    try {
      const collapsed = JSON.parse(localStorage.getItem('sshmanager-group-collapsed') ?? '{}');
      if (Object.prototype.hasOwnProperty.call(collapsed, oldName)) {
        collapsed[newName] = collapsed[oldName];
        delete collapsed[oldName];
        localStorage.setItem('sshmanager-group-collapsed', JSON.stringify(collapsed));
      }
    } catch {}
    setConnections(prev => {
      const next = prev.map(c => c.group === oldName ? { ...c, group: newName } : c);
      persistConnections(next);
      return next;
    });
  }, []);

  const handleSetGroupColor = useCallback((groupName: string, color: string) => {
    setGroupColors(prev => {
      const next = { ...prev, [groupName]: color };
      saveGroupColorsLS(next);
      return next;
    });
  }, []);

  const handleDeleteGroup = useCallback((name: string) => {
    setGroups(prev => { const next = prev.filter(g => g !== name); saveGroupsLS(next); return next; });
    setGroupColors(prev => { const next = { ...prev }; delete next[name]; saveGroupColorsLS(next); return next; });
    // Clean up the collapsed state for this group
    try {
      const collapsed = JSON.parse(localStorage.getItem('sshmanager-group-collapsed') ?? '{}');
      delete collapsed[name];
      localStorage.setItem('sshmanager-group-collapsed', JSON.stringify(collapsed));
    } catch {}
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
    tabOrigins.current.set(tabId, conn);
    setConnections(prev => [...prev, sessionConn]);
    setTabs(prev => [...prev, {
      id: tabId, connectionId: tabId,
      connectionName: conn.name || conn.host,
      type: isFile ? 'filebrowser' : 'terminal',
      connectionType: conn.type, status: 'connecting',
    }]);
    setActiveTabId(tabId);
    // Enregistrer dans les récentes (connexions sauvegardées uniquement)
    if (!isEphemeral && conn.id && !conn.id.startsWith('tab-')) {
      pushRecent(conn.id);
      setRecentIds(loadRecents());
    }
  }, []);

  const handleConnect = useCallback((conn: Connection) => {
    if (needsPasswordPrompt(conn)) setPwPrompt({ conn, isEphemeral: false });
    else openTab(conn);
  }, [openTab]);

  const handleQuickConnect = useCallback((conn: Connection) => {
    if (needsPasswordPrompt(conn)) setPwPrompt({ conn, isEphemeral: true });
    else openTab(conn, true);
  }, [openTab]);

  const handlePwConfirm = useCallback((password: string, save: boolean) => {
    if (!pwPrompt) return;
    const { conn, isEphemeral } = pwPrompt;
    const connWithPw: Connection = { ...conn, password };
    if (save && !isEphemeral && conn.id && !conn.id.startsWith('tab-')) {
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
      setPwPrompt({ conn: connWithoutPw, isEphemeral: !origin.id || origin.id.startsWith('tab-') });
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

  const openNewForm = (defaultGroup?: string) => { setEditingConn(null); setFormDefaultGroup(defaultGroup); setFormOpen(true); };
  const openEditForm = (conn: Connection) => { setEditingConn(conn); setFormDefaultGroup(undefined); setFormOpen(true); };

  const savedConnections = connections.filter(c => !c.id.startsWith('tab-'));
  const tiledTabIds = new Set(Object.values(tiledTabs));

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

  // ── First launch: setup wizard ────────────────────────────
  if (!setupReady) {
    return (
      <div className="app">
        <div className="titlebar" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}>
          <div className="titlebar-title"><span className="titlebar-icon">⬡</span> SSH Manager</div>
          <div className="titlebar-controls" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
            <button className="titlebar-btn" onClick={() => window.electronAPI.windowMinimize()}>─</button>
            <button className="titlebar-btn" onClick={() => window.electronAPI.windowMaximize()}>□</button>
            <button className="titlebar-btn close" onClick={() => window.electronAPI.windowClose()}>✕</button>
          </div>
        </div>
        <SetupWizard onComplete={handleSetupComplete} portable={isPortable} />
      </div>
    );
  }

  return (
    <div className="app">
      {/* Titlebar */}
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

      <div className="main-layout">
        <Sidebar
          connections={savedConnections} recentIds={recentIds} groups={groups}
          activeConnectionId={activeTabId ? (tabs.find(t => t.id === activeTabId)?.connectionId ?? null) : null}
          onConnect={handleConnect} onNew={openNewForm} onEdit={openEditForm}
          onDelete={handleDeleteConnection} onToggleFavorite={handleToggleFavorite}
          onRenameGroup={handleRenameGroup} onDeleteGroup={handleDeleteGroup}
          onCreateGroup={handleCreateGroup} onExport={handleExport} onImport={handleImport}
        />

        <div className="content-area">
          <UnifiedBar
            groups={groups}
            currentFilter={gridFilter}
            onFilterChange={f => { setGridFilter(f); setGridSearch(''); }}
            onConnect={handleQuickConnect}
            onSaveConnection={handleSaveQuickConn}
            tileSize={tileSize}
            onTileSizeChange={setTileSize}
          />

          {tabs.length > 0 && (
            <div className="tab-bar">
              {tabs.map(tab => {
                const inTile = tiledTabIds.has(tab.id);
                const isSynced = syncedTabIds.has(tab.id);
                return (
                  <div key={tab.id}
                    className={`tab ${tab.id === activeTabId ? 'active' : ''} ${inTile ? 'tab-tiled' : ''} ${isSynced ? 'tab-synced' : ''}`}
                    draggable onDragStart={e => handleTabDragStart(e, tab.id)} onDragEnd={handleTabDragEnd}
                    onClick={() => { if (!inTile) setActiveTabId(tab.id); }}
                    onContextMenu={e => handleTabContextMenu(e, tab.id)}
                  >
                    <span className="tab-icon">{tab.connectionType === 'ssh' ? '>' : tab.connectionType === 'sftp' ? '⇅' : '≈'}</span>
                    <span className="tab-name">{tab.connectionName}</span>
                    {isSynced && <span className="tab-sync-dot" title="Saisie synchronisée">⟳</span>}
                    {inTile && <span className="tab-tile-dot" title="Mode tuilé">⊡</span>}
                    <span className={`tab-status status-${tab.status}`} />
                    <button className="tab-close" onClick={e => { e.stopPropagation(); handleCloseTab(tab.id); }}>✕</button>
                  </div>
                );
              })}
              {draggingTabId && <div className="tab-bar-drag-hint">⊕ Déposez dans un coin de l'écran</div>}
            </div>
          )}

          <div className="tab-content">
            {draggingTabId && (
              <div className="drop-zones-overlay">
                {(['topLeft', 'topRight', 'bottomLeft', 'bottomRight'] as Quadrant[]).map(quadrant => (
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

            {tabs.length === 0 && (
              <ConnectionGrid
                connections={savedConnections}
                groups={groups}
                groupColors={groupColors}
                filter={gridFilter}
                search={gridSearch}
                tileSize={tileSize}
                onConnect={handleConnect}
                onNew={openNewForm}
                onEdit={openEditForm}
                onCreateGroup={handleCreateGroup}
                onSetGroupColor={handleSetGroupColor}
              />
            )}

            {isTiledMode && (
              <TiledLayout
                tiledTabs={tiledTabs} tabs={tabs} connections={connections}
                syncedTabIds={syncedTabIds} tabRefs={tabRefsMap.current}
                draggingTabId={draggingTabId} onStatusChange={handleTabStatusChange}
                onCloseTab={handleCloseTab} onDropToQuadrant={handleDropToQuadrant}
                onRemoveFromTile={handleRemoveFromTile} onSyncInput={handleSyncInput}
                terminalTheme={terminalTheme} onThemeChange={handleThemeChange}
              />
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
                  }}><span>⊡</span> Retirer du mode tuilé</button>
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
              <h2>🔐 Fichier chiffré — phrase requise</h2>
              <button className="btn-icon" onClick={() => { setMnemonicImport(null); setMnemonicInput(''); setMnemonicError(''); }}>✕</button>
            </div>
            <div className="modal-form">
              <p style={{ color: 'var(--text-muted)', fontSize: 13, margin: '0 0 12px' }}>
                Ce fichier a été chiffré avec une phrase de récupération. Entrez les 12 mots pour le déchiffrer.
              </p>
              <div className="form-group">
                <label>Phrase de récupération (12 mots séparés par des espaces)</label>
                <input type="text" value={mnemonicInput} onChange={e => { setMnemonicInput(e.target.value); setMnemonicError(''); }}
                  placeholder="mot1 mot2 mot3 ... mot12"
                  className={mnemonicError ? 'input-error' : ''}
                  onKeyDown={e => { if (e.key === 'Enter') handleMnemonicImportConfirm(); }}
                  autoFocus />
                {mnemonicError && <span className="field-error">{mnemonicError}</span>}
              </div>
              <div className="modal-actions">
                <button className="btn-secondary" onClick={() => { setMnemonicImport(null); setMnemonicInput(''); setMnemonicError(''); }}>Annuler</button>
                <button className="btn-primary" onClick={handleMnemonicImportConfirm} disabled={!mnemonicInput.trim()}>
                  🔓 Déchiffrer et importer
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
