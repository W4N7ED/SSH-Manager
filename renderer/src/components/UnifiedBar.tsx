import React, { useState, useRef, useCallback } from 'react';
import { Connection, ConnectionType } from '../types';

export type TileSize = 'xl' | 'md' | 'sm' | 'xs';

const TILE_SIZE_KEY = 'sshmanager-tile-size';
export const loadTileSize = (): TileSize => {
  const v = localStorage.getItem(TILE_SIZE_KEY);
  return (v === 'xl' || v === 'md' || v === 'sm' || v === 'xs') ? v : 'md';
};

const TILE_SIZE_OPTIONS: { value: TileSize; label: string; title: string; icon: string }[] = [
  { value: 'xl', label: 'XL', title: 'Grand',      icon: '⬛' },
  { value: 'md', label: 'M',  title: 'Moyen',      icon: '▪' },
  { value: 'sm', label: 'S',  title: 'Petit',      icon: '▫' },
  { value: 'xs', label: 'XS', title: 'Très petit', icon: '·' },
];

interface Props {
  groups: string[];
  /** Called when the user wants to filter the grid by type (or 'all') */
  onFilterChange: (filter: 'all' | ConnectionType) => void;
  /** Called when the user initiates a quick connection */
  onConnect: (conn: Connection) => void;
  /** Called when the user saves a quick-connect as a named connection */
  onSaveConnection: (conn: Connection) => void;
  currentFilter: 'all' | ConnectionType;
  tileSize: TileSize;
  onTileSizeChange: (size: TileSize) => void;
}

const DEFAULT_PORTS: Record<ConnectionType, number> = { ssh: 22, sftp: 22, ftp: 21 };

/** Detects whether the input string looks like a remote host address. */
function looksLikeHost(s: string): boolean {
  if (!s.trim()) return false;
  if (s.includes('@')) return true;
  if (/^\d{1,3}(\.\d{1,3}){1,3}(:\d+)?$/.test(s.trim())) return true;
  if (/^[a-zA-Z0-9-]+(\.[a-zA-Z0-9-]+)+(:\d+)?$/.test(s.trim())) return true;
  return false;
}

function parseInput(raw: string): { username: string; host: string; port: number | null } {
  let s = raw.trim();
  let username = '';
  let port: number | null = null;

  if (s.includes('@')) {
    const at = s.indexOf('@');
    username = s.slice(0, at).trim();
    s = s.slice(at + 1);
  }

  const lastColon = s.lastIndexOf(':');
  if (lastColon !== -1) {
    const maybePort = parseInt(s.slice(lastColon + 1), 10);
    if (!isNaN(maybePort) && maybePort > 0 && maybePort <= 65535) {
      port = maybePort;
      s = s.slice(0, lastColon);
    }
  }

  return { username, host: s.trim(), port };
}

