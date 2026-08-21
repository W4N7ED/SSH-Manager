import React, { useState, useMemo, useRef, useEffect } from 'react';
import { Connection, GroupFilter } from '../types';
import { CONNECTION_COLOR_PALETTE } from '../connection-colors';

interface Props {
  connections: Connection[];
  groups: string[];
  groupColors: Record<string, string>;
  selection: GroupFilter;
  onSelect: (selection: GroupFilter) => void;
  onCreateGroup: (name: string) => void;
  onRenameGroup: (oldName: string, newName: string) => void;
  onDeleteGroup: (name: string) => void;
  onSetGroupColor: (name: string, color: string) => void;
  onImport: () => void;
  onExport: () => void;
}

interface GroupContextMenu { name: string; x: number; y: number; }

function isSameSelection(a: GroupFilter, b: GroupFilter): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === 'group' && b.kind === 'group') return a.name === b.name;
  return true;
}

/**
 * Left rail listing every way to slice the saved connections: everything,
 * favorites, each user group, and the connections left ungrouped.
 */
export default function GroupRail({
  connections, groups, groupColors, selection, onSelect,
  onCreateGroup, onRenameGroup, onDeleteGroup, onSetGroupColor,
  onImport, onExport,
}: Props) {
  const [newGroupMode, setNewGroupMode] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [contextMenu, setContextMenu] = useState<GroupContextMenu | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const counts = useMemo(() => {
    const byGroup: Record<string, number> = {};
    for (const group of groups) byGroup[group] = 0;
    let favorites = 0;
    let ungrouped = 0;
    for (const conn of connections) {
      if (conn.favorite) favorites++;
      if (conn.group && groups.includes(conn.group)) byGroup[conn.group]++;
      else ungrouped++;
    }
    return { byGroup, favorites, ungrouped, total: connections.length };
  }, [connections, groups]);

  useEffect(() => {
    if (!contextMenu) return;
    const close = (e: MouseEvent) => {
      if (menuRef.current?.contains(e.target as Node)) return;
      setContextMenu(null);
      setDeleteConfirm(null);
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, [contextMenu]);

  const commitNewGroup = () => {
    const name = newGroupName.trim();
    if (name && !groups.includes(name)) onCreateGroup(name);
    setNewGroupMode(false);
    setNewGroupName('');
  };

  const commitRename = () => {
    const name = renameValue.trim();
    if (renaming && name && name !== renaming) onRenameGroup(renaming, name);
    setRenaming(null);
    setRenameValue('');
  };

  const renderItem = (
    key: string,
    filter: GroupFilter,
    label: React.ReactNode,
    count: number,
    marker?: React.ReactNode,
    onContextMenu?: (e: React.MouseEvent) => void,
  ) => {
    const active = isSameSelection(selection, filter);
    return (
      <button
        key={key}
        className={`rail-item ${active ? 'active' : ''}`}
        onClick={() => onSelect(filter)}
        onContextMenu={onContextMenu}
      >
        {marker}
        <span className="rail-item-label">{label}</span>
        <span className="rail-item-count">{count}</span>
      </button>
    );
  };

  return (
    <div className="group-rail">
      <div className="rail-title">Groupes</div>

      <div className="rail-items">
        {renderItem('all', { kind: 'all' }, 'Toutes', counts.total)}
        {renderItem('favorites', { kind: 'favorites' }, 'Favoris', counts.favorites)}

        {groups.map(group => {
          if (renaming === group) {
            return (
              <input
                key={group}
                autoFocus
                type="text"
                className="rail-rename-input"
                value={renameValue}
                onChange={e => setRenameValue(e.target.value)}
                onBlur={commitRename}
                onKeyDown={e => {
                  if (e.key === 'Enter') commitRename();
                  if (e.key === 'Escape') { setRenaming(null); setRenameValue(''); }
                }}
              />
            );
          }
          return renderItem(
            group,
            { kind: 'group', name: group },
            group,
            counts.byGroup[group] ?? 0,
            <span className="rail-item-dot" style={{ background: groupColors[group] || 'var(--border)' }} />,
            e => {
              e.preventDefault();
              setContextMenu({ name: group, x: e.clientX, y: e.clientY });
              setDeleteConfirm(null);
            },
          );
        })}

        {renderItem('ungrouped', { kind: 'ungrouped' }, 'Sans groupe', counts.ungrouped)}
      </div>

      <div className="rail-new-group">
        {newGroupMode ? (
          <input
            autoFocus
            type="text"
            className="rail-rename-input"
            placeholder="Nom du groupe…"
            value={newGroupName}
            onChange={e => setNewGroupName(e.target.value)}
            onBlur={commitNewGroup}
            onKeyDown={e => {
              if (e.key === 'Enter') commitNewGroup();
              if (e.key === 'Escape') { setNewGroupMode(false); setNewGroupName(''); }
            }}
          />
        ) : (
          <button className="rail-action" onClick={() => setNewGroupMode(true)}>+ Nouveau groupe</button>
        )}
      </div>

      <span className="rail-spacer" />

      <div className="rail-footer">
        <button className="rail-action" onClick={onImport}>Importer</button>
        <button className="rail-action" onClick={onExport}>Exporter</button>
      </div>

      {contextMenu && (
        <div className="context-menu" ref={menuRef} style={{ top: contextMenu.y, left: contextMenu.x }}>
          <button
            className="ctx-item"
            onClick={() => {
              setRenaming(contextMenu.name);
              setRenameValue(contextMenu.name);
              setContextMenu(null);
            }}
          >✎ Renommer</button>
          <div className="ctx-color-label">Couleur du groupe</div>
          <div className="ctx-color-row">
            <button
              className={`palette-swatch palette-swatch--none ${!groupColors[contextMenu.name] ? 'active' : ''}`}
              title="Aucune couleur"
              onClick={() => { onSetGroupColor(contextMenu.name, ''); setContextMenu(null); }}
            >✕</button>
            {CONNECTION_COLOR_PALETTE.map(color => (
              <button
                key={color}
                className={`palette-swatch ${groupColors[contextMenu.name] === color ? 'active' : ''}`}
                style={{ background: color }}
                title={color}
                onClick={() => { onSetGroupColor(contextMenu.name, color); setContextMenu(null); }}
              />
            ))}
          </div>
          <div className="ctx-separator" />
          {deleteConfirm === contextMenu.name ? (
            <button
              className="ctx-item danger"
              onClick={() => {
                onDeleteGroup(contextMenu.name);
                if (isSameSelection(selection, { kind: 'group', name: contextMenu.name })) onSelect({ kind: 'all' });
                setContextMenu(null);
                setDeleteConfirm(null);
              }}
            >⚠ Confirmer la suppression</button>
          ) : (
            <button className="ctx-item danger" onClick={() => setDeleteConfirm(contextMenu.name)}>
              ✕ Supprimer le groupe
            </button>
          )}
        </div>
      )}
    </div>
  );
}
