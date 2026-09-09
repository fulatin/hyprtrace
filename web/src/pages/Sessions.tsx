import { useEffect, useState } from 'react';
import { format, subDays } from 'date-fns';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { ChevronLeft, ChevronRight, Filter, List, ListX, X } from 'lucide-react';
import { api } from '../lib/api';
import type { Session, PaginatedResponse, AppMetadata } from '../lib/types';
import AppName from '../components/AppName';
import ErrorState from '../components/ErrorState';
import { EmptyState, ProgressBar, Skeleton } from '../components/ui/Feedback';
import { formatDurationFine } from '../lib/format';
import { listItem, spring } from '../lib/motion';

function formatTime(iso: string): string {
  try {
    return format(new Date(iso), 'HH:mm:ss');
  } catch {
    return iso;
  }
}

// Per-app series palette (matches AppName so dots stay consistent app-wide).
const COLORS = [
  '#22d3ee', '#8b5cf6', '#10b981', '#f59e0b', '#ef4444',
  '#3b82f6', '#ec4899', '#14b8a6', '#f97316', '#6366f1',
];

// Activity state → semantic chip token.
const STATE_CHIP: Record<string, string> = {
  active: 'chip-accent',
  focused: 'chip-good',
  idle: 'chip-neutral',
  away: 'chip-bad',
};

const PER_PAGE = 50;

/** Loading placeholder shaped like the real table, so nothing jumps on load. */
function SkeletonTable() {
  return (
    <div className="card p-4">
      <Skeleton className="mb-4 h-4 w-32" />
      <div className="space-y-2">
        {Array.from({ length: 8 }).map((_, i) => (
          <div key={i} className="flex items-center gap-4">
            <Skeleton className="h-4 w-32" />
            <Skeleton className="h-4 flex-1" />
            <Skeleton className="h-4 w-20" />
            <Skeleton className="h-4 w-16" />
          </div>
        ))}
      </div>
    </div>
  );
}

