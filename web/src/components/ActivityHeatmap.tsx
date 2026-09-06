import { useMemo } from 'react';
import { startOfWeek, endOfWeek, eachDayOfInterval, format, parseISO } from 'date-fns';
import { CalendarDays } from 'lucide-react';
import type { DailyActivity } from '../lib/types';
import { formatDuration } from '../lib/format';

interface ActivityHeatmapProps {
  data: DailyActivity[];
}

const CELL = 11;
const GAP = 3;

// GitHub-style intensity scale keyed by active minutes. The legend below reads
// from the same array, so changing the scale only needs one edit.
const COLOR_SCALE = [
  '#1f2937', // gray-800  (0 min)
  '#155e75', // cyan-800  (<30 min)
  '#0e7490', // cyan-700  (<2h)
  '#06b6d4', // cyan-500  (<4h)
  '#67e8f9', // cyan-300  (>=4h)
];

function colorFor(ms: number): string {
  const minutes = ms / 60000;
  if (minutes < 30) return COLOR_SCALE[1];
  if (minutes < 120) return COLOR_SCALE[2];
  if (minutes < 240) return COLOR_SCALE[3];
  return COLOR_SCALE[4];
}

/** Normalise a date string to `yyyy-MM-dd` so a value with a time or timezone
 * suffix (e.g. `2026-01-05T00:00:00+08:00`) still matches a cell key. */
function dayKey(iso: string): string {
  try {
    return format(parseISO(iso), 'yyyy-MM-dd');
  } catch {
    return iso.slice(0, 10);
  }
}

export default function ActivityHeatmap({ data }: ActivityHeatmapProps) {
  const { weeks, totals } = useMemo(() => {
    if (data.length === 0) return { weeks: [] as Date[][], totals: null };

    // Sort by date so the grid span is correct even if the backend returns
    // days out of order.
    const sorted = [...data].sort((a, b) => a.date.localeCompare(b.date));
    const first = sorted[0].date;
    const last = sorted[sorted.length - 1].date;
    const start = startOfWeek(parseISO(first), { weekStartsOn: 0 });
    const end = endOfWeek(parseISO(last), { weekStartsOn: 0 });
    const days = eachDayOfInterval({ start, end });

    const weeks: Date[][] = [];
    for (let i = 0; i < days.length; i += 7) {
      weeks.push(days.slice(i, i + 7));
    }

    const totalMs = sorted.reduce((sum, d) => sum + d.total_ms, 0);
    const activeDays = sorted.filter((d) => d.total_ms > 0).length;
    const busiest = sorted.reduce((max, d) => (d.total_ms > max.total_ms ? d : max), sorted[0]);

    return { weeks, totals: { totalMs, activeDays, busiest } };
  }, [data]);

  if (weeks.length === 0 || !totals) {
    return (
      <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 flex items-center justify-center h-32 text-gray-400">
        No activity data available
      </div>
    );
  }

  // Key the map by normalised yyyy-MM-dd so lookup matches the cell key.
  const byDate = new Map(data.map((d) => [dayKey(d.date), d.total_ms]));
  const dayLabels = ['', 'Mon', '', 'Wed', '', 'Fri', ''];

  return (
    <div className="bg-gray-900 border border-gray-800 rounded-lg p-4">
      <div className="flex flex-wrap items-center gap-2 mb-4">
        <h3 className="text-sm font-medium text-gray-400 flex items-center gap-2">
          <CalendarDays size={14} className="text-cyan-400" />
          Activity
          <span className="text-xs text-gray-500 font-normal">
            last {data.length} days · {totals.activeDays} active days · {formatDuration(totals.totalMs)} total
          </span>
        </h3>
        <div className="ml-auto flex items-center gap-1 text-[10px] text-gray-500">
          <span>Less</span>
          {COLOR_SCALE.map((c) => (
            <span
              key={c}
              className="rounded-sm"
              style={{ width: CELL, height: CELL, backgroundColor: c }}
            />
          ))}
          <span>More</span>
        </div>
      </div>

      <div className="overflow-x-auto">
        <div className="inline-flex">
          {/* Day-of-week gutter */}
          <div className="flex flex-col mr-2" style={{ gap: GAP }}>
            {dayLabels.map((label, i) => (
              <div
                key={i}
                className="text-[9px] text-gray-600 leading-none"
                style={{ width: 22, height: CELL, display: 'flex', alignItems: 'center' }}
              >
                {label}
              </div>
            ))}
          </div>

          <div className="flex" style={{ gap: GAP }}>
            {weeks.map((week, wi) => {
              const monthLabel = format(week[0], 'MMM');
              const showMonth =
                wi === 0 || format(weeks[wi - 1][0], 'MMM') !== monthLabel;
              return (
                <div key={wi} className="flex flex-col" style={{ gap: GAP }}>
                  <div
                    className="text-[9px] text-gray-600 leading-none"
                    style={{ height: CELL, display: 'flex', alignItems: 'center', visibility: showMonth ? 'visible' : 'hidden' }}
                  >
                    {monthLabel}
                  </div>
                  {week.map((day) => {
                    const key = format(day, 'yyyy-MM-dd');
                    const ms = byDate.get(key) ?? 0;
                    return (
                      <div
                        key={key}
                        role="gridcell"
                        aria-label={`${key}: ${formatDuration(ms)}`}
                        className="rounded-sm"
                        style={{
                          width: CELL,
                          height: CELL,
                          backgroundColor: colorFor(ms),
                        }}
                        title={`${key}: ${formatDuration(ms)}`}
                      />
                    );
                  })}
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {totals.busiest.total_ms > 0 && (
        <p className="text-xs text-gray-500 mt-3">
          Busiest day: <span className="text-gray-300">{totals.busiest.date}</span> ({formatDuration(totals.busiest.total_ms)})
        </p>
      )}
    </div>
  );
}
