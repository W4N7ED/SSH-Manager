import React, {
  useEffect, useRef, useCallback, useState,
  forwardRef, useImperativeHandle,
} from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import '@xterm/xterm/css/xterm.css';
import { Tab, Connection } from '../types';
import {
  TerminalThemeConfig, TERMINAL_PRESETS, DEFAULT_TERMINAL_THEME,
} from '../terminal-themes';

export interface TerminalTabHandle {
  injectData: (data: string) => void;
}

interface Props {
  tab: Tab;
  connection: Connection;
  terminalTheme: TerminalThemeConfig;
  onStatusChange: (tabId: string, status: Tab['status'], error?: string) => void;
  onClose: () => void;
  onRetry?: () => void;
  onSyncInput?: (data: string) => void;
  onThemeChange: (theme: TerminalThemeConfig) => void;
  synced?: boolean;
  compact?: boolean;
}

// ── Converts a TerminalThemeConfig into the xterm ITheme object ──────────────
function buildXtermTheme(t: TerminalThemeConfig) {
  return {
    background: t.background,
    foreground: t.foreground,
    cursor: t.cursor,
    selectionBackground: t.cursor + '40',
    black: '#484f58', red: '#ff7b72', green: '#3fb950', yellow: '#d29922',
    blue: '#58a6ff', magenta: '#bc8cff', cyan: '#39c5cf', white: '#b1bac4',
    brightBlack: '#6e7681', brightRed: '#ffa198', brightGreen: '#56d364',
    brightYellow: '#e3b341', brightBlue: '#79c0ff', brightMagenta: '#d2a8ff',
    brightCyan: '#56d4dd', brightWhite: '#e6edf3',
  };
}