export default function Sessions() {
  const today = format(new Date(), 'yyyy-MM-dd');
  const weekAgo = format(subDays(new Date(), 7), 'yyyy-MM-dd');
  const [page, setPage] = useState(1);
  const [data, setData] = useState<PaginatedResponse<Session> | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  const [classes, setClasses] = useState<string[]>([]);
  const [appMetadata, setAppMetadata] = useState<Record<string, AppMetadata>>({});
  const [reloadKey, setReloadKey] = useState(0);
  const reduced = useReducedMotion();

  useEffect(() => {
    api.appClasses(weekAgo, today).then(setClasses).catch(() => {});
  }, [reloadKey]);

  // Resolve friendly names/icons for the app classes present in this page's
  // sessions once.
  useEffect(() => {
    const classSet = new Set<string>();
    data?.data.forEach((s) => classSet.add(s.class));
    classes.forEach((c) => classSet.add(c));
    const unique = Array.from(classSet);
    if (unique.length === 0) {
      setAppMetadata({});
      return;
    }
    api.appsMetadata(unique).then((res) => setAppMetadata(res.entries)).catch(() => setAppMetadata({}));
  }, [data, classes]);

  useEffect(() => {
    setPage(1);
  }, [filter]);

  useEffect(() => {
    setLoading(true);
    setError(null);
    api.sessions(weekAgo, today, page, PER_PAGE, filter || undefined).then((d) => {
      setData(d);
      setLoading(false);
    }).catch((e) => {
      setError(e instanceof Error ? e.message : 'Unknown error');
      setLoading(false);
    });
  }, [page, filter, reloadKey]);

  const getColor = (cls: string) => {
    const idx = classes.indexOf(cls);
    return COLORS[idx % COLORS.length];
  };

  const rows = data?.data ?? [];
  const totalPages = Math.max(1, Math.ceil((data?.total ?? 0) / PER_PAGE));
  // Longest session on this page: the reference for every duration bar.
  const maxDuration = rows.reduce((m, s) => Math.max(m, s.duration_ms ?? 0), 0);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="flex items-center gap-2 text-xl font-bold">
            <List size={20} className="text-accent" />
            Sessions
          </h2>
          <p className="mt-1 text-xs text-fg-muted">
            {data
              ? `${data.total} session${data.total === 1 ? '' : 's'} · last 7 days`
              : 'Last 7 days'}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <AnimatePresence initial={false}>
            {filter && (
              <motion.button
                type="button"
                onClick={() => setFilter('')}
                className="chip-accent"
                title="Clear filter"
                initial={reduced ? false : { opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.9 }}
                whileTap={{ scale: 0.95 }}
              >
                <Filter size={11} />
                {filter}
                <X size={11} />
              </motion.button>
            )}
          </AnimatePresence>

          <label className="flex items-center gap-2 text-xs text-fg-muted">
            App
            <select
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              className="input"
            >
              <option value="">All Apps</option>
              {classes.map((c) => (
                <option key={c} value={c}>{c}</option>
              ))}
            </select>
          </label>
        </div>
      </div>

      {loading ? (
        <SkeletonTable />
      ) : error ? (
        <ErrorState message={error} onRetry={() => setReloadKey((k) => k + 1)} />
      ) : (
        <div className="card">
          {rows.length === 0 ? (
            <EmptyState
              className="m-4"
              icon={<ListX size={22} />}
              title="No sessions in this window"
              hint={filter
                ? `Nothing was recorded for ${filter} in the last 7 days.`
                : 'The last 7 days have no recorded sessions.'}
            />
          ) : (
            /* Horizontal scroll container: the table needs ~880px for all
               seven columns, and the sticky header still works inside it. */
            <div className="overflow-x-auto">
            <table className="w-full min-w-[760px] table-fixed text-sm">
              {/* Fixed widths keep the seven columns stable while the title
                  column absorbs the remaining space and truncates. */}
              <colgroup>
                <col className="w-[26%]" />
                <col />
                <col className="w-[9%]" />
                <col className="w-[9%]" />
                <col className="w-[11%]" />
                <col className="w-[15%]" />
                <col className="hidden w-[9%] 2xl:table-column" />
              </colgroup>
              <thead className="sticky top-0 z-10 bg-surface/95 backdrop-blur shadow-[0_1px_0_0_rgb(var(--c-line))]">
                <tr className="border-b border-line">
                  <th className="px-3 py-3 text-left font-medium text-fg-muted first:rounded-tl-xl">App</th>
                  <th className="px-3 py-3 text-left font-medium text-fg-muted">Title</th>
                  <th className="px-3 py-3 text-left font-medium text-fg-muted">Start</th>
                  <th className="px-3 py-3 text-left font-medium text-fg-muted">End</th>
                  <th className="px-3 py-3 text-left font-medium text-fg-muted">State</th>
                  <th className="px-3 py-3 text-right font-medium text-fg-muted">Duration</th>
                  <th className="hidden px-3 py-3 text-right font-medium text-fg-muted last:rounded-tr-xl 2xl:table-cell">Focus</th>
                </tr>
              </thead>

              {/* Cross-fade the whole page of rows when the page or filter changes. */}
              <AnimatePresence mode="wait" initial={false}>
                <motion.tbody
                  key={`${page}:${filter}`}
                  initial={reduced ? false : { opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.22 }}
                >
                  {rows.map((s, i) => {
                    const color = getColor(s.class);
                    const pct = maxDuration > 0 ? ((s.duration_ms ?? 0) / maxDuration) * 100 : 0;
                    return (
                      <motion.tr
                        key={s.id}
                        variants={listItem}
                        initial={reduced ? false : 'hidden'}
                        animate="show"
                        transition={{ delay: reduced ? 0 : Math.min(i * 0.015, 0.3) }}
                        className="border-b border-line/60 transition-colors hover:bg-surface-2/60"
                      >
                        <td className="px-3 py-2.5">
                          {/* min-w-0 + overflow-hidden let the long class names
                              truncate inside the fixed-width column instead of
                              bleeding into the Title cell. */}
                          <div className="flex min-w-0 items-center gap-2 overflow-hidden">
                            <span
                              className="h-2 w-2 shrink-0 rounded-full"
                              style={{ backgroundColor: color }}
                            />
                            <AppName cls={s.class} metadata={appMetadata[s.class] ?? null} />
                          </div>
                        </td>
                        <td className="max-w-[160px] truncate px-4 py-2.5 text-fg-muted" title={s.title || undefined}>
                          {s.title || '-'}
                        </td>
                        <td className="px-3 py-2.5 tnum text-fg-muted">{formatTime(s.started_at)}</td>
                        <td className="px-3 py-2.5 tnum text-fg-muted">
                          {s.ended_at ? formatTime(s.ended_at) : 'Active'}
                        </td>
                        <td className="px-3 py-2.5">
                          {s.activity_state && (
                            <span className={STATE_CHIP[s.activity_state] ?? 'chip-neutral'}>
                              {s.activity_state}
                            </span>
                          )}
                        </td>
                        <td className="px-3 py-2.5">
                          <div className="flex items-center justify-end gap-2">
                            <span className="tnum text-accent">{formatDurationFine(s.duration_ms)}</span>
                            <div className="w-16 shrink-0">
                              <ProgressBar pct={pct} color={color} height={5} />
                            </div>
                          </div>
                        </td>
                        <td className="hidden px-3 py-2.5 text-right text-xs tnum text-fg-faint 2xl:table-cell">
                          {formatDurationFine(s.focused_ms)}
                        </td>
                      </motion.tr>
                    );
                  })}
                </motion.tbody>
              </AnimatePresence>
            </table>
            </div>
          )}

          {data && (
            <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line px-4 py-3">
              <span className="text-xs text-fg-muted">
                Page{' '}
                <AnimatePresence mode="wait" initial={false}>
                  <motion.span
                    key={page}
                    className="inline-block tnum text-fg"
                    initial={reduced ? false : { opacity: 0, y: 6 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={{ opacity: 0, y: -6 }}
                    transition={{ duration: 0.2 }}
                  >
                    {page}
                  </motion.span>
                </AnimatePresence>{' '}
                of {totalPages}
                <span className="text-fg-faint"> · {data.total} sessions</span>
              </span>
              <div className="flex gap-2">
                <motion.button
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                  disabled={page <= 1}
                  className="btn"
                  whileTap={page <= 1 ? undefined : { scale: 0.94, transition: spring }}
                >
                  <ChevronLeft size={14} />
                  Previous
                </motion.button>
                <motion.button
                  onClick={() => setPage((p) => p + 1)}
                  disabled={page >= totalPages}
                  className="btn"
                  whileTap={page >= totalPages ? undefined : { scale: 0.94, transition: spring }}
                >
                  Next
                  <ChevronRight size={14} />
                </motion.button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
