import { motion, useReducedMotion } from 'framer-motion';
import { useMemo, useState } from 'react';
import { Link2 } from 'lucide-react';
import type { AppMetadata, CooccurrenceResponse } from '../../lib/types';
import { formatSpan } from '../../lib/format';
import { appColor, displayName, pct } from './shared';

interface CooccurrenceGraphProps {
  data: CooccurrenceResponse;
  metadata?: Record<string, AppMetadata>;
}

const SIZE = 360;
const CX = SIZE / 2;
const CY = SIZE / 2;
const R = 128;

/**
 * Apps that get used together. The chord thickness is raw co-occurrence
 * volume; lift tells you whether the pairing is more than coincidence
 * (lift > 1 means they appear together more often than chance would predict).
 */
export default function CooccurrenceGraph({ data, metadata }: CooccurrenceGraphProps) {
  const reduced = useReducedMotion();
  const [hoverPair, setHoverPair] = useState<string | null>(null);

  const layout = useMemo(() => {
    const classes = data.nodes.map((n) => n.class);
    const positions = new Map<string, { x: number; y: number; angle: number }>();
    classes.forEach((cls, i) => {
      const angle = (i / Math.max(classes.length, 1)) * Math.PI * 2 - Math.PI / 2;
      positions.set(cls, {
        x: CX + R * Math.cos(angle),
        y: CY + R * Math.sin(angle),
        angle,
      });
    });
    const maxCo = Math.max(1, ...data.pairs.map((p) => p.co_occurrences));
    return { classes, positions, maxCo };
  }, [data]);

  if (data.pairs.length === 0) return null;

  const { positions, maxCo } = layout;

  return (
    <div className="grid gap-5 lg:grid-cols-[minmax(0,360px)_minmax(0,1fr)]">
      <div className="mx-auto w-full max-w-[360px]">
        <svg viewBox={`0 0 ${SIZE} ${SIZE}`} className="w-full">
          {/* Chords */}
          {data.pairs.map((p, i) => {
            const a = positions.get(p.a);
            const b = positions.get(p.b);
            if (!a || !b) return null;
            const key = `${p.a}|${p.b}`;
            const isHover = hoverPair === key;
            const width = 1.5 + (p.co_occurrences / maxCo) * 13;
            return (
              <motion.path
                key={key}
                d={`M${a.x},${a.y} Q${CX},${CY} ${b.x},${b.y}`}
                fill="none"
                stroke={appColor(p.a)}
                strokeWidth={width}
                strokeLinecap="round"
                initial={reduced ? undefined : { opacity: 0, pathLength: 0 }}
                animate={{ opacity: isHover ? 0.85 : 0.28, pathLength: 1 }}
                transition={{
                  opacity: { duration: 0.2 },
                  pathLength: { duration: 0.8, delay: reduced ? 0 : i * 0.06 },
                }}
                onMouseEnter={() => setHoverPair(key)}
                onMouseLeave={() => setHoverPair(null)}
                style={{ cursor: 'pointer' }}
              >
                <title>
                  {displayName(p.a, metadata)} + {displayName(p.b, metadata)}: {p.co_occurrences} times ·
                  lift {p.lift.toFixed(2)}
                </title>
              </motion.path>
            );
          })}

          {/* Nodes */}
          {layout.classes.map((cls, i) => {
            const pos = positions.get(cls);
            if (!pos) return null;
            const labelX = CX + (R + 16) * Math.cos(pos.angle);
            const labelY = CY + (R + 16) * Math.sin(pos.angle);
            const anchor = Math.cos(pos.angle) >= 0 ? 'start' : 'end';
            return (
              <motion.g
                key={cls}
                initial={reduced ? undefined : { opacity: 0, scale: 0.6 }}
                animate={{ opacity: 1, scale: 1 }}
                transition={{ delay: reduced ? 0 : i * 0.05 }}
              >
                <circle cx={pos.x} cy={pos.y} r={7} fill={appColor(cls)} />
                <circle cx={pos.x} cy={pos.y} r={11} fill={appColor(cls)} opacity={0.22} />
                <text
                  x={labelX}
                  y={labelY + 3}
                  textAnchor={anchor}
                  className="fill-fg-muted text-[10px]"
                >
                  {displayName(cls, metadata).slice(0, 18)}
                </text>
              </motion.g>
            );
          })}
        </svg>
      </div>

      <div className="space-y-2">
        <div className="flex items-center gap-2 text-xs text-fg-faint">
          <Link2 size={12} />
          <span>
            Pairs used within {data.window_minutes} min of each other. Lift &gt; 1 means more than
            chance.
          </span>
        </div>
        {data.pairs.map((p, i) => {
          const key = `${p.a}|${p.b}`;
          const maxLift = Math.max(...data.pairs.map((x) => x.lift), 1);
          return (
            <motion.div
              key={key}
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: 0.1 + i * 0.04 }}
              onMouseEnter={() => setHoverPair(key)}
              onMouseLeave={() => setHoverPair(null)}
              className={`rounded-lg border px-3 py-2 transition-colors ${
                hoverPair === key ? 'border-accent/40 bg-accent/5' : 'border-line bg-surface-2/50'
              }`}
            >
              <div className="flex items-center gap-2 text-xs">
                <span className="flex min-w-0 items-center gap-1.5">
                  <span
                    className="h-2 w-2 shrink-0 rounded-full"
                    style={{ backgroundColor: appColor(p.a) }}
                  />
                  <span className="truncate text-fg">{displayName(p.a, metadata)}</span>
                </span>
                <span className="text-fg-faint">+</span>
                <span className="flex min-w-0 items-center gap-1.5">
                  <span
                    className="h-2 w-2 shrink-0 rounded-full"
                    style={{ backgroundColor: appColor(p.b) }}
                  />
                  <span className="truncate text-fg">{displayName(p.b, metadata)}</span>
                </span>
                <span className="ml-auto shrink-0 chip-accent tnum">×{p.lift.toFixed(2)} lift</span>
              </div>
              <div className="mt-1.5 flex items-center gap-2">
                <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-3">
                  <motion.div
                    className="h-full rounded-full"
                    style={{ backgroundColor: appColor(p.a) }}
                    initial={{ width: 0 }}
                    animate={{ width: `${Math.min(100, (p.lift / maxLift) * 100)}%` }}
                    transition={{ delay: 0.15 + i * 0.04, type: 'spring', stiffness: 120, damping: 20 }}
                  />
                </div>
                <span className="shrink-0 text-[10px] tnum text-fg-faint">
                  {p.co_occurrences}× · {pct(p.support, 0)} of days · Jaccard {p.jaccard.toFixed(2)} ·{' '}
                  avg gap {formatSpan(p.avg_gap_ms)}
                </span>
              </div>
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}
