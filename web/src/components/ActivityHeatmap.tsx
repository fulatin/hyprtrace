import { useMemo } from 'react';
import HeatMap from '@uiw/react-heat-map';
import { CalendarDays } from 'lucide-react';
import type { DailyActivity } from '../lib/types';
import { formatDuration } from '../lib/format';

interface ActivityHeatmapProps {
  data: DailyActivity[];
}

// GitHub-style intensity scale keyed by active minutes.
const COLOR_SCALE = [
  '#1f2937', // 0 min
  '#155e75', // <30 min
  '#0e7490', // <2h
  '#06b6d4', // <4h
  '#67e8f9', // >=4h
];

// Legend labels aligned with COLOR_SCALE thresholds.
const LEGEND_LABELS = ['0', '<30m', '<2h', '<4h', '>=4h'];

/** Convert a `yyyy-MM-dd` (or `yyyy-MM-ddTHH:...Z`) date to the `YYYY/M/D`
 * (no leading zero) format that `@uiw/react-heat-map` uses internally, so a
 * cell's `valueItem.date` matches the `countByDate` lookup key. */
function heatDate(iso: string): string {
  const [y, m, d] = iso.slice(0, 10).split('-').map(Number);
  return `${y}/${m}/${d}`;
}

export default function ActivityHeatmap({ data }: ActivityHeatmapProps) {
  const { value, startDate, endDate, totals } = useMemo(() => {
    if (data.length === 0) {
      return {
        value: [] as { date: string; count: number }[],
        startDate: new Date(),
        endDate: new Date(),
        totals: null,
      };
    }

    // Sort by date so the grid span is correct even if the backend returns
    // days out of order, and key the count by the library's YYYY/M/D format so
    // the rectRender lookup matches a cell's valueItem.date.
    const sorted = [...data].sort((a, b) => a.date.localeCompare(b.date));
    const countByDate = new Map<string, number>();
    for (const d of sorted) {
      countByDate.set(heatDate(d.date), d.total_ms);
    }

    const value = sorted.map((d) => ({
      date: heatDate(d.date),
      count: Math.round(d.total_ms / 60000),
    }));

    const startDate = new Date(heatDate(sorted[0].date));
    const endDate = new Date(heatDate(sorted[sorted.length - 1].date));

    const totalMs = sorted.reduce((sum, d) => sum + d.total_ms, 0);
    const activeDays = sorted.filter((d) => d.total_ms > 0).length;
    const busiest = sorted.reduce((max, d) => (d.total_ms > max.total_ms ? d : max), sorted[0]);

    return { value, startDate, endDate, totals: { totalMs, activeDays, busiest, countByDate } };
  }, [data]);

  if (!totals || value.length === 0) {
    return (
      <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 flex items-center justify-center h-32 text-gray-400">
        No activity data available
      </div>
    );
  }

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
      </div>

      <div className="overflow-x-auto">
        <HeatMap
          value={value}
          startDate={startDate}
          endDate={endDate}
          rectSize={12}
          space={3}
          // Disable the library's built-in legend (its cell gap is fixed at 1px
          // and looks cramped); we render our own with a wider gap below.
          legendCellSize={0}
          panelColors={COLOR_SCALE}
          // Accessibility: every cell exposes its date + active time, and the
          // tooltip uses the shared formatter.
          rectRender={(props, valueItem) => {
            const ms = totals.countByDate.get(valueItem.date) ?? 0;
            return (
              <rect {...props} role="gridcell" aria-label={`${valueItem.date}: ${formatDuration(ms)}`}>
                <title>{`${valueItem.date}: ${formatDuration(ms)}`}</title>
              </rect>
            );
          }}
        />
      </div>

      {/* Custom legend with a comfortable gap between the intensity cells. */}
      <div className="flex items-center justify-end gap-3 mt-3">
        <span className="text-[10px] text-gray-500">Less</span>
        <div className="flex items-center gap-2">
          {COLOR_SCALE.map((c, i) => (
            <div key={c} className="flex items-center gap-1.5">
              <span className="rounded-sm" style={{ width: 11, height: 11, backgroundColor: c }} />
              <span className="text-[10px] text-gray-500">{LEGEND_LABELS[i]}</span>
            </div>
          ))}
        </div>
        <span className="text-[10px] text-gray-500">More</span>
      </div>

      {totals.busiest.total_ms > 0 && (
        <p className="text-xs text-gray-500 mt-3">
          Busiest day: <span className="text-gray-300">{totals.busiest.date}</span> ({formatDuration(totals.busiest.total_ms)})
        </p>
      )}
    </div>
  );
}
