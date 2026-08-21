import React, { useEffect, useState } from 'react';
import { Connection, ConnectionType } from '../types';
import { formatLastUsed } from '../format';

interface Props {
  connection: Connection | null;
  groupColor?: string;
  onConnect: (conn: Connection) => void;
  onOpenAs: (conn: Connection, type: ConnectionType) => void;
  onEdit: (conn: Connection) => void;
  onDelete: (id: string) => void;
}

/** "SHA256:kQ8xR…7fA" — enough to compare at a glance, short enough for the panel. */
function shortenFingerprint(fingerprint: string): string {
  if (fingerprint.length <= 22) return fingerprint;
  return `${fingerprint.slice(0, 13)}…${fingerprint.slice(-3)}`;
}

function keyFileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

export default function ConnectionInspector({
  connection, groupColor, onConnect, onOpenAs, onEdit, onDelete,
}: Props) {
  const [fingerprint, setFingerprint] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState(false);

  const usesHostKey = connection?.type === 'ssh' || connection?.type === 'sftp';

  useEffect(() => {
    setDeleteConfirm(false);
    if (!connection || !usesHostKey) { setFingerprint(null); return; }

    let current = true;
    window.electronAPI
      .sshGetHostKeyFingerprint({ host: connection.host, port: connection.port || 22 })
      .then(value => { if (current) setFingerprint(value); });
    return () => { current = false; };
  }, [connection, usesHostKey]);

  if (!connection) {
    return (
      <aside className="conn-inspector conn-inspector--empty">
        <span className="inspector-label">Connexion</span>
        <p className="inspector-placeholder">
          Sélectionnez une connexion pour voir son détail.
        </p>
      </aside>
    );
  }

  const port = connection.port || (connection.type === 'ftp' ? 21 : 22);
  const isFileTransfer = connection.type === 'sftp' || connection.type === 'ftp';

  return (
    <aside className="conn-inspector">
      <div className="inspector-head">
        <span className="inspector-label">Connexion</span>
        <h2 className="inspector-name">
          {groupColor && <span className="inspector-dot" style={{ background: groupColor }} />}
          {connection.name}
        </h2>
        <span className="inspector-target">{connection.username}@{connection.host}:{port}</span>
      </div>

      <dl className="inspector-facts">
        <div className="inspector-fact">
          <dt>Groupe</dt>
          <dd>{connection.group || 'Sans groupe'}</dd>
        </div>
        <div className="inspector-fact">
          <dt>Authentification</dt>
          <dd>{connection.privateKeyPath ? 'Clé privée' : connection.password ? 'Mot de passe enregistré' : 'Mot de passe demandé'}</dd>
        </div>
        {connection.privateKeyPath && (
          <div className="inspector-fact">
            <dt>Clé</dt>
            <dd className="mono" title={connection.privateKeyPath}>{keyFileName(connection.privateKeyPath)}</dd>
          </div>
        )}
        {usesHostKey && (
          <div className="inspector-fact">
            <dt>Empreinte</dt>
            <dd className="mono muted" title={fingerprint ?? undefined}>
              {fingerprint ? shortenFingerprint(fingerprint) : 'pas encore épinglée'}
            </dd>
          </div>
        )}
        <div className="inspector-fact">
          <dt>Favori</dt>
          <dd className={connection.favorite ? 'accent' : ''}>{connection.favorite ? 'Oui' : 'Non'}</dd>
        </div>
      </dl>

      <div className="inspector-actions">
        <button className="inspector-btn inspector-btn--primary" onClick={() => onConnect(connection)}>
          {isFileTransfer ? 'Ouvrir les fichiers' : 'Ouvrir un terminal'}
        </button>
        {connection.type === 'ssh' && (
          <button className="inspector-btn" onClick={() => onOpenAs(connection, 'sftp')}>Ouvrir en SFTP</button>
        )}
        {connection.type === 'sftp' && (
          <button className="inspector-btn" onClick={() => onOpenAs(connection, 'ssh')}>Ouvrir un terminal</button>
        )}
        <button className="inspector-btn" onClick={() => onEdit(connection)}>Modifier</button>
        {deleteConfirm ? (
          <button
            className="inspector-btn inspector-btn--danger"
            onClick={() => { onDelete(connection.id); setDeleteConfirm(false); }}
          >⚠ Confirmer la suppression</button>
        ) : (
          <button className="inspector-btn inspector-btn--danger" onClick={() => setDeleteConfirm(true)}>
            Supprimer
          </button>
        )}
      </div>

      <span className="inspector-spacer" />

      <div className="inspector-footer">
        Dernier accès<br />
        {connection.lastUsedAt
          ? `${new Date(connection.lastUsedAt).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' })} · ${formatLastUsed(connection.lastUsedAt)}`
          : 'jamais ouverte'}
      </div>
    </aside>
  );
}
