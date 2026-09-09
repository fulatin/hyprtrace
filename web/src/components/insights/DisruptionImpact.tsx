import { motion, useReducedMotion } from 'framer-motion';
import { useMemo, useState } from 'react';
import { AlertTriangle, BellRing, Sparkles } from 'lucide-react';
import type { DisruptionImpactResponse } from '../../lib/types';
import { formatDuration } from '../../lib/format';
import { corrColor, corrLabel, linearFit, formatSigned } from './shared';

interface DisruptionImpactProps {
  data: DisruptionImpactResponse;
}

const W = 420;
const H = 220;
const PAD = { top: 12, right: 14, bottom: 26, left: 34 };

/**
 * Does being interrupted hurt output? Each dot is one day: x = interruptions,
 * y = efficiency score. The dashed line is the least-squares fit; the chips
 * below quantify the same relationship three different ways.
 */
export default function DisruptionImpact({ data }: DisruptionImpactProps) {
  const reduced = useReducedMotion();
  const [hover, setHover] = useState<string | null>(null);

  const model = useMemo(() => {
    const pts = data.days.filter((d) => d.disruptions > 0 || d.active_ms > 0);
    const xs = pts.map((d) => d.disruptions);
    const ys = pts.map((d) => d.efficiency);
    const fit = linearFit(xs, ys);
    const maxX = Math.max(1, ...xs);
    const minX = 0;
    const innerW = W - PAD.left - PAD.right;
    const innerH = H - PAD.top - PAD.bottom;
    const px = (x: number) => PAD.left + ((x - minX) / (maxX - minX)) * innerW;
    const py = (y: number) => PAD.top + innerH - (Math.max(0, Math.min(100, y)) / 100) * innerH;
    return { pts, fit, maxX, px, py, innerW, innerH };
  }, [data]);

  const { pts, fit, maxX, px, py, innerH } = model;
  const lineY0 = py(fit.intercept);
  const lineY1 = py(fit.intercept + fit.slope * maxX);

  const correlations = [
    { label: 'Efficiency', value: data.correlations.efficiency_vs_disruptions },
    { label: 'Focus ratio', value: data.correlations.focus_ratio_vs_disruptions },
    { label: 'Avg session', value: data.correlations.avg_dwell_vs_disruptions },
    { label: 'Active time', value: data.correlations.active_vs_disruptions },
  ];

  const { high_disruption_days: high, low_disruption_days: low } = data;

  return (
    <div className="space-y-4">
      <div className="flex items-start gap-2.5 rounded-lg border border-accent/25 bg-accent/5 px-3 py-2.5">
        <Sparkles size={15} className="mt-0.5 shrink-0 text-accent" />
        <p className="text-sm text-fg">{data.insight}</p>
      </div>

      <div className="grid gap-5 lg:grid-cols-[minmax(0,420px)_minmax(0,1fr)]">
        <div>
          <svg viewBox={`0 0 ${W} ${H}`} className="w-full">
            {/* Gridlines at 0 / 50 / 100 efficiency */}
            {[0, 50, 100].map((v) => (
              <g key={v}>
                <line
                  x1={PAD.left}
                  x2={W - PAD.right}
                  y1={py(v)}
                  y2={py(v)}
                  stroke="rgb(var(--c-line))"
                  strokeDasharray="3 3"
                />
                <text
                  x={PAD.left - 6}
                  y={py(v) + 3}
                  textAnchor="end"
                  className="fill-fg-faint text-[9px]"
                >
                  {v}
                </text>
              </g>
            ))}

            {/* Regression line */}
            <motion.line
              x1={px(0)}
              y1={lineY0}
              x2={px(maxX)}
              y2={lineY1}
              stroke="rgb(var(--c-accent-2))"
              strokeWidth={1.75}
              strokeDasharray="5 4"
              initial={reduced ? undefined : { pathLength: 0 }}
              animate={{ pathLength: 1 }}
              transition={{ duration: 0.9, delay: 0.25 }}
            />

            {/* Days */}
            {pts.map((d, i) => {
              const cx = px(d.disruptions);
              const cy = py(d.efficiency);
              const isHover = hover === d.date;
              return (
                <motion.circle
                  key={d.date}
                  cx={cx}
                  cy={cy}
                  r={isHover ? 6 : 4}
                  fill={
                    d.efficiency >= 70
                      ? 'rgb(var(--c-good))'
                      : d.efficiency >= 40
                        ? 'rgb(var(--c-warn))'
                        : 'rgb(var(--c-bad))'
                  }
                  fillOpacity={isHover ? 1 : 0.65}
                  stroke="rgb(var(--c-surface))"
                  strokeWidth={1}
                  initial={reduced ? undefined : { opacity: 0, scale: 0 }}
                  animate={{ opacity: 1, scale: 1 }}
                  transition={{ delay: reduced ? 0 : Math.min(0.5, i * 0.02) }}
                  onMouseEnter={() => setHover(d.date)}
                  onMouseLeave={() => setHover(null)}
                  style={{ cursor: 'pointer' }}
                />
              );
            })}

            {/* Axis labels */}
            <text x={W / 2} y={H - 6} textAnchor="middle" className="fill-fg-faint text-[9px]">
              interruptions per day →
            </text>
            <text
              x={10}
              y={PAD.top + innerH / 2}
              textAnchor="middle"
              className="fill-fg-faint text-[9px]"
              transform={`rotate(-90 10 ${PAD.top + innerH / 2})`}
            >
              efficiency score
            </text>
          </svg>

          <div className="h-5 text-xs">
            {hover &&
              (() => {
                const d = pts.find((x) => x.date === hover);
                if (!d) return null;
                return (
                  <span className="flex flex-wrap items-center gap-2">
                    <span className="font-medium text-fg">{d.date}</span>
                    <span className="chip-warn">
                      <BellRing size={10} />
                      {d.disruptions} ({d.notifications} notif · {d.clipboard} copy)
                    </span>
                    <span className="tnum text-fg-muted">efficiency {d.efficiency.toFixed(0)}</span>
                    <span className="tnum text-fg-faint">
                      {formatDuration(d.avg_dwell_ms)} avg session
                    </span>
                  </span>
                );
              })()}
          </div>
        </div>

        <div className="space-y-3">
          <div className="grid grid-cols-2 gap-2">
            {correlations.map((c, i) => (
              <motion.div
                key={c.label}
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: i * 0.05 }}
                className="rounded-lg border border-line bg-surface-2/60 px-3 py-2"
              >
                <div className="text-[10px] uppercase tracking-wide text-fg-faint">{c.label}</div>
                <div className={`text-sm font-semibold tnum ${corrColor(c.value)}`}>
                  r = {formatSigned(c.value, 2)}
                </div>
                <div className="text-[10px] text-fg-faint">{corrLabel(c.value)}</div>
              </motion.div>
            ))}
          </div>

          <div className="rounded-lg border border-line bg-surface-2/50 p-3">
            <div className="mb-2 text-[11px] uppercase tracking-wide text-fg-faint">
              Quietest 25% of days vs busiest 25%
            </div>
            {[
              {
                label: 'Efficiency',
                low: low.avg_efficiency,
                high: high.avg_efficiency,
                max: 100,
                unit: '',
              },
              {
                label: 'Focus ratio',
                low: low.avg_focus_ratio * 100,
                high: high.avg_focus_ratio * 100,
                max: 100,
                unit: '%',
              },
            ].map((row) => (
              <div key={row.label} className="mb-2 last:mb-0">
                <div className="mb-1 flex items-center justify-between text-[11px]">
                  <span className="text-fg-muted">{row.label}</span>
                  <span className="tnum text-fg-faint">
                    {row.low.toFixed(0)}
                    {row.unit} → {row.high.toFixed(0)}
                    {row.unit}
                  </span>
                </div>
                <div className="space-y-1">
                  {[
                    { v: row.low, cls: 'bg-good', name: `quiet (${low.count}d)` },
                    { v: row.high, cls: 'bg-bad', name: `busy (${high.count}d)` },
                  ].map((bar) => (
                    <div key={bar.name} className="flex items-center gap-2">
                      <span className="w-20 shrink-0 text-[10px] text-fg-faint">{bar.name}</span>
                      <div className="h-2 flex-1 overflow-hidden rounded-full bg-surface-3">
                        <motion.div
                          className={`h-full rounded-full ${bar.cls}`}
                          initial={{ width: 0 }}
                          animate={{ width: `${Math.min(100, (bar.v / row.max) * 100)}%` }}
                          transition={{ type: 'spring', stiffness: 110, damping: 20 }}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ))}
            <div className="mt-2 flex items-center gap-1.5 text-[10px] text-fg-faint">
              <AlertTriangle size={11} />
              Correlation is not causation — interruptions and low output can share a cause.
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
