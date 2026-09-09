import { motion } from 'framer-motion';
import {
  Bar,
  CartesianGrid,
  ComposedChart,
  Line,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { ArrowDownRight, ArrowUpRight, Layers, Minus } from 'lucide-react';
import type { FragmentationResponse } from '../../lib/types';
import { formatShortDate, formatSpan } from '../../lib/format';
import { pct } from './shared';

interface FragmentationPanelProps {
  data: FragmentationResponse;
}

/**
 * Context-switching cost: how often focus is broken, how long sessions last,
 * and the estimated re-orientation time lost to each switch.
 */
export default function FragmentationPanel({ data }: FragmentationPanelProps) {
  // Sessions here are frequently sub-minute, so the trend line uses seconds:
  // rounding to whole minutes would flatten almost every day to zero.
  const rows = data.days.map((d) => ({
    date: d.date,
    switches: d.switch_count,
    dwellSec: Math.round(d.avg_dwell_ms / 1000),
    cost: d.context_switch_cost_ms,
    shortRatio: d.short_session_ratio,
    focusBlocks: d.focus_blocks,
  }));

  const trend = data.trend_slope_ms_per_day;
  const TrendIcon = trend > 50 ? ArrowUpRight : trend < -50 ? ArrowDownRight : Minus;
  const trendCls = trend > 50 ? 'text-good' : trend < -50 ? 'text-bad' : 'text-fg-muted';

  const avgCostPerDay =
    data.days.length > 0
      ? data.days.reduce((a, d) => a + d.context_switch_cost_ms, 0) / data.days.length
      : 0;

  const tiles = [
    {
      label: 'Avg session length',
      value: formatSpan(data.avg_dwell_ms),
      hint: `median ${formatSpan(data.median_dwell_ms)}`,
      accent: 'text-accent',
    },
    {
      label: 'Switches per hour',
      value: data.switches_per_hour.toFixed(1),
      hint: `${Math.round(data.avg_switch_count)} per day`,
      accent: data.switches_per_hour > 20 ? 'text-warn' : 'text-fg',
    },
    {
      label: 'Sub-minute sessions',
      value: pct(data.short_session_ratio, 0),
      hint: 'of all sessions',
      accent: data.short_session_ratio > 0.6 ? 'text-warn' : 'text-fg',
    },
    {
      label: 'Est. re-focus cost',
      value: formatSpan(avgCostPerDay),
      hint: 'lost per day',
      accent: 'text-bad',
    },
  ];

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {tiles.map((t, i) => (
          <motion.div
            key={t.label}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: i * 0.05 }}
            className="rounded-lg border border-line bg-surface-2/60 px-3 py-2.5"
          >
            <div className="text-[10px] uppercase tracking-wide text-fg-faint">{t.label}</div>
            <div className={`text-lg font-semibold tnum ${t.accent}`}>{t.value}</div>
            <div className="text-[10px] text-fg-faint">{t.hint}</div>
          </motion.div>
        ))}
      </div>

      <div>
        <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
          <span className={`inline-flex items-center gap-1 font-medium ${trendCls}`}>
            <TrendIcon size={12} />
            {trend >= 0 ? '+' : '−'}
            {formatSpan(Math.abs(trend))} per day
          </span>
          <span className="text-fg-faint">session length trend</span>
          <span className="ml-auto flex items-center gap-1.5 text-fg-faint">
            <Layers size={12} />
            {data.days.reduce((a, d) => a + d.focus_blocks, 0)} focus blocks · longest{' '}
            {formatSpan(Math.max(0, ...data.days.map((d) => d.longest_focus_ms)))}
          </span>
        </div>

        <ResponsiveContainer width="100%" height={210}>
          <ComposedChart data={rows} margin={{ top: 6, right: 8, bottom: 0, left: 0 }}>
            <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--c-line))" vertical={false} />
            <XAxis
              dataKey="date"
              tick={{ fill: 'rgb(var(--c-fg-faint))', fontSize: 10 }}
              tickFormatter={formatShortDate}
              minTickGap={26}
              axisLine={{ stroke: 'rgb(var(--c-line))' }}
              tickLine={false}
            />
            <YAxis
              yAxisId="switches"
              tick={{ fill: 'rgb(var(--c-fg-faint))', fontSize: 10 }}
              axisLine={false}
              tickLine={false}
              width={34}
            />
            <YAxis
              yAxisId="dwell"
              orientation="right"
              tick={{ fill: 'rgb(var(--c-fg-faint))', fontSize: 10 }}
              tickFormatter={(v: number) => (v >= 60 ? `${Math.round(v / 60)}m` : `${v}s`)}
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
              formatter={(value, name) => {
                if (name === 'switches') return [value as number, 'Switches'];
                if (name === 'dwellSec') return [formatSpan((value as number) * 1000), 'Avg session'];
                return [value as number, String(name)];
              }}
            />
            <Bar
              yAxisId="switches"
              dataKey="switches"
              fill="rgb(var(--c-accent) / 0.35)"
              radius={[3, 3, 0, 0]}
              isAnimationActive
              animationDuration={700}
            />
            <Line
              yAxisId="dwell"
              type="monotone"
              dataKey="dwellSec"
              stroke="rgb(var(--c-accent-2))"
              strokeWidth={2}
              dot={false}
              isAnimationActive
              animationDuration={900}
            />
          </ComposedChart>
        </ResponsiveContainer>
      </div>

      {data.worst_day && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg border border-warn/25 bg-warn/5 px-3 py-2 text-xs">
          <span className="font-medium text-warn">Most fragmented day</span>
          <span className="text-fg-muted">{data.worst_day.date}</span>
          <span className="tnum text-fg">{data.worst_day.switch_count} switches</span>
          <span className="tnum text-fg-faint">
            {formatSpan(data.worst_day.avg_dwell_ms)} average session
          </span>
        </div>
      )}
    </div>
  );
}
