import { useCallback, useEffect, useMemo, useState } from 'react';
import { format, subDays } from 'date-fns';
import { motion } from 'framer-motion';
import {
  Activity,
  AlertTriangle,
  BellRing,
  BrainCircuit,
  CalendarRange,
  GitBranch,
  Link2,
  RefreshCw,
  Scissors,
  Sparkles,
  TrendingUp,
} from 'lucide-react';
import { api } from '../lib/api';
import type {
  AnomaliesResponse,
  AppMetadata,
  AppRank,
  CooccurrenceResponse,
  DisruptionImpactResponse,
  ForecastResponse,
  FragmentationResponse,
  RhythmResponse,
  TransitionsResponse,
} from '../lib/types';
import { formatCount, formatDuration, formatDurationFine, formatSpan } from '../lib/format';
import { Panel } from '../components/ui/Card';
import StatCard from '../components/StatCard';
import SegmentedControl from '../components/ui/SegmentedControl';
import { Reveal, RevealItem, RevealOnScroll } from '../components/ui/Reveal';
import { EmptyState, Skeleton, SkeletonStats } from '../components/ui/Feedback';
import TransitionMatrix from '../components/insights/TransitionMatrix';
import FlowDiagram from '../components/insights/FlowDiagram';
import ForecastChart from '../components/insights/ForecastChart';
import RhythmHeatmap from '../components/insights/RhythmHeatmap';
import FragmentationPanel from '../components/insights/FragmentationPanel';
import CooccurrenceGraph from '../components/insights/CooccurrenceGraph';
import DisruptionImpact from '../components/insights/DisruptionImpact';
import AnomalyList from '../components/insights/AnomalyList';

type RangeKey = '7d' | '30d' | '90d';
type MinMs = 0 | 3000 | 30000;

const RANGE_DAYS: Record<RangeKey, number> = { '7d': 7, '30d': 30, '90d': 90 };

interface InsightsState {
  transitions: TransitionsResponse | null;
  forecastAll: ForecastResponse | null;
  forecastApp: ForecastResponse | null;
  rhythm: RhythmResponse | null;
  fragmentation: FragmentationResponse | null;
  cooccurrence: CooccurrenceResponse | null;
  disruption: DisruptionImpactResponse | null;
  anomalies: AnomaliesResponse | null;
  topApps: AppRank[];
  metadata: Record<string, AppMetadata>;
}

const EMPTY: InsightsState = {
  transitions: null,
  forecastAll: null,
  forecastApp: null,
  rhythm: null,
  fragmentation: null,
  cooccurrence: null,
  disruption: null,
  anomalies: null,
  topApps: [],
  metadata: {},
};

