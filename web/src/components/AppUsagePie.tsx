import { useMemo, useState } from 'react';
import { motion } from 'framer-motion';
import { PieChart, Pie, Cell, Tooltip, ResponsiveContainer } from 'recharts';
import { PieChart as PieIcon } from 'lucide-react';
import type { AppRank, AppMetadata } from '../lib/types';
import { formatDuration } from '../lib/format';
import { Panel } from './ui/Card';
import { EmptyState } from './ui/Feedback';
import CountUp from './ui/CountUp';

const COLORS = [
  '#22d3ee',
  '#a78bfa',
  '#34d399',
  '#fbbf24',
  '#f87171',
  '#60a5fa',
  '#f472b6',
  '#2dd4bf',
  '#fb923c',
  '#818cf8',
];

interface AppUsagePieProps {
  data: AppRank[];
  metadata?: Record<string, AppMetadata>;
}

export default function AppUsagePie({ data, metadata }: AppUsagePieProps) {
  const [activeIndex, setActiveIndex] = useState<number | null>(null);

  const chartData = useMemo(
    () =>
      data.map((app, i) => ({
        name: metadata?.[app.class]?.display_name || app.class,
        value: app.total_ms,
        percentage: app.percentage,
        color: COLORS[i % COLORS.length],
      })),
    [data, metadata],
  );

  const total = useMemo(() => chartData.reduce((a, d) => a + d.value, 0), [chartData]);

  if (data.length === 0) {
    return (
      <Panel title="App usage distribution" icon={<PieIcon size={15} className="text-accent" />}>
        <EmptyState
          icon={<PieIcon size={20} />}
          title="No app usage recorded"
          hint="Pick another day or start tracking."
        />
      </Panel>
    );
  }

  const focused = activeIndex !== null ? chartData[activeIndex] : null;

  return (
    <Panel
      title="App usage distribution"
      icon={<PieIcon size={15} className="text-accent" />}
      hint={`${chartData.length} apps · ${formatDuration(total)}`}
    >
      <div className="relative">
        <ResponsiveContainer width="100%" height={240}>
          <PieChart>
            <Pie
              data={chartData}
              cx="50%"
              cy="50%"
              innerRadius={62}
              outerRadius={96}
              paddingAngle={2}
              nameKey="name"
              dataKey="value"
              stroke="rgb(var(--c-surface))"
              strokeWidth={2}
              isAnimationActive
              animationDuration={800}
              animationEasing="ease-out"
              onMouseEnter={(_: unknown, index: number) => setActiveIndex(index)}
              onMouseLeave={() => setActiveIndex(null)}
            >
              {chartData.map((entry, index) => (
                <Cell
                  key={entry.name}
                  fill={entry.color}
                  fillOpacity={activeIndex === null || activeIndex === index ? 1 : 0.35}
                />
              ))}
            </Pie>
            <Tooltip
              contentStyle={{
                backgroundColor: 'rgb(var(--c-surface-2))',
                border: '1px solid rgb(var(--c-line))',
                borderRadius: 10,
                color: 'rgb(var(--c-fg))',
                fontSize: 12,
              }}
              itemStyle={{ color: 'rgb(var(--c-fg))' }}
              labelStyle={{ color: 'rgb(var(--c-fg-muted))' }}
              formatter={(value: number) => [
                `${formatDuration(value)} (${((value / Math.max(total, 1)) * 100).toFixed(1)}%)`,
                'Active time',
              ]}
            />
          </PieChart>
        </ResponsiveContainer>

        {/* Centre readout: total by default, the hovered slice on hover. */}
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
          <span className="text-[10px] uppercase tracking-wide text-fg-faint">
            {focused ? focused.name.slice(0, 14) : 'Total'}
          </span>
          <span className="text-lg font-semibold tnum text-fg">
            {focused ? (
              formatDuration(focused.value)
            ) : (
              <CountUp value={total} format={formatDuration} />
            )}
          </span>
          {focused && (
            <span className="text-[10px] tnum text-accent">{focused.percentage.toFixed(1)}%</span>
          )}
        </div>
      </div>

      <div className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1.5">
        {chartData.map((d, i) => (
          <motion.div
            key={d.name}
            initial={{ opacity: 0, x: -6 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ delay: 0.15 + i * 0.04 }}
            onMouseEnter={() => setActiveIndex(i)}
            onMouseLeave={() => setActiveIndex(null)}
            className={`flex items-center gap-2 rounded-md px-1.5 py-0.5 text-xs transition-colors ${
              activeIndex === i ? 'bg-surface-3' : ''
            }`}
          >
            <span className="h-2 w-2 shrink-0 rounded-full" style={{ backgroundColor: d.color }} />
            <span className="truncate text-fg-muted">{d.name}</span>
            <span className="ml-auto shrink-0 tnum text-fg-faint">{d.percentage.toFixed(1)}%</span>
          </motion.div>
        ))}
      </div>
    </Panel>
  );
}
