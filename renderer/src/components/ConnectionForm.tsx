import React, { useState } from 'react';
import { Connection, ConnectionType } from '../types';

interface Props {
  connection: Connection | null;
  newId: string;
  groups: string[];
  defaultGroup?: string;
  onSave: (conn: Connection) => void;
  onClose: () => void;
}

const DEFAULT_PORTS: Record<ConnectionType, number> = { ssh: 22, sftp: 22, ftp: 21 };

export default function ConnectionForm({ connection, newId, groups, defaultGroup, onSave, onClose }: Props) {
  const isEdit = !!connection;

  const [name, setName] = useState(connection?.name ?? '');
  const [type, setType] = useState<ConnectionType>(connection?.type ?? 'ssh');
  const [host, setHost] = useState(connection?.host ?? '');
  const [port, setPort] = useState<number>(connection?.port ?? 22);
  const [username, setUsername] = useState(connection?.username ?? '');
  const [password, setPassword] = useState(connection?.password ?? '');
  const [privateKeyPath, setPrivateKeyPath] = useState(connection?.privateKeyPath ?? '');
  const [ftpSecure, setFtpSecure] = useState(connection?.ftpSecure ?? false);
  const [useKey, setUseKey] = useState(!!connection?.privateKeyPath);
  const [favorite, setFavorite] = useState(connection?.favorite ?? false);

  // ── Group state ──────────────────────────────────────────────
  // groupMode: 'existing' | 'new' | 'none'
  const initialGroup = connection?.group ?? defaultGroup ?? '';
  const [groupMode, setGroupMode] = useState<'existing' | 'new' | 'none'>(
    initialGroup ? (groups.includes(initialGroup) ? 'existing' : 'new') : 'none'
  );
  const [selectedGroup, setSelectedGroup] = useState(
    groups.includes(initialGroup) ? initialGroup : ''
  );
  const [newGroupValue, setNewGroupValue] = useState(
    initialGroup && !groups.includes(initialGroup) ? initialGroup : ''
  );

  const [errors, setErrors] = useState<Record<string, string>>({});

  const handleTypeChange = (t: ConnectionType) => {
    setType(t);
    setPort(DEFAULT_PORTS[t]);
    if (t === 'ftp') setUseKey(false);
  };

  const handleGroupSelectChange = (val: string) => {
    if (val === '__new__') {
      setGroupMode('new');
      setSelectedGroup('');
    } else if (val === '') {
      setGroupMode('none');
      setSelectedGroup('');
    } else {
      setGroupMode('existing');
      setSelectedGroup(val);
    }
  };

  const resolveGroup = (): string | undefined => {
    if (groupMode === 'existing') return selectedGroup || undefined;
    if (groupMode === 'new') return newGroupValue.trim() || undefined;
    return undefined;
  };

  const validate = (): boolean => {
    const e: Record<string, string> = {};
    if (!name.trim()) e.name = 'Le nom est requis';
    if (!host.trim()) e.host = "L'hôte est requis";
    if (!username.trim()) e.username = "Le nom d'utilisateur est requis";
    if (port < 1 || port > 65535) e.port = 'Port invalide (1–65535)';
    if (useKey && !privateKeyPath) e.privateKeyPath = 'Sélectionnez une clé privée';
    if (groupMode === 'new' && !newGroupValue.trim()) e.group = 'Entrez un nom de groupe';
    setErrors(e);
    return Object.keys(e).length === 0;
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    const conn: Connection = {
      id: connection?.id ?? newId,
      name: name.trim(),
      type, host: host.trim(), port,
      username: username.trim(),
      password: useKey ? undefined : (password || undefined),
      privateKeyPath: useKey ? privateKeyPath : undefined,
      ftpSecure: type === 'ftp' ? ftpSecure : undefined,
      group: resolveGroup(),
      favorite: favorite || undefined,
      createdAt: connection?.createdAt ?? Date.now(),
    };
    onSave(conn);
  };

  const selectValue = groupMode === 'none' ? '' : groupMode === 'new' ? '__new__' : selectedGroup;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>{isEdit ? 'Modifier la connexion' : 'Nouvelle connexion'}</h2>
          <button className="btn-icon" onClick={onClose}>✕</button>
        </div>

        <form className="modal-form" onSubmit={handleSubmit}>
          {/* Type */}
          <div className="form-group">
            <label>Type de connexion</label>
            <div className="type-selector">
              {(['ssh', 'sftp', 'ftp'] as ConnectionType[]).map(t => (
                <button key={t} type="button" className={`type-btn ${type === t ? 'active' : ''}`} onClick={() => handleTypeChange(t)}>
                  {t === 'ssh' ? '▶ SSH' : t === 'sftp' ? '⇅ SFTP' : '≈ FTP'}
                </button>
              ))}
            </div>
          </div>

          {/* Name */}
          <div className={`form-group ${errors.name ? 'has-error' : ''}`}>
            <label>Nom de la connexion *</label>
            <input type="text" value={name} onChange={e => setName(e.target.value)} placeholder="Ex: Serveur Production" autoFocus />
            {errors.name && <span className="field-error">{errors.name}</span>}
          </div>

          {/* Group */}
          <div className={`form-group ${errors.group ? 'has-error' : ''}`}>
            <label>Groupe</label>
            <select className="form-select" value={selectValue} onChange={e => handleGroupSelectChange(e.target.value)}>
              <option value="">Sans groupe</option>
              {groups.map(g => <option key={g} value={g}>{g}</option>)}
              <option value="__new__">+ Créer un nouveau groupe…</option>
            </select>
            {groupMode === 'new' && (
              <input
                type="text"
                className="group-new-input"
                placeholder="Nom du nouveau groupe"
                value={newGroupValue}
                onChange={e => setNewGroupValue(e.target.value)}
                autoFocus
                style={{ marginTop: 6 }}
              />
            )}
            {errors.group && <span className="field-error">{errors.group}</span>}
          </div>

          {/* Favorite */}
          <div className="form-group">
            <label className="checkbox-label">
              <input type="checkbox" checked={favorite} onChange={e => setFavorite(e.target.checked)} />
              <span>⭐ Ajouter aux favoris</span>
            </label>
          </div>

          {/* Host + Port */}
          <div className="form-row">
            <div className={`form-group flex3 ${errors.host ? 'has-error' : ''}`}>
              <label>Hôte / Adresse IP *</label>
              <input type="text" value={host} onChange={e => setHost(e.target.value)} placeholder="192.168.1.1 ou mon-serveur.com" />
              {errors.host && <span className="field-error">{errors.host}</span>}
            </div>
            <div className={`form-group flex1 ${errors.port ? 'has-error' : ''}`}>
              <label>Port</label>
              <input type="number" value={port} onChange={e => setPort(parseInt(e.target.value) || 22)} min={1} max={65535} />
              {errors.port && <span className="field-error">{errors.port}</span>}
            </div>
          </div>

          {/* Username */}
          <div className={`form-group ${errors.username ? 'has-error' : ''}`}>
            <label>Nom d'utilisateur *</label>
            <input type="text" value={username} onChange={e => setUsername(e.target.value)} placeholder="root" />
            {errors.username && <span className="field-error">{errors.username}</span>}
          </div>

          {/* Auth */}
          {type !== 'ftp' && (
            <div className="form-group">
              <label>Authentification</label>
              <div className="auth-toggle">
                <button type="button" className={`type-btn ${!useKey ? 'active' : ''}`} onClick={() => setUseKey(false)}>🔑 Mot de passe</button>
                <button type="button" className={`type-btn ${useKey ? 'active' : ''}`} onClick={() => setUseKey(true)}>📄 Clé privée</button>
              </div>
            </div>
          )}

          {(!useKey || type === 'ftp') && (
            <div className="form-group">
              <label>Mot de passe</label>
              <input type="password" value={password} onChange={e => setPassword(e.target.value)} placeholder="••••••••" autoComplete="new-password" />
            </div>
          )}

          {useKey && type !== 'ftp' && (
            <div className={`form-group ${errors.privateKeyPath ? 'has-error' : ''}`}>
              <label>Clé privée *</label>
              <div className="file-picker">
                <input type="text" value={privateKeyPath} readOnly placeholder="C:\Users\user\.ssh\id_rsa" />
                <button type="button" className="btn-secondary" onClick={async () => { const p = await window.electronAPI.pickKeyFile(); if (p) setPrivateKeyPath(p); }}>Parcourir…</button>
              </div>
              {errors.privateKeyPath && <span className="field-error">{errors.privateKeyPath}</span>}
            </div>
          )}

          {type === 'ftp' && (
            <div className="form-group">
              <label className="checkbox-label">
                <input type="checkbox" checked={ftpSecure} onChange={e => setFtpSecure(e.target.checked)} />
                <span>FTPS (connexion sécurisée)</span>
              </label>
              {!ftpSecure && (
                <div className="form-warning">
                  <span className="form-warning-icon">⚠</span>
                  <span>
                    Le FTP simple transmet votre identifiant et votre mot de passe
                    <strong> en clair</strong> sur le réseau. Activez FTPS si le
                    serveur le permet.
                  </span>
                </div>
              )}
            </div>
          )}

          <div className="modal-actions">
            <button type="button" className="btn-secondary" onClick={onClose}>Annuler</button>
            <button type="submit" className="btn-primary">{isEdit ? '✓ Enregistrer' : '+ Créer'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}
