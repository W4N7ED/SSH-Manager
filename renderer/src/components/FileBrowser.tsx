import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { Tab, Connection } from '../types';

interface Props {
  tab: Tab;
  connection: Connection;
  onStatusChange: (tabId: string, status: Tab['status'], error?: string) => void;
  onClose: () => void;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
  if (bytes < 1024 * 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + ' MB';
  return (bytes / 1024 / 1024 / 1024).toFixed(2) + ' GB';
}

function formatDate(ts?: number): string {
  if (!ts) return '—';
  return new Date(ts * 1000).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' });
}

/** "rwxr-x---" from whichever shape the protocol returned, or "—". */
function formatRights(entry: FileEntry): string {
  if (entry.rights) return `${entry.rights.user}${entry.rights.group}${entry.rights.other}`;
  if (entry.permissions) {
    const triplet = (bits: number) =>
      (bits & 4 ? 'r' : '-') + (bits & 2 ? 'w' : '-') + (bits & 1 ? 'x' : '-');
    return triplet(entry.permissions.user) + triplet(entry.permissions.group) + triplet(entry.permissions.world);
  }
  return '—';
}

const isDirectory = (entry: FileEntry) => entry.type === 'd' || entry.type === 'directory';

export default function FileBrowser({ tab, connection, onStatusChange, onClose }: Props) {
  const [currentPath, setCurrentPath] = useState('/');
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [newFolderMode, setNewFolderMode] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(null);

  const isFtp = tab.connectionType === 'ftp';

  const showMsg = (text: string, ok = true) => {
    setMessage({ text, ok });
    setTimeout(() => setMessage(null), 4000);
  };

  const connect = useCallback(async () => {
    setLoading(true);
    setError(null);
    const result = isFtp
      ? await window.electronAPI.ftpConnect({ tabId: tab.id, connection })
      : await window.electronAPI.sftpConnect({ tabId: tab.id, connection });

    if (result.success) {
      onStatusChange(tab.id, 'connected');
    } else {
      onStatusChange(tab.id, 'error', result.error);
      setError(result.error ?? 'Erreur de connexion');
      setLoading(false);
    }
  }, [tab.id, connection, isFtp, onStatusChange]);

  const listDir = useCallback(async (path: string) => {
    setLoading(true);
    setSelected(null);
    const result = isFtp
      ? await window.electronAPI.ftpList({ tabId: tab.id, remotePath: path })
      : await window.electronAPI.sftpList({ tabId: tab.id, remotePath: path });

    if (result.success && result.list) {
      const sorted = [...result.list].sort((a, b) => {
        if (isDirectory(a) && !isDirectory(b)) return -1;
        if (!isDirectory(a) && isDirectory(b)) return 1;
        return a.name.localeCompare(b.name);
      });
      setFiles(sorted);
      setCurrentPath(path);
    } else {
      showMsg(result.error ?? 'Impossible de lister le répertoire', false);
    }
    setLoading(false);
  }, [tab.id, isFtp]);

  useEffect(() => {
    connect().then(() => listDir('/'));
    return () => {
      if (isFtp) window.electronAPI.ftpDisconnect(tab.id);
      else window.electronAPI.sftpDisconnect(tab.id);
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const pathSegments = currentPath.split('/').filter(Boolean);

  const remotePathOf = (name: string) =>
    currentPath.endsWith('/') ? currentPath + name : currentPath + '/' + name;

  const openEntry = (entry: FileEntry) => {
    if (isDirectory(entry)) listDir(remotePathOf(entry.name));
  };

  const goUp = () => {
    if (currentPath === '/') return;
    const parts = pathSegments.slice(0, -1);
    listDir('/' + parts.join('/'));
  };

  const goToSegment = (index: number) => listDir('/' + pathSegments.slice(0, index + 1).join('/'));

  const totalSize = useMemo(
    () => files.filter(f => !isDirectory(f)).reduce((sum, f) => sum + (f.size ?? 0), 0),
    [files],
  );

  const download = async () => {
    if (!selected) return;
    const result = isFtp
      ? await window.electronAPI.ftpDownload({ tabId: tab.id, remotePath: remotePathOf(selected) })
      : await window.electronAPI.sftpDownload({ tabId: tab.id, remotePath: remotePathOf(selected) });
    if (result.success) showMsg('Téléchargement terminé');
    else if (result.error) showMsg(result.error, false);
  };

  const upload = async () => {
    const remotePath = currentPath.endsWith('/') ? currentPath : currentPath + '/';
    const result = isFtp
      ? await window.electronAPI.ftpUpload({ tabId: tab.id, remotePath })
      : await window.electronAPI.sftpUpload({ tabId: tab.id, remotePath });
    if (result.success) { showMsg('Envoi terminé'); listDir(currentPath); }
    else if (result.error) showMsg(result.error, false);
  };

  const deleteEntry = async () => {
    if (!selected) return;
    const entry = files.find(f => f.name === selected);
    if (!entry) return;
    if (!confirm(`Supprimer « ${selected} » ?`)) return;
    const isDir = isDirectory(entry);
    const result = isFtp
      ? await window.electronAPI.ftpDelete({ tabId: tab.id, remotePath: remotePathOf(selected), isDir })
      : await window.electronAPI.sftpDelete({ tabId: tab.id, remotePath: remotePathOf(selected), isDir });
    if (result.success) { showMsg('Supprimé'); listDir(currentPath); }
    else showMsg(result.error ?? 'Erreur de suppression', false);
  };

  const createFolder = async () => {
    const name = newFolderName.trim();
    if (!name) return;
    const result = isFtp
      ? await window.electronAPI.ftpMkdir({ tabId: tab.id, remotePath: remotePathOf(name) })
      : await window.electronAPI.sftpMkdir({ tabId: tab.id, remotePath: remotePathOf(name) });
    if (result.success) { showMsg('Dossier créé'); listDir(currentPath); }
    else showMsg(result.error ?? 'Erreur de création du dossier', false);
    setNewFolderMode(false);
    setNewFolderName('');
  };

  if (error) {
    return (
      <div className="filebrowser-error">
        <div className="error-icon">⚠</div>
        <h3>Connexion échouée</h3>
        <p>{error}</p>
        <div className="term-error-actions">
          <button className="btn-primary" onClick={() => connect().then(() => listDir('/'))}>Réessayer</button>
          <button className="btn-secondary" onClick={onClose}>Fermer</button>
        </div>
      </div>
    );
  }

  return (
    <div className="filebrowser">
      <div className="fb-header">
        <button className="btn-round" title="Dossier parent" onClick={goUp} disabled={currentPath === '/'}>↑</button>
        <button className="btn-round" title="Actualiser" onClick={() => listDir(currentPath)}>↺</button>

        <div className="fb-breadcrumb">
          <span className="bc-item" onClick={() => listDir('/')}>/</span>
          {pathSegments.map((segment, index) => (
            <React.Fragment key={index}>
              {index > 0 && <span className="bc-sep">/</span>}
              <span
                className={`bc-item ${index === pathSegments.length - 1 ? 'current' : ''}`}
                onClick={() => goToSegment(index)}
              >{segment}</span>
            </React.Fragment>
          ))}
        </div>

        <span className="fb-spacer" />

        <button className="btn-toolbar" onClick={upload}>Envoyer</button>
        <button className="btn-toolbar" onClick={download} disabled={!selected}>Télécharger</button>
        <button className="btn-toolbar" onClick={() => setNewFolderMode(true)}>Nouveau dossier</button>
        <button className="btn-toolbar danger" onClick={deleteEntry} disabled={!selected}>Supprimer</button>
        <button className="btn-round" title="Fermer l'onglet" onClick={onClose}>✕</button>
      </div>

      {newFolderMode && (
        <div className="fb-newdir">
          <input
            autoFocus
            type="text"
            placeholder="Nom du dossier"
            value={newFolderName}
            onChange={e => setNewFolderName(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') createFolder(); if (e.key === 'Escape') setNewFolderMode(false); }}
          />
          <button className="btn-primary small" onClick={createFolder}>Créer</button>
          <button className="btn-secondary small" onClick={() => setNewFolderMode(false)}>Annuler</button>
        </div>
      )}

      <div className="fb-columns">
        <span>Nom</span><span>Taille</span><span>Modifié</span><span>Droits</span>
      </div>

      {loading ? (
        <div className="fb-loading">Chargement…</div>
      ) : (
        <div className="fb-rows">
          {currentPath !== '/' && (
            <div className="fb-row fb-row--parent" onDoubleClick={goUp}>
              <span className="fb-cell-name mono">..</span>
              <span /><span /><span />
            </div>
          )}

          {files.map(file => {
            const directory = isDirectory(file);
            return (
              <div
                key={file.name}
                className={`fb-row ${selected === file.name ? 'selected' : ''}`}
                onClick={() => setSelected(file.name)}
                onDoubleClick={() => openEntry(file)}
              >
                <span className="fb-cell-name">
                  <span className={`fb-marker ${directory ? 'fb-marker--dir' : ''}`} />
                  {file.name}
                </span>
                <span className="fb-cell-size">{directory ? '—' : formatSize(file.size)}</span>
                <span className="fb-cell-date">{formatDate(file.modifyTime)}</span>
                <span className="fb-cell-rights">{formatRights(file)}</span>
              </div>
            );
          })}

          {files.length === 0 && <div className="fb-empty">Dossier vide</div>}
        </div>
      )}

      <div className="fb-footer">
        <span>{files.length} élément{files.length > 1 ? 's' : ''} · {formatSize(totalSize)}</span>
        {selected && <span>1 sélectionné</span>}
        <span className="fb-spacer" />
        {message && (
          <span className={message.ok ? 'fb-footer-ok' : 'fb-footer-err'}>
            {message.ok ? '✓' : '⚠'} {message.text}
          </span>
        )}
      </div>
    </div>
  );
}
