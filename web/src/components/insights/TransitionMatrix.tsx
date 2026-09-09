import { motion, useReducedMotion } from 'framer-motion';
import { useMemo, useState } from 'react';
import type { AppMetadata, TransitionMatrix as Matrix } from '../../lib/types';
import { appColor, displayName, pct } from './shared';

interface TransitionMatrixProps {
  matrix: Matrix;
  nodes: { class: string; total_ms: number; out_total: number }[];
  metadata?: Record<string, AppMetadata>;
  /** Highlights the row/column of this app (set by the flow diagram). */
  focus?: string | null;
  onFocus?: (cls: string | null) => void;
}

/**
 * Row-normalised Markov matrix: cell (i, j) is the probability that the next
 * app is j given the current app is i. Intensity encodes probability, so a hot
 * row means "this app almost always leads somewhere predictable".
 */
export default function TransitionMatrix({
  matrix,
  nodes,
  metadata,
  focus,
  onFocus,
}: TransitionMatrixProps) {
  const reduced = useReducedMotion();
  const [hover, setHover] = useState<{ row: number; col: number } | null>(null);

  const maxP = useMemo(
    () => Math.max(0.0001, ...matrix.values.flat().filter((v) => Number.isFinite(v))),
    [matrix],
  );

  const cellReadout = useMemo(() => {
    if (!hover) return null;
    const from = matrix.classes[hover.row];
    const to = matrix.classes[hover.col];
    const p = matrix.values[hover.row]?.[hover.col] ?? 0;
    const outTotal = nodes[hover.row]?.out_total ?? 0;
    const count = Math.round(p * outTotal);
    return { from, to, p, count, outTotal };
  }, [hover, matrix, nodes]);

  if (matrix.classes.length === 0) return null;

  const n = matrix.classes.length;

  return (
    <div>
      <div className="mb-2 flex h-5 items-center gap-2 text-xs">
        {cellReadout ? (
          <>
            <span className="font-medium" style={{ color: appColor(cellReadout.from) }}>
              {displayName(cellReadout.from, metadata)}
            </span>
            <span className="text-fg-faint">→</span>
            <span className="font-medium" style={{ color: appColor(cellReadout.to) }}>
              {displayName(cellReadout.to, metadata)}
            </span>
            <span className="tnum text-accent">{pct(cellReadout.p)}</span>
            <span className="tnum text-fg-faint">
              {cellReadout.count} of {cellReadout.outTotal} switches
            </span>
          </>
        ) : (
          <span className="text-fg-faint">
            Hover a cell to read the transition probability. Rows sum to 100%.
          </span>
        )}
      </div>

      <div className="overflow-x-auto pb-1">
        <div
          className="grid min-w-[420px] gap-[3px]"
          style={{ gridTemplateColumns: `minmax(96px, 1.2fr) repeat(${n}, minmax(38px, 1fr))` }}
        >
          {/* Column headers */}
          <div />
          {matrix.classes.map((cls) => (
            <div
              key={`h-${cls}`}
              className="flex h-16 items-end justify-center"
              title={displayName(cls, metadata)}
            >
              <span
                className="max-h-16 overflow-hidden text-[10px] font-medium leading-tight text-fg-muted"
                style={{
                  writingMode: 'vertical-rl',
                  transform: 'rotate(180deg)',
                }}
              >
                {displayName(cls, metadata)}
              </span>
            </div>
          ))}

          {matrix.classes.map((fromCls, i) => (
            <div key={`row-${fromCls}`} className="contents">
              <button
                type="button"
                onClick={() => onFocus?.(focus === fromCls ? null : fromCls)}
                className={`flex h-9 items-center justify-end pr-2 text-right text-xs transition-colors ${
                  focus === fromCls ? 'font-semibold text-fg' : 'text-fg-muted hover:text-fg'
                }`}
                title={`${displayName(fromCls, metadata)} — ${nodes[i]?.out_total ?? 0} outgoing switches`}
              >
                <span
                  className="mr-1.5 h-2 w-2 shrink-0 rounded-full"
                  style={{ backgroundColor: appColor(fromCls) }}
                />
                <span className="truncate">{displayName(fromCls, metadata)}</span>
              </button>

              {matrix.classes.map((toCls, j) => {
                const p = matrix.values[i]?.[j] ?? 0;
                const intensity = Math.min(1, p / maxP);
                const isDiagonal = i === j;
                const dim =
                  (hover && hover.row !== i && hover.col !== j) ||
                  (focus && focus !== fromCls && focus !== toCls);
                return (
                  <motion.div
                    key={`c-${fromCls}-${toCls}`}
                    initial={reduced ? undefined : { opacity: 0, scale: 0.82 }}
                    animate={{ opacity: dim ? 0.32 : 1, scale: 1 }}
                    transition={{
                      duration: 0.25,
                      delay: reduced ? 0 : Math.min(0.4, (i + j) * 0.012),
                    }}
                    onMouseEnter={() => setHover({ row: i, col: j })}
                    onMouseLeave={() => setHover(null)}
                    className={`relative grid h-9 place-items-center rounded-md text-[10px] tnum transition-[filter] ${
                      p > 0.4 * maxP ? 'font-semibold text-fg' : 'text-fg-muted'
                    }`}
                    style={{
                      backgroundColor: `rgb(var(--c-accent) / ${(0.06 + intensity * 0.85).toFixed(3)})`,
                      outline: isDiagonal ? '1px solid rgb(var(--c-line-strong) / 0.6)' : undefined,
                      outlineOffset: '-1px',
                    }}
                    title={`${displayName(fromCls, metadata)} → ${displayName(toCls, metadata)}: ${pct(p)}`}
                  >
                    {p >= 0.005 ? pct(p, p < 0.1 ? 1 : 0) : ''}
                    {isDiagonal && (
                      <span className="pointer-events-none absolute bottom-[2px] h-[2px] w-1.5 rounded-full bg-fg/40" />
                    )}
                  </motion.div>
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
