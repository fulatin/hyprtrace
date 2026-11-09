import { format } from 'date-fns';

/**
 * A local calendar day as `yyyy-MM-dd`.
 *
 * The API's `date`, `date_local` and `hourly_summary.date` columns are *local*
 * days; deriving them with `toISOString().slice(0, 10)` yields the UTC day and
 * shifts the date by one for UTC+8 users between 00:00 and 08:00 (issue #26).
 * Kept in one place so the rule is testable and cannot quietly reappear in a
 * new component.
 */
export function localDate(date: Date = new Date()): string {
  return format(date, 'yyyy-MM-dd');
}