export default function UnifiedBar({
  groups, onFilterChange, onConnect, onSaveConnection, currentFilter,
  tileSize, onTileSizeChange,
}: Props) {
  const [input, setInput]               = useState('');
  const [showSave, setShowSave]         = useState(false);
  const [lastConn, setLastConn]         = useState<Connection | null>(null);
  const [saveName, setSaveName]         = useState('');
  const [saveGroup, setSaveGroup]       = useState('');
  const [saveNewGroup, setSaveNewGroup] = useState('');
  const [saveNewGroupMode, setSaveNewGroupMode] = useState(false);
  const [saveFavorite, setSaveFavorite] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const isConnectMode = looksLikeHost(input);
  // The type used for quick-connect: if a specific type is selected use it, otherwise default SSH
  const connectType: ConnectionType = currentFilter === 'all' ? 'ssh' : currentFilter;

  const handleInputChange = useCallback((val: string) => {
    setInput(val);
    // If not a host address, propagate as a search filter in real time
    if (!looksLikeHost(val)) {
      // The parent ConnectionGrid handles filtering via its own prop
    }
  }, []);

  const handleConnect = useCallback(() => {
    const { username, host, port } = parseInput(input);
    if (!host) { inputRef.current?.focus(); return; }

    const conn: Connection = {
      id: '__quickconn__',
      name: host,
      type: connectType,
      host,
      port: port ?? DEFAULT_PORTS[connectType],
      username: username || 'root',
      createdAt: Date.now(),
    };

    onConnect(conn);
    setSaveName(host);
    setSaveGroup('');
    setSaveNewGroup('');
    setSaveNewGroupMode(false);
    setSaveFavorite(false);
    setLastConn(conn);
    setShowSave(true);
  }, [input, connectType, onConnect]);

  const handleSave = useCallback(() => {
    if (!lastConn) return;
    const resolvedGroup = saveNewGroupMode
      ? (saveNewGroup.trim() || undefined)
      : (saveGroup || undefined);
    onSaveConnection({
      ...lastConn,
      id: 'conn-' + Date.now() + '-' + Math.random().toString(36).slice(2, 7),
      name: saveName.trim() || lastConn.host,
      group: resolvedGroup,
      favorite: saveFavorite || undefined,
    });
    setShowSave(false);
    setInput('');
  }, [lastConn, saveName, saveGroup, saveNewGroup, saveNewGroupMode, saveFavorite, onSaveConnection]);

  return (
    <>
      <div className="unified-bar">
        {/* ── Filtres de type ────────────────────────────────── */}
        <div className="unified-type-tabs">
          {(['all', 'ssh', 'sftp', 'ftp'] as const).map(t => (
            <button
              key={t}
              className={`unified-type-btn ${currentFilter === t ? 'active' : ''}`}
              onClick={() => onFilterChange(t)}
              title={t === 'all' ? 'Toutes les connexions' : t.toUpperCase()}
            >
              {t === 'all' ? 'Tous' : t.toUpperCase()}
            </button>
          ))}
        </div>

        {/* ── Champ unique : recherche ou adresse ────────────── */}
        <div className={`unified-input-wrap ${isConnectMode ? 'is-connect-mode' : ''}`}>
          <span className="unified-input-icon">
            {isConnectMode ? '▶' : '⌕'}
          </span>
          <input
            ref={inputRef}
            className="unified-input"
            type="text"
            placeholder="Rechercher  ou  user@host:port  ou  192.168.1.1"
            value={input}
            onChange={e => handleInputChange(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter' && isConnectMode) handleConnect(); }}
            spellCheck={false}
            autoCorrect="off"
            autoCapitalize="off"
          />
          {input && (
            <button
              className="unified-clear"
              onClick={() => { setInput(''); inputRef.current?.focus(); }}
            >✕</button>
          )}
        </div>

        {/* ── Bouton Connecter ──────────────────────────────── */}
        <button
          className={`unified-connect-btn ${isConnectMode ? 'active' : ''}`}
          onClick={handleConnect}
          disabled={!isConnectMode}
          title={isConnectMode ? `Connecter en ${connectType.toUpperCase()}` : 'Entrez une adresse pour vous connecter'}
        >
          ▶ Connecter
        </button>

        {/* ── Sélecteur de taille de tuile ─────────────────── */}
        <div className="tile-size-picker" title="Taille des tuiles">
          {TILE_SIZE_OPTIONS.map(opt => (
            <button
              key={opt.value}
              className={`tile-size-picker-btn ${tileSize === opt.value ? 'active' : ''}`}
              onClick={() => {
                localStorage.setItem(TILE_SIZE_KEY, opt.value);
                onTileSizeChange(opt.value);
              }}
              title={opt.title}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      {/* ── Bandeau "enregistrer la connexion rapide" ────────── */}
      {showSave && lastConn && (
        <div className="qc-save-banner">
          <span className="qc-save-icon">✓</span>
          <span className="qc-save-label">Connexion ouverte — enregistrer ?</span>
          <input
            className="qc-save-name"
            type="text"
            placeholder="Nom de la connexion"
            value={saveName}
            onChange={e => setSaveName(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') handleSave(); }}
          />
          {!saveNewGroupMode ? (
            <select
              className="qc-save-select"
              value={saveGroup}
              onChange={e => {
                if (e.target.value === '__new__') { setSaveNewGroupMode(true); setSaveGroup(''); }
                else setSaveGroup(e.target.value);
              }}
            >
              <option value="">Sans groupe</option>
              {groups.map(g => <option key={g} value={g}>{g}</option>)}
              <option value="__new__">+ Nouveau groupe…</option>
            </select>
          ) : (
            <input
              autoFocus className="qc-save-name"
              type="text" placeholder="Nom du groupe"
              value={saveNewGroup}
              onChange={e => setSaveNewGroup(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter') setSaveNewGroupMode(false);
                if (e.key === 'Escape') { setSaveNewGroupMode(false); setSaveNewGroup(''); }
              }}
            />
          )}
          <label className="qc-fav-label" title="Ajouter aux favoris">
            <input type="checkbox" checked={saveFavorite} onChange={e => setSaveFavorite(e.target.checked)} />⭐
          </label>
          <button className="qc-save-btn" onClick={handleSave}>Enregistrer</button>
          <button className="qc-dismiss-btn" onClick={() => setShowSave(false)}>Ignorer</button>
        </div>
      )}
    </>
  );
}
