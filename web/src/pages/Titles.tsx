import { useEffect, useMemo, useState } from 'react';
import { format, subDays } from 'date-fns';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { FileText, Layers, Search, SearchX, Trophy } from 'lucide-react';
import { api } from '../lib/api';
import type { TitleStat, AppMetadata } from '../lib/types';
import AppName from '../components/AppName';
import StatCard from '../components/StatCard';
import { EmptyState, SkeletonPanel } from '../components/ui/Feedback';
import { Reveal, RevealItem } from '../components/ui/Reveal';
import { formatDuration, formatPercent, formatRelativeDate } from '../lib/format';
import { barTransition, listItem, spring, staggerContainer } from '../lib/motion';

// Per-document series palette (matches AppName so a class keeps one colour).
const DOC_COLORS = [
  '#22d3ee', '#8b5cf6', '#10b981', '#f59e0b', '#ef4444',
  '#3b82f6', '#ec4899', '#14b8a6', '#f97316', '#6366f1',
];

function colorForClass(cls: string): string {
  let h = 0;
  for (let i = 0; i < cls.length; i++) h = (h * 31 + cls.charCodeAt(i)) >>> 0;
  return DOC_COLORS[h % DOC_COLORS.length];
}

const formatCount = (n: number) => String(Math.round(n));

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
  const reduced = useReducedMotion();

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

  // Totals for the summary strip, over the rows actually loaded.
  const summary = useMemo(() => {
    let totalMs = 0;
    let top: TitleStat | null = null;
    const docs = new Set<string>();
    for (const t of titles) {
      totalMs += t.total_ms;
      docs.add(`${t.class}\u0000${t.title}`);
      if (!top || t.total_ms > top.total_ms) top = t;
    }
    return { totalMs, docCount: docs.size, top };
  }, [titles]);

  const topMs = summary.top?.total_ms ?? 0;

  return (
    <div className="space-y-6">
      <Reveal className="flex flex-wrap items-center justify-between gap-3">
        <RevealItem>
          <h2 className="flex items-center gap-2 text-xl font-bold">
            <FileText size={20} className="text-accent" />
            Documents
          </h2>
          <p className="mt-1 text-xs text-fg-muted">
            Time spent on individual window titles (files, tabs, pages).
          </p>
        </RevealItem>

        <RevealItem className="flex flex-wrap items-center gap-2">
          <input
            type="date"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
            className="input"
            aria-label="From date"
          />
          <span className="text-xs text-fg-faint">→</span>
          <input
            type="date"
            value={to}
            onChange={(e) => setTo(e.target.value)}
            className="input"
            aria-label="To date"
          />
          <label className="flex items-center gap-2 text-xs text-fg-muted">
            App
            <select
              value={cls}
              onChange={(e) => setCls(e.target.value)}
              className="input"
            >
              <option value="">All Apps</option>
              {classes.map((c) => (
                <option key={c} value={c}>{c}</option>
              ))}
            </select>
          </label>
        </RevealItem>
      </Reveal>

      <div className="relative">
        <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-fg-faint" />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Filter titles…"
          className="input w-full pl-9"
          aria-label="Filter titles"
        />
      </div>

      {loading ? (
        <SkeletonPanel />
      ) : (
        <>
          {titles.length > 0 && (
            <Reveal className="grid grid-cols-1 gap-4 sm:grid-cols-3">
              <StatCard
                icon={<FileText size={15} />}
                label="Total time"
                value={summary.totalMs}
                format={formatDuration}
                subtext={`${titles.length} titles loaded`}
                accent="accent"
              />
              <StatCard
                icon={<Layers size={15} />}
                label="Documents"
                value={summary.docCount}
                format={formatCount}
                subtext="distinct titles"
                accent="accent-2"
              />
              <StatCard
                icon={<Trophy size={15} />}
                label="Top document"
                value={summary.top ? formatDuration(summary.top.total_ms) : '—'}
                subtext={summary.top ? (
                  <span className="flex items-center gap-1.5">
                    <span
                      className="h-1.5 w-1.5 shrink-0 rounded-full"
                      style={{ backgroundColor: colorForClass(summary.top.class) }}
                    />
                    <span className="truncate" title={summary.top.title}>{summary.top.title}</span>
                  </span>
                ) : undefined}
                accent="warn"
              />
            </Reveal>
          )}

          {filtered.length === 0 ? (
            <EmptyState
              icon={<SearchX size={22} />}
              title={titles.length === 0 ? 'No title data in this range' : 'No titles match your filter'}
              hint={titles.length === 0
                ? 'Titles are recorded while the "Record window titles" privacy setting is enabled.'
                : 'Try a shorter search term, a wider date range, or clear the app filter.'}
            />
          ) : (
            <motion.div
              className="relative space-y-2"
              variants={staggerContainer(0.035)}
              initial={reduced ? false : 'hidden'}
              animate="show"
            >
              <AnimatePresence initial={false} mode="popLayout">
                {filtered.map((t) => {
                  const color = colorForClass(t.class);
                  const share = topMs > 0 ? (t.total_ms / topMs) * 100 : 0;
                  return (
                    <motion.div
                      key={`${t.class}:${t.title}`}
                      layout
                      variants={listItem}
                      exit={{ opacity: 0, x: -12, transition: { duration: 0.2 } }}
                      whileHover={{ y: -2, transition: spring }}
                      className="card-interactive p-3"
                    >
                      <div className="flex items-center gap-3">
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-sm text-fg" title={t.title}>
                            {t.title}
                          </div>
                          <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-fg-faint">
                            <AppName cls={t.class} metadata={appMetadata[t.class] ?? null} />
                            <span>·</span>
                            <span className="tnum">
                              {t.session_count} session{t.session_count === 1 ? '' : 's'}
                            </span>
                            <span>·</span>
                            <span>{formatRelativeDate(t.last_used_at)}</span>
                          </div>
                          <div className="mt-2 h-1 w-full overflow-hidden rounded-full bg-surface-3">
                            <motion.div
                              className="h-full rounded-full"
                              style={{ backgroundColor: color }}
                              initial={{ width: 0 }}
                              animate={{ width: `${share}%` }}
                              transition={barTransition}
                            />
                          </div>
                        </div>
                        <div className="shrink-0 text-right">
                          <div className="text-sm tnum text-accent">{formatDuration(t.total_ms)}</div>
                          <div className="text-[10px] tnum text-fg-faint">
                            {formatPercent(t.total_ms / (summary.totalMs || 1), 1)}
                          </div>
                        </div>
                      </div>
                    </motion.div>
                  );
                })}
              </AnimatePresence>
            </motion.div>
          )}
        </>
      )}
    </div>
  );
}
