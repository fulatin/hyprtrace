import { useEffect, useState } from 'react';
import { format, subDays } from 'date-fns';
import { api } from '../lib/api';
import type { Session, PaginatedResponse } from '../lib/types';

function formatDuration(ms: number | null): string {
  if (ms === null || ms === 0) return '-';
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  const secs = Math.floor((ms % 60000) / 1000);
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m ${secs}s`;
  return `${secs}s`;
}

function formatTime(iso: string): string {
  try {
    return format(new Date(iso), 'HH:mm:ss');
  } catch {
    return iso;
  }
}

const COLORS = [
  '#22d3ee', '#8b5cf6', '#10b981', '#f59e0b', '#ef4444',
  '#3b82f6', '#ec4899', '#14b8a6', '#f97316', '#6366f1',
];

export default function Sessions() {
  const today = format(new Date(), 'yyyy-MM-dd');
  const weekAgo = format(subDays(new Date(), 7), 'yyyy-MM-dd');
  const [page, setPage] = useState(1);
  const [data, setData] = useState<PaginatedResponse<Session> | null>(null);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState('');

  useEffect(() => {
    setPage(1);
  }, [filter]);

  useEffect(() => {
    setLoading(true);
    api.sessions(weekAgo, today, page, 50, filter || undefined).then((d) => {
      setData(d);
      setLoading(false);
    });
  }, [page, filter]);

  const classes = [...new Set(data?.data.map((s) => s.class) ?? [])];
  const getColor = (cls: string) => {
    const idx = classes.indexOf(cls);
    return COLORS[idx % COLORS.length];
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">Sessions</h2>
        <select
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className="bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 focus:ring-cyan-500"
        >
          <option value="">All Apps</option>
          {classes.map((c) => (
            <option key={c} value={c}>{c}</option>
          ))}
        </select>
      </div>

      {loading ? (
        <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 animate-pulse h-64" />
      ) : (
        <div className="bg-gray-900 border border-gray-800 rounded-xl overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-800">
                <th className="text-left px-4 py-3 text-gray-400 font-medium">App</th>
                <th className="text-left px-4 py-3 text-gray-400 font-medium">Title</th>
                <th className="text-left px-4 py-3 text-gray-400 font-medium">Start</th>
                <th className="text-left px-4 py-3 text-gray-400 font-medium">End</th>
                <th className="text-right px-4 py-3 text-gray-400 font-medium">Duration</th>
              </tr>
            </thead>
            <tbody>
              {data?.data.map((s) => (
                <tr key={s.id} className="border-b border-gray-800/50 hover:bg-gray-800/50">
                  <td className="px-4 py-2.5">
                    <span
                      className="inline-block w-2 h-2 rounded-full mr-2"
                      style={{ backgroundColor: getColor(s.class) }}
                    />
                    {s.class}
                  </td>
                  <td className="px-4 py-2.5 text-gray-400 truncate max-w-[200px]">{s.title || '-'}</td>
                  <td className="px-4 py-2.5 text-gray-400">{formatTime(s.started_at)}</td>
                  <td className="px-4 py-2.5 text-gray-400">{s.ended_at ? formatTime(s.ended_at) : 'Active'}</td>
                  <td className="px-4 py-2.5 text-right text-cyan-400">{formatDuration(s.duration_ms)}</td>
                </tr>
              ))}
            </tbody>
          </table>

          {data && (
            <div className="flex items-center justify-between px-4 py-3 border-t border-gray-800">
              <span className="text-xs text-gray-400">
                Page {page} of {Math.max(1, Math.ceil(data.total / 50))}
              </span>
              <div className="flex gap-2">
                <button
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                  disabled={page <= 1}
                  className="px-3 py-1 bg-gray-800 rounded text-xs text-gray-300 disabled:opacity-50"
                >
                  Previous
                </button>
                <button
                  onClick={() => setPage((p) => p + 1)}
                  disabled={page >= Math.ceil(data.total / 50)}
                  className="px-3 py-1 bg-gray-800 rounded text-xs text-gray-300 disabled:opacity-50"
                >
                  Next
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}