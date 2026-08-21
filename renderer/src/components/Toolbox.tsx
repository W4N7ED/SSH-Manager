import React, { useCallback, useEffect, useState } from 'react';
import {
  PasswordOptions, DEFAULT_PASSWORD_OPTIONS, PASSWORD_LENGTH_MIN, PASSWORD_LENGTH_MAX,
  generatePassword, passwordEntropyBits, describeStrength,
} from '../password';
import {
  ClipboardSettings, CLIPBOARD_DELAY_OPTIONS, loadClipboardSettings, saveClipboardSettings,
  copyWithAutoClear, describeCopy, formatDelay,
} from '../clipboard';

const CHARACTER_TOGGLES: { key: keyof PasswordOptions; label: string }[] = [
  { key: 'lowercase',        label: 'Minuscules  a-z' },
  { key: 'uppercase',        label: 'Majuscules  A-Z' },
  { key: 'digits',           label: 'Chiffres  0-9' },
  { key: 'symbols',          label: 'Caractères spéciaux' },
  { key: 'excludeAmbiguous', label: 'Exclure les caractères ambigus' },
];

/**
 * Espace « Boîte à outils » : les utilitaires qui n'appartiennent à aucune
 * connexion en particulier.
 */
export default function Toolbox() {
  const [options, setOptions] = useState<PasswordOptions>(DEFAULT_PASSWORD_OPTIONS);
  const [password, setPassword] = useState('');
  const [copyNotice, setCopyNotice] = useState('');
  const [copyError, setCopyError] = useState('');
  const [clipboard, setClipboard] = useState<ClipboardSettings>(loadClipboardSettings);

  const regenerate = useCallback(() => setPassword(generatePassword(options)), [options]);

  // Un changement d'option produit immédiatement un mot de passe cohérent.
  useEffect(() => { regenerate(); }, [regenerate]);

  const toggleOption = (key: keyof PasswordOptions) =>
    setOptions(prev => ({ ...prev, [key]: !prev[key] }));

  const updateClipboard = (next: ClipboardSettings) => {
    setClipboard(next);
    saveClipboardSettings(next);
  };

  const copy = () => {
    if (!password) return;
    setCopyError('');
    copyWithAutoClear(password)
      .then(result => {
        setCopyNotice(describeCopy(result));
        setTimeout(() => setCopyNotice(''), 4000);
      })
      .catch(() => setCopyError('Copie impossible : le presse-papiers est inaccessible.'));
  };

  const entropy = passwordEntropyBits(options);
  const strength = describeStrength(entropy);
  const noCharacterSet = password === '';

  return (
    <div className="toolbox">
      <section className="toolbox-card">
        <header className="toolbox-card-head">
          <span className="toolbox-label">Générateur</span>
          <h2 className="toolbox-title">Mot de passe</h2>
        </header>

        <div className="pw-output">
          <span className={`pw-value ${noCharacterSet ? 'pw-value--empty' : ''}`}>
            {password || 'Activez au moins un jeu de caractères'}
          </span>
          <button className="btn-round" title="Régénérer" onClick={regenerate} disabled={noCharacterSet}>↺</button>
        </div>

        <div className="pw-actions">
          <button className="btn-primary" onClick={copy} disabled={noCharacterSet}>
            {copyNotice || 'Copier'}
          </button>
          <span className={`pw-strength pw-strength--${strength.level}`}>
            {entropy} bits · robustesse {strength.label}
          </span>
        </div>
        {copyError && <span className="field-error">{copyError}</span>}

        <div className="pw-controls">
          <div className="pw-length">
            <label htmlFor="pw-length">Longueur</label>
            <input
              id="pw-length"
              type="range"
              min={PASSWORD_LENGTH_MIN}
              max={PASSWORD_LENGTH_MAX}
              value={options.length}
              onChange={e => setOptions(prev => ({ ...prev, length: Number(e.target.value) }))}
            />
            <span className="pw-length-value">{options.length}</span>
          </div>

          <div className="pw-toggles">
            {CHARACTER_TOGGLES.map(toggle => (
              <label key={toggle.key} className="checkbox-label">
                <input
                  type="checkbox"
                  checked={Boolean(options[toggle.key])}
                  onChange={() => toggleOption(toggle.key)}
                />
                <span>{toggle.label}</span>
              </label>
            ))}
          </div>
        </div>
      </section>

      <section className="toolbox-card">
        <header className="toolbox-card-head">
          <span className="toolbox-label">Sécurité</span>
          <h2 className="toolbox-title">Presse-papiers</h2>
        </header>

        <p className="toolbox-help">
          S'applique à toutes les copies de l'application : mots de passe générés
          et clés publiques. Le presse-papiers n'est effacé que s'il contient
          encore ce que vous venez de copier. Ces copies sont également tenues
          hors de l'historique Windows (Win+V) et du presse-papiers cloud.
        </p>

        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={clipboard.autoClear}
            onChange={e => updateClipboard({ ...clipboard, autoClear: e.target.checked })}
          />
          <span>Effacer automatiquement après copie</span>
        </label>

        <div className="clipboard-delay">
          <label htmlFor="clipboard-delay">Délai</label>
          <select
            id="clipboard-delay"
            className="form-select"
            value={clipboard.delaySeconds}
            disabled={!clipboard.autoClear}
            onChange={e => updateClipboard({ ...clipboard, delaySeconds: Number(e.target.value) })}
          >
            {CLIPBOARD_DELAY_OPTIONS.map(seconds => (
              <option key={seconds} value={seconds}>{formatDelay(seconds)}</option>
            ))}
          </select>
        </div>
      </section>
    </div>
  );
}
