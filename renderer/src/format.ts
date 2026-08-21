/** Human-readable formatting shared across the interface. */

/** "il y a 2 heures", "hier", "il y a 3 semaines"… from a timestamp. */
export function formatLastUsed(timestamp?: number): string {
  if (!timestamp) return 'jamais';
  const minutes = Math.round((Date.now() - timestamp) / 60_000);
  if (minutes < 1) return "à l'instant";

  const relative = new Intl.RelativeTimeFormat('fr', { numeric: 'auto' });
  if (minutes < 60) return relative.format(-minutes, 'minute');
  const hours = Math.round(minutes / 60);
  if (hours < 24) return relative.format(-hours, 'hour');
  const days = Math.round(hours / 24);
  if (days < 7) return relative.format(-days, 'day');
  const weeks = Math.round(days / 7);
  if (weeks < 5) return relative.format(-weeks, 'week');
  return relative.format(-Math.round(days / 30), 'month');
}
