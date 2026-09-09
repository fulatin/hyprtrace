import { useMemo } from 'react';
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { Clock } from 'lucide-react';
import type { HourlyBucket } from '../lib/types';
import { formatDuration } from '../lib/format';
import { Panel } from './ui/Card';
import { EmptyState } from './ui/Feedback';

interface HourlyBarsProps {
  data: HourlyBucket[];
}

/**
 * 24-hour activity, split into focused and unfocused time. The split matters:
 * a long day made of unfocused time looks very different from a shorter one
 * spent in flow.
 */
export default function HourlyBars({ data }: HourlyBarsProps) {
  const { rows, peak } = useMemo(() => {
    const chart = data
      .map((bucket) => {
        const focused = Math.min(bucket.focused_ms, bucket.total_ms);
        return {
          hour: `${String(bucket.hour).padStart(2, '0')}`,
          total: Math.round(bucket.total_ms / 60000),
          focused: Math.round(focused / 60000),
          rest: Math.round((bucket.total_ms - focused) / 60000),
          sessions: bucket.session_count,
          totalMs: bucket.total_ms,
          focusedMs: focused,
        };
      })
      .sort((a, b) => a.hour.localeCompare(b.hour));
    const peak = chart.reduce(
      (best, r) => (r.totalMs > (best?.totalMs ?? 0) ? r : best),
      chart[0],
    );
    return { rows: chart, peak };
  }, [data]);

  if (data.length === 0) {
    return (
      <Panel title="24h activity" icon={<Clock size={15} className="text-accent" />}>
        <EmptyState
          icon={<Clock size={20} />}
          title="No hourly data for this day"
          hint="Hourly buckets come from recorded sessions."
        />
      </Panel>
    );
  }

  return (
    <Panel
      title="24h activity"
      icon={<Clock size={15} className="text-accent" />}
      hint={
        peak
          ? `peak ${peak.hour}:00 · ${formatDuration(peak.totalMs)} (${formatDuration(peak.focusedMs)} focused)`
          : undefined
      }
      actions={
        <div className="flex items-center gap-3 text-[10px] text-fg-faint">
          <span className="flex items-center gap-1.5">
            <span className="h-2 w-2 rounded-sm bg-accent" />
            focused
          </span>
          <span className="flex items-center gap-1.5">
            <span className="h-2 w-2 rounded-sm bg-accent/30" />
            unfocused
          </span>
        </div>
      }
    >
      <ResponsiveContainer width="100%" height={220}>
        <BarChart data={rows} margin={{ top: 6, right: 8, bottom: 0, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--c-line))" vertical={false} />
          <XAxis
            dataKey="hour"
            tick={{ fill: 'rgb(var(--c-fg-faint))', fontSize: 10 }}
            interval={1}
            axisLine={{ stroke: 'rgb(var(--c-line))' }}
            tickLine={false}
          />
          <YAxis
            tick={{ fill: 'rgb(var(--c-fg-faint))', fontSize: 10 }}
            tickFormatter={(v: number) => `${v}m`}
            axisLine={false}
            tickLine={false}
            width={34}
          />
          <Tooltip
            cursor={{ fill: 'rgb(var(--c-accent) / 0.08)' }}
            contentStyle={{
              backgroundColor: 'rgb(var(--c-surface-2))',
              border: '1px solid rgb(var(--c-line))',
              borderRadius: 10,
              color: 'rgb(var(--c-fg))',
              fontSize: 12,
            }}
            labelStyle={{ color: 'rgb(var(--c-fg-muted))' }}
            labelFormatter={(h: string) => `${h}:00 – ${h}:59`}
            formatter={(value, name) => [
              `${value} min`,
              name === 'focused' ? 'Focused' : 'Unfocused',
            ]}
          />
          <Bar
            dataKey="focused"
            stackId="a"
            fill="rgb(var(--c-accent))"
            radius={[0, 0, 0, 0]}
            isAnimationActive
            animationDuration={700}
          />
          <Bar
            dataKey="rest"
            stackId="a"
            fill="rgb(var(--c-accent) / 0.28)"
            radius={[3, 3, 0, 0]}
            isAnimationActive
            animationDuration={700}
          />
        </BarChart>
      </ResponsiveContainer>
    </Panel>
  );
}
