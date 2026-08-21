/**
 * Terminal color theme presets.
 * Only background, foreground and cursor are user-configurable.
 * ANSI colors remain at their theme defaults.
 */

export interface TerminalThemeConfig {
  id: string; // 'github-dark' | ... | 'custom'
  label: string;
  background: string;
  foreground: string;
  cursor: string;
}

export const TERMINAL_PRESETS: TerminalThemeConfig[] = [
  {
    // Thème par défaut : prolonge la direction graphique « Minuit » de l'interface.
    id: 'minuit', label: 'Minuit',
    background: '#0e0b16', foreground: '#d9d4e8', cursor: '#9a6bff',
  },
  {
    id: 'github-dark', label: 'GitHub Dark',
    background: '#0d1117', foreground: '#e6edf3', cursor: '#58a6ff',
  },
  {
    id: 'dracula', label: 'Dracula',
    background: '#282a36', foreground: '#f8f8f2', cursor: '#ff79c6',
  },
  {
    id: 'monokai', label: 'Monokai',
    background: '#272822', foreground: '#f8f8f2', cursor: '#f8f8f0',
  },
  {
    id: 'solarized-dark', label: 'Solarized Dark',
    background: '#002b36', foreground: '#839496', cursor: '#268bd2',
  },
  {
    id: 'nord', label: 'Nord',
    background: '#2e3440', foreground: '#d8dee9', cursor: '#88c0d0',
  },
  {
    id: 'one-dark', label: 'One Dark',
    background: '#282c34', foreground: '#abb2bf', cursor: '#528bff',
  },
  {
    id: 'light', label: 'Clair',
    background: '#ffffff', foreground: '#24292f', cursor: '#0969da',
  },
];

export const DEFAULT_TERMINAL_THEME: TerminalThemeConfig = TERMINAL_PRESETS[0];

export const TERMINAL_THEME_KEY = 'sshmanager-terminal-theme';

export function loadTerminalTheme(): TerminalThemeConfig {
  try {
    const raw = localStorage.getItem(TERMINAL_THEME_KEY);
    return raw ? (JSON.parse(raw) as TerminalThemeConfig) : DEFAULT_TERMINAL_THEME;
  } catch {
    return DEFAULT_TERMINAL_THEME;
  }
}
