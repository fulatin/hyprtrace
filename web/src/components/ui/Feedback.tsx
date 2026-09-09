import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import { barTransition } from '../../lib/motion';

interface ProgressBarProps {
  /** 0..100 */
  pct: number;
  color?: string;
  className?: string;
  /** Render a shimmering stripe while a value is still being computed. */
  indeterminate?: boolean;
  height?: number;
}

export function ProgressBar({
  pct,
  color,
  className = '',
  indeterminate,
  height = 8,
}: ProgressBarProps) {
  const clamped = Math.max(0, Math.min(100, pct));
  return (
    <div
      className={`w-full overflow-hidden rounded-full bg-surface-3 ${className}`}
      style={{ height }}
      role="progressbar"
      aria-valuenow={Math.round(clamped)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <motion.div
        className={`h-full rounded-full ${indeterminate ? 'animate-pulse' : ''}`}
        style={color ? { backgroundColor: color } : undefined}
        initial={{ width: 0 }}
        animate={{ width: `${clamped}%` }}
        transition={barTransition}
      />
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  hint,
  action,
  className = '',
}: {
  icon?: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      className={`flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-line px-6 py-10 text-center ${className}`}
    >
      {icon && <div className="text-fg-faint">{icon}</div>}
      <p className="text-sm text-fg-muted">{title}</p>
      {hint && <p className="max-w-sm text-xs text-fg-faint">{hint}</p>}
      {action}
    </motion.div>
  );
}

export function Skeleton({ className = '' }: { className?: string }) {
  return <div className={`skeleton ${className}`} />;
}

/** Standard loading block for a page section. */
export function SkeletonPanel({ height = 'h-64' }: { height?: string }) {
  return (
    <div className="card p-5">
      <Skeleton className="mb-4 h-4 w-40" />
      <Skeleton className={height} />
    </div>
  );
}

/** A row of skeleton tiles matching the stat grid. */
export function SkeletonStats({ count = 6 }: { count?: number }) {
  return (
    <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
      {Array.from({ length: count }).map((_, i) => (
        <div key={i} className="card p-4" style={{ animationDelay: `${i * 60}ms` }}>
          <Skeleton className="mb-3 h-3 w-16" />
          <Skeleton className="h-6 w-20" />
        </div>
      ))}
    </div>
  );
}
