import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
  Cell,
} from 'recharts';
import type { TooltipProps } from 'recharts';
import type { AppRank } from '../lib/types';
import { formatDuration } from '../lib/format';
import { EmptyState } from './ui/Feedback';

/**
 * Recharts only accepts concrete colour values (it writes them into SVG
 * attributes), so tokens are referenced as CSS variables instead of hex — the
 * palette then follows the active theme.
 */
const token = (name: string) => `rgb(var(--c-${name}))`;

/** Per-bar palette, in rank order. Top bar gets the accent gradient below. */
const COLORS = [
  token('accent'),
  token('accent-2'),
  token('good'),
  token('warn'),
  token('bad'),
  token('accent'),
  token('accent-2'),
  token('good'),
  token('warn'),
  token('bad'),
];

const GRADIENT_ID = 'app-ranking-bar-accent';

interface AppRankingBarProps {
  data: AppRank[];
}

interface RankTooltipProps extends TooltipProps<number, string> {
  /** Set on the chart data, so the tooltip can show share as well as time. */
  name?: string;
  percentage?: number;
  display?: string;
}

function RankTooltip({ active, payload }: RankTooltipProps) {
  const point = payload?.[0]?.payload as RankTooltipProps | undefined;
  if (!active || !point) return null;
  return (
    <div className="rounded-lg border border-line bg-surface-2 px-3 py-2 text-xs text-fg shadow-soft">
      <p className="mb-1 font-medium">{point.name}</p>
      <p className="text-fg-muted">
        <span className="tnum text-fg">{point.display}</span> active
      </p>
      <p className="text-fg-muted">
        <span className="tnum text-accent">{point.percentage?.toFixed(1)}%</span> of tracked time
      </p>
    </div>
  );
}

export default function AppRankingBar({ data }: AppRankingBarProps) {
  if (data.length === 0) {
    return (
      <EmptyState
        className="h-48"
        title="No app activity in this range"
        hint="Pick a wider range, or start tracking to populate the ranking."
      />
    );
  }

  const chartData = data.map((app) => ({
    name: app.class,
    minutes: Math.round(app.total_ms / 60000),
    percentage: app.percentage,
    display: formatDuration(app.total_ms),
  }));

  return (
    <div className="card p-4">
      <ResponsiveContainer width="100%" height={data.length * 40 + 40}>
        <BarChart data={chartData} layout="vertical" margin={{ left: 80, right: 40 }}>
          <defs>
            {/* Subtle accent wash reserved for the #1 app. */}
            <linearGradient id={GRADIENT_ID} x1="0" y1="0" x2="1" y2="0">
              <stop offset="0%" stopColor={token('accent')} stopOpacity={1} />
              <stop offset="100%" stopColor={token('accent')} stopOpacity={0.55} />
            </linearGradient>
          </defs>
          <CartesianGrid strokeDasharray="2 6" stroke={token('line')} horizontal={false} />
          <XAxis
            type="number"
            tick={{ fill: token('fg-faint'), fontSize: 10 }}
            axisLine={{ stroke: token('line') }}
            tickLine={false}
            tickFormatter={(value: number) => `${value}m`}
          />
          <YAxis
            type="category"
            dataKey="name"
            tick={{ fill: token('fg-muted'), fontSize: 12 }}
            axisLine={false}
            tickLine={false}
            width={70}
          />
          <Tooltip
            cursor={{ fill: token('surface-2'), fillOpacity: 0.6 }}
            content={<RankTooltip />}
          />
          <Bar
            dataKey="minutes"
            radius={[0, 6, 6, 0]}
            isAnimationActive
            animationDuration={800}
            animationEasing="ease-out"
          >
            {chartData.map((_entry, index) => (
              <Cell
                key={`cell-${index}`}
                fill={index === 0 ? `url(#${GRADIENT_ID})` : COLORS[index % COLORS.length]}
              />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
