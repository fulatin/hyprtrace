import type { AppMetadata } from '../../lib/types';

/** Stable per-app colour: the same class always gets the same hue. */
const PALETTE = [
  '#22d3ee',
  '#a78bfa',
  '#34d399',
  '#fbbf24',
  '#f87171',
  '#60a5fa',
  '#f472b6',
  '#2dd4bf',
  '#fb923c',
  '#818cf8',
  '#4ade80',
  '#e879f9',
];

export function appColor(cls: string): string {
  let h = 0;
  for (let i = 0; i < cls.length; i++) h = (h * 31 + cls.charCodeAt(i)) >>> 0;
  return PALETTE[h % PALETTE.length];
}

export function displayName(cls: string, metadata?: Record<string, AppMetadata>): string {
  return metadata?.[cls]?.display_name || cls;
}

export const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

/** 0..1 → "23.3%"; values under 0.05% collapse to "<0.1%". */
export function pct(value: number, digits = 1): string {
  if (!Number.isFinite(value)) return '—';
  const p = value * 100;
  if (p > 0 && p < 0.05) return '<0.1%';
  return `${p.toFixed(digits)}%`;
}

export function formatSigned(value: number, digits = 1): string {
  if (!Number.isFinite(value)) return '—';
  return `${value >= 0 ? '+' : '−'}${Math.abs(value).toFixed(digits)}`;
}

/** Pearson correlation from two equal-length series (client-side, for charts). */
export function pearson(xs: number[], ys: number[]): number {
  const n = Math.min(xs.length, ys.length);
  if (n < 3) return 0;
  const mx = xs.reduce((a, b) => a + b, 0) / n;
  const my = ys.reduce((a, b) => a + b, 0) / n;
  let sxy = 0;
  let sxx = 0;
  let syy = 0;
  for (let i = 0; i < n; i++) {
    const dx = xs[i] - mx;
    const dy = ys[i] - my;
    sxy += dx * dy;
    sxx += dx * dx;
    syy += dy * dy;
  }
  if (sxx === 0 || syy === 0) return 0;
  return sxy / Math.sqrt(sxx * syy);
}

/** OLS fit for a scatter overlay: returns slope + intercept. */
export function linearFit(xs: number[], ys: number[]): { slope: number; intercept: number } {
  const n = Math.min(xs.length, ys.length);
  if (n < 2) return { slope: 0, intercept: ys[0] ?? 0 };
  const mx = xs.reduce((a, b) => a + b, 0) / n;
  const my = ys.reduce((a, b) => a + b, 0) / n;
  let sxy = 0;
  let sxx = 0;
  for (let i = 0; i < n; i++) {
    sxy += (xs[i] - mx) * (ys[i] - my);
    sxx += (xs[i] - mx) ** 2;
  }
  const slope = sxx === 0 ? 0 : sxy / sxx;
  return { slope, intercept: my - slope * mx };
}

/** Diverging colour for correlation strength (blue → red). */
export function corrColor(r: number): string {
  if (r <= -0.5) return 'text-bad';
  if (r <= -0.2) return 'text-warn';
  if (r < 0.2) return 'text-fg-muted';
  return 'text-good';
}

export function corrLabel(r: number): string {
  const a = Math.abs(r);
  const strength = a >= 0.6 ? 'strong' : a >= 0.3 ? 'moderate' : a >= 0.15 ? 'weak' : 'none';
  if (strength === 'none') return 'no correlation';
  return `${strength} ${r < 0 ? 'negative' : 'positive'}`;
}
