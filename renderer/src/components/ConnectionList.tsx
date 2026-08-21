import React, { useState, useMemo, useRef, useEffect, useLayoutEffect } from 'react';
import { createPortal } from 'react-dom';
import { Connection } from '../types';
import { CONNECTION_COLOR_PALETTE } from '../connection-colors';
import { formatLastUsed } from '../format';
import ConnectionInspector from './ConnectionInspector';

const CONTEXT_MENU_MARGIN = 8;
const SORT_KEY = 'sshmanager-list-sort';

export type SortColumn = 'name' | 'host' | 'username' | 'type' | 'lastUsedAt';
interface SortState { column: SortColumn; ascending: boolean; }

const COLUMNS: { column: SortColumn; label: string }[] = [
  { column: 'name',       label: 'Nom' },
  { column: 'host',       label: 'Hôte' },
  { column: 'username',   label: 'Utilisateur' },
  { column: 'type',       label: 'Type' },
  { column: 'lastUsedAt', label: 'Accès' },
];

const TYPE_LABELS: Record<string, string> = { ssh: 'SSH', sftp: 'SFTP', ftp: 'FTP' };

function loadSort(): SortState {
  try {
    const raw = localStorage.getItem(SORT_KEY);
    if (raw) return JSON.parse(raw) as SortState;
  } catch { /* réglage illisible : on repart du tri par défaut */ }
  return { column: 'name', ascending: true };
}

/** Keeps a fixed context menu inside the viewport (flip upward near the bottom). */
function clampContextMenuPosition(
  anchorX: number,
  anchorY: number,
  menuWidth: number,
  menuHeight: number,
): { x: number; y: number } {
  let x = anchorX;
  let y = anchorY;

  if (x + menuWidth > window.innerWidth - CONTEXT_MENU_MARGIN) {
    x = Math.max(CONTEXT_MENU_MARGIN, window.innerWidth - menuWidth - CONTEXT_MENU_MARGIN);
  }
  if (y + menuHeight > window.innerHeight - CONTEXT_MENU_MARGIN) {
    y = Math.max(CONTEXT_MENU_MARGIN, anchorY - menuHeight);
  }
  if (x < CONTEXT_MENU_MARGIN) x = CONTEXT_MENU_MARGIN;
  if (y < CONTEXT_MENU_MARGIN) y = CONTEXT_MENU_MARGIN;

  return { x, y };
}

function compare(a: Connection, b: Connection, column: SortColumn): number {
  if (column === 'lastUsedAt') return (a.lastUsedAt ?? 0) - (b.lastUsedAt ?? 0);
  return String(a[column] ?? '').localeCompare(String(b[column] ?? ''), 'fr', { numeric: true });
}

interface Props {
  /** Connections to display, already narrowed by group, type and search. */
  connections: Connection[];
  groupColors: Record<string, string>;
  /** Non-empty when the current view is the result of a text search. */
  search: string;
  onConnect: (conn: Connection) => void;
  onOpenAs: (conn: Connection, type: Connection['type']) => void;
  onNew: () => void;
  onEdit: (conn: Connection) => void;
  onDelete: (id: string) => void;
  onToggleFavorite: (id: string) => void;
  onSetConnectionColor: (id: string, color: string) => void;
}

