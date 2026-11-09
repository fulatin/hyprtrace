import { describe, expect, it } from 'vitest';
import {
  HEAT_BUCKETS,
  heatBucketIndex,
  heatDate,
  heatFill,
  heatLabel,
  heatScale,
} from './heatmap';

describe('heatDate (issue #19)', () => {
  it('normalises plain API dates to the library key', () => {
    expect(heatDate('2026-01-05')).toBe('2026/1/5');
    expect(heatDate('2026-12-31')).toBe('2026/12/31');
  });

  it('handles dates with a time or offset suffix without a lookup miss', () => {
    // The backend emits `date_local` (plain yyyy-MM-dd), but a timestamped
    // value used to make every cell render as 0 (silent Map miss).
    expect(heatDate('2026-01-05T00:00:00+08:00')).toBe('2026/1/5');
    expect(heatDate('2026-01-05 12:34:56')).toBe('2026/1/5');
  });
});

describe('heat buckets (issue #19)', () => {
  it('classifies active minutes into the palette', () => {
    expect(heatBucketIndex(0)).toBe(0);
    expect(heatBucketIndex(-5)).toBe(0);
    expect(heatBucketIndex(Number.NaN)).toBe(0);
    expect(heatBucketIndex(1)).toBe(1);
    expect(heatBucketIndex(29)).toBe(1);
    expect(heatBucketIndex(30)).toBe(2);
    expect(heatBucketIndex(119)).toBe(2);
    expect(heatBucketIndex(120)).toBe(3);
    expect(heatBucketIndex(239)).toBe(3);
    expect(heatBucketIndex(240)).toBe(4);
    expect(heatBucketIndex(10_000)).toBe(4);
  });

  it('the palette, legend labels and cell fills all come from one table', () => {
    // Changing HEAT_BUCKETS must be enough: every derived view has exactly the
    // same length and order.
    expect(heatScale('dark')).toHaveLength(HEAT_BUCKETS.length);
    expect(heatScale('light')).toHaveLength(HEAT_BUCKETS.length);
    expect(heatScale('dark')).toEqual(HEAT_BUCKETS.map((bucket) => bucket.dark));
    expect(heatScale('light')).toEqual(HEAT_BUCKETS.map((bucket) => bucket.light));
    // One sample minute value per bucket (first minute of the bucket).
    const sampleMinutes = [0, 1, 30, 120, 240];
    expect(sampleMinutes).toHaveLength(HEAT_BUCKETS.length);
    for (const [index, bucket] of HEAT_BUCKETS.entries()) {
      expect(heatBucketIndex(sampleMinutes[index])).toBe(index);
      expect(heatFill(sampleMinutes[index], 'dark')).toBe(bucket.dark);
      expect(heatFill(sampleMinutes[index], 'light')).toBe(bucket.light);
    }
  });
});

describe('heatLabel (issue #19)', () => {
  const formatDuration = (ms: number) => (ms > 0 ? `${Math.round(ms / 60000)}m` : '<1m');

  it('announces the date and active time', () => {
    expect(heatLabel('2026-01-05', 2 * 3_600_000, formatDuration)).toBe(
      '2026-01-05: active 120m',
    );
  });

  it('says "no activity" for an empty day instead of "<1m"', () => {
    expect(heatLabel('2026-01-05', 0, formatDuration)).toBe('2026-01-05: no activity');
  });
});
