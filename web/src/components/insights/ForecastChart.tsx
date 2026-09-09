import { motion } from 'framer-motion';
import { useMemo } from 'react';
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { TrendingDown, TrendingUp, Minus } from 'lucide-react';
import type { AppMetadata, ForecastResponse } from '../../lib/types';
import { formatDuration, formatShortDate } from '../../lib/format';
import { WEEKDAYS, displayName, pct } from './shared';

interface ForecastChartProps {
  data: ForecastResponse;
  metadata?: Record<string, AppMetadata>;
}

interface Row {
  date: string;
  actual: number | null;
  predicted: number | null;
  band: [number, number] | null;
  isFuture: boolean;
}

const TREND_META = {
  up: { icon: TrendingUp, cls: 'chip-good', label: 'trending up' },
  down: { icon: TrendingDown, cls: 'chip-bad', label: 'trending down' },
  flat: { icon: Minus, cls: 'chip-neutral', label: 'holding steady' },
} as const;

/**
 * Linear-regression forecast with a 95% band. The solid line is measured usage,
 * the dashed line is the fitted model (slope + weekday factors), and the shaded
 * band widens into the future where the model has no data.
 */
export default function ForecastChart({ data, metadata }: ForecastChartProps) {
  const rows = useMemo<Row[]>(() => {
    const history: Row[] = data.points.map((p) => ({
      date: p.date,
      actual: p.actual_ms ?? null,
      predicted: p.predicted_ms,
      band: [p.lower_ms, p.upper_ms],
      isFuture: false,
    }));
    const future: Row[] = data.forecast.map((p) => ({
      date: p.date,
      actual: null,
      predicted: p.predicted_ms,
      band: [p.lower_ms, p.upper_ms],
      isFuture: true,
    }));
    // Bridge the two segments so the dashed line is continuous.
    const last = history[history.length - 1];
    if (last) history.push({ ...last, actual: null, isFuture: false });
    return [...history, ...future];
  }, [data]);

  const todayIso = new Date().toISOString().slice(0, 10);
  const trend = TREND_META[data.trend];
  const TrendIcon = trend.icon;

  const slopePerDay = data.slope_ms_per_day;
  const slopeLabel = `${slopePerDay >= 0 ? '+' : '−'}${formatDuration(Math.abs(slopePerDay))}/day`;

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <span className={`${trend.cls}`}>
          <TrendIcon size={12} />
          {trend.label}
        </span>
        <span className="chip-neutral tnum" title="Ordinary least squares slope over the window">
          {slopeLabel}
        </span>
        <span className="chip-neutral tnum" title="Coefficient of determination: how much of the day-to-day variation the trend explains">
          R² {data.r2.toFixed(2)}
        </span>
        <span className="chip-neutral tnum" title="Mean absolute error of the fitted model">
          ±{formatDuration(data.mae_ms)} error
        </span>
        <span className="ml-auto text-xs text-fg-faint">
          {displayName(data.class ?? '', metadata) || 'All apps'} · last {data.days} days
        </span>
      </div>

      <ResponsiveContainer width="100%" height={260}>
        <ComposedChart data={rows} margin={{ top: 8, right: 12, bottom: 0, left: 4 }}>
          <defs>
            <linearGradient id="fc-band" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="rgb(var(--c-accent))" stopOpacity={0.28} />
              <stop offset="100%" stopColor="rgb(var(--c-accent))" stopOpacity={0.06} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--c-line))" vertical={false} />
          <XAxis
            dataKey="date"
            tick={{ fill: 'rgb(var(--c-fg-faint))', fontSize: 10 }}
            tickFormatter={formatShortDate}
            minTickGap={28}
            axisLine={{ stroke: 'rgb(var(--c-line))' }}
            tickLine={false}
          />
          <YAxis
            tick={{ fill: 'rgb(var(--c-fg-faint))', fontSize: 10 }}
            tickFormatter={(v: number) => `${Math.round(v / 3600000)}h`}
            axisLine={false}
            tickLine={false}
            width={38}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: 'rgb(var(--c-surface-2))',
              border: '1px solid rgb(var(--c-line))',
              borderRadius: 10,
              color: 'rgb(var(--c-fg))',
              fontSize: 12,
            }}
            labelStyle={{ color: 'rgb(var(--c-fg-muted))' }}
            labelFormatter={(label: string) => label}
            formatter={(value, name) => {
              if (name === 'band') {
                const [lo, hi] = value as [number, number];
                return [`${formatDuration(lo)} – ${formatDuration(hi)}`, '95% band'];
              }
              const label =
                name === 'actual' ? 'Actual' : name === 'predicted' ? 'Model' : String(name);
              return [formatDuration(value as number), label];
            }}
          />
          <ReferenceLine x={todayIso} stroke="rgb(var(--c-accent-2))" strokeDasharray="2 3" label={undefined} />
          <Area
            type="monotone"
            dataKey="band"
            stroke="none"
            fill="url(#fc-band)"
            isAnimationActive
            animationDuration={900}
            connectNulls
          />
          <Line
            type="monotone"
            dataKey="actual"
            stroke="rgb(var(--c-accent))"
            strokeWidth={2}
            dot={false}
            connectNulls
            isAnimationActive
            animationDuration={900}
          />
          <Line
            type="monotone"
            dataKey="predicted"
            stroke="rgb(var(--c-accent-2))"
            strokeWidth={1.75}
            strokeDasharray="5 4"
            dot={false}
            connectNulls
            isAnimationActive
            animationDuration={1100}
          />
        </ComposedChart>
      </ResponsiveContainer>

      <div className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-4">
        {[
          { label: 'Today so far', value: formatDuration(data.today_ms), cls: 'text-fg' },
          {
            label: 'Today projected',
            value: formatDuration(data.today_projected_ms),
            cls: 'text-accent',
          },
          {
            label: 'Tomorrow projected',
            value: formatDuration(data.tomorrow_projected_ms),
            cls: 'text-accent-2',
          },
          { label: `Daily average (${data.days}d)`, value: formatDuration(data.avg_ms), cls: 'text-fg-muted' },
        ].map((s, i) => (
          <motion.div
            key={s.label}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.1 + i * 0.06 }}
            className="rounded-lg border border-line bg-surface-2/60 px-3 py-2"
          >
            <div className="text-[10px] uppercase tracking-wide text-fg-faint">{s.label}</div>
            <div className={`text-sm font-semibold tnum ${s.cls}`}>{s.value}</div>
          </motion.div>
        ))}
      </div>

      <div className="mt-4">
        <div className="mb-1.5 flex items-center justify-between text-[11px] text-fg-faint">
          <span>Weekday effect (multiplier applied to the trend)</span>
          <span className="tnum">1.00 = an average day</span>
        </div>
        <div className="flex items-end gap-1.5">
          {data.weekday_factors.map((f, i) => {
            const height = Math.max(4, Math.min(46, (f / Math.max(...data.weekday_factors, 1)) * 46));
            const strong = f >= 1.05 ? 'bg-accent' : f <= 0.95 ? 'bg-warn' : 'bg-line-strong';
            return (
              <div key={WEEKDAYS[i]} className="flex flex-1 flex-col items-center gap-1">
                <motion.div
                  className={`w-full rounded-t ${strong}`}
                  initial={{ height: 0 }}
                  animate={{ height }}
                  transition={{ delay: 0.2 + i * 0.04, type: 'spring', stiffness: 140, damping: 18 }}
                />
                <span className="text-[10px] text-fg-faint">{WEEKDAYS[i][0]}</span>
                <span className="text-[10px] tnum text-fg-muted">{pct(f, 0)}</span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
