import { useEffect, useState } from 'react';
import { format, subDays } from 'date-fns';
import { FileText, Search } from 'lucide-react';
import { api } from '../lib/api';
import type { TitleStat, AppMetadata } from '../lib/types';
import AppName from '../components/AppName';

function formatDuration(ms: number): string {
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m`;
  return '<1m';
}

function formatWhen(iso: string): string {
  try {
    return format(new Date(iso), 'MMM d, HH:mm');
  } catch {
    return iso;
  }
}

export default function Titles() {
  const today = format(new Date(), 'yyyy-MM-dd');
  const weekAgo = format(subDays(new Date(), 7), 'yyyy-MM-dd');
  const [from, setFrom] = useState(weekAgo);
  const [to, setTo] = useState(today);
  const [cls, setCls] = useState('');
  const [titles, setTitles] = useState<TitleStat[]>([]);
  const [classes, setClasses] = useState<string[]>([]);
  const [appMetadata, setAppMetadata] = useState<Record<string, AppMetadata>>({});
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');

  useEffect(() => {
    api.appClasses(weekAgo, today).then(setClasses).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Resolve friendly names/icons for the classes present in the results.
  useEffect(() => {
    const unique = Array.from(new Set(titles.map((t) => t.class)));
    if (unique.length === 0) {
      setAppMetadata({});
      return;
    }
    api.appsMetadata(unique).then((res) => setAppMetadata(res.entries)).catch(() => setAppMetadata({}));
  }, [titles]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api.titles(from, to, cls || undefined, 200)
      .then((t) => {
        if (!cancelled) {
          setTitles(t);
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setTitles([]);
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [from, to, cls]);

  const filtered = titles.filter((t) =>
    t.title.toLowerCase().includes(search.trim().toLowerCase())
  );

  const totalMs = titles.reduce((sum, t) => sum + t.total_ms, 0);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-bold flex items-center gap-2">
            <FileText size={20} className="text-cyan-400" />
            Documents
          </h2>
          <p className="text-xs text-gray-500 mt-1">
            Time spent on individual window titles (files, tabs, pages). {totalMs > 0 ? `${formatDuration(totalMs)} across ${titles.length} titles.` : ''}
          </p>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          <input
            type="date"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
            className="bg-gray-800 border border-gray-700 rounded-md px-2 py-1.5 text-xs text-gray-200 focus:ring-cyan-500"
          />
          <span className="text-gray-500 text-xs">→</span>
          <input
            type="date"
            value={to}
            onChange={(e) => setTo(e.target.value)}
            className="bg-gray-800 border border-gray-700 rounded-md px-2 py-1.5 text-xs text-gray-200 focus:ring-cyan-500"
          />
          <select
            value={cls}
            onChange={(e) => setCls(e.target.value)}
            className="bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 focus:ring-cyan-500"
          >
            <option value="">All Apps</option>
            {classes.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
        </div>
      </div>

      <div className="relative">
        <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-gray-500" />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Filter titles..."
          className="w-full bg-gray-900 border border-gray-800 rounded-lg pl-9 pr-3 py-2 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
        />
      </div>

      {loading ? (
        <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 animate-pulse h-64" />
      ) : filtered.length === 0 ? (
        <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 text-center text-gray-500">
          {titles.length === 0
            ? 'No title data in this range. Titles are recorded while the "Record window titles" privacy setting is enabled.'
            : 'No titles match your filter.'}
        </div>
      ) : (
        <div className="bg-gray-900 border border-gray-800 rounded-lg overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-gray-800">
                <th className="text-left px-4 py-3 text-gray-400 font-medium">App</th>
                <th className="text-left px-4 py-3 text-gray-400 font-medium">Title</th>
                <th className="text-right px-4 py-3 text-gray-400 font-medium">Sessions</th>
                <th className="text-left px-4 py-3 text-gray-400 font-medium">Last used</th>
                <th className="text-right px-4 py-3 text-gray-400 font-medium">Time</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((t) => (
                <tr key={`${t.class}:${t.title}`} className="border-b border-gray-800/50 hover:bg-gray-800/50 transition-colors">
                  <td className="px-4 py-2.5">
                    <AppName cls={t.class} metadata={appMetadata[t.class] ?? null} />
                  </td>
                  <td className="px-4 py-2.5 text-gray-300 truncate max-w-[320px]" title={t.title}>
                    {t.title}
                  </td>
                  <td className="px-4 py-2.5 text-right text-gray-400">{t.session_count}</td>
                  <td className="px-4 py-2.5 text-gray-400">{formatWhen(t.last_used_at)}</td>
                  <td className="px-4 py-2.5 text-right text-cyan-400">{formatDuration(t.total_ms)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
