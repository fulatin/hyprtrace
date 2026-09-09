import { useEffect, useState } from 'react';
import { format, subDays } from 'date-fns';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { Activity, MemoryStick, Trophy } from 'lucide-react';
import { api } from '../lib/api';
import type { AppRank, AppResource, DailyTrend, AppMetadata } from '../lib/types';
import { formatDuration, formatMemKb } from '../lib/format';
import { barTransition, expand, listItem, spring } from '../lib/motion';
import AppRankingBar from '../components/AppRankingBar';
import AppName from '../components/AppName';
import AppTrendChart from '../components/AppTrendChart';
import ErrorState from '../components/ErrorState';
import { Panel } from '../components/ui/Card';
import SegmentedControl from '../components/ui/SegmentedControl';
import { Reveal, RevealItem } from '../components/ui/Reveal';
import { EmptyState, Skeleton, SkeletonPanel } from '../components/ui/Feedback';

type Range = 'today' | 'week' | 'month';

const RANGE_OPTIONS: { value: Range; label: string }[] = [
  { value: 'today', label: 'Today' },
  { value: 'week', label: 'Week' },
  { value: 'month', label: 'Month' },
];

/** Per-row hover lift; local because the shared preset carries its own transition. */
const rowHover = { whileHover: { y: -3 }, whileTap: { scale: 0.995 }, transition: spring };

