import React, { useState } from 'react';
import { copyWithAutoClear, describeCopy } from '../clipboard';

type KeyType = 'ed25519' | 'rsa';

const KEY_TYPES: { value: KeyType; label: string; hint: string }[] = [
  { value: 'ed25519', label: 'Ed25519', hint: 'Moderne, court, recommandé' },
  { value: 'rsa',     label: 'RSA 4096', hint: 'Compatible avec les serveurs anciens' },
];

/** Turns a connection name into a safe file name fragment. */
function slugify(value: string): string {
  // NFD sépare les accents de leur lettre ; la classe [^a-z0-9] absorbe
  // ensuite les marques combinantes en même temps que la ponctuation.
  return value
    .toLowerCase()
    .normalize('NFD')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 40);
}

export function suggestKeyName(type: KeyType, connectionLabel: string): string {
  const slug = slugify(connectionLabel);
  return slug ? `id_${type}_${slug}` : `id_${type}`;
}

interface Props {
  /** Connection name or host, used to propose a file name. */
  connectionLabel: string;
  onGenerated: (privateKeyPath: string) => void;
  onCancel: () => void;
}

/**
 * Creates a new SSH key pair in ~/.ssh and hands the private key path back to
 * the form. The public key is shown once, to be copied onto the server.
 */
export default function KeyGenerator({ connectionLabel, onGenerated, onCancel }: Props) {
  const [type, setType] = useState<KeyType>('ed25519');
  const [name, setName] = useState(() => suggestKeyName('ed25519', connectionLabel));
  const [nameEdited, setNameEdited] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [publicKey, setPublicKey] = useState<string | null>(null);
  const [copyNotice, setCopyNotice] = useState('');

  const changeType = (next: KeyType) => {
    setType(next);
    if (!nameEdited) setName(suggestKeyName(next, connectionLabel));
  };

  const generate = async () => {
    setBusy(true);
    setError('');
    const result = await window.electronAPI.sshGenerateKey({ name: name.trim(), type });
    setBusy(false);

    if (!result.success || !result.privateKeyPath) {
      setError(result.error ?? 'La génération de la clé a échoué');
      return;
    }
    setPublicKey(result.publicKey ?? '');
    onGenerated(result.privateKeyPath);
  };

  const copyPublicKey = () => {
    if (!publicKey) return;
    copyWithAutoClear(publicKey)
      .then(result => {
        setCopyNotice(describeCopy(result, true));
        setTimeout(() => setCopyNotice(''), 4000);
      })
      .catch(() => setError('Copie impossible : le presse-papiers est inaccessible'));
  };

  if (publicKey !== null) {
    return (
      <div className="key-gen key-gen--done">
        <span className="key-gen-title">Clé créée dans ~/.ssh</span>
        <p className="key-gen-help">
          Ajoutez cette clé publique dans <code>~/.ssh/authorized_keys</code> sur le serveur
          pour que la connexion aboutisse.
        </p>
        <div className="key-gen-public">{publicKey}</div>
        <div className="key-gen-actions">
          <button type="button" className="btn-secondary small" onClick={copyPublicKey}>
            {copyNotice || 'Copier la clé publique'}
          </button>
          <span className="key-gen-spacer" />
          <button type="button" className="btn-secondary small" onClick={onCancel}>Fermer</button>
        </div>
        {error && <span className="field-error">{error}</span>}
      </div>
    );
  }

  return (
    <div className="key-gen">
      <span className="key-gen-title">Créer une nouvelle clé</span>

      <div className="key-gen-field">
        <label>Type</label>
        <div className="type-selector">
          {KEY_TYPES.map(option => (
            <button
              key={option.value}
              type="button"
              className={`type-btn ${type === option.value ? 'active' : ''}`}
              onClick={() => changeType(option.value)}
              title={option.hint}
            >{option.label}</button>
          ))}
        </div>
      </div>

      <div className="key-gen-field">
        <label>Nom du fichier</label>
        <input
          type="text"
          value={name}
          onChange={e => { setName(e.target.value); setNameEdited(true); }}
          placeholder="id_ed25519_mon-serveur"
          spellCheck={false}
        />
        <span className="key-gen-path">~/.ssh/{name.trim() || '…'}</span>
      </div>

      {error && <span className="field-error">{error}</span>}

      <div className="key-gen-actions">
        <span className="key-gen-hint">La clé est créée sans phrase de passe.</span>
        <span className="key-gen-spacer" />
        <button type="button" className="btn-secondary small" onClick={onCancel}>Annuler</button>
        <button type="button" className="btn-primary small" onClick={generate} disabled={busy || !name.trim()}>
          {busy ? 'Génération…' : 'Générer'}
        </button>
      </div>
    </div>
  );
}
