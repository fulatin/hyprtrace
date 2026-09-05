import { useMemo } from 'react';
import { startOfWeek, endOfWeek, eachDayOfInterval, format, parseISO } from 'date-fns';
import { CalendarDays } from 'lucide-react';
import type { DailyActivity } from '../lib/types';

interface ActivityHeatmapProps {
  data: DailyActivity[];
}

const CELL = 11;
const GAP = 3;

// GitHub-style intensity scale keyed by active minutes.
function colorFor(ms: number): string {
  if (ms <= 0) return '#1f2937'; // gray-800
  const minutes = ms / 60000;
  if (minutes < 30) return '#155e75'; // cyan-800
  if (minutes < 120) return '#0e7490'; // cyan-700
  if (minutes < 240) return '#06b6d4'; // cyan-500
  return '#67e8f9'; // cyan-300
}

function formatDuration(ms: number): string {
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m`;
  return '0m';
}

export default function ActivityHeatmap({ data }: ActivityHeatmapProps) {
  const { weeks, totals } = useMemo(() => {
    if (data.length === 0) return { weeks: [] as Date[][], totals: null };

    const first = data[0].date;
    const last = data[data.length - 1].date;
    const start = startOfWeek(parseISO(first), { weekStartsOn: 0 });
    const end = endOfWeek(parseISO(last), { weekStartsOn: 0 });
    const days = eachDayOfInterval({ start, end });

    const weeks: Date[][] = [];
    for (let i = 0; i < days.length; i += 7) {
      weeks.push(days.slice(i, i + 7));
    }

    const totalMs = data.reduce((sum, d) => sum + d.total_ms, 0);
    const activeDays = data.filter((d) => d.total_ms > 0).length;
    const busiest = data.reduce((max, d) => (d.total_ms > max.total_ms ? d : max), data[0]);

    return { weeks, totals: { totalMs, activeDays, busiest } };
  }, [data]);

  if (weeks.length === 0 || !totals) {
    return (
      <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 flex items-center justify-center h-32 text-gray-400">
        No activity data available
      </div>
    );
  }

  const byDate = new Map(data.map((d) => [d.date, d.total_ms]));
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
          {['#1f2937', '#155e75', '#0e7490', '#06b6d4', '#67e8f9'].map((c) => (
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
