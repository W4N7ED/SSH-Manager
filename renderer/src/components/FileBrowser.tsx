import React, { useState, useEffect, useCallback } from 'react';
import { Tab, Connection } from '../types';

interface FileEntry {
  name: string;
  type: string;
  size: number;
  modifyTime?: number;
}

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

export default function FileBrowser({ tab, connection, onStatusChange, onClose }: Props) {
  const [currentPath, setCurrentPath] = useState('/');
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [newFolderMode, setNewFolderMode] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [message, setMessage] = useState<string | null>(null);

  const isFtp = tab.connectionType === 'ftp';

  const showMsg = (msg: string, isErr = false) => {
    setMessage((isErr ? '⚠ ' : '✓ ') + msg);
    setTimeout(() => setMessage(null), 3000);
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
        if (a.type === 'd' && b.type !== 'd') return -1;
        if (a.type !== 'd' && b.type === 'd') return 1;
        return a.name.localeCompare(b.name);
      });
      setFiles(sorted);
      setCurrentPath(path);
    } else {
      showMsg(result.error ?? 'Impossible de lister le répertoire', true);
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

  const navigate = (entry: FileEntry) => {
    if (entry.type === 'd' || entry.type === 'directory') {
      const next = currentPath.endsWith('/') ? currentPath + entry.name : currentPath + '/' + entry.name;
      listDir(next);
    }
  };

  const goUp = () => {
    if (currentPath === '/') return;
    const parts = currentPath.split('/').filter(Boolean);
    parts.pop();
    listDir('/' + parts.join('/') || '/');
  };

  const goPath = (seg: string[]) => {
    listDir('/' + seg.join('/'));
  };

  const pathSegments = currentPath.split('/').filter(Boolean);

  const download = async () => {
    if (!selected) return;
    const remotePath = currentPath.endsWith('/')
      ? currentPath + selected
      : currentPath + '/' + selected;
    const result = isFtp
      ? await window.electronAPI.ftpDownload({ tabId: tab.id, remotePath })
      : await window.electronAPI.sftpDownload({ tabId: tab.id, remotePath });
    if (result.success) showMsg('Téléchargement terminé');
    else if (result.error) showMsg(result.error, true);
  };

  const upload = async () => {
    const remotePath = currentPath.endsWith('/') ? currentPath : currentPath + '/';
    const result = isFtp
      ? await window.electronAPI.ftpUpload({ tabId: tab.id, remotePath })
      : await window.electronAPI.sftpUpload({ tabId: tab.id, remotePath });
    if (result.success) { showMsg('Upload terminé'); listDir(currentPath); }
    else if (result.error) showMsg(result.error, true);
  };

  const deleteEntry = async () => {
    if (!selected) return;
    const entry = files.find(f => f.name === selected);
    if (!entry) return;
    if (!confirm(`Supprimer "${selected}" ?`)) return;
    const remotePath = currentPath.endsWith('/')
      ? currentPath + selected
      : currentPath + '/' + selected;
    const isDir = entry.type === 'd' || entry.type === 'directory';
    const result = isFtp
      ? await window.electronAPI.ftpDelete({ tabId: tab.id, remotePath, isDir })
      : await window.electronAPI.sftpDelete({ tabId: tab.id, remotePath, isDir });
    if (result.success) { showMsg('Supprimé'); listDir(currentPath); }
    else showMsg(result.error ?? 'Erreur suppression', true);
  };

  const createFolder = async () => {
    if (!newFolderName.trim()) return;
    const remotePath = currentPath.endsWith('/')
      ? currentPath + newFolderName.trim()
      : currentPath + '/' + newFolderName.trim();
    const result = isFtp
      ? await window.electronAPI.ftpMkdir({ tabId: tab.id, remotePath })
      : await window.electronAPI.sftpMkdir({ tabId: tab.id, remotePath });
    if (result.success) { showMsg('Dossier créé'); listDir(currentPath); }
    else showMsg(result.error ?? 'Erreur création dossier', true);
    setNewFolderMode(false);
    setNewFolderName('');
  };

  if (error) {
    return (
      <div className="filebrowser-error">
        <div className="error-icon">⚠</div>
        <h3>Connexion échouée</h3>
        <p>{error}</p>
        <button className="btn-primary" onClick={() => connect().then(() => listDir('/'))}>
          Réessayer
        </button>
        <button className="btn-secondary" onClick={onClose}>Fermer</button>
      </div>
    );
  }

  return (
    <div className="filebrowser">
      {/* Toolbar */}
      <div className="fb-toolbar">
        <button className="btn-toolbar" onClick={goUp} disabled={currentPath === '/'}>↑ Parent</button>
        <button className="btn-toolbar" onClick={() => listDir(currentPath)}>↺ Actualiser</button>
        <button className="btn-toolbar" onClick={upload}>⬆ Upload</button>
        <button className="btn-toolbar" onClick={download} disabled={!selected}>⬇ Télécharger</button>
        <button className="btn-toolbar" onClick={() => setNewFolderMode(true)}>+ Dossier</button>
        <button className="btn-toolbar danger" onClick={deleteEntry} disabled={!selected}>✕ Supprimer</button>
        <div className="fb-spacer" />
        <button className="btn-toolbar danger" onClick={onClose}>✕ Fermer</button>
      </div>

      {/* Breadcrumb */}
      <div className="fb-breadcrumb">
        <span className="bc-item" onClick={() => listDir('/')}>/</span>
        {pathSegments.map((seg, i) => (
          <React.Fragment key={i}>
            <span className="bc-sep">/</span>
            <span
              className="bc-item"
              onClick={() => goPath(pathSegments.slice(0, i + 1))}
            >{seg}</span>
          </React.Fragment>
        ))}
      </div>

      {/* New folder input */}
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

      {/* Message */}
      {message && <div className={`fb-message ${message.startsWith('⚠') ? 'error' : 'success'}`}>{message}</div>}

      {/* File list */}
      {loading ? (
        <div className="fb-loading">Chargement...</div>
      ) : (
        <div className="fb-table-wrap">
          <table className="fb-table">
            <thead>
              <tr>
                <th>Nom</th>
                <th>Taille</th>
                <th>Modifié</th>
                <th>Type</th>
              </tr>
            </thead>
            <tbody>
              {files.map(f => {
                const isDir = f.type === 'd' || f.type === 'directory';
                return (
                  <tr
                    key={f.name}
                    className={selected === f.name ? 'selected' : ''}
                    onClick={() => setSelected(f.name)}
                    onDoubleClick={() => isDir ? navigate(f) : undefined}
                  >
                    <td className="fb-name">
                      <span className="fb-file-icon">{isDir ? '📁' : '📄'}</span>
                      {f.name}
                    </td>
                    <td className="fb-size">{isDir ? '—' : formatSize(f.size)}</td>
                    <td className="fb-date">{formatDate(f.modifyTime)}</td>
                    <td className="fb-type">{isDir ? 'Dossier' : 'Fichier'}</td>
                  </tr>
                );
              })}
              {files.length === 0 && (
                <tr><td colSpan={4} className="fb-empty">Dossier vide</td></tr>
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
