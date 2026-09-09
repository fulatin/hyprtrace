import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from 'recharts';
import type { TooltipProps } from 'recharts';
import type { DailyTrend } from '../lib/types';
import { formatDuration } from '../lib/format';

/**
 * Recharts writes colours straight into SVG attributes, so tokens are passed as
 * CSS variables — the chart then re-themes with the rest of the app.
 */
const token = (name: string) => `rgb(var(--c-${name}))`;

const FILL_ID = 'app-trend-fill';
const STROKE_ID = 'app-trend-stroke';

interface AppTrendChartProps {
  data: DailyTrend[];
  range?: 'today' | 'week' | 'month';
}

interface TrendTooltipProps extends TooltipProps<number, string> {
  /** Carried on the chart data so the tooltip can show the real date. */
  date?: string;
  minutes?: number;
  sessions?: number;
}

function TrendTooltip({ active, payload }: TrendTooltipProps) {
  const point = payload?.[0]?.payload as TrendTooltipProps | undefined;
  if (!active || !point) return null;
  return (
    <div className="rounded-lg border border-line bg-surface-2 px-3 py-2 text-xs text-fg shadow-soft">
      <p className="mb-1 font-medium">{point.date}</p>
      <p className="text-fg-muted">
        <span className="tnum text-accent">
          {formatDuration((point.minutes ?? 0) * 60000)}
        </span>{' '}
        active
      </p>
      <p className="text-fg-faint tnum">{point.sessions} sessions</p>
    </div>
  );
}

export default function AppTrendChart({ data, range }: AppTrendChartProps) {
  if (data.length === 0) {
    return <p className="p-4 text-sm text-fg-muted">No trend data available</p>;
  }

  const formatXLabel = (dateStr: string) => {
    if (range === 'today') return dateStr;
    return dateStr.slice(5);
  };

  const chartData = data.map((d) => ({
    date: d.date,
    label: formatXLabel(d.date),
    minutes: Math.round(d.total_ms / 60000),
    sessions: d.session_count,
  }));

  return (
    <div className="card mt-4 p-4">
      <div className="mb-2 flex items-center gap-2">
        <h4 className="panel-title">{range === 'today' ? 'Hourly Trend' : 'Daily Trend'}</h4>
        {range === 'week' && <span className="panel-sub">(past 7 days)</span>}
        {range === 'month' && <span className="panel-sub">(past 30 days)</span>}
      </div>
      <ResponsiveContainer width="100%" height={180}>
        <AreaChart data={chartData}>
          <defs>
            <linearGradient id={FILL_ID} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={token('accent')} stopOpacity={0.35} />
              <stop offset="55%" stopColor={token('accent')} stopOpacity={0.12} />
              <stop offset="100%" stopColor={token('accent')} stopOpacity={0} />
            </linearGradient>
            {/* Vertical fade makes the single-hue line read as a gradient. */}
            <linearGradient id={STROKE_ID} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={token('accent')} stopOpacity={1} />
              <stop offset="100%" stopColor={token('accent-2')} stopOpacity={0.9} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="2 6" stroke={token('line')} vertical={false} />
          <XAxis
            dataKey="label"
            tick={{ fill: token('fg-faint'), fontSize: 10 }}
            axisLine={{ stroke: token('line') }}
            tickLine={false}
          />
          <YAxis
            tick={{ fill: token('fg-faint'), fontSize: 10 }}
            axisLine={false}
            tickLine={false}
            width={34}
            tickFormatter={(value: number) => `${value}m`}
          />
          <Tooltip
            cursor={{ stroke: token('line-strong'), strokeDasharray: '3 3' }}
            content={<TrendTooltip />}
          />
          <Area
            type="monotone"
            dataKey="minutes"
            stroke={`url(#${STROKE_ID})`}
            strokeWidth={2}
            fill={`url(#${FILL_ID})`}
            dot={{ fill: token('surface'), stroke: token('accent'), strokeWidth: 1.5, r: 2.5 }}
            activeDot={{ r: 4, fill: token('accent'), stroke: token('surface'), strokeWidth: 2 }}
            isAnimationActive
            animationDuration={900}
            animationEasing="ease-out"
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
