/**
 * Copie vers le presse-papiers avec effacement automatique.
 *
 * Un mot de passe ou une clé laissés dans le presse-papiers restent lisibles
 * par n'importe quelle application ; l'effacement différé limite cette fenêtre.
 */

const SETTINGS_KEY = 'sshmanager-clipboard';

export interface ClipboardSettings {
  /** Efface le presse-papiers après le délai, une fois la copie faite. */
  autoClear: boolean;
  delaySeconds: number;
}

export const CLIPBOARD_DELAY_OPTIONS = [10, 20, 30, 60, 120, 300];

export const DEFAULT_CLIPBOARD_SETTINGS: ClipboardSettings = {
  autoClear: true,
  delaySeconds: 30,
};

export function loadClipboardSettings(): ClipboardSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return DEFAULT_CLIPBOARD_SETTINGS;
    const parsed = JSON.parse(raw) as Partial<ClipboardSettings>;
    return {
      autoClear: parsed.autoClear ?? DEFAULT_CLIPBOARD_SETTINGS.autoClear,
      delaySeconds: parsed.delaySeconds ?? DEFAULT_CLIPBOARD_SETTINGS.delaySeconds,
    };
  } catch {
    return DEFAULT_CLIPBOARD_SETTINGS;
  }
}

export function saveClipboardSettings(settings: ClipboardSettings): void {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
}

/** Formats a delay for display: "30 s", "2 min". */
export function formatDelay(seconds: number): string {
  return seconds < 60 ? `${seconds} s` : `${seconds / 60} min`;
}

export interface CopyResult {
  /** Délai d'effacement programmé, ou null si l'effacement est désactivé. */
  delaySeconds: number | null;
  /** Vrai si la valeur a été tenue hors de l'historique Windows (Win+V). */
  privateMode: boolean;
}

/**
 * Copies `text`, then lets the main process clear the clipboard after the
 * configured delay — but only if it still holds `text`, so a later copy by the
 * user is never wiped.
 *
 * Tout est délégué au processus principal pour deux raisons :
 * `navigator.clipboard` exige une fenêtre au premier plan — condition jamais
 * remplie au moment de l'effacement, puisque l'utilisateur est parti coller —
 * et seule l'API Win32 permet d'exclure la valeur de l'historique Windows.
 */
export async function copyWithAutoClear(text: string): Promise<CopyResult> {
  const { autoClear, delaySeconds } = loadClipboardSettings();
  const result = await window.electronAPI.clipboardCopy({ text, autoClear, delaySeconds });
  return {
    delaySeconds: result.autoClear ? result.delaySeconds ?? delaySeconds : null,
    privateMode: result.privateMode,
  };
}

/** "✓ Copié · effacé dans 30 s", complété si l'exclusion n'a pas pu s'appliquer. */
export function describeCopy(result: CopyResult, feminine = false): string {
  const copied = feminine ? '✓ Copiée' : '✓ Copié';
  const cleared = result.delaySeconds === null
    ? copied
    : `${copied} · ${feminine ? 'effacée' : 'effacé'} dans ${formatDelay(result.delaySeconds)}`;
  return result.privateMode ? cleared : `${cleared} · historique Windows non exclu`;
}
