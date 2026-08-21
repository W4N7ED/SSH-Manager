export type ConnectionType = 'ssh' | 'sftp' | 'ftp';

export interface Connection {
  id: string;
  name: string;
  type: ConnectionType;
  host: string;
  port: number;
  username: string;
  password?: string;
  privateKeyPath?: string;
  ftpSecure?: boolean;
  group?: string;
  favorite?: boolean;
  /** Optional accent color (overrides the group color when set). */
  color?: string;
  createdAt: number;
  /** Timestamp of the last time a session was opened on this connection. */
  lastUsedAt?: number;
}

/** Which slice of the saved connections the group rail is showing. */
export type GroupFilter =
  | { kind: 'all' }
  | { kind: 'favorites' }
  | { kind: 'ungrouped' }
  | { kind: 'group'; name: string };

export type TabType = 'terminal' | 'filebrowser';
export type Quadrant = 'topLeft' | 'topRight' | 'bottomLeft' | 'bottomRight';

export interface Tab {
  id: string;
  connectionId: string;
  connectionName: string;
  type: TabType;
  connectionType: ConnectionType;
  status: 'connecting' | 'connected' | 'disconnected' | 'error';
  error?: string;
}

export interface QuickConnectInfo {
  host: string;
  port: number;
  username: string;
  type: ConnectionType;
}
