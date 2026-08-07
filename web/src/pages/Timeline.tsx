import { useEffect, useState } from 'react';
import { format } from 'date-fns';
import { api } from '../lib/api';
import type { Session } from '../lib/types';

const CATEGORY_COLORS = [
  '#22d3ee', '#a78bfa', '#34d399', '#f472b6', '#fbbf24',
  '#60a5fa', '#f87171', '#4ade80', '#e879f9', '#38bdf8',
];

function colorForClass(cls: string): string {
  let h = 0;
  for (let i = 0; i < cls.length; i++) h = (h * 31 + cls.charCodeAt(i)) >>> 0;
  return CATEGORY_COLORS[h % CATEGORY_COLORS.length];
}

function toMinutes(iso: string): number {
  const d = new Date(iso);
  return d.getHours() * 60 + d.getMinutes();
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

  useEffect(() => {
    setLoading(true);
    // Fetch the whole day (large per_page) ordered by start time.
    api.sessions(date, date, 1, 2000).then((res) => {
      const sorted = [...res.data].sort((a, b) => a.started_at.localeCompare(b.started_at));
      setSessions(sorted);
      setLoading(false);
    });
  }, [date]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">Session Timeline</h2>
        <input
          type="date"
          value={date}
          onChange={(e) => setDate(e.target.value)}
          className="bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 focus:ring-cyan-500 focus:border-cyan-500"
        />
      </div>

      {loading ? (
        <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 animate-pulse h-64" />
      ) : (
        <div className="bg-gray-900 border border-gray-800 rounded-xl p-4 overflow-x-auto">
          <div className="flex">
            {/* Row labels */}
            <div className="shrink-0 w-40 pr-3">
              {sessions.map((s) => (
                <div key={s.id} className="h-7 flex items-center overflow-hidden">
                  <span className="text-xs text-gray-400 truncate" title={s.title}>
                    {s.class}
                  </span>
                </div>
              ))}
              {sessions.length === 0 && (
                <span className="text-xs text-gray-500">No sessions this day</span>
              )}
            </div>

            {/* Chart area: 24h = 1440 minutes */}
            <div className="relative flex-1 min-w-[960px]">
              {/* Hour gridlines */}
              {Array.from({ length: 25 }, (_, i) => (
                <div
                  key={i}
                  className="absolute top-0 bottom-0 border-l border-gray-800/60"
                  style={{ left: `${(i / 24) * 100}%` }}
                />
              ))}
              {/* Hour labels */}
              {Array.from({ length: 24 }, (_, i) => (
                <div
                  key={`l${i}`}
                  className="absolute -top-6 text-[10px] text-gray-600"
                  style={{ left: `${(i / 24) * 100}%` }}
                >
                  {i}
                </div>
              ))}

              {sessions.map((s) => {
                const start = new Date(s.started_at);
                const end = s.ended_at ? new Date(s.ended_at) : new Date();
                // Clamp to day bounds
                const startM = Math.max(toMinutes(start.toISOString()), 0);
                const endM = Math.min(toMinutes(end.toISOString()), 1440);
                if (endM <= startM) return null;
                const left = (startM / 1440) * 100;
                const width = ((endM - startM) / 1440) * 100;
                const color = colorForClass(s.class);
                const mins = Math.round((s.duration_ms ?? 0) / 60000);
                return (
                  <div
                    key={s.id}
                    className="absolute h-5 mt-1 rounded-sm flex items-center overflow-hidden"
                    style={{ left: `${left}%`, width: `${width}%`, backgroundColor: `${color}33`, border: `1px solid ${color}66` }}
                    title={`${s.class} — ${s.title} — ${formatDur(s.duration_ms ?? 0)}`}
                  >
                    <span
                      className="text-[9px] text-gray-300 px-1 truncate"
                      style={{ minWidth: width > 2 ? undefined : 0 }}
                    >
                      {width > 4 ? `${s.class} ${mins}m` : ''}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
