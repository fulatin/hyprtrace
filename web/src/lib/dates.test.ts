import { describe, expect, it } from 'vitest';
import { localDate } from './dates';

describe('localDate (issue #26)', () => {
  it('formats the local calendar day, not the UTC one', () => {
    // 07:00 local on 2026-11-09. `toISOString()` returns the UTC instant, so in
    // UTC+8 the old expression (`new Date().toISOString().slice(0, 10)`)
    // produced 2026-11-08; `localDate` must not.
    const morning = new Date(2026, 10, 9, 7, 0, 0);
    expect(localDate(morning)).toBe('2026-11-09');

    // Late evening is the mirror-image failure of the same bug: it is still the
    // same local day even though UTC has already moved on.
    const evening = new Date(2026, 10, 9, 23, 30, 0);
    expect(localDate(evening)).toBe('2026-11-09');
  });

  it('zero-pads month and day', () => {
    expect(localDate(new Date(2026, 0, 5, 12, 0, 0))).toBe('2026-01-05');
  });

  it('differs from the UTC date during the local early morning', () => {
    // Guard the actual defect: for a local-midnight-to-08:00 moment in a
    // positive-offset timezone, the UTC slice is the previous day.
    const morning = new Date(2026, 10, 9, 1, 0, 0);
    const utcDay = morning.toISOString().slice(0, 10);
    if (morning.getTimezoneOffset() < 0) {
      expect(utcDay).not.toBe(localDate(morning));
    } else {
      // In UTC or a negative-offset zone both must agree; the point of the test
      // is that `localDate` never disagrees with the local components.
      expect(localDate(morning)).toBe('2026-11-09');
    }
  });
});