export default function Insights() {
  const [range, setRange] = useState<RangeKey>('30d');
  const [minMs, setMinMs] = useState<MinMs>(3000);
  const [forecastClass, setForecastClass] = useState<string>('');
  const [focus, setFocus] = useState<string | null>(null);
  const [data, setData] = useState<InsightsState>(EMPTY);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const today = format(new Date(), 'yyyy-MM-dd');
  const days = RANGE_DAYS[range];
  const from = format(subDays(new Date(), days - 1), 'yyyy-MM-dd');

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    const safe = <T,>(p: Promise<T>, fallback: T) => p.catch(() => fallback);

    Promise.all([
      safe(api.insights.transitions(from, today, minMs, 12), null),
      safe(api.insights.forecast(undefined, 30, 7), null),
      safe(
        forecastClass
          ? api.insights.forecast(forecastClass, 30, 7)
          : Promise.resolve(null),
        null,
      ),
      safe(api.insights.rhythm(from, today), null),
      safe(api.insights.fragmentation(from, today), null),
      safe(api.insights.cooccurrence(from, today, 30, 8), null),
      safe(api.insights.disruptionImpact(from, today), null),
      safe(api.insights.anomalies(from, today, 14, 2), null),
      safe(api.appRanking(from, today, 20), [] as AppRank[]),
    ]).then(
      ([
        transitions,
        forecastAll,
        forecastApp,
        rhythm,
        fragmentation,
        cooccurrence,
        disruption,
        anomalies,
        topApps,
      ]) => {
        if (cancelled) return;
        setData({
          transitions,
          forecastAll,
          forecastApp,
          rhythm,
          fragmentation,
          cooccurrence,
          disruption,
          anomalies,
          topApps,
          metadata: {},
        });
        setLoading(false);
        // Every endpoint failed → the insights API is most likely not deployed.
        if (
          !transitions &&
          !forecastAll &&
          !rhythm &&
          !fragmentation &&
          !cooccurrence &&
          !disruption &&
          !anomalies
        ) {
          setError(
            'The insights API returned no data. The backend may not have the /api/insights endpoints yet.',
          );
        }
      },
    );

    return () => {
      cancelled = true;
    };
  }, [from, today, minMs, forecastClass, reloadKey]);

  // Friendly app names/icons for the transition and co-occurrence views.
  useEffect(() => {
    const classes = new Set<string>();
    data.transitions?.nodes.forEach((n) => classes.add(n.class));
    data.cooccurrence?.nodes.forEach((n) => classes.add(n.class));
    data.topApps.forEach((a) => classes.add(a.class));
    if (forecastClass) classes.add(forecastClass);
    const list = Array.from(classes).filter(Boolean);
    if (list.length === 0) return;
    let cancelled = false;
    api.appsMetadata(list).then((res) => {
      if (!cancelled) setData((prev) => ({ ...prev, metadata: res.entries }));
    }).catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [data.transitions, data.cooccurrence, data.topApps, forecastClass]);

  const kpis = useMemo(() => {
    const totalMs = data.topApps.reduce((a, app) => a + app.total_ms, 0);
    const frag = data.fragmentation;
    const forecast = data.forecastAll;
    const predictedTomorrow = forecast?.tomorrow_projected_ms ?? 0;
    const todayProjected = forecast?.today_projected_ms ?? 0;
    return [
      {
        label: 'Tracked',
        value: totalMs,
        format: formatDuration,
        icon: <CalendarRange size={15} />,
        subtext: `${days} days · ${data.topApps.length} apps`,
        accent: 'accent' as const,
      },
      {
        label: 'Switches/h',
        value: frag ? Number(frag.switches_per_hour.toFixed(1)) : 0,
        format: (n: number) => n.toFixed(1),
        icon: <GitBranch size={15} />,
        subtext: frag ? `${Math.round(frag.avg_switch_count)} per day` : '—',
        accent: (frag && frag.switches_per_hour > 20 ? 'warn' : 'accent-2') as 'warn' | 'accent-2',
      },
      {
        label: 'Avg session',
        value: frag?.avg_dwell_ms ?? 0,
        format: formatSpan,
        icon: <Scissors size={15} />,
        subtext: frag ? `median ${formatSpan(frag.median_dwell_ms)}` : '—',
        accent: 'good' as const,
      },
      {
        label: 'Tomorrow',
        value: predictedTomorrow,
        format: formatDuration,
        icon: <TrendingUp size={15} />,
        subtext: forecast ? `today ${formatDuration(todayProjected)} projected` : '—',
        accent: 'accent' as const,
      },
      {
        label: 'Interrupts',
        value: data.disruption?.days.reduce((a, d) => a + d.disruptions, 0) ?? 0,
        format: formatCount,
        icon: <BellRing size={15} />,
        subtext: data.disruption
          ? `r = ${data.disruption.correlations.efficiency_vs_disruptions.toFixed(2)} vs efficiency`
          : '—',
        accent: 'warn' as const,
      },
      {
        label: 'Anomalies',
        value: data.anomalies?.days.length ?? 0,
        format: formatCount,
        icon: <AlertTriangle size={15} />,
        subtext: data.anomalies ? `of ${days} days` : '—',
        accent: 'bad' as const,
      },
    ];
  }, [data, days]);

  const refresh = useCallback(() => setReloadKey((k) => k + 1), []);

  const focusNode = focus
    ? data.transitions?.nodes.find((n) => n.class === focus) ?? null
    : null;

  return (
    <div className="space-y-6">
      {/* Header ------------------------------------------------------------ */}
      <Reveal className="flex flex-wrap items-end justify-between gap-4">
        <RevealItem>
          <h2 className="flex items-center gap-2 text-xl font-bold text-fg">
            <Sparkles size={19} className="text-accent" />
            Insights
          </h2>
          <p className="mt-1 text-sm text-fg-muted">
            What you do after opening an app, where your time is heading, and which days broke the
            pattern.
          </p>
        </RevealItem>
        <RevealItem className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-fg-faint">
            {from} → {today}
          </span>
          <SegmentedControl
            layoutId="insights-range"
            value={range}
            onChange={setRange}
            options={[
              { value: '7d', label: '7 days' },
              { value: '30d', label: '30 days' },
              { value: '90d', label: '90 days' },
            ]}
          />
          <motion.button
            type="button"
            onClick={refresh}
            className="btn btn-ghost px-2"
            whileTap={{ rotate: 180, scale: 0.9 }}
            transition={{ duration: 0.35 }}
            title="Reload insights"
          >
            <RefreshCw size={15} />
          </motion.button>
        </RevealItem>
      </Reveal>

      {error && (
        <div className="flex items-start gap-2.5 rounded-xl border border-warn/30 bg-warn/5 px-4 py-3 text-sm text-fg">
          <AlertTriangle size={16} className="mt-0.5 shrink-0 text-warn" />
          <span>{error}</span>
        </div>
      )}

      {/* KPI row ----------------------------------------------------------- */}
      {loading ? (
        <SkeletonStats count={6} />
      ) : (
        <Reveal className="grid grid-cols-2 gap-4 md:grid-cols-3 2xl:grid-cols-6" stagger={0.04}>
          {kpis.map((k) => (
            <RevealItem key={k.label}>
              <StatCard
                icon={k.icon}
                label={k.label}
                value={k.value}
                format={k.format}
                subtext={k.subtext}
                accent={k.accent}
              />
            </RevealItem>
          ))}
        </Reveal>
      )}

      {/* Forecast ---------------------------------------------------------- */}
      <RevealOnScroll>
        <Panel
          title="Usage forecast"
          icon={<TrendingUp size={15} className="text-accent" />}
          hint="linear regression + weekday factors, 95% band"
          actions={
            <select
              className="input"
              value={forecastClass}
              onChange={(e) => setForecastClass(e.target.value)}
              aria-label="Forecast scope"
            >
              <option value="">All apps (total)</option>
              {data.topApps.slice(0, 15).map((a) => (
                <option key={a.class} value={a.class}>
                  {data.metadata[a.class]?.display_name || a.class}
                </option>
              ))}
            </select>
          }
        >
          {loading ? (
            <Skeleton className="h-[260px]" />
          ) : (
            (() => {
              const f = forecastClass ? data.forecastApp : data.forecastAll;
              if (!f) {
                return (
                  <EmptyState
                    icon={<TrendingUp size={20} />}
                    title="Not enough history to fit a trend"
                    hint="Once a few days of sessions exist, the model and its confidence band appear here."
                  />
                );
              }
              return <ForecastChart data={f} metadata={data.metadata} />;
            })()
          )}
        </Panel>
      </RevealOnScroll>

      {/* Transitions ------------------------------------------------------- */}
      <RevealOnScroll>
        <Panel
          title="App transitions"
          icon={<GitBranch size={15} className="text-accent-2" />}
          hint={
            data.transitions
              ? `${data.transitions.total_transitions} switches between ${data.transitions.total_sessions} sessions`
              : undefined
          }
          actions={
            <SegmentedControl
              layoutId="insights-minms"
              size="sm"
              value={String(minMs) as '0' | '3000' | '30000'}
              onChange={(v) => setMinMs(Number(v) as MinMs)}
              options={[
                { value: '0', label: 'all sessions' },
                { value: '3000', label: '>3s' },
                { value: '30000', label: '>30s' },
              ]}
            />
          }
        >
          {loading ? (
            <Skeleton className="h-[420px]" />
          ) : data.transitions && data.transitions.edges.length > 0 ? (
            <div className="grid gap-6 xl:grid-cols-2">
              <div>
                <div className="mb-3 flex items-center gap-2">
                  <span className="panel-sub">Probability matrix</span>
                  {focus && (
                    <button
                      type="button"
                      onClick={() => setFocus(null)}
                      className="chip-accent"
                      title="Clear the focus filter"
                    >
                      focused: {focus} ✕
                    </button>
                  )}
                </div>
                <TransitionMatrix
                  matrix={data.transitions.matrix}
                  nodes={data.transitions.nodes}
                  metadata={data.metadata}
                  focus={focus}
                  onFocus={setFocus}
                />
                {focusNode && (
                  <div className="mt-3 grid grid-cols-3 gap-2 text-xs">
                    {[
                      { l: 'Time', v: formatDuration(focusNode.total_ms) },
                      { l: 'Sessions', v: String(focusNode.session_count) },
                      {
                        l: 'Stays put',
                        v: `${Math.round(focusNode.self_probability * 100)}%`,
                      },
                    ].map((s) => (
                      <div
                        key={s.l}
                        className="rounded-lg border border-line bg-surface-2/60 px-2.5 py-1.5"
                      >
                        <div className="text-[10px] uppercase text-fg-faint">{s.l}</div>
                        <div className="tnum text-fg">{s.v}</div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
              <div>
                <div className="mb-3 panel-sub">Most-travelled paths</div>
                <FlowDiagram
                  nodes={data.transitions.nodes}
                  edges={data.transitions.edges}
                  metadata={data.metadata}
                  focus={focus}
                  onFocus={setFocus}
                />
              </div>
            </div>
          ) : (
            <EmptyState
              icon={<GitBranch size={20} />}
              title="No transitions recorded in this window"
              hint="Lower the session-length filter or widen the date range."
            />
          )}
        </Panel>
      </RevealOnScroll>

      {/* Rhythm ------------------------------------------------------------ */}
      <RevealOnScroll>
        <Panel
          title="Weekly rhythm"
          icon={<Activity size={15} className="text-accent" />}
          hint="average time per weekday × hour"
        >
          {loading ? (
            <Skeleton className="h-[260px]" />
          ) : data.rhythm && data.rhythm.cells.length > 0 ? (
            <RhythmHeatmap data={data.rhythm} />
          ) : (
            <EmptyState
              icon={<Activity size={20} />}
              title="No hourly data in this window"
              hint="Hourly buckets are built from recorded sessions; pick a wider range."
            />
          )}
        </Panel>
      </RevealOnScroll>

      {/* Fragmentation ----------------------------------------------------- */}
      <RevealOnScroll>
        <Panel
          title="Focus & context switching"
          icon={<Scissors size={15} className="text-warn" />}
          hint="how often focus breaks and what it costs"
        >
          {loading ? (
            <Skeleton className="h-[320px]" />
          ) : data.fragmentation && data.fragmentation.days.length > 0 ? (
            <FragmentationPanel data={data.fragmentation} />
          ) : (
            <EmptyState
              icon={<Scissors size={20} />}
              title="No sessions to analyse"
              hint="Fragmentation needs recorded sessions in the selected range."
            />
          )}
        </Panel>
      </RevealOnScroll>

      {/* Co-occurrence ----------------------------------------------------- */}
      <RevealOnScroll>
        <Panel
          title="Apps used together"
          icon={<Link2 size={15} className="text-accent-2" />}
          hint="co-occurrence within a 30-minute window"
        >
          {loading ? (
            <Skeleton className="h-[360px]" />
          ) : data.cooccurrence && data.cooccurrence.pairs.length > 0 ? (
            <CooccurrenceGraph data={data.cooccurrence} metadata={data.metadata} />
          ) : (
            <EmptyState
              icon={<Link2 size={20} />}
              title="No repeated app pairings yet"
              hint="Pairs appear once the same two apps are used close together more than once."
            />
          )}
        </Panel>
      </RevealOnScroll>

      {/* Disruption impact ------------------------------------------------- */}
      <RevealOnScroll>
        <Panel
          title="Interruptions vs output"
          icon={<BellRing size={15} className="text-warn" />}
          hint="correlation between notifications and efficiency"
        >
          {loading ? (
            <Skeleton className="h-[300px]" />
          ) : data.disruption && data.disruption.days.length > 0 ? (
            <DisruptionImpact data={data.disruption} />
          ) : (
            <EmptyState
              icon={<BellRing size={20} />}
              title="No interruption data yet"
              hint="Enable notification and clipboard capture in the daemon config to see this."
            />
          )}
        </Panel>
      </RevealOnScroll>

      {/* Anomalies --------------------------------------------------------- */}
      <RevealOnScroll>
        <Panel
          title="Days that broke the pattern"
          icon={<BrainCircuit size={15} className="text-bad" />}
          hint={
            data.anomalies
              ? `${data.anomalies.window}-day baseline · ${data.anomalies.threshold.toFixed(1)}σ threshold`
              : undefined
          }
        >
          {loading ? (
            <Skeleton className="h-[260px]" />
          ) : data.anomalies ? (
            <AnomalyList data={data.anomalies} />
          ) : (
            <EmptyState
              icon={<AlertTriangle size={20} />}
              title="Not enough history for a baseline"
              hint="Anomaly detection needs at least two weeks of daily data."
            />
          )}
        </Panel>
      </RevealOnScroll>

      <p className="pb-2 text-center text-[11px] text-fg-faint">
        Forecasts are least-squares fits over your own history — a description of the past, not a
        promise about tomorrow. Longest session in range:{' '}
        {formatDurationFine(
          Math.max(
            0,
            ...(data.fragmentation?.days.map((d) => d.longest_focus_ms) ?? [0]),
          ),
        )}
        .
      </p>
    </div>
  );
}
