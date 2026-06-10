import React, { useState, useRef, useEffect } from 'react';
import { Connection } from '../types';

interface Props {
  connection: Connection;
  /** Optional group color — tints the modal header and accent button */
  groupColor?: string;
  onConfirm: (password: string, save: boolean) => void;
  onCancel: () => void;
}

export default function PasswordPrompt({ connection, groupColor, onConfirm, onCancel }: Props) {
  const [password, setPassword] = useState('');
  const [save, setSave]         = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onConfirm(password, save);
  };

  // Dynamic styles derived from group color
  const accentColor   = groupColor || 'var(--accent)';
  const headerBg      = groupColor
    ? `color-mix(in srgb, ${groupColor} 12%, var(--bg-elevated))`
    : 'var(--bg-elevated)';
  const headerBorder  = groupColor ? `3px solid ${groupColor}` : '3px solid var(--border)';
  const buttonStyle   = groupColor
    ? { background: groupColor, borderColor: groupColor, color: '#fff' }
    : {};

  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal pw-modal" onClick={e => e.stopPropagation()}>

        {/* Header teinté avec la couleur du groupe */}
        <div
          className="modal-header pw-modal-header"
          style={{ background: headerBg, borderLeft: headerBorder }}
        >
          <div className="pw-modal-header-inner">
            <span className="pw-modal-icon" style={{ color: accentColor }}>🔒</span>
            <h2>Authentification SSH</h2>
          </div>
          <button className="btn-icon" onClick={onCancel}>✕</button>
        </div>

        <form className="modal-form" onSubmit={handleSubmit}>

          {/* Infos connexion */}
          <div className="pw-conn-info" style={{ borderLeft: `3px solid ${accentColor}` }}>
            <span className="pw-conn-icon" style={{ color: accentColor }}>▶</span>
            <div>
              <div className="pw-conn-name">{connection.name}</div>
              <div className="pw-conn-host">
                {connection.username}@{connection.host}:{connection.port || 22}
              </div>
            </div>
            {groupColor && connection.group && (
              <span className="pw-conn-group" style={{ background: groupColor + '25', color: groupColor }}>
                {connection.group}
              </span>
            )}
          </div>

          <div className="form-group">
            <label>Mot de passe</label>
            <input
              ref={inputRef}
              type="password"
              value={password}
              onChange={e => setPassword(e.target.value)}
              placeholder="••••••••"
              autoComplete="current-password"
              style={groupColor ? { borderColor: groupColor + '66', outlineColor: groupColor } : {}}
            />
          </div>

          <div className="form-group">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={save}
                onChange={e => setSave(e.target.checked)}
              />
              <span>Mémoriser le mot de passe</span>
            </label>
            {!save && (
              <span className="pw-nosave-hint">
                Le mot de passe sera utilisé uniquement pour cette session.
              </span>
            )}
          </div>

          <div className="modal-actions">
            <button type="button" className="btn-secondary" onClick={onCancel}>
              Annuler
            </button>
            <button type="submit" className="btn-primary" style={buttonStyle}>
              ▶ Connecter
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
