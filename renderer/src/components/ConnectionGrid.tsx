import React, { useState, useMemo, useRef, useEffect } from 'react';
import { Connection, ConnectionType } from '../types';
import { TileSize } from './UnifiedBar';

// Palette de couleurs prédéfinies pour les groupes
const GROUP_PALETTE = [
  '#ef4444', // red
  '#f97316', // orange
  '#eab308', // yellow
  '#22c55e', // green
  '#14b8a6', // teal
  '#3b82f6', // blue
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#64748b', // slate
  '#a16207', // amber-dark
  '#0891b2', // cyan
  '#be185d', // rose-dark
];

interface Props {
  connections: Connection[];
  groups: string[];
  groupColors: Record<string, string>;
  filter: 'all' | ConnectionType;
  search: string;
  tileSize: TileSize;
  onConnect: (conn: Connection) => void;
  onNew: (defaultGroup?: string) => void;
  onEdit: (conn: Connection) => void;
  onCreateGroup: (name: string) => void;
  onSetGroupColor: (groupName: string, color: string) => void;
}

const TYPE_LABELS: Record<string, string> = { ssh: 'SSH', sftp: 'SFTP', ftp: 'FTP' };
const TYPE_ICONS:  Record<string, string> = { ssh: '▶',   sftp: '⇅',    ftp: '≈'   };

export default function ConnectionGrid({
  connections, groups, groupColors, filter, search, tileSize,
  onConnect, onNew, onEdit, onCreateGroup, onSetGroupColor,
}: Props) {
  const [newGroupMode, setNewGroupMode] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');

  const savedConns = useMemo(() =>
    connections.filter(c => {
      if (c.id.startsWith('tab-')) return false;
      const matchType   = filter === 'all' || c.type === filter;
      const matchSearch = !search ||
        c.name.toLowerCase().includes(search.toLowerCase()) ||
        c.host.toLowerCase().includes(search.toLowerCase());
      return matchType && matchSearch;
    }),
    [connections, filter, search]
  );

  const grouped = useMemo(() => {
    const map: Record<string, Connection[]> = {};
    for (const g of groups) map[g] = [];
    const ungrouped: Connection[] = [];
    for (const c of savedConns) {
      if (c.group && groups.includes(c.group)) {
        map[c.group].push(c);
      } else {
        ungrouped.push(c);
      }
    }
    return { byGroup: map, ungrouped };
  }, [savedConns, groups]);

  const commitNewGroup = () => {
    const name = newGroupName.trim();
    if (name) onCreateGroup(name);
    setNewGroupMode(false);
    setNewGroupName('');
  };

  return (
    <div className={`conn-grid-wrapper tile-size-${tileSize}`}>
      <div className="conn-grid">

        {/* ── Tuile "+ Nouvelle connexion" (toujours en premier) ── */}
        <button className="conn-tile conn-tile--add" onClick={() => onNew()}>
          <span className="tile-add-icon">+</span>
          <span className="tile-add-label">Nouvelle connexion</span>
        </button>

        {/* ── Tuile "Nouveau groupe" ───────────────────────────── */}
        {!newGroupMode ? (
          <button className="conn-tile conn-tile--add-group" onClick={() => setNewGroupMode(true)}>
            <span className="tile-add-icon">⊞</span>
            <span className="tile-add-label">Nouveau groupe</span>
          </button>
        ) : (
          <div className="conn-tile conn-tile--new-group-form">
            <span className="tile-add-icon">⊞</span>
            <input
              autoFocus
              className="new-group-input"
              type="text"
              placeholder="Nom du groupe…"
              value={newGroupName}
              onChange={e => setNewGroupName(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter')  commitNewGroup();
                if (e.key === 'Escape') { setNewGroupMode(false); setNewGroupName(''); }
              }}
            />
            <div className="new-group-actions">
              <button className="tile-btn tile-btn--connect" onClick={commitNewGroup}>Créer</button>
              <button className="tile-btn tile-btn--edit"
                onClick={() => { setNewGroupMode(false); setNewGroupName(''); }}>✕</button>
            </div>
          </div>
        )}
      </div>

      {/* ── Sections par groupe ──────────────────────────────────── */}
      {groups.map(group => {
        const items = grouped.byGroup[group] ?? [];
        return (
          <GroupSection
            key={group}
            label={group}
            color={groupColors[group]}
            items={items}
            tileSize={tileSize}
            onConnect={onConnect}
            onEdit={onEdit}
            onNew={() => onNew(group)}
            onSetColor={c => onSetGroupColor(group, c)}
          />
        );
      })}

      {/* ── Connexions sans groupe ───────────────────────────────── */}
      {grouped.ungrouped.length > 0 && (
        <GroupSection
          label="Sans groupe"
          color={undefined}
          items={grouped.ungrouped}
          tileSize={tileSize}
          onConnect={onConnect}
          onEdit={onEdit}
          onNew={() => onNew()}
          isUngrouped
        />
      )}

      {savedConns.length === 0 && (
        <p className="conn-grid-empty">
          {search ? `Aucun résultat pour "${search}".` : 'Aucune connexion enregistrée.'}
        </p>
      )}
    </div>
  );
}

