import { motion } from 'framer-motion';
import { useId } from 'react';

interface SparklineProps {
  values: number[];
  width?: number;
  height?: number;
  className?: string;
  /** Tailwind colour class for the stroke, e.g. "text-accent". */
  colorClass?: string;
  fill?: boolean;
}

/**
 * Tiny inline trend line for stat tiles. Drawn as a normalised polyline with a
 * one-shot stroke-dash draw-in animation.
 */
export default function Sparkline({
  values,
  width = 96,
  height = 28,
  className = '',
  colorClass = 'text-accent',
  fill = true,
}: SparklineProps) {
  const gradientId = useId().replace(/:/g, '');
  if (values.length < 2) return <div style={{ width, height }} className={className} />;

  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const stepX = width / (values.length - 1);
  const pad = 2;
  const usable = height - pad * 2;

  const points = values.map((v, i) => {
    const x = i * stepX;
    const y = pad + usable - ((v - min) / span) * usable;
    return [x, y] as const;
  });

  const line = points.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`).join(' ');
  const area = `${line} L${width},${height} L0,${height} Z`;

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className={`${colorClass} ${className}`}
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      {fill && (
        <>
          <defs>
            <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="currentColor" stopOpacity="0.35" />
              <stop offset="100%" stopColor="currentColor" stopOpacity="0" />
            </linearGradient>
          </defs>
          <motion.path
            d={area}
            fill={`url(#${gradientId})`}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.6, delay: 0.15 }}
          />
        </>
      )}
      <motion.path
        d={line}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.75}
        strokeLinecap="round"
        strokeLinejoin="round"
        initial={{ pathLength: 0 }}
        animate={{ pathLength: 1 }}
        transition={{ duration: 0.9, ease: 'easeOut' }}
      />
    </svg>
  );
}