export default function ConnectionList({
  connections, groupColors, search,
  onConnect, onOpenAs, onNew, onEdit, onDelete, onToggleFavorite, onSetConnectionColor,
}: Props) {
  const [sort, setSort] = useState<SortState>(loadSort);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ conn: Connection; x: number; y: number } | null>(null);
  const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(null);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const rowsRef = useRef<HTMLDivElement>(null);

  const sorted = useMemo(() => {
    const list = [...connections].sort((a, b) => compare(a, b, sort.column));
    return sort.ascending ? list : list.reverse();
  }, [connections, sort]);

  // Keep the selection on an existing row when the filters change.
  useEffect(() => {
    if (sorted.length === 0) { setSelectedId(null); return; }
    if (!sorted.some(c => c.id === selectedId)) setSelectedId(sorted[0].id);
  }, [sorted, selectedId]);

  const selected = sorted.find(c => c.id === selectedId) ?? null;

  useEffect(() => {
    if (!contextMenu) return;
    const close = () => { setContextMenu(null); setMenuPosition(null); setDeleteConfirmId(null); };
    document.addEventListener('click', close);
    return () => document.removeEventListener('click', close);
  }, [contextMenu]);

  useLayoutEffect(() => {
    if (!contextMenu || !menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    setMenuPosition(clampContextMenuPosition(contextMenu.x, contextMenu.y, rect.width, rect.height));
  }, [contextMenu, deleteConfirmId]);

  const applySort = (column: SortColumn) => {
    setSort(prev => {
      const next: SortState = prev.column === column
        ? { column, ascending: !prev.ascending }
        // Le dernier accès se lit du plus récent au plus ancien.
        : { column, ascending: column !== 'lastUsedAt' };
      localStorage.setItem(SORT_KEY, JSON.stringify(next));
      return next;
    });
  };

  const moveSelection = (delta: number) => {
    if (sorted.length === 0) return;
    const current = sorted.findIndex(c => c.id === selectedId);
    const next = Math.min(sorted.length - 1, Math.max(0, (current === -1 ? 0 : current) + delta));
    setSelectedId(sorted[next].id);
    rowsRef.current?.querySelectorAll('.conn-row')[next]?.scrollIntoView({ block: 'nearest' });
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown')      { e.preventDefault(); moveSelection(1); }
    else if (e.key === 'ArrowUp')   { e.preventDefault(); moveSelection(-1); }
    else if (e.key === 'Enter' && selected) { e.preventDefault(); onConnect(selected); }
    else if (e.key === 'F2' && selected)    { e.preventDefault(); onEdit(selected); }
  };

  return (
    <div className="conn-list-pane">
      <div className="conn-table" tabIndex={0} onKeyDown={handleKeyDown}>
        <div className="conn-row conn-row-header">
          {COLUMNS.map(({ column, label }) => (
            <button
              key={column}
              className={`conn-col-btn ${sort.column === column ? 'active' : ''}`}
              onClick={() => applySort(column)}
              title={`Trier par ${label.toLowerCase()}`}
            >
              {label}
              {sort.column === column && <span className="conn-col-arrow">{sort.ascending ? '▲' : '▼'}</span>}
            </button>
          ))}
        </div>

        <div className="conn-rows" ref={rowsRef}>
          {sorted.map(conn => {
            const port = conn.port || (conn.type === 'ftp' ? 21 : 22);
            const accent = conn.color || groupColors[conn.group ?? ''] || undefined;
            return (
              <div
                key={conn.id}
                className={`conn-row ${conn.id === selectedId ? 'selected' : ''}`}
                onClick={() => setSelectedId(conn.id)}
                onDoubleClick={() => onConnect(conn)}
                onContextMenu={e => {
                  e.preventDefault();
                  setSelectedId(conn.id);
                  setMenuPosition({ x: e.clientX, y: e.clientY });
                  setContextMenu({ conn, x: e.clientX, y: e.clientY });
                  setDeleteConfirmId(null);
                }}
              >
                <span className="conn-cell-name">
                  <span className="conn-row-dot" style={{ background: accent || 'var(--border)' }} />
                  {conn.name}
                  {conn.favorite && <span className="conn-row-fav">⭐</span>}
                </span>
                <span className="conn-cell-mono">{conn.host}:{port}</span>
                <span className="conn-cell-mono">{conn.username}</span>
                <span className={`conn-type-pill ${conn.id === selectedId ? 'accent' : ''}`}>{TYPE_LABELS[conn.type]}</span>
                <span className="conn-cell-faint">{formatLastUsed(conn.lastUsedAt)}</span>
              </div>
            );
          })}

          {sorted.length === 0 && (
            <div className="conn-rows-empty">
              {search
                ? `Aucun résultat pour « ${search} ».`
                : 'Aucune connexion dans cette vue.'}
              <button className="btn-secondary small" onClick={onNew}>+ Nouvelle connexion</button>
            </div>
          )}
        </div>

        <div className="conn-table-hint">Entrée pour ouvrir · ↑↓ pour naviguer · F2 pour modifier</div>
      </div>

      <ConnectionInspector
        connection={selected}
        groupColor={selected ? groupColors[selected.group ?? ''] : undefined}
        onConnect={onConnect}
        onOpenAs={onOpenAs}
        onEdit={onEdit}
        onDelete={onDelete}
      />

      {contextMenu && menuPosition && createPortal(
        <div
          ref={menuRef}
          className="context-menu"
          style={{ top: menuPosition.y, left: menuPosition.x }}
          onClick={e => e.stopPropagation()}
        >
          <button className="ctx-item" onClick={() => { onConnect(contextMenu.conn); setContextMenu(null); }}>
            ▶ Connecter
          </button>
          <button className="ctx-item" onClick={() => { onEdit(contextMenu.conn); setContextMenu(null); }}>
            ✎ Modifier
          </button>
          <button className="ctx-item" onClick={() => { onToggleFavorite(contextMenu.conn.id); setContextMenu(null); }}>
            {contextMenu.conn.favorite ? '☆ Retirer des favoris' : '⭐ Ajouter aux favoris'}
          </button>
          {contextMenu.conn.type === 'ssh' && (
            <>
              <div className="ctx-separator" />
              <button className="ctx-item"
                onClick={() => { window.electronAPI.sshOpenInPowerShell(contextMenu.conn); setContextMenu(null); }}>
                ⊞ Ouvrir dans PowerShell
              </button>
              <button className="ctx-item"
                onClick={() => { window.electronAPI.sshOpenInCmd(contextMenu.conn); setContextMenu(null); }}>
                ▣ Ouvrir dans CMD
              </button>
            </>
          )}
          <div className="ctx-color-label">Couleur de la connexion</div>
          <div className="ctx-color-row">
            <button
              className={`palette-swatch palette-swatch--none ${!contextMenu.conn.color ? 'active' : ''}`}
              title="Couleur du groupe / aucune"
              onClick={() => { onSetConnectionColor(contextMenu.conn.id, ''); setContextMenu(null); }}
            >✕</button>
            {CONNECTION_COLOR_PALETTE.map(color => (
              <button
                key={color}
                className={`palette-swatch ${contextMenu.conn.color === color ? 'active' : ''}`}
                style={{ background: color }}
                title={color}
                onClick={() => { onSetConnectionColor(contextMenu.conn.id, color); setContextMenu(null); }}
              />
            ))}
          </div>
          <div className="ctx-separator" />
          {deleteConfirmId === contextMenu.conn.id ? (
            <button className="ctx-item danger" onClick={() => {
              onDelete(contextMenu.conn.id);
              setContextMenu(null);
              setDeleteConfirmId(null);
            }}>⚠ Confirmer la suppression</button>
          ) : (
            <button className="ctx-item danger" onClick={() => setDeleteConfirmId(contextMenu.conn.id)}>
              ✕ Supprimer
            </button>
          )}
        </div>,
        document.body,
      )}
    </div>
  );
}
