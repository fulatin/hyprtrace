/**
 * Pure helpers for the yearly activity heatmap.
 *
 * Kept out of the component so the date-normalisation and bucket logic can be
 * unit tested without rendering SVG, and so the palette has exactly **one**
 * definition: the cells, the legend and the tooltip all read `HEAT_BUCKETS`.
 * Issue #19 was filed because the old component duplicated the colour scale
 * (and its thresholds) between the cell fill function and the legend.
 */

export type HeatTheme = 'light' | 'dark';

export interface HeatBucket {
  /** Exclusive upper bound in active minutes; `Infinity` for the last bucket. */
  maxMinutes: number;
  /** Legend label for this bucket. */
  label: string;
  /** Cell colour in the dark theme. */
  dark: string;
  /** Cell colour in the light theme. */
  light: string;
}

/**
 * Intensity scale, keyed by active minutes. The first bucket is "no activity".
 * Changing a colour or threshold here changes the cells, the legend and the
 * screen-reader label together.
 */
export const HEAT_BUCKETS: readonly HeatBucket[] = [
  { maxMinutes: 1, label: '0', dark: '#16222e', light: '#e2e8f0' },
  { maxMinutes: 30, label: '<30m', dark: '#155e75', light: '#a5f3fc' },
  { maxMinutes: 120, label: '<2h', dark: '#0e7490', light: '#67e8f9' },
  { maxMinutes: 240, label: '<4h', dark: '#06b6d4', light: '#22d3ee' },
  { maxMinutes: Infinity, label: '≥4h', dark: '#67e8f9', light: '#0891b2' },
];

/** Index into {@link HEAT_BUCKETS} for a day's active minutes. */
export function heatBucketIndex(minutes: number): number {
  if (!Number.isFinite(minutes) || minutes <= 0) return 0;
  for (let index = 1; index < HEAT_BUCKETS.length; index += 1) {
    if (minutes < HEAT_BUCKETS[index].maxMinutes) return index;
  }
  return HEAT_BUCKETS.length - 1;
}

/** Cell fill for a day's active minutes in the given theme. */
export function heatFill(minutes: number, theme: HeatTheme): string {
  const bucket = HEAT_BUCKETS[heatBucketIndex(minutes)];
  return theme === 'light' ? bucket.light : bucket.dark;
}

/** The full palette, in legend order (also the library's `panelColors`). */
export function heatScale(theme: HeatTheme): string[] {
  return HEAT_BUCKETS.map((bucket) => (theme === 'light' ? bucket.light : bucket.dark));
}

/**
 * Convert an API date (`yyyy-MM-dd`, optionally with a time suffix) to the
 * `YYYY/M/D` key `@uiw/react-heat-map` uses for its cells.
 */
export function heatDate(iso: string): string {
  const [year, month, day] = iso.slice(0, 10).split('-').map(Number);
  return `${year}/${month}/${day}`;
}

/**
 * Screen-reader/tooltip label. A zero-activity day says so explicitly instead
 * of reading "<1m" (which the shared formatter returns for 0 ms).
 */
export function heatLabel(iso: string, totalMs: number, formatDuration: (ms: number) => string): string {
  const date = iso.slice(0, 10);
  if (!Number.isFinite(totalMs) || totalMs <= 0) return `${date}: no activity`;
  return `${date}: active ${formatDuration(totalMs)}`;
}
