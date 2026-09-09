import { useEffect, useMemo, useState } from 'react';
import { format, subDays, addDays } from 'date-fns';
import { motion } from 'framer-motion';
import {
  Clock,
  AppWindow,
  Hash,
  Moon,
  BrainCircuit,
  BellRing,
  Copy,
  Gauge,
  Target,
  TrendingUp,
  ChevronLeft,
  ChevronRight,
  ArrowUp,
  ArrowDown,
  FolderKanban,
  CalendarDays,
  Sparkles,
  Minus,
} from 'lucide-react';
import { Link } from 'react-router-dom';
import { api } from '../lib/api';
import type {
  TodaySummary,
  HourlyBucket,
  DisruptionEvent,
  EfficiencyScore,
  GoalProgress,
  TrendPrediction,
  AppMetadata,
  ProjectStat,
  DailyActivity,
} from '../lib/types';
import { formatDuration, formatDelta } from '../lib/format';
import StatCard from '../components/StatCard';
import AppUsagePie from '../components/AppUsagePie';
import HourlyBars from '../components/HourlyBars';
import ActivityHeatmap from '../components/ActivityHeatmap';
import { Panel } from '../components/ui/Card';
import { Reveal, RevealItem, RevealOnScroll } from '../components/ui/Reveal';
import { ProgressBar, SkeletonStats, SkeletonPanel, EmptyState } from '../components/ui/Feedback';

interface CompareState {
  summary: TodaySummary | null;
  efficiency: EfficiencyScore | null;
}

const EMPTY_COMPARE: CompareState = { summary: null, efficiency: null };

