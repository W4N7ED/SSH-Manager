import React, { useState, useEffect, useRef } from 'react';

interface Props {
  onComplete: () => void;
  /** Portable build: the user may opt out of encryption */
  portable?: boolean;
}

type Step = 'welcome' | 'show' | 'verify' | 'done' | 'skip-confirm';

export default function SetupWizard({ onComplete, portable = false }: Props) {
  const [step, setStep] = useState<Step>('welcome');
  const [mnemonic, setMnemonic] = useState('');
  const [words, setWords] = useState<string[]>([]);
  const [copied, setCopied] = useState(false);
  const [verifyInput, setVerifyInput] = useState('');
  const [verifyError, setVerifyError] = useState('');
  const [loading, setLoading] = useState(false);
  const [skipError, setSkipError] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (step === 'verify') setTimeout(() => inputRef.current?.focus(), 100);
  }, [step]);

  const handleGenerate = async () => {
    setLoading(true);
    const phrase: string = await window.electronAPI.cryptoGenerateMnemonic();
    setMnemonic(phrase);
    setWords(phrase.split(' '));
    setLoading(false);
    setStep('show');
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(mnemonic).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
      // Auto-clear the clipboard after 30s so the recovery phrase doesn't linger
      // where other apps could read it. Only clears if it still holds the phrase.
      setTimeout(async () => {
        try {
          const current = await navigator.clipboard.readText();
          if (current === mnemonic) await navigator.clipboard.writeText('');
        } catch { /* clipboard read may be denied — ignore */ }
      }, 30000);
    });
  };

  const handleSkipEncryption = async () => {
    setLoading(true);
    const result = await window.electronAPI.cryptoSkipSetup();
    setLoading(false);
    if (result.success) {
      onComplete();
    } else {
      setSkipError(result.error ?? 'Erreur lors de la configuration');
    }
  };

  const handleVerify = async () => {
    const input = verifyInput.trim().toLowerCase();
    if (input !== mnemonic.trim().toLowerCase()) {
      setVerifyError('La phrase ne correspond pas. Vérifiez l\'ordre des mots.');
      return;
    }
    setLoading(true);
    const result = await window.electronAPI.cryptoCompleteSetup(mnemonic);
    setLoading(false);
    if (result.success) {
      setStep('done');
    } else {
      setVerifyError(result.error ?? 'Erreur lors de la configuration');
    }
  };

  return (
    <div className="setup-overlay">
      <div className="setup-wizard">
        {/* Step indicators */}
        <div className="setup-steps">
          {(['welcome', 'show', 'verify', 'done'] as Step[]).map((s, i) => (
            <div key={s} className={`setup-step-dot ${step === s ? 'active' : ''} ${['show','verify','done'].indexOf(step) > i - 1 ? 'past' : ''}`} />
          ))}
        </div>

        {/* ── Step 1: Welcome ───────────────────────────────── */}
        {step === 'welcome' && (
          <div className="setup-content">
            <div className="setup-icon">🔐</div>
            <h1 className="setup-title">Bienvenue dans SSH Manager</h1>
            <p className="setup-desc">
              Vos connexions (mots de passe, clés, identifiants) seront chiffrées avec <strong>AES-256</strong>.
            </p>
            <p className="setup-desc">
              Pour sécuriser vos données, l'application va générer une <strong>phrase de récupération de 12 mots</strong>.
            </p>
            <div className="setup-warning">
              <span className="setup-warn-icon">⚠</span>
              <div>
                <strong>Notez ces mots sur papier.</strong><br />
                Sans eux, vos données seront irrécupérables si vous changez de machine ou réinstallez l'application.
              </div>
            </div>
            <button className="btn-primary setup-btn" onClick={handleGenerate} disabled={loading}>
              {loading ? 'Génération...' : 'Générer ma phrase de récupération →'}
            </button>

            {/* Mode portable uniquement : choix de ne pas chiffrer */}
            {portable && (
              <button
                className="setup-skip-link"
                onClick={() => setStep('skip-confirm')}
                disabled={loading}
              >
                Continuer sans chiffrement (version portable)
              </button>
            )}
          </div>
        )}

        {/* ── Étape alternative : confirmation sans chiffrement ── */}
        {step === 'skip-confirm' && (
          <div className="setup-content">
            <div className="setup-icon">⚠️</div>
            <h1 className="setup-title">Continuer sans chiffrement ?</h1>
            <p className="setup-desc">
              Vos identifiants et mots de passe seront stockés <strong>en clair</strong> dans
              le fichier de données, à côté de l'exécutable portable.
            </p>
            <div className="setup-warning">
              <span className="setup-warn-icon">⚠</span>
              <div>
                <strong>Toute personne ayant accès au fichier pourra lire vos mots de passe.</strong><br />
                Ce mode est adapté à une clé USB personnelle utilisée sur plusieurs machines,
                mais déconseillé sur un ordinateur partagé.
              </div>
            </div>
            <p className="setup-desc">
              Vous pourrez réactiver le chiffrement plus tard en réinitialisant l'application
              (vos connexions devront être ré-importées).
            </p>
            {skipError && <span className="field-error">{skipError}</span>}
            <div className="setup-actions">
              <button className="btn-primary" onClick={() => setStep('welcome')}>
                ← Revenir au chiffrement (recommandé)
              </button>
              <button className="btn-secondary" onClick={handleSkipEncryption} disabled={loading}>
                {loading ? 'Configuration...' : 'Je comprends, continuer sans chiffrement'}
              </button>
            </div>
          </div>
        )}

        {/* ── Step 2: Show mnemonic ─────────────────────────── */}
        {step === 'show' && (
          <div className="setup-content">
            <div className="setup-icon">📝</div>
            <h1 className="setup-title">Votre phrase de récupération</h1>
            <p className="setup-desc">
              Notez ces <strong>12 mots dans cet ordre exact</strong>. Vous en aurez besoin pour restaurer vos données.
            </p>

            <div className="mnemonic-grid">
              {words.map((word, i) => (
                <div key={i} className="mnemonic-word">
                  <span className="mnemonic-num">{i + 1}</span>
                  <span className="mnemonic-text">{word}</span>
                </div>
              ))}
            </div>

            <button className="btn-secondary setup-copy-btn" onClick={handleCopy}>
              {copied ? '✓ Copié !' : '⎘ Copier dans le presse-papiers'}
            </button>

            <div className="setup-warning">
              <span className="setup-warn-icon">⚠</span>
              <div>
                Ne partagez jamais cette phrase. Quiconque la possède peut accéder à vos connexions.
              </div>
            </div>

            <button className="btn-primary setup-btn" onClick={() => setStep('verify')}>
              J'ai noté les 12 mots →
            </button>
          </div>
        )}

        {/* ── Step 3: Verify ───────────────────────────────── */}
        {step === 'verify' && (
          <div className="setup-content">
            <div className="setup-icon">✔</div>
            <h1 className="setup-title">Confirmez votre phrase</h1>
            <p className="setup-desc">
              Entrez les 12 mots dans l'ordre pour confirmer que vous les avez bien notés.
            </p>

            <div className="form-group">
              <label>Les 12 mots séparés par des espaces</label>
              <input
                ref={inputRef}
                type="text"
                value={verifyInput}
                onChange={e => { setVerifyInput(e.target.value); setVerifyError(''); }}
                placeholder="mot1 mot2 mot3 ... mot12"
                className={verifyError ? 'input-error' : ''}
                onKeyDown={e => { if (e.key === 'Enter') handleVerify(); }}
              />
              {verifyError && <span className="field-error">{verifyError}</span>}
            </div>

            <div className="setup-actions">
              <button className="btn-secondary" onClick={() => setStep('show')}>← Revoir les mots</button>
              <button className="btn-primary" onClick={handleVerify} disabled={loading || !verifyInput.trim()}>
                {loading ? 'Configuration...' : 'Confirmer et chiffrer →'}
              </button>
            </div>
          </div>
        )}

        {/* ── Step 4: Done ─────────────────────────────────── */}
        {step === 'done' && (
          <div className="setup-content">
            <div className="setup-icon">✅</div>
            <h1 className="setup-title">Configuration terminée</h1>
            <p className="setup-desc">
              Vos données sont maintenant protégées par chiffrement AES-256.
            </p>
            <p className="setup-desc">
              Lors de vos prochains lancements, l'application s'ouvre directement sans demander la phrase.
              La phrase est uniquement nécessaire pour importer une sauvegarde sur une nouvelle machine.
            </p>
            <div className="setup-success-box">
              <span>🔒</span> Chiffrement activé — connexions sécurisées
            </div>
            <button className="btn-primary setup-btn" onClick={onComplete}>
              Démarrer SSH Manager →
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
