import React from 'react';
import { Tab } from '../types';

const STATUS_LABELS: Record<Tab['status'], string> = {
  connecting:   'connexion en cours',
  connected:    'connecté',
  disconnected: 'déconnecté',
  error:        'erreur',
};

interface Props {
  tabs: Tab[];
  activeTab: Tab | null;
  /** Target of the active session, e.g. root@10.0.0.1:22. */
  activeTarget: string | null;
  syncedCount: number;
  /** False when the vault runs unencrypted (portable mode without a passphrase). */
  vaultEncrypted: boolean;
  version: string | null;
}

export default function StatusBar({
  tabs, activeTab, activeTarget, syncedCount, vaultEncrypted, version,
}: Props) {
  const connectedCount = tabs.filter(tab => tab.status === 'connected').length;

  return (
    <footer className="status-bar">
      <span className="status-item">
        <span className={`status-dot ${connectedCount > 0 ? 'status-dot--on' : ''}`} />
        {connectedCount} session{connectedCount > 1 ? 's' : ''} active{connectedCount > 1 ? 's' : ''}
      </span>

      {activeTab && (
        <span className="status-item">
          {activeTarget ?? activeTab.connectionName} · {STATUS_LABELS[activeTab.status]}
        </span>
      )}

      {syncedCount > 1 && (
        <span className="status-item">saisie synchronisée : {syncedCount} onglets</span>
      )}

      <span className="status-spacer" />

      <span className="status-item">{vaultEncrypted ? 'coffre chiffré' : 'coffre non chiffré'}</span>
      {version && <span className="status-item">v{version}</span>}
    </footer>
  );
}