export default function Dashboard() {
  const today = format(new Date(), 'yyyy-MM-dd');
  const [selectedDate, setSelectedDate] = useState(today);
  const [summary, setSummary] = useState<TodaySummary | null>(null);
  const [timeline, setTimeline] = useState<HourlyBucket[]>([]);
  const [disruptions, setDisruptions] = useState<DisruptionEvent[]>([]);
  const [efficiency, setEfficiency] = useState<EfficiencyScore | null>(null);
  const [goalProgress, setGoalProgress] = useState<GoalProgress[]>([]);
  const [prediction, setPrediction] = useState<TrendPrediction | null>(null);
  const [compare, setCompare] = useState<CompareState>(EMPTY_COMPARE);
  const [appMetadata, setAppMetadata] = useState<Record<string, AppMetadata>>({});
  const [projectStats, setProjectStats] = useState<ProjectStat[]>([]);
  const [activity, setActivity] = useState<DailyActivity[]>([]);
  const [loading, setLoading] = useState(true);

  const prevDate = subDays(new Date(selectedDate + 'T00:00:00'), 1);
  const prevDateString = format(prevDate, 'yyyy-MM-dd');
  const isToday = selectedDate === today;
  const isNextDisabled = selectedDate >= today;

  // Resolve friendly names/icons for the displayed app classes once.
  useEffect(() => {
    const classes = summary?.top_apps.map((a) => a.class) ?? [];
    if (classes.length === 0) {
      setAppMetadata({});
      return;
    }
    api.appsMetadata(classes).then((res) => setAppMetadata(res.entries)).catch(() => setAppMetadata({}));
  }, [summary]);

  // Yearly activity heatmap is independent of the selected date; fetch once.
  useEffect(() => {
    api.activityDaily(371).then(setActivity).catch(() => setActivity([]));
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setCompare(EMPTY_COMPARE);

    Promise.all([
      api.summary(selectedDate).catch(() => null),
      api.timeline(selectedDate).catch(() => []),
      api.disruptions(selectedDate, selectedDate, 30).catch(() => []),
      api.efficiency(selectedDate).catch(() => null),
      selectedDate === today
        ? api.goals().catch(() => ({ goals: [], progress: [] as GoalProgress[] }))
        : Promise.resolve({ goals: [], progress: [] as GoalProgress[] }),
      api.predict(14).catch(() => null),
      api.projectStats(selectedDate, selectedDate).catch(() => []),
    ]).then(([s, t, d, e, g, p, ps]) => {
      if (cancelled) return;
      setSummary(s);
      setTimeline(t);
      setDisruptions(d);
      setEfficiency(e);
      setGoalProgress(g.progress ?? []);
      setPrediction(p);
      setProjectStats(ps);
      setLoading(false);
    });

    // Previous-day comparison (errors ignored -> "—").
    Promise.all([
      api.summary(prevDateString).catch(() => null),
      api.efficiency(prevDateString).catch(() => null),
    ]).then(([s, e]) => {
      if (cancelled) return;
      setCompare({ summary: s, efficiency: e });
    });

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedDate, today, prevDateString]);

  const prevSummary = compare.summary;
  const prevEfficiency = compare.efficiency;

  const activeDeltaMs = (summary?.total_active_ms ?? 0) - (prevSummary?.total_active_ms ?? 0);
  const focusDeltaMs = (summary?.total_focused_ms ?? 0) - (prevSummary?.total_focused_ms ?? 0);
  const efficiencyDelta = (efficiency?.score ?? 0) - (prevEfficiency?.score ?? 0);

  const spark = useMemo(
    () => activity.slice(-14).map((d) => d.total_ms),
    [activity],
  );

  const focusRatio = summary
    ? summary.total_focused_ms / Math.max(summary.total_active_ms, 1)
    : 0;

  const cards = [
    {
      icon: <Clock size={15} />,
      label: 'Active time',
      value: summary?.total_active_ms ?? 0,
      format: formatDuration,
      delta: { value: activeDeltaMs, format: formatDelta },
      spark,
      accent: 'accent' as const,
    },
    {
      icon: <BrainCircuit size={15} />,
      label: 'Focus time',
      value: summary?.total_focused_ms ?? 0,
      format: formatDuration,
      delta: { value: focusDeltaMs, format: formatDelta },
      subtext: `${Math.round(focusRatio * 100)}% of active time`,
      accent: 'good' as const,
    },
    {
      icon: <AppWindow size={15} />,
      label: 'Apps',
      value: summary?.app_count ?? 0,
      format: (n: number) => String(Math.round(n)),
      subtext: 'used today',
      accent: 'accent-2' as const,
    },
    {
      icon: <Hash size={15} />,
      label: 'Sessions',
      value: summary?.session_count ?? 0,
      format: (n: number) => String(Math.round(n)),
      subtext: summary
        ? `${Math.round((summary.total_active_ms ?? 0) / Math.max(summary.session_count, 1) / 60000)}m average`
        : undefined,
      accent: 'accent-2' as const,
    },
    {
      icon: <Moon size={15} />,
      label: 'Idle time',
      value: summary?.total_idle_ms ?? 0,
      format: formatDuration,
      subtext: 'away from keyboard',
      accent: 'warn' as const,
    },
    {
      icon: <Gauge size={15} />,
      label: 'Efficiency',
      value: efficiency?.score ?? 0,
      format: (n: number) => `${Math.round(n)}`,
      delta: { value: efficiencyDelta, format: (n: number) => `${Math.abs(Math.round(n))} pts` },
      subtext: efficiency
        ? `${Math.round(efficiency.focus_ratio * 100)}% focus · ${Math.round(efficiency.avg_session_secs / 60)}m/session`
        : 'no score yet',
      accent: 'accent' as const,
    },
  ];

  return (
    <div className="space-y-6">
      {/* Header ------------------------------------------------------------ */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="flex items-center gap-2 text-xl font-bold text-fg">
            <CalendarDays size={19} className="text-accent" />
            {isToday ? 'Today' : format(new Date(selectedDate + 'T00:00:00'), 'EEEE, MMM d')}
          </h2>
          <p className="mt-1 text-sm text-fg-muted">
            {isToday
              ? 'Live view of the current day.'
              : `Historical view · compared with ${prevDateString}`}
          </p>
        </div>

        <div className="flex items-center gap-2">
          <motion.button
            type="button"
            whileTap={{ scale: 0.94 }}
            onClick={() =>
              setSelectedDate(
                format(subDays(new Date(selectedDate + 'T00:00:00'), 1), 'yyyy-MM-dd'),
              )
            }
            className="btn"
            title="Previous day"
          >
            <ChevronLeft size={15} />
          </motion.button>
          <input
            type="date"
            value={selectedDate}
            max={today}
            onChange={(e) => setSelectedDate(e.target.value)}
            className="input"
          />
          <motion.button
            type="button"
            whileTap={{ scale: 0.94 }}
            onClick={() =>
              setSelectedDate(
                format(addDays(new Date(selectedDate + 'T00:00:00'), 1), 'yyyy-MM-dd'),
              )
            }
            disabled={isNextDisabled}
            className="btn disabled:cursor-not-allowed disabled:opacity-40"
            title="Next day"
          >
            <ChevronRight size={15} />
          </motion.button>
          {!isToday && (
            <button type="button" onClick={() => setSelectedDate(today)} className="btn btn-accent">
              Today
            </button>
          )}
        </div>
      </div>

      {/* KPI grid ---------------------------------------------------------- */}
      {loading ? (
        <SkeletonStats count={6} />
      ) : (
        <Reveal className="grid grid-cols-2 gap-4 md:grid-cols-3 2xl:grid-cols-6" stagger={0.045}>
          {cards.map((card) => (
            <RevealItem key={card.label}>
              <StatCard
                icon={card.icon}
                label={card.label}
                value={card.value}
                format={card.format}
                delta={card.delta}
                spark={card.spark}
                subtext={card.subtext}
                accent={card.accent}
              />
            </RevealItem>
          ))}
        </Reveal>
      )}

      {/* Day-over-day comparison ------------------------------------------- */}
      {!loading && (
        <Reveal>
          <RevealItem className="card flex flex-wrap items-center gap-3 p-4">
            <span className="flex items-center gap-2 text-sm font-medium text-fg-muted">
              <TrendingUp size={15} className="text-good" />
              vs {prevDateString}
            </span>
            <span className="chip-neutral">
              <Clock size={11} />
              Active {formatDelta(activeDeltaMs)}
            </span>
            <span className={focusDeltaMs >= 0 ? 'chip-good' : 'chip-bad'}>
              {focusDeltaMs >= 0 ? <ArrowUp size={11} /> : <ArrowDown size={11} />}
              Focus {formatDelta(focusDeltaMs)}
            </span>
            <span className={efficiencyDelta >= 0 ? 'chip-good' : 'chip-bad'}>
              {efficiencyDelta === 0 ? (
                <Minus size={11} />
              ) : efficiencyDelta > 0 ? (
                <ArrowUp size={11} />
              ) : (
                <ArrowDown size={11} />
              )}
              Efficiency {Math.abs(Math.round(efficiencyDelta))} pts
            </span>
            <Link to="/insights" className="btn btn-ghost ml-auto">
              <Sparkles size={13} />
              Deeper analysis
            </Link>
          </RevealItem>
        </Reveal>
      )}

      <RevealOnScroll>
        <ActivityHeatmap data={activity} />
      </RevealOnScroll>

      {/* Goals ------------------------------------------------------------- */}
      {!loading && goalProgress.length > 0 && isToday && (
        <Reveal className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3" stagger={0.06}>
          {goalProgress.map((p) => {
            const done = p.pct >= 100;
            return (
              <RevealItem key={p.goal.id ?? p.goal.name}>
                <div className="card p-4">
                  <div className="mb-2 flex items-center justify-between">
                    <span className="flex items-center gap-2 text-sm font-medium text-fg">
                      <Target size={14} className={done ? 'text-good' : 'text-accent'} />
                      {p.goal.name}
                    </span>
                    <span className={`chip ${done ? 'chip-good' : 'chip-accent'} tnum`}>
                      {Math.round(p.pct)}%
                    </span>
                  </div>
                  <ProgressBar
                    pct={p.pct}
                    color={done ? 'rgb(var(--c-good))' : 'rgb(var(--c-accent))'}
                  />
                  <div className="mt-2 text-xs text-fg-faint tnum">
                    {formatDuration(p.today_ms)} / {formatDuration(p.goal.daily_target_ms || 0)}
                  </div>
                </div>
              </RevealItem>
            );
          })}
        </Reveal>
      )}

      {/* Trend prediction --------------------------------------------------- */}
      {!loading && prediction && isToday && (
        <RevealOnScroll>
          <Panel
            title="Trend prediction"
            icon={<TrendingUp size={15} className="text-good" />}
            hint={`based on the last ${prediction.window_days} days`}
            actions={
              <Link to="/insights" className="btn btn-ghost">
                Full forecast
                <ChevronRight size={13} />
              </Link>
            }
          >
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              {[
                { label: 'Today so far', value: prediction.today_ms, cls: 'text-fg' },
                {
                  label: 'Today projected',
                  value: prediction.predicted_today_ms,
                  cls: 'text-accent',
                },
                {
                  label: 'Tomorrow projected',
                  value: prediction.predicted_tomorrow_ms,
                  cls: 'text-accent-2',
                },
                { label: `Daily average`, value: prediction.daily_avg_ms, cls: 'text-fg-muted' },
              ].map((s, i) => (
                <motion.div
                  key={s.label}
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ delay: i * 0.06 }}
                  className="rounded-lg border border-line bg-surface-2/60 px-3 py-2"
                >
                  <div className="text-[10px] uppercase tracking-wide text-fg-faint">{s.label}</div>
                  <div className={`text-sm font-semibold tnum ${s.cls}`}>
                    {formatDuration(s.value)}
                  </div>
                </motion.div>
              ))}
            </div>
            <p className="mt-3 text-[11px] text-fg-faint">
              Linear trend {prediction.slope >= 0 ? 'up' : 'down'}{' '}
              {formatDuration(Math.abs(prediction.slope))} per day across the window.
            </p>
          </Panel>
        </RevealOnScroll>
      )}

      {/* Charts ------------------------------------------------------------ */}
      <RevealOnScroll>
        <div className="grid gap-4 lg:grid-cols-2">
          {loading ? (
            <>
              <SkeletonPanel height="h-[280px]" />
              <SkeletonPanel height="h-[280px]" />
            </>
          ) : (
            <>
              <AppUsagePie data={summary?.top_apps ?? []} metadata={appMetadata} />
              <HourlyBars data={timeline} />
            </>
          )}
        </div>
      </RevealOnScroll>

      {/* Projects ---------------------------------------------------------- */}
      {!loading && (
        <RevealOnScroll>
          <Panel
            title="Projects"
            icon={<FolderKanban size={15} className="text-accent" />}
            hint="time grouped by project rules"
          >
            {projectStats.length === 0 ? (
              <EmptyState
                icon={<FolderKanban size={20} />}
                title="No projects configured"
                hint="Add project rules in Settings to track time by project."
              />
            ) : (
              <div className="space-y-2.5">
                {projectStats.slice(0, 6).map((p, i) => (
                  <motion.div
                    key={p.project_id ?? 'uncategorized'}
                    initial={{ opacity: 0, x: -8 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ delay: i * 0.05 }}
                    className="flex items-center gap-3"
                  >
                    <span
                      className="h-2.5 w-2.5 shrink-0 rounded-full"
                      style={{ backgroundColor: p.color || 'rgb(var(--c-fg-faint))' }}
                    />
                    <span className="w-28 truncate text-sm text-fg sm:w-36">{p.name}</span>
                    <div className="h-2 flex-1 overflow-hidden rounded-full bg-surface-3">
                      <motion.div
                        className="h-full rounded-full"
                        style={{ backgroundColor: p.color || 'rgb(var(--c-fg-faint))' }}
                        initial={{ width: 0 }}
                        animate={{ width: `${Math.min(p.percentage, 100)}%` }}
                        transition={{ type: 'spring', stiffness: 110, damping: 20, delay: i * 0.05 }}
                      />
                    </div>
                    <span className="w-16 text-right text-sm tnum text-fg-muted">
                      {formatDuration(p.total_ms)}
                    </span>
                    <span className="w-10 text-right text-xs tnum text-fg-faint">
                      {Math.round(p.percentage)}%
                    </span>
                  </motion.div>
                ))}
              </div>
            )}
          </Panel>
        </RevealOnScroll>
      )}

      {/* Interruptions ------------------------------------------------------ */}
      {!loading && disruptions.length > 0 && (
        <RevealOnScroll>
          <Panel
            title={isToday ? "Today's interruptions" : `Interruptions · ${selectedDate}`}
            icon={<BellRing size={15} className="text-warn" />}
            hint={`${disruptions.filter((d) => d.kind === 'notification').length} notifications · ${
              disruptions.filter((d) => d.kind === 'clipboard').length
            } copies`}
          >
            <div className="max-h-64 space-y-1 overflow-auto pr-1">
              {disruptions.slice(0, 14).map((d, i) => (
                <motion.div
                  key={d.id}
                  initial={{ opacity: 0, x: -6 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ delay: Math.min(0.3, i * 0.03) }}
                  className="flex items-center gap-2 rounded-md px-1 py-1 text-sm hover:bg-surface-2"
                >
                  {d.kind === 'notification' ? (
                    <BellRing size={12} className="shrink-0 text-warn" />
                  ) : (
                    <Copy size={12} className="shrink-0 text-accent" />
                  )}
                  <span className="truncate text-fg-muted">
                    {d.kind === 'notification'
                      ? `${d.app ?? 'unknown'}: ${d.summary ?? ''}`
                      : 'Clipboard copy'}
                  </span>
                  <span className="ml-auto shrink-0 text-xs tnum text-fg-faint">
                    {new Date(d.occurred_at).toLocaleTimeString([], {
                      hour: '2-digit',
                      minute: '2-digit',
                    })}
                  </span>
                </motion.div>
              ))}
            </div>
          </Panel>
        </RevealOnScroll>
      )}
    </div>
  );
}
