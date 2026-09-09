/** Shared formatters, used across pages so behaviour stays consistent. */

/** "3h 12m" / "12m" / "<1m" — coarse, for rankings and totals. */
export function formatDuration(ms: number): string {
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m`;
  return '<1m';
}

/** "3h 12m" / "12m 30s" / "45s" — finer, for session rows. */
export function formatDurationFine(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || ms === 0) return '—';
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  const secs = Math.floor((ms % 60000) / 1000);
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m ${secs}s`;
  return `${secs}s`;
}

/** "3h 12m" / "12m 30s" / "45s" / "0s" — never a dash. Use for measured spans
 * (session length, gap between apps, re-focus cost) where zero is real data. */
export function formatSpan(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms <= 0) return '0s';
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  const secs = Math.floor((ms % 60000) / 1000);
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m ${secs}s`;
  if (secs > 0) return `${secs}s`;
  return '<1s';
}

/** Signed delta for day-over-day comparisons: "+1h 20m" / "−12m". */
export function formatDelta(ms: number): string {
  const sign = ms >= 0 ? '+' : '−';
  const abs = Math.abs(ms);
  const hours = Math.floor(abs / 3600000);
  const mins = Math.floor((abs % 3600000) / 60000);
  if (hours > 0) return `${sign}${hours}h ${mins}m`;
  if (mins > 0) return `${sign}${mins}m`;
  return `${sign}<1m`;
}

export function formatPercent(value: number, digits = 0): string {
  return `${(value * 100).toFixed(digits)}%`;
}

/** Compact integer for headline tiles: 1240 → "1.2k", 14564 → "14.6k". */
export function formatCount(value: number): string {
  const n = Math.round(value);
  if (Math.abs(n) >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
  if (Math.abs(n) >= 10000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

/** "1.4 GB" / "820 MB" — memory samples arrive in KB. */
export function formatMemKb(kb: number): string {
  if (kb >= 1048576) return `${(kb / 1048576).toFixed(1)} GB`;
  if (kb >= 1024) return `${(kb / 1024).toFixed(0)} MB`;
  return `${Math.round(kb)} KB`;
}

export function formatHour(hour: number): string {
  return `${String(hour).padStart(2, '0')}:00`;
}

export function formatShortDate(iso: string): string {
  const [y, m, d] = iso.slice(0, 10).split('-');
  if (!y || !m || !d) return iso;
  return `${Number(m)}/${Number(d)}`;
}

/** Relative "3 days ago" for anomaly / report lists. */
export function formatRelativeDate(iso: string): string {
  const then = new Date(iso.length === 10 ? `${iso}T00:00:00` : iso);
  if (Number.isNaN(then.getTime())) return iso;
  const days = Math.round((Date.now() - then.getTime()) / 86400000);
  if (days <= 0) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 30) return `${days} days ago`;
  const months = Math.round(days / 30);
  return `${months} month${months > 1 ? 's' : ''} ago`;
}
