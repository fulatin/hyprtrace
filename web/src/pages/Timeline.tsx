import { useEffect, useMemo, useState } from 'react';
import { format } from 'date-fns';
import { api } from '../lib/api';
import type { Session } from '../lib/types';
import ErrorState from '../components/ErrorState';

const CATEGORY_COLORS = [
  '#22d3ee', '#a78bfa', '#34d399', '#f472b6', '#fbbf24',
  '#60a5fa', '#f87171', '#4ade80', '#e879f9', '#38bdf8',
];

// Padding (minutes) added on both sides of the active time range.
const AXIS_PADDING = 15;
const DAY_MINUTES = 1440;

function colorForClass(cls: string): string {
  let h = 0;
  for (let i = 0; i < cls.length; i++) h = (h * 31 + cls.charCodeAt(i)) >>> 0;
  return CATEGORY_COLORS[h % CATEGORY_COLORS.length];
}

function toMinutes(iso: string): number {
  const d = new Date(iso);
  return d.getHours() * 60 + d.getMinutes();
}

// Minutes of the current moment, clamped to [0, 1440]. Used as the end point
// of an ongoing (not-yet-ended) session so its block doesn't stretch to 23:59.
function nowMinutes(): number {
  return Math.max(0, Math.min(toMinutes(new Date().toISOString()), DAY_MINUTES));
}

function formatDur(ms: number): string {
  const m = Math.round(ms / 60000);
  if (m >= 60) return `${Math.floor(m / 60)}h ${m % 60}m`;
  return `${m}m`;
}

