import React, { useState } from 'react';
import { Connection } from '../types';

interface Props {
  connections: Connection[];
  recentIds: string[];           // IDs ordonnés du plus récent au plus ancien
  groups: string[];
  activeConnectionId: string | null;
  onConnect: (conn: Connection) => void;
  onNew: (defaultGroup?: string) => void;
  onEdit: (conn: Connection) => void;
  onDelete: (id: string) => void;
  onToggleFavorite: (id: string) => void;
  onRenameGroup: (oldName: string, newName: string) => void;
  onDeleteGroup: (name: string) => void;
  onCreateGroup: (name: string) => void;
  onExport: () => void;
  onImport: () => void;
}

const TYPE_ICONS:  Record<string, string> = { ssh: '▶', sftp: '⇅', ftp: '≈' };

export default function Sidebar({
  connections, recentIds, groups, activeConnectionId,
  onConnect, onNew, onEdit, onDelete, onToggleFavorite,
  onRenameGroup, onDeleteGroup, onCreateGroup, onExport, onImport,
}: Props) {
  const [ctx, setCtx] = useState<
    | { kind: 'conn'; conn: Connection; x: number; y: number }
    | null
  >(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);
  const [newGroupMode, setNewGroupMode]   = useState(false);
  const [newGroupName, setNewGroupName]   = useState('');

  const closeCtx = () => { setCtx(null); setDeleteConfirm(null); };

  const commitNewGroup = () => {
    const n = newGroupName.trim();
    if (n && !groups.includes(n)) onCreateGroup(n);
    setNewGroupMode(false);
    setNewGroupName('');
  };

  // Connexions récentes — dans l'ordre d'utilisation
  const savedConns = connections.filter(c => !c.id.startsWith('tab-'));
  const recentConns: Connection[] = recentIds
    .map(id => savedConns.find(c => c.id === id))
    .filter((c): c is Connection => c !== undefined);

  const totalSaved = savedConns.length;

  return (
    <div className="sidebar" onClick={closeCtx}>

      {/* ── Bouton nouvelle connexion ─────────────────────────────── */}
      <button className="sidebar-new-conn-btn" onClick={() => onNew()}>
        + Nouvelle connexion
      </button>

      {/* ── Barre d'outils : nouveau groupe ──────────────────────── */}
      <div className="sidebar-toolbar">
        <span className="sidebar-section-title">Récentes</span>
        <div style={{ display: 'flex', gap: 4 }}>
          <button className="btn-icon" title="Nouveau groupe"
            onClick={e => { e.stopPropagation(); setNewGroupMode(true); }}>⊞</button>
        </div>
      </div>

      {newGroupMode && (
        <div className="sidebar-new-group">
          <input
            autoFocus type="text" placeholder="Nom du groupe…"
            value={newGroupName}
            onChange={e => setNewGroupName(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter')  commitNewGroup();
              if (e.key === 'Escape') { setNewGroupMode(false); setNewGroupName(''); }
            }}
          />
          <button className="btn-icon small" onClick={commitNewGroup} title="Créer">✓</button>
          <button className="btn-icon small"
            onClick={() => { setNewGroupMode(false); setNewGroupName(''); }} title="Annuler">✕</button>
        </div>
      )}

      {/* ── Liste connexions récentes ─────────────────────────────── */}
      <div className="conn-list">
        {recentConns.length === 0 && (
          <div className="conn-empty" style={{ paddingTop: 16 }}>
            Aucune connexion récente.{'\n'}
            Double-cliquez sur une tuile pour vous connecter.
          </div>
        )}

        {recentConns.map(conn => (
          <div
            key={conn.id}
            className={`conn-item-compact ${conn.id === activeConnectionId ? 'active' : ''}`}
            onDoubleClick={() => onConnect(conn)}
            onContextMenu={e => { e.preventDefault(); setCtx({ kind: 'conn', conn, x: e.clientX, y: e.clientY }); }}
            title={`${conn.username}@${conn.host}:${conn.port || (conn.type === 'ftp' ? 21 : 22)}`}
          >
            <span className="compact-type-dot" data-type={conn.type} />
            <div className="compact-info">
              <span className="compact-name">{conn.name}</span>
              <span className="compact-host">{conn.host}</span>
            </div>
            {conn.favorite && <span className="compact-fav">⭐</span>}
          </div>
        ))}
      </div>

      {/* ── Pied de sidebar ───────────────────────────────────────── */}
      <div className="sidebar-footer">
        <span>{totalSaved} conn.</span>
        <div style={{ display: 'flex', gap: 4 }}>
          <button className="btn-icon small" title="Importer" onClick={onImport}>⬆</button>
          <button className="btn-icon small" title="Exporter" onClick={onExport}>⬇</button>
        </div>
      </div>

      {/* ── Menu contextuel ───────────────────────────────────────── */}
      {ctx?.kind === 'conn' && (
        <div className="context-menu" style={{ top: ctx.y, left: ctx.x }}
          onClick={e => e.stopPropagation()}>
          <div className="ctx-item" onClick={() => { onConnect(ctx.conn); closeCtx(); }}>▶ Connecter</div>
          <div className="ctx-item" onClick={() => { onEdit(ctx.conn); closeCtx(); }}>✎ Modifier</div>
          <div className="ctx-item" onClick={() => { onToggleFavorite(ctx.conn.id); closeCtx(); }}>
            {ctx.conn.favorite ? '☆ Retirer des favoris' : '⭐ Ajouter aux favoris'}
          </div>
          {ctx.conn.type === 'ssh' && (
            <>
              <div className="ctx-separator" />
              <div className="ctx-item"
                onClick={() => { window.electronAPI.sshOpenInPowerShell(ctx.conn); closeCtx(); }}>
                ⊞ Ouvrir dans PowerShell
              </div>
              <div className="ctx-item"
                onClick={() => { window.electronAPI.sshOpenInCmd(ctx.conn); closeCtx(); }}>
                ▣ Ouvrir dans CMD
              </div>
            </>
          )}
          <div className="ctx-separator" />
          {deleteConfirm === ctx.conn.id ? (
            <div className="ctx-item danger"
              onClick={() => { onDelete(ctx.conn.id); closeCtx(); }}>
              ⚠ Confirmer la suppression
            </div>
          ) : (
            <div className="ctx-item danger"
              onClick={() => setDeleteConfirm(ctx.conn.id)}>✕ Supprimer</div>
          )}
        </div>
      )}
    </div>
  );
}
