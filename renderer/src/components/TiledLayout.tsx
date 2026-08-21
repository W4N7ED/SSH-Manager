import React from 'react';
import { Tab, Connection, Quadrant } from '../types';
import TerminalTab, { TerminalTabHandle } from './TerminalTab';
import FileBrowser from './FileBrowser';
import { TerminalThemeConfig } from '../terminal-themes';

const QUADRANT_LABELS: Record<Quadrant, string> = {
  topLeft: 'Haut gauche',
  topRight: 'Haut droite',
  bottomLeft: 'Bas gauche',
  bottomRight: 'Bas droite',
};

const ALL_QUADRANTS: Quadrant[] = ['topLeft', 'topRight', 'bottomLeft', 'bottomRight'];

interface Props {
  tiledTabs: Partial<Record<Quadrant, string>>;
  tabs: Tab[];
  connections: Connection[];
  syncedTabIds: Set<string>;
  tabRefs: Map<string, React.RefObject<TerminalTabHandle>>;
  draggingTabId: string | null;
  terminalTheme: TerminalThemeConfig;
  onStatusChange: (tabId: string, status: Tab['status'], error?: string) => void;
  onCloseTab: (tabId: string) => void;
  onDropToQuadrant: (tabId: string, quadrant: Quadrant) => void;
  onRemoveFromTile: (quadrant: Quadrant) => void;
  onSyncInput: (sourceTabId: string, data: string) => void;
  onThemeChange: (theme: TerminalThemeConfig) => void;
}

export default function TiledLayout({
  tiledTabs, tabs, connections, syncedTabIds, tabRefs,
  draggingTabId, terminalTheme, onStatusChange, onCloseTab, onDropToQuadrant,
  onRemoveFromTile, onSyncInput, onThemeChange,
}: Props) {
  const panelCount = Object.keys(tiledTabs).length;
  const syncedPanelCount = Object.values(tiledTabs).filter(id => id && syncedTabIds.has(id)).length;

  return (
    <div className="tiled-layout">
      <div className="tiled-header">
        <span className="tiled-title">Mosaïque</span>
        <span className="tiled-badge">
          {syncedPanelCount > 1 ? `SYNC · ${syncedPanelCount} panneaux` : `${panelCount} panneau${panelCount > 1 ? 'x' : ''}`}
        </span>
        <span className="tiled-spacer" />
        <span className="tiled-hint">Glissez un onglet dans un coin pour l'ajouter</span>
      </div>

      <div className="tiled-grid">
      {ALL_QUADRANTS.map((quadrant, index) => {
        const tabId = tiledTabs[quadrant];
        const tab = tabId ? tabs.find(t => t.id === tabId) : undefined;
        const conn = tab ? connections.find(c => c.id === tab.connectionId) : undefined;
        const isSynced = tabId ? syncedTabIds.has(tabId) : false;
        const isDragOver = draggingTabId !== null;

        if (tab && conn) {
          const tabRef = tabRefs.get(tab.id) ?? React.createRef<TerminalTabHandle>();
          return (
            <div key={quadrant} className={`tile-cell ${isSynced ? 'tile-cell--synced' : ''}`}>
              <div className="tile-header">
                <span className={`status-dot ${tab.status === 'connected' ? 'status-dot--on' : ''}`} />
                <span className="tile-label">{tab.connectionName}</span>
                <span className="tile-spacer" />
                <span className="tile-index">{index + 1}</span>
                <button
                  className="tile-undock"
                  title="Retirer de la mosaïque"
                  onClick={() => onRemoveFromTile(quadrant)}
                >
                  ⊡
                </button>
              </div>
              <div className="tile-content">
                {tab.type === 'terminal' ? (
                  <TerminalTab
                    ref={tabRef}
                    tab={tab}
                    connection={conn}
                    terminalTheme={terminalTheme}
                    onStatusChange={onStatusChange}
                    onClose={() => { onRemoveFromTile(quadrant); onCloseTab(tab.id); }}
                    onSyncInput={isSynced ? (data) => onSyncInput(tab.id, data) : undefined}
                    onThemeChange={onThemeChange}
                    synced={isSynced}
                    compact
                  />
                ) : (
                  <FileBrowser
                    tab={tab}
                    connection={conn}
                    onStatusChange={onStatusChange}
                    onClose={() => { onRemoveFromTile(quadrant); onCloseTab(tab.id); }}
                  />
                )}
              </div>
            </div>
          );
        }

        return (
          <div
            key={quadrant}
            className={`tile-cell tile-empty ${isDragOver ? 'tile-drop-target' : ''}`}
            onDragOver={e => { e.preventDefault(); e.dataTransfer.dropEffect = 'move'; }}
            onDrop={e => {
              e.preventDefault();
              const id = e.dataTransfer.getData('tabId');
              if (id) onDropToQuadrant(id, quadrant);
            }}
          >
            <div className="tile-drop-hint">
              <div className="tile-drop-icon">+</div>
              <div>{QUADRANT_LABELS[quadrant]}</div>
              <div className="tile-drop-sub">Déposez un onglet ici</div>
            </div>
          </div>
        );
      })}
      </div>
    </div>
  );
}