export default function Timeline() {
  const [date, setDate] = useState(format(new Date(), 'yyyy-MM-dd'));
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    setLoading(true);
    setError(null);
    // Fetch the whole day (large per_page) ordered by start time.
    api.sessions(date, date, 1, 2000).then((res) => {
      const sorted = [...res.data].sort((a, b) => a.started_at.localeCompare(b.started_at));
      setSessions(sorted);
      setLoading(false);
    }).catch((e) => {
      setError(e instanceof Error ? e.message : 'Unknown error');
      setLoading(false);
    });
  }, [date, reloadKey]);

  // Group sessions by app class so each app occupies a single row with all of
  // its time blocks, instead of one row per session.
  const groups = useMemo(() => {
    const map = new Map<string, Session[]>();
    for (const s of sessions) {
      const arr = map.get(s.class) ?? [];
      arr.push(s);
      map.set(s.class, arr);
    }
    return Array.from(map.entries());
  }, [sessions]);

  // Dynamic horizontal axis: only show the window where the user was actually
  // active (clamped to the day), padded slightly on each side. Falls back to
  // the full 24h when there are no sessions.
  const axis = useMemo(() => {
    if (sessions.length === 0) {
      return { start: 0, end: DAY_MINUTES };
    }
    let minStart = DAY_MINUTES;
    let maxEnd = 0;
    for (const s of sessions) {
      const startM = Math.max(0, Math.min(toMinutes(new Date(s.started_at).toISOString()), DAY_MINUTES));
      const endM = s.ended_at
        ? Math.max(0, Math.min(toMinutes(new Date(s.ended_at).toISOString()), DAY_MINUTES))
        : nowMinutes();
      if (startM < minStart) minStart = startM;
      if (endM > maxEnd) maxEnd = endM;
    }
    let start = Math.max(0, minStart - AXIS_PADDING);
    let end = Math.min(DAY_MINUTES, maxEnd + AXIS_PADDING);
    // Guarantee a minimum visible span so tiny windows stay readable.
    const MIN_SPAN = 60;
    if (end - start < MIN_SPAN) {
      const mid = (start + end) / 2;
      start = Math.max(0, mid - MIN_SPAN / 2);
      end = Math.min(DAY_MINUTES, mid + MIN_SPAN / 2);
    }
    return { start, end };
  }, [sessions]);

  const axisLen = Math.max(axis.end - axis.start, 1);

  // Hour tick marks within the active window.
  const hourTicks = useMemo(() => {
    const ticks: number[] = [];
    for (let h = Math.floor(axis.start / 60); h * 60 <= axis.end; h++) {
      ticks.push(h);
    }
    return ticks;
  }, [axis]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">Session Timeline</h2>
        <input
          type="date"
          value={date}
          onChange={(e) => setDate(e.target.value)}
          className="bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 focus:ring-cyan-500 focus:border-cyan-500"
        />
      </div>

      {loading ? (
        <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 animate-pulse h-64" />
      ) : error ? (
        <ErrorState message={error} onRetry={() => setReloadKey((k) => k + 1)} />
      ) : (
        <div className="bg-gray-900 border border-gray-800 rounded-lg p-4 overflow-x-auto">
          <div className="flex">
            {/* Row labels */}
            <div className="shrink-0 w-40 pr-3">
              {/* Spacer matching the hour-label row so app rows align */}
              <div className="h-6 mb-1" />
              {groups.map(([cls, items]) => (
                <div key={cls} className="h-7 flex items-center overflow-hidden">
                  <span className="text-xs text-gray-400 truncate" title={cls}>
                    {cls}
                    <span className="text-gray-600"> ({items.length})</span>
                  </span>
                </div>
              ))}
              {groups.length === 0 && (
                <span className="text-xs text-gray-500">No sessions this day</span>
              )}
            </div>

            {/* Chart area: dynamic active-time axis */}
            <div className="flex-1 min-w-[480px]">
              {groups.length === 0 ? (
                <div className="h-40 flex flex-col items-center justify-center rounded-md border border-dashed border-gray-800 text-gray-500">
                  <span className="text-sm">No activity recorded this day</span>
                  <span className="text-xs mt-1">The timeline will appear here once sessions exist</span>
                </div>
              ) : (
                <>
                  {/* Hour labels in normal flow (not clipped) */}
                  <div className="relative h-6 mb-1">
                    {hourTicks.map((h) => {
                      const hh = h % 24;
                      const left = (((h * 60 - axis.start) / axisLen) * 100);
                      return (
                        <div
                          key={`l${h}`}
                          className="absolute -translate-x-1/2 text-[10px] text-gray-600 whitespace-nowrap"
                          style={{ left: `${left}%` }}
                        >
                          {hh}:00
                        </div>
                      );
                    })}
                  </div>

              {/* Time blocks, one row per app class */}
              <div className="relative">
                {/* Gridlines: every 30 min faint, every hour stronger */}
                {(() => {
                  const lines: JSX.Element[] = [];
                  for (let m = Math.floor(axis.start / 30) * 30; m <= axis.end; m += 30) {
                    const left = ((m - axis.start) / axisLen) * 100;
                    lines.push(
                      <div
                        key={`g${m}`}
                        className="absolute top-0 bottom-0 border-l"
                        style={{
                          left: `${left}%`,
                          borderColor: m % 60 === 0 ? 'rgba(75,85,99,0.5)' : 'rgba(75,85,99,0.2)',
                        }}
                      />,
                    );
                  }
                  return lines;
                })()}

                {groups.map(([cls, items]) => (
                  <div key={cls} className="relative h-7">
                    {items.map((s) => {
                      const start = new Date(s.started_at);
                      // Clamp to day bounds. An ongoing session ends at "now"
                      // instead of stretching to 23:59.
                      const startM = Math.max(0, Math.min(toMinutes(start.toISOString()), DAY_MINUTES));
                      const endM = s.ended_at
                        ? Math.max(0, Math.min(toMinutes(new Date(s.ended_at).toISOString()), DAY_MINUTES))
                        : nowMinutes();
                      if (endM <= startM) return null;
                      const left = ((startM - axis.start) / axisLen) * 100;
                      const width = ((endM - startM) / axisLen) * 100;
                      const color = colorForClass(cls);
                      const mins = Math.round((s.duration_ms ?? 0) / 60000);
                      return (
                        <div
                          key={s.id}
                          className="absolute top-1 h-5 rounded-sm flex items-center overflow-hidden"
                          style={{
                            left: `${left}%`,
                            width: `${width}%`,
                            backgroundColor: `${color}33`,
                            border: `1px solid ${color}66`,
                          }}
                          title={`${cls} — ${s.title} — ${formatDur(s.duration_ms ?? 0)}`}
                        >
                          <span className="text-[9px] text-gray-300 px-1 truncate">
                            {width > 3 ? `${mins}m` : ''}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                ))}
              </div>
                </>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
