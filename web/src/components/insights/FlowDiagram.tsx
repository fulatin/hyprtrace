import { motion, useReducedMotion } from 'framer-motion';
import { useMemo, useState } from 'react';
import type { AppMetadata, TransitionEdge, TransitionNode } from '../../lib/types';
import { appColor, displayName, pct } from './shared';

interface FlowDiagramProps {
  nodes: TransitionNode[];
  edges: TransitionEdge[];
  metadata?: Record<string, AppMetadata>;
  focus?: string | null;
  onFocus?: (cls: string | null) => void;
}

const W = 780;
const H = 420;
const NODE_W = 10;
const MARGIN_X = 118;
const GAP = 8;

/**
 * Two-column transition flow ("what do I open next"). Ribbon thickness is the
 * conditional probability P(to | from) times the source's switch volume, so
 * wide ribbons are the paths the user actually walks most often.
 * Self-transitions are excluded from ribbons and shown as a badge instead.
 */
export default function FlowDiagram({ nodes, edges, metadata, focus, onFocus }: FlowDiagramProps) {
  const reduced = useReducedMotion();
  const [hoverEdge, setHoverEdge] = useState<string | null>(null);
  const [hoverNode, setHoverNode] = useState<string | null>(null);

  const layout = useMemo(() => {
    if (nodes.length === 0) return null;
    const classes = nodes.map((n) => n.class);
    const index = new Map(classes.map((c, i) => [c, i]));

    // Outgoing / incoming volume per node, ignoring self-loops.
    const out = new Array(classes.length).fill(0);
    const inc = new Array(classes.length).fill(0);
    const links: {
      from: number;
      to: number;
      count: number;
      probability: number;
      y0: number;
      y1: number;
    }[] = [];
    for (const e of edges) {
      const i = index.get(e.from);
      const j = index.get(e.to);
      if (i === undefined || j === undefined || i === j) continue;
      out[i] += e.count;
      inc[j] += e.count;
      links.push({ from: i, to: j, count: e.count, probability: e.probability, y0: 0, y1: 0 });
    }

    const total = Math.max(out.reduce((a, b) => a + b, 0), inc.reduce((a, b) => a + b, 0), 1);
    const usable = H - GAP * (classes.length - 1);
    const scale = usable / total;

    const leftY: number[] = [];
    const rightY: number[] = [];
    let ly = 0;
    let ry = 0;
    for (let i = 0; i < classes.length; i++) {
      leftY.push(ly);
      rightY.push(ry);
      ly += out[i] * scale + GAP;
      ry += inc[i] * scale + GAP;
    }

    // Stacking offsets so ribbons leave/enter their node in a stable order.
    const leftOffset = new Array(classes.length).fill(0);
    const rightOffset = new Array(classes.length).fill(0);
    const ordered = [...links].sort((a, b) => a.from - b.from || a.to - b.to);
    for (const l of ordered) {
      l.y0 = leftY[l.from] + leftOffset[l.from];
      l.y1 = rightY[l.to] + rightOffset[l.to];
      leftOffset[l.from] += l.count * scale;
      rightOffset[l.to] += l.count * scale;
    }

    return { classes, out, inc, links: ordered, scale, leftY, rightY };
  }, [nodes, edges]);

  if (!layout) return null;

  const { classes, links, scale, leftY, rightY, out, inc } = layout;
  const maxProb = Math.max(0.0001, ...links.map((l) => l.probability));
  const x0 = MARGIN_X;
  const x1 = W - MARGIN_X;
  const active = hoverEdge ?? (hoverNode ? `node:${hoverNode}` : null);

  const ribbonPath = (y0: number, y1: number, h0: number, h1: number) => {
    const mid = (x0 + x1) / 2;
    return `M${x0},${y0} C${mid},${y0} ${mid},${y1} ${x1},${y1} L${x1},${y1 + h1} C${mid},${y1 + h1} ${mid},${y0 + h0} ${x0},${y0 + h0} Z`;
  };

  const edgeKey = (l: { from: number; to: number }) => `${classes[l.from]}->${classes[l.to]}`;

  return (
    <div>
      <svg viewBox={`0 0 ${W} ${H}`} className="w-full" style={{ maxHeight: 440 }}>
        {/* Ribbons */}
        {links.map((l, i) => {
          const key = edgeKey(l);
          const fromCls = classes[l.from];
          const toCls = classes[l.to];
          const isActive =
            hoverEdge === key || hoverNode === fromCls || hoverNode === toCls;
          const dim = active !== null && !isActive;
          const h0 = Math.max(1.2, l.count * scale);
          const h1 = Math.max(1.2, l.count * scale);
          return (
            <motion.path
              key={key}
              d={ribbonPath(l.y0, l.y1, h0, h1)}
              fill={appColor(fromCls)}
              initial={reduced ? undefined : { opacity: 0, pathLength: 0 }}
              animate={{
                opacity: dim ? 0.05 : isActive ? 0.55 : 0.2,
                pathLength: 1,
              }}
              transition={{
                opacity: { duration: 0.2 },
                pathLength: { duration: 0.7, delay: reduced ? 0 : i * 0.02, ease: 'easeOut' },
              }}
              onMouseEnter={() => setHoverEdge(key)}
              onMouseLeave={() => setHoverEdge(null)}
            >
              <title>
                {displayName(fromCls, metadata)} → {displayName(toCls, metadata)}: {pct(l.probability)} (
                {l.count} switches)
              </title>
            </motion.path>
          );
        })}

        {/* Nodes */}
        {classes.map((cls, i) => {
          const lh = Math.max(3, out[i] * scale);
          const rh = Math.max(3, inc[i] * scale);
          const isFocus = focus === cls;
          const dim = focus !== null && !isFocus;
          return (
            <g key={cls} opacity={dim ? 0.4 : 1}>
              <motion.rect
                x={x0 - NODE_W}
                y={leftY[i]}
                width={NODE_W}
                height={lh}
                rx={3}
                fill={appColor(cls)}
                initial={reduced ? undefined : { scaleY: 0 }}
                animate={{ scaleY: 1 }}
                style={{ transformOrigin: `${x0 - NODE_W / 2}px ${leftY[i]}px` }}
                transition={{ duration: 0.4, delay: reduced ? 0 : i * 0.04 }}
                onMouseEnter={() => setHoverNode(cls)}
                onMouseLeave={() => setHoverNode(null)}
                onClick={() => onFocus?.(isFocus ? null : cls)}
                className="cursor-pointer"
              >
                <title>
                  {displayName(cls, metadata)} — {out[i]} outgoing switches
                </title>
              </motion.rect>
              <motion.rect
                x={x1}
                y={rightY[i]}
                width={NODE_W}
                height={rh}
                rx={3}
                fill={appColor(cls)}
                initial={reduced ? undefined : { scaleY: 0 }}
                animate={{ scaleY: 1 }}
                style={{ transformOrigin: `${x1 + NODE_W / 2}px ${rightY[i]}px` }}
                transition={{ duration: 0.4, delay: reduced ? 0 : i * 0.04 }}
                onMouseEnter={() => setHoverNode(cls)}
                onMouseLeave={() => setHoverNode(null)}
                onClick={() => onFocus?.(isFocus ? null : cls)}
                className="cursor-pointer"
              >
                <title>
                  {displayName(cls, metadata)} — {inc[i]} incoming switches
                </title>
              </motion.rect>

              {/* Left labels */}
              <text
                x={x0 - NODE_W - 8}
                y={leftY[i] + Math.min(lh, 14) / 2 + 4}
                textAnchor="end"
                className="fill-fg text-[11px]"
              >
                {displayName(cls, metadata)}
              </text>
              <text
                x={x0 - NODE_W - 8}
                y={leftY[i] + Math.min(lh, 14) / 2 + 16}
                textAnchor="end"
                className="fill-fg-faint tnum text-[10px]"
              >
                {out[i]} out · {pct(nodes[i]?.self_probability ?? 0, 0)} stay
              </text>

              {/* Right labels */}
              <text
                x={x1 + NODE_W + 8}
                y={rightY[i] + Math.min(rh, 14) / 2 + 4}
                textAnchor="start"
                className="fill-fg text-[11px]"
              >
                {displayName(cls, metadata)}
              </text>
              <text
                x={x1 + NODE_W + 8}
                y={rightY[i] + Math.min(rh, 14) / 2 + 16}
                textAnchor="start"
                className="fill-fg-faint tnum text-[10px]"
              >
                {inc[i]} in
              </text>
            </g>
          );
        })}
      </svg>

      <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-fg-faint">
        <span>
          Left = current app, right = next app. Ribbon width ∝ number of switches; the
          thickest ribbons are your habitual paths.
        </span>
        <span className="tnum">Hottest path: {pct(maxProb)}</span>
      </div>
    </div>
  );
}