/** localStorage key storing collapsed state per group label: Record<string, boolean> */
const GROUP_COLLAPSED_KEY = 'sshmanager-group-collapsed';

function loadGroupCollapsed(): Record<string, boolean> {
  try {
    const raw = localStorage.getItem(GROUP_COLLAPSED_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function saveGroupCollapsed(state: Record<string, boolean>): void {
  localStorage.setItem(GROUP_COLLAPSED_KEY, JSON.stringify(state));
}

// ── Section groupe ─────────────────────────────────────────────────────────
function GroupSection({
  label, color, items, tileSize, onConnect, onEdit, onNew, onSetColor, isUngrouped = false,
}: {
  label: string;
  color?: string;
  items: Connection[];
  tileSize: TileSize;
  onConnect: (c: Connection) => void;
  onEdit: (c: Connection) => void;
  onNew: () => void;
  onSetColor?: (color: string) => void;
  isUngrouped?: boolean;
}) {
  // Groups are collapsed by default; state is persisted across sessions.
  // The "Sans groupe" section uses a fixed key so it is also persisted.
  const storageKey = isUngrouped ? '__ungrouped__' : label;

  const [collapsed, setCollapsed]       = useState<boolean>(() => {
    const saved = loadGroupCollapsed();
    // If the group has never been opened before, default to collapsed (true)
    return saved[storageKey] ?? true;
  });
  const [showPalette, setShowPalette]   = useState(false);
  // Fixed-position coordinates for the palette popup (avoids any parent overflow clipping)
  const [palettePos, setPalettePos]     = useState<{ top: number; left: number } | null>(null);
  const colorBtnRef                     = useRef<HTMLButtonElement>(null);
  const paletteRef                      = useRef<HTMLDivElement>(null);

  const toggleCollapsed = () => {
    const next = !collapsed;
    setCollapsed(next);
    // Persist the new state for this group
    const all = loadGroupCollapsed();
    all[storageKey] = next;
    saveGroupCollapsed(all);
  };

  const openPalette = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (showPalette) { setShowPalette(false); return; }
    // Compute fixed position from the button's bounding rect
    if (colorBtnRef.current) {
      const rect = colorBtnRef.current.getBoundingClientRect();
      setPalettePos({ top: rect.bottom + 6, left: rect.left + rect.width / 2 });
    }
    setShowPalette(true);
  };

  // Close palette when clicking outside
  useEffect(() => {
    if (!showPalette) return;
    const handler = (e: MouseEvent) => {
      if (paletteRef.current && !paletteRef.current.contains(e.target as Node))
        setShowPalette(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [showPalette]);

  const headerStyle = color
    ? { borderLeft: `3px solid ${color}`, paddingLeft: 10 }
    : {};

  return (
    <div className="grid-group">
      <div className="grid-group-header" style={headerStyle} onClick={toggleCollapsed}>
        <span className="grid-group-chevron">{collapsed ? '▶' : '▼'}</span>
        <span className="grid-group-label">{label}</span>
        <span className="grid-group-count">{items.length}</span>

        {/* Bouton couleur — la palette est rendue en position fixed pour éviter le clipping */}
        {!isUngrouped && onSetColor && (
          <>
            <button
              ref={colorBtnRef}
              className="grid-group-color-btn"
              title="Couleur du groupe"
              style={{ background: color || 'var(--bg-base)', borderColor: color || 'var(--border)' }}
              onClick={openPalette}
            />
            {showPalette && palettePos && (
              <div
                className="group-color-palette"
                ref={paletteRef}
                style={{ position: 'fixed', top: palettePos.top, left: palettePos.left }}
              >
                {/* Option "sans couleur" */}
                <button
                  className={`palette-swatch palette-swatch--none ${!color ? 'active' : ''}`}
                  title="Aucune couleur"
                  onClick={() => { onSetColor(''); setShowPalette(false); }}
                >✕</button>
                {GROUP_PALETTE.map(c => (
                  <button
                    key={c}
                    className={`palette-swatch ${color === c ? 'active' : ''}`}
                    style={{ background: c }}
                    title={c}
                    onClick={() => { onSetColor(c); setShowPalette(false); }}
                  />
                ))}
              </div>
            )}
          </>
        )}

        {!isUngrouped && (
          <button
            className="grid-group-add"
            title={`Ajouter dans ${label}`}
            onClick={e => { e.stopPropagation(); onNew(); }}
          >+</button>
        )}
      </div>

      {!collapsed && (
        <div className="conn-grid conn-grid--group">
          {items.map(conn => (
            <ConnectionTile
              key={conn.id}
              conn={conn}
              groupColor={color}
              tileSize={tileSize}
              onConnect={onConnect}
              onEdit={onEdit}
            />
          ))}
          {items.length === 0 && !isUngrouped && (
            <button className="conn-tile conn-tile--add conn-tile--small" onClick={onNew}>
              <span className="tile-add-icon">+</span>
              <span className="tile-add-label">Ajouter</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// ── Tuile connexion individuelle ────────────────────────────────────────────
function ConnectionTile({
  conn, groupColor, tileSize, onConnect, onEdit,
}: {
  conn: Connection;
  groupColor?: string;
  tileSize: TileSize;
  onConnect: (c: Connection) => void;
  onEdit: (c: Connection) => void;
}) {
  const port = conn.port || (conn.type === 'ftp' ? 21 : 22);
  // En mode XS, le clic simple connecte (pas de bouton visible)
  const handleClick = tileSize === 'xs' ? () => onConnect(conn) : undefined;

  // Couleur de groupe : bordure complète (2px) + légère teinte de fond
  const colorStyle: React.CSSProperties = groupColor
    ? {
        border: `2px solid ${groupColor}`,
        background: `color-mix(in srgb, ${groupColor} 10%, var(--bg-elevated))`,
        boxShadow: `0 0 0 1px ${groupColor}22`,
      }
    : {};

  return (
    <div
      className={`conn-tile conn-tile--${conn.type}`}
      style={colorStyle}
      onDoubleClick={() => onConnect(conn)}
      onClick={handleClick}
      title={`${conn.username}@${conn.host}:${port}${tileSize === 'xs' ? ' — clic pour ouvrir' : ' — double-clic pour ouvrir'}`}
    >
      {/* Ligne 1 : badge type  +  nom  +  favori */}
      <div className="tile-header">
        <span className="tile-type-badge" data-type={conn.type}>
          {TYPE_LABELS[conn.type]}
        </span>
        <span className="tile-name">{conn.name}</span>
        {conn.favorite && <span className="tile-fav">⭐</span>}
      </div>

      {/* Ligne 2 : host:port · user */}
      <div className="tile-meta">
        <span className="tile-host">{conn.host}:{port}</span>
        <span className="tile-meta-sep">·</span>
        <span className="tile-user">{conn.username}</span>
      </div>

      {/* Ligne 3 : actions (masquées en XS) */}
      <div className="tile-actions">
        <button className="tile-btn tile-btn--connect"
          onClick={e => { e.stopPropagation(); onConnect(conn); }}>
          Connecter
        </button>
        <button className="tile-btn tile-btn--edit"
          onClick={e => { e.stopPropagation(); onEdit(conn); }}>✎</button>
      </div>
    </div>
  );
}
