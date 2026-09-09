import { motion } from 'framer-motion';
import { AlertOctagon, AlertTriangle, Info, Moon } from 'lucide-react';
import type { AnomaliesResponse, AnomalyDay } from '../../lib/types';
import { formatDuration, formatRelativeDate, formatSpan } from '../../lib/format';
import { EmptyState } from '../ui/Feedback';
import { WEEKDAYS, pct } from './shared';

interface AnomalyListProps {
  data: AnomaliesResponse;
}

const SEVERITY = {
  high: { cls: 'chip-bad', icon: AlertOctagon, label: 'notable' },
  medium: { cls: 'chip-warn', icon: AlertTriangle, label: 'unusual' },
  low: { cls: 'chip-neutral', icon: Info, label: 'mild' },
} as const;

function AnomalyCard({ day, index }: { day: AnomalyDay; index: number }) {
  const meta = SEVERITY[day.severity];
  const Icon = meta.icon;
  const max = Math.max(day.active_ms, day.baseline_ms, 1);
  const ratio = day.baseline_ms > 0 ? day.active_ms / day.baseline_ms : 1;
  // A zero baseline means the trailing window had no data at all: the z-score
  // is then meaningless (the backend falls back to a raw deviation), so show
  // the absence of a baseline instead of a scary number.
  const hasBaseline = day.baseline_ms > 0;

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: Math.min(0.4, index * 0.05) }}
      className="rounded-lg border border-line bg-surface-2/50 p-3"
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className={meta.cls}>
          <Icon size={11} />
          {meta.label}
        </span>
        <span className="text-sm font-medium text-fg">{day.date}</span>
        <span className="text-xs text-fg-faint">
          {WEEKDAYS[day.weekday] ?? ''} · {formatRelativeDate(day.date)}
        </span>
        <span className="ml-auto tnum text-xs text-fg-muted">
          {hasBaseline
            ? `z = ${day.z_score >= 0 ? '+' : '−'}${Math.abs(day.z_score).toFixed(1)}`
            : 'no baseline data'}
        </span>
      </div>

      <div className="mt-2.5 space-y-1.5">
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[10px] text-fg-faint">actual</span>
          <div className="h-2.5 flex-1 overflow-hidden rounded-full bg-surface-3">
            <motion.div
              className={`h-full rounded-full ${ratio >= 1 ? 'bg-accent' : 'bg-warn'}`}
              initial={{ width: 0 }}
              animate={{ width: `${Math.min(100, (day.active_ms / max) * 100)}%` }}
              transition={{ type: 'spring', stiffness: 110, damping: 20 }}
            />
          </div>
          <span className="w-16 shrink-0 text-right text-[11px] tnum text-fg">
            {formatDuration(day.active_ms)}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[10px] text-fg-faint">baseline</span>
          <div className="h-2.5 flex-1 overflow-hidden rounded-full bg-surface-3">
            <motion.div
              className="h-full rounded-full bg-line-strong"
              initial={{ width: 0 }}
              animate={{ width: `${Math.min(100, (day.baseline_ms / max) * 100)}%` }}
              transition={{ type: 'spring', stiffness: 110, damping: 20, delay: 0.05 }}
            />
          </div>
          <span className="w-16 shrink-0 text-right text-[11px] tnum text-fg-muted">
            {hasBaseline ? formatDuration(day.baseline_ms) : 'no data'}
          </span>
        </div>
      </div>

      <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
        {day.reasons.map((r) => (
          <span key={r} className="chip-neutral">
            {r}
          </span>
        ))}
        {day.late_night_share > 0 && (
          <span className="chip-neutral">
            <Moon size={10} />
            {pct(day.late_night_share, 0)} late night
          </span>
        )}
        <span className="chip-neutral tnum">{day.session_count} sessions</span>
        <span className="chip-neutral tnum">
          {formatSpan(day.avg_dwell_ms)} avg session
        </span>
      </div>
    </motion.div>
  );
}

/** Days that broke the user's own routine — ranked by severity. */
export default function AnomalyList({ data }: AnomalyListProps) {
  if (data.days.length === 0) {
    return (
      <EmptyState
        icon={<AlertTriangle size={22} />}
        title="No anomalies in this window"
        hint={`Every day stayed within ${data.threshold.toFixed(1)}σ of your ${data.window}-day baseline.`}
      />
    );
  }

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2 text-xs text-fg-faint">
        <span>
          Baseline: median of the trailing {data.window} days, robust σ from MAD.
        </span>
        <span className="tnum">
          {data.days.filter((d) => d.severity === 'high').length} notable ·{' '}
          {data.days.filter((d) => d.severity === 'medium').length} unusual
        </span>
      </div>
      {data.days.map((day, i) => (
        <AnomalyCard key={day.date} day={day} index={i} />
      ))}
    </div>
  );
}