export default function Apps() {
  const [range, setRange] = useState<Range>('today');
  const [data, setData] = useState<AppRank[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedApp, setSelectedApp] = useState<string | null>(null);
  const [trend, setTrend] = useState<DailyTrend[]>([]);
  const [resources, setResources] = useState<AppResource[]>([]);
  const [appMetadata, setAppMetadata] = useState<Record<string, AppMetadata>>({});
  const [reloadKey, setReloadKey] = useState(0);
  const reduced = useReducedMotion();

  const getDateRange = () => {
    const today = format(new Date(), 'yyyy-MM-dd');
    switch (range) {
      case 'today':
        return { from: today, to: today };
      case 'week':
        return { from: format(subDays(new Date(), 7), 'yyyy-MM-dd'), to: today };
      case 'month':
        return { from: format(subDays(new Date(), 30), 'yyyy-MM-dd'), to: today };
    }
  };

  useEffect(() => {
    setLoading(true);
    setError(null);
    const { from, to } = getDateRange();
    api.appRanking(from, to, 15).then((d) => {
      setData(d);
      setLoading(false);
    }).catch((e) => {
      setError(e instanceof Error ? e.message : 'Unknown error');
      setLoading(false);
    });
    api.resources(from, to, 5).then(setResources).catch(() => setResources([]));
  }, [range, reloadKey]);

  // Resolve friendly names/icons for the displayed app classes once.
  useEffect(() => {
    const classes = data.map((a) => a.class);
    if (classes.length === 0) {
      setAppMetadata({});
      return;
    }
    api.appsMetadata(classes).then((res) => setAppMetadata(res.entries)).catch(() => setAppMetadata({}));
  }, [data]);

  useEffect(() => {
    if (!selectedApp) {
      setTrend([]);
      return;
    }
    const { from, to } = getDateRange();
    const granularity = range === 'today' ? 'hour' : undefined;
    api.appTrend(selectedApp, from, to, granularity).then(setTrend).catch(() => setTrend([]));
  }, [selectedApp, range, reloadKey]);

  const selectedRank = data.find((a) => a.class === selectedApp) ?? null;
  const trendTitle = range === 'today' ? 'Hourly' : range === 'week' ? '7-Day' : '30-Day';
  const maxCpu = resources.reduce((max, r) => Math.max(max, r.avg_cpu_pct), 0) || 1;
  const maxMem = resources.reduce((max, r) => Math.max(max, r.peak_mem_kb), 0) || 1;

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-bold text-fg">App Ranking</h2>
          <p className="panel-sub mt-0.5">
            Where your tracked time went, ranked by active minutes.
          </p>
        </div>
        <SegmentedControl
          layoutId="apps-range"
          options={RANGE_OPTIONS}
          value={range}
          onChange={(next) => { setRange(next); setSelectedApp(null); }}
        />
      </div>

      {loading ? (
        <div className="space-y-4">
          <SkeletonPanel height="h-56" />
          <div className="space-y-2">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-12" />
            ))}
          </div>
        </div>
      ) : error ? (
        <ErrorState message={error} onRetry={() => setReloadKey((k) => k + 1)} />
      ) : (
        <>
          <Reveal>
            <RevealItem>
              <AppRankingBar data={data} />
            </RevealItem>

            {/* Detail panel: expands in place so the ranking stays visible. */}
            <AnimatePresence initial={false} mode="wait">
              {selectedApp && (
                <RevealItem key={selectedApp}>
                  <motion.div
                    variants={expand}
                    initial="hidden"
                    animate="show"
                    exit="exit"
                    className="overflow-hidden"
                  >
                    <div className="pt-4">
                      <div className="mb-2 flex flex-wrap items-baseline gap-x-3 gap-y-1">
                        <h3 className="text-sm font-medium text-fg">
                          {appMetadata[selectedApp]?.display_name || selectedApp}
                        </h3>
                        <span className="chip-accent tnum">
                          {selectedRank ? `${selectedRank.percentage.toFixed(1)}%` : '—'}
                        </span>
                        <span className="panel-sub">{trendTitle} Trend</span>
                      </div>
                      <AppTrendChart data={trend} range={range} />
                    </div>
                  </motion.div>
                </RevealItem>
              )}
            </AnimatePresence>

            {resources.length > 0 && (
              <div className="grid gap-4 md:grid-cols-2">
                <RevealItem>
                  <Panel
                    title="Avg CPU Usage"
                    icon={<Activity size={13} className="text-good" />}
                    hint={`top ${Math.min(4, resources.length)}`}
                  >
                    <div className="space-y-3">
                      {resources.slice(0, 4).map((r) => (
                        <div key={`cpu-${r.class}`} className="space-y-1.5">
                          <div className="flex items-center justify-between gap-3 text-sm">
                            <span className="truncate text-fg-muted">{r.class}</span>
                            <span className="tnum shrink-0 font-mono text-xs text-good">
                              {r.avg_cpu_pct.toFixed(1)}%
                            </span>
                          </div>
                          <div className="h-1.5 w-full overflow-hidden rounded-full bg-surface-3">
                            <motion.div
                              className="h-full rounded-full bg-good"
                              initial={{ width: 0 }}
                              animate={{ width: `${Math.min(100, (r.avg_cpu_pct / maxCpu) * 100)}%` }}
                              transition={reduced ? { duration: 0 } : barTransition}
                            />
                          </div>
                        </div>
                      ))}
                    </div>
                  </Panel>
                </RevealItem>
                <RevealItem>
                  <Panel
                    title="Peak Memory"
                    icon={<MemoryStick size={13} className="text-accent-2" />}
                    hint={`${resources.length} apps`}
                  >
                    <div className="space-y-3">
                      {resources.map((r) => (
                        <div key={`mem-${r.class}`} className="space-y-1.5">
                          <div className="flex items-center justify-between gap-3 text-sm">
                            <span className="truncate text-fg-muted">{r.class}</span>
                            <span className="tnum shrink-0 font-mono text-xs text-accent-2">
                              {formatMemKb(r.peak_mem_kb)}
                            </span>
                          </div>
                          <div className="h-1.5 w-full overflow-hidden rounded-full bg-surface-3">
                            <motion.div
                              className="h-full rounded-full bg-accent-2"
                              initial={{ width: 0 }}
                              animate={{ width: `${Math.min(100, (r.peak_mem_kb / maxMem) * 100)}%` }}
                              transition={reduced ? { duration: 0 } : barTransition}
                            />
                          </div>
                        </div>
                      ))}
                    </div>
                  </Panel>
                </RevealItem>
              </div>
            )}
          </Reveal>

          {data.length === 0 ? (
            <EmptyState
              icon={<Trophy size={22} />}
              title="No apps tracked in this range"
              hint="Try a wider range — today may not have any activity yet."
            />
          ) : (
            <Reveal className="space-y-2" stagger={0.035}>
              {data.map((app, i) => {
                const selected = selectedApp === app.class;
                return (
                  <RevealItem
                    key={app.class}
                    variants={listItem}
                    whileHover={reduced ? undefined : rowHover.whileHover}
                    whileTap={reduced ? undefined : rowHover.whileTap}
                    transition={spring}
                    onClick={() => setSelectedApp(app.class)}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e: React.KeyboardEvent) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        setSelectedApp(app.class);
                      }
                    }}
                    className="relative cursor-pointer overflow-hidden rounded-lg border border-line bg-surface px-3 py-2.5 transition-colors hover:border-line-strong"
                  >
                    {selected && (
                      <motion.span
                        layoutId="apps-row-highlight"
                        className="pointer-events-none absolute inset-0 rounded-lg border border-accent/50 bg-accent/[0.07]"
                        transition={spring}
                      />
                    )}
                    <div className="relative z-10 flex items-center gap-3">
                      <span className="tnum w-6 shrink-0 text-xs text-fg-faint">{i + 1}</span>
                      <AppName cls={app.class} metadata={appMetadata[app.class] ?? null} />
                      <span className="ml-auto flex shrink-0 items-center gap-4">
                        <span className="tnum text-xs text-fg-muted">{app.percentage.toFixed(1)}%</span>
                        <span className="tnum w-20 text-right text-sm font-medium text-accent">
                          {formatDuration(app.total_ms)}
                        </span>
                      </span>
                    </div>
                    {/* Share of total time, animated so range changes ease across. */}
                    <div className="relative z-10 mt-2 h-1 w-full overflow-hidden rounded-full bg-surface-3">
                      <motion.div
                        className="h-full rounded-full bg-accent"
                        initial={{ width: 0 }}
                        animate={{ width: `${Math.min(100, app.percentage)}%` }}
                        transition={reduced ? { duration: 0 } : barTransition}
                      />
                    </div>
                  </RevealItem>
                );
              })}
            </Reveal>
          )}
        </>
      )}
    </div>
  );
}