// ── Theme picker panel ────────────────────────────────────────────────────────
function ThemePicker({
  current, onSelect, onClose,
}: {
  current: TerminalThemeConfig;
  onSelect: (t: TerminalThemeConfig) => void;
  onClose: () => void;
}) {
  const [customBg, setCustomBg]   = useState(current.background);
  const [customFg, setCustomFg]   = useState(current.foreground);
  const [customCur, setCustomCur] = useState(current.cursor);
  const ref = useRef<HTMLDivElement>(null);

  // Close on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [onClose]);

  const applyCustom = () => {
    onSelect({ id: 'custom', label: 'Personnalisé', background: customBg, foreground: customFg, cursor: customCur });
  };

  return (
    <div className="term-theme-picker" ref={ref}>
      <div className="term-theme-picker-title">Thème du terminal</div>

      {/* ── Presets ── */}
      <div className="term-theme-presets">
        {TERMINAL_PRESETS.map(preset => (
          <button
            key={preset.id}
            className={`term-theme-swatch ${current.id === preset.id ? 'active' : ''}`}
            title={preset.label}
            onClick={() => onSelect(preset)}
            style={{ background: preset.background, color: preset.foreground }}
          >
            <span className="term-theme-swatch-label">{preset.label}</span>
            <span className="term-theme-swatch-preview" style={{ color: preset.cursor }}>▶_</span>
          </button>
        ))}
      </div>

      {/* ── Custom ── */}
      <div className="term-theme-custom">
        <div className="term-theme-custom-title">Personnalisé</div>
        <div className="term-theme-custom-row">
          <label>Fond</label>
          <input type="color" value={customBg}  onChange={e => setCustomBg(e.target.value)} />
          <span className="term-theme-hex">{customBg}</span>
        </div>
        <div className="term-theme-custom-row">
          <label>Texte</label>
          <input type="color" value={customFg}  onChange={e => setCustomFg(e.target.value)} />
          <span className="term-theme-hex">{customFg}</span>
        </div>
        <div className="term-theme-custom-row">
          <label>Curseur</label>
          <input type="color" value={customCur} onChange={e => setCustomCur(e.target.value)} />
          <span className="term-theme-hex">{customCur}</span>
        </div>
        <button className="term-theme-apply-btn" onClick={applyCustom}>
          Appliquer
        </button>
      </div>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────
const TerminalTab = forwardRef<TerminalTabHandle, Props>(
  ({
    tab, connection, terminalTheme, onStatusChange, onClose, onRetry,
    onSyncInput, onThemeChange, synced, compact,
  }, ref) => {
    const containerRef   = useRef<HTMLDivElement>(null);
    const termRef        = useRef<Terminal | null>(null);
    const fitAddonRef    = useRef<FitAddon | null>(null);
    const cleanupRef     = useRef<(() => void)[]>([]);
    const connectedRef   = useRef(false);
    const onSyncInputRef = useRef(onSyncInput);
    onSyncInputRef.current = onSyncInput;

    const [errorOverlay, setErrorOverlay] = useState<string | null>(null);
    const [showThemePicker, setShowThemePicker] = useState(false);

    useImperativeHandle(ref, () => ({
      injectData: (data: string) => {
        if (connectedRef.current) {
          window.electronAPI.sshWrite({ tabId: tab.id, data });
        }
      },
    }));

    const cleanup = useCallback(() => {
      cleanupRef.current.forEach(fn => { try { fn(); } catch {} });
      cleanupRef.current = [];
      if (connectedRef.current) {
        window.electronAPI.sshDisconnect(tab.id);
        connectedRef.current = false;
      }
      if (termRef.current) {
        termRef.current.dispose();
        termRef.current = null;
      }
    }, [tab.id]);

    // ── Terminal initialisation ────────────────────────────────────────────
    useEffect(() => {
      if (!containerRef.current) return;

      const theme = terminalTheme ?? DEFAULT_TERMINAL_THEME;

      const term = new Terminal({
        fontFamily: '"Cascadia Code", "Fira Code", "Consolas", monospace',
        fontSize: compact ? 12 : 14,
        lineHeight: 1.3,
        cursorBlink: true,
        cursorStyle: 'block',
        theme: buildXtermTheme(theme),
        allowTransparency: false,
        scrollback: 5000,
        convertEol: false,
      });

      const fitAddon = new FitAddon();
      term.loadAddon(fitAddon);
      term.loadAddon(new WebLinksAddon());
      term.open(containerRef.current);
      fitAddon.fit();

      termRef.current    = term;
      fitAddonRef.current = fitAddon;

      // Right-click = paste
      const handleContextMenu = (e: MouseEvent) => {
        e.preventDefault();
        navigator.clipboard.readText().then(text => {
          if (text && connectedRef.current)
            window.electronAPI.sshWrite({ tabId: tab.id, data: text });
        }).catch(() => {});
      };
      containerRef.current.addEventListener('contextmenu', handleContextMenu);
      cleanupRef.current.push(() =>
        containerRef.current?.removeEventListener('contextmenu', handleContextMenu)
      );

      // Ctrl+C = copy selection
      term.attachCustomKeyEventHandler((event: KeyboardEvent) => {
        if (event.type === 'keydown' && event.ctrlKey && event.key === 'c' && term.hasSelection()) {
          const selected = term.getSelection();
          if (selected) navigator.clipboard.writeText(selected).catch(() => {});
          return false;
        }
        return true;
      });

      // Input → SSH + sync
      const onData = term.onData((data) => {
        if (connectedRef.current) {
          window.electronAPI.sshWrite({ tabId: tab.id, data });
          onSyncInputRef.current?.(data);
        }
      });
      cleanupRef.current.push(() => onData.dispose());

      // SSH data → terminal
      const unsubData = window.electronAPI.onSshData(tab.id, (data: string) => {
        term.write(data);
      });
      cleanupRef.current.push(unsubData);

      const unsubClose = window.electronAPI.onSshClose(tab.id, () => {
        connectedRef.current = false;
        onStatusChange(tab.id, 'disconnected');
        term.write('\r\n\x1b[31m[Connexion fermée]\x1b[0m\r\n');
      });
      cleanupRef.current.push(unsubClose);

      // Resize
      const onResize = term.onResize(({ cols, rows }) => {
        if (connectedRef.current)
          window.electronAPI.sshResize({ tabId: tab.id, cols, rows });
      });
      cleanupRef.current.push(() => onResize.dispose());

      const resizeObserver = new ResizeObserver(() => {
        try { fitAddon.fit(); } catch {}
      });
      resizeObserver.observe(containerRef.current!);
      cleanupRef.current.push(() => resizeObserver.disconnect());

      // Connect
      term.write(`\x1b[36mConnexion à ${connection.username}@${connection.host}:${connection.port || 22}...\x1b[0m\r\n`);

      window.electronAPI.sshConnect({ tabId: tab.id, connection })
        .then(() => {
          connectedRef.current = true;
          setErrorOverlay(null);
          onStatusChange(tab.id, 'connected');
        })
        .catch((err: unknown) => {
          const msg = typeof err === 'string'
            ? err
            : (err instanceof Error ? err.message : String(err ?? 'Erreur de connexion inconnue'));
          onStatusChange(tab.id, 'error', msg);
          setErrorOverlay(msg);
        });

      return cleanup;
    }, []); // eslint-disable-line react-hooks/exhaustive-deps

    // ── Apply theme changes to existing terminal instance ─────────────────
    useEffect(() => {
      if (termRef.current && terminalTheme) {
        termRef.current.options.theme = buildXtermTheme(terminalTheme);
      }
    }, [terminalTheme]);

    const errLower   = errorOverlay?.toLowerCase() ?? '';
    const isAuthError = errorOverlay !== null && (
      errLower.includes('authentication') ||
      errLower.includes('auth') ||
      errLower.includes('password') ||
      errLower.includes('failed') ||
      errLower.includes('permission')
    );

    const handleThemeSelect = (theme: TerminalThemeConfig) => {
      onThemeChange(theme);
      setShowThemePicker(false);
    };

    return (
      <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0, position: 'relative' }}>
        {/* ── Toolbar ── */}
        <div className="terminal-toolbar">
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            {synced && <span className="sync-badge" title="Saisie synchronisée">⟳ SYNC</span>}
            <span className="terminal-conn-label">
              {connection.username}@{connection.host}:{connection.port || 22}
            </span>
          </div>
          <div className="terminal-actions">
            {/* Bouton thème */}
            <div style={{ position: 'relative' }}>
              <button
                className={`btn-toolbar ${showThemePicker ? 'active' : ''}`}
                title="Couleurs du terminal"
                onClick={() => setShowThemePicker(p => !p)}
              >
                🎨 Thème
              </button>
              {showThemePicker && (
                <ThemePicker
                  current={terminalTheme ?? DEFAULT_TERMINAL_THEME}
                  onSelect={handleThemeSelect}
                  onClose={() => setShowThemePicker(false)}
                />
              )}
            </div>

            {!compact && (
              <>
                <button className="btn-toolbar" title="Ouvrir dans PowerShell"
                  onClick={() => window.electronAPI.sshOpenInPowerShell(connection)}>
                  ⊞ PowerShell
                </button>
                <button className="btn-toolbar" title="Ouvrir dans CMD"
                  onClick={() => window.electronAPI.sshOpenInCmd(connection)}>
                  ▣ CMD
                </button>
              </>
            )}
            <button className="btn-toolbar danger" onClick={() => { cleanup(); onClose(); }}>
              ✕ Fermer
            </button>
          </div>
        </div>

        {/* ── Terminal ── */}
        <div ref={containerRef} className="terminal-container"
          style={{
            flex: 1, minHeight: 0, padding: '4px',
            background: terminalTheme?.background ?? DEFAULT_TERMINAL_THEME.background,
            visibility: errorOverlay ? 'hidden' : 'visible',
          }}
        />

        {/* ── Error overlay ── */}
        {errorOverlay && (
          <div className="term-error-overlay">
            <div className="term-error-box">
              <div className="term-error-icon">⚠</div>
              <div className="term-error-title">Connexion échouée</div>
              <div className="term-error-msg">{errorOverlay}</div>
              {isAuthError && (
                <p className="term-error-hint">
                  Mot de passe incorrect ou méthode d'authentification refusée.
                </p>
              )}
              <div className="term-error-actions">
                {isAuthError && onRetry && (
                  <button className="btn-primary" onClick={() => { cleanup(); onRetry(); }}>
                    ↺ Réessayer avec un autre mot de passe
                  </button>
                )}
                <button className="btn-secondary" onClick={() => { cleanup(); onClose(); }}>
                  Fermer l'onglet
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    );
  }
);

TerminalTab.displayName = 'TerminalTab';
export default TerminalTab;
