import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Connection, ConnectionType } from '../types';
import { TERMINAL_PRESETS, TerminalThemeConfig } from '../terminal-themes';

const MAX_CONNECTION_RESULTS = 6;
const TYPE_LABELS: Record<string, string> = { ssh: 'SSH', sftp: 'SFTP', ftp: 'FTP' };

interface Props {
  connections: Connection[];
  /** True when at least one terminal session can receive synchronised input. */
  canSyncInput: boolean;
  tiled: boolean;
  onConnect: (conn: Connection) => void;
  onOpenAs: (conn: Connection, type: ConnectionType) => void;
  onNewConnection: () => void;
  onOpenToolbox: () => void;
  onToggleTiled: () => void;
  onToggleSync: () => void;
  onThemeChange: (theme: TerminalThemeConfig) => void;
  onClose: () => void;
}

interface PaletteItem {
  key: string;
  section: string;
  render: React.ReactNode;
  shortcut?: string;
  /** Executed on Enter or click. */
  run: () => void;
  /** Executed on Shift+Enter, when the item offers an alternative. */
  runAlternate?: () => void;
}

function matches(haystack: string, query: string): boolean {
  return haystack.toLowerCase().includes(query.toLowerCase());
}

/**
 * Ctrl+K launcher: jumps to a saved connection or runs a global action.
 * Falls into a second page when an action needs a choice (terminal theme).
 */
export default function CommandPalette({
  connections, canSyncInput, tiled,
  onConnect, onOpenAs, onNewConnection, onOpenToolbox, onToggleTiled, onToggleSync, onThemeChange, onClose,
}: Props) {
  const [query, setQuery] = useState('');
  const [page, setPage] = useState<'root' | 'theme'>('root');
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => { inputRef.current?.focus(); }, [page]);

  const items = useMemo<PaletteItem[]>(() => {
    if (page === 'theme') {
      return TERMINAL_PRESETS
        .filter(preset => matches(preset.label, query))
        .map(preset => ({
          key: 'theme-' + preset.id,
          section: 'Thèmes du terminal',
          render: (
            <>
              <span className="palette-theme-preview" style={{ background: preset.background, color: preset.foreground }}>Aa</span>
              <span className="palette-item-name">{preset.label}</span>
            </>
          ),
          run: () => { onThemeChange(preset); onClose(); },
        }));
    }

    const connectionItems: PaletteItem[] = connections
      .filter(conn => !query
        || matches(conn.name, query) || matches(conn.host, query) || matches(conn.username, query))
      .slice(0, MAX_CONNECTION_RESULTS)
      .map(conn => ({
        key: 'conn-' + conn.id,
        section: 'Connexions',
        render: (
          <>
            <span className="palette-type-pill">{TYPE_LABELS[conn.type]}</span>
            <span className="palette-item-name">{conn.name}</span>
            <span className="palette-item-target">{conn.username}@{conn.host}</span>
          </>
        ),
        run: () => { onConnect(conn); onClose(); },
        runAlternate: conn.type === 'ssh' ? () => { onOpenAs(conn, 'sftp'); onClose(); } : undefined,
      }));

    const actionDefinitions: { key: string; label: string; shortcut?: string; run: () => void }[] = [
      {
        key: 'new', label: 'Nouvelle connexion…', shortcut: 'Ctrl+N',
        run: () => { onNewConnection(); onClose(); },
      },
      {
        key: 'toolbox', label: 'Ouvrir la boîte à outils',
        run: () => { onOpenToolbox(); onClose(); },
      },
      {
        key: 'tiled', label: tiled ? 'Quitter la mosaïque' : 'Basculer en mosaïque', shortcut: 'Ctrl+M',
        run: () => { onToggleTiled(); onClose(); },
      },
      ...(canSyncInput ? [{
        key: 'sync', label: 'Synchroniser la saisie', shortcut: 'Ctrl+Maj+S',
        run: () => { onToggleSync(); onClose(); },
      }] : []),
      {
        key: 'theme', label: 'Changer le thème du terminal…',
        run: () => { setPage('theme'); setQuery(''); setHighlight(0); },
      },
    ];

    const actions: PaletteItem[] = actionDefinitions
      .filter(action => !query || matches(action.label, query) || matches(action.shortcut ?? '', query))
      .map(action => ({
        key: 'action-' + action.key,
        section: 'Actions',
        shortcut: action.shortcut,
        render: <span className="palette-item-name">{action.label}</span>,
        run: action.run,
      }));

    return [...connectionItems, ...actions];
  }, [page, query, connections, canSyncInput, tiled, onConnect, onOpenAs,
      onNewConnection, onOpenToolbox, onToggleTiled, onToggleSync, onThemeChange, onClose]);

  useEffect(() => {
    if (highlight >= items.length) setHighlight(Math.max(0, items.length - 1));
  }, [items, highlight]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      if (page === 'theme') { setPage('root'); setQuery(''); setHighlight(0); }
      else onClose();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHighlight(h => Math.min(items.length - 1, h + 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlight(h => Math.max(0, h - 1));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const item = items[highlight];
      if (!item) return;
      if (e.shiftKey && item.runAlternate) item.runAlternate();
      else item.run();
    }
  };

  useEffect(() => {
    listRef.current?.querySelectorAll('.palette-item')[highlight]?.scrollIntoView({ block: 'nearest' });
  }, [highlight]);

  let lastSection = '';

  return (
    <div className="palette-overlay" onClick={onClose}>
      <div className="palette" onClick={e => e.stopPropagation()}>
        <div className="palette-input-row">
          <span className="palette-prompt">{page === 'theme' ? '#' : '>'}</span>
          <input
            ref={inputRef}
            type="text"
            className="palette-input"
            value={query}
            placeholder={page === 'theme' ? 'Filtrer les thèmes…' : 'Rechercher une connexion ou une action…'}
            onChange={e => { setQuery(e.target.value); setHighlight(0); }}
            onKeyDown={handleKeyDown}
            spellCheck={false}
          />
        </div>

        <div className="palette-list" ref={listRef}>
          {items.map((item, index) => {
            const sectionHeader = item.section !== lastSection ? item.section : null;
            lastSection = item.section;
            return (
              <React.Fragment key={item.key}>
                {sectionHeader && <div className="palette-section">{sectionHeader}</div>}
                <button
                  className={`palette-item ${index === highlight ? 'active' : ''}`}
                  onMouseEnter={() => setHighlight(index)}
                  onClick={() => item.run()}
                >
                  {item.render}
                  <span className="palette-item-spacer" />
                  {item.shortcut && <span className="palette-item-shortcut">{item.shortcut}</span>}
                  {index === highlight && !item.shortcut && <span className="palette-item-shortcut">Entrée</span>}
                </button>
              </React.Fragment>
            );
          })}
          {items.length === 0 && <div className="palette-empty">Aucun résultat.</div>}
        </div>

        <div className="palette-hint">
          {page === 'theme'
            ? '↑↓ naviguer · Entrée appliquer · Échap revenir'
            : '↑↓ naviguer · Entrée ouvrir · Maj+Entrée en SFTP · Échap fermer'}
        </div>
      </div>
    </div>
  );
}
