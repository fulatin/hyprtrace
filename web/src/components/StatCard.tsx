import { motion } from 'framer-motion';
import { ArrowDown, ArrowUp } from 'lucide-react';
import type { ReactNode } from 'react';
import CountUp from './ui/CountUp';
import Sparkline from './ui/Sparkline';
import { fadeUp, hoverLift } from '../lib/motion';

type Accent = 'accent' | 'accent-2' | 'good' | 'warn' | 'bad';

const ACCENT_TEXT: Record<Accent, string> = {
  accent: 'text-accent',
  'accent-2': 'text-accent-2',
  good: 'text-good',
  warn: 'text-warn',
  bad: 'text-bad',
};

const ACCENT_WASH: Record<Accent, string> = {
  accent: 'bg-accent/10',
  'accent-2': 'bg-accent-2/10',
  good: 'bg-good/10',
  warn: 'bg-warn/10',
  bad: 'bg-bad/10',
};

interface StatCardProps {
  icon: ReactNode;
  label: string;
  /** A number animates with a count-up; a string renders verbatim. */
  value: string | number;
  /** Required to render a numeric `value`. */
  format?: (n: number) => string;
  subtext?: ReactNode;
  /** Day-over-day style change indicator. */
  delta?: { value: number; format: (n: number) => string; positiveIsGood?: boolean };
  /** Small trend line under the value. */
  spark?: number[];
  accent?: Accent;
}

/**
 * Headline metric tile. Numbers count up on mount, the sparkline draws itself,
 * and the whole card lifts on hover — the three cues that make a dashboard feel
 * alive without being distracting.
 */
export default function StatCard({
  icon,
  label,
  value,
  format,
  subtext,
  delta,
  spark,
  accent = 'accent',
}: StatCardProps) {
  const good = delta ? (delta.positiveIsGood === false ? delta.value < 0 : delta.value > 0) : false;
  const neutralDelta = delta ? delta.value === 0 : true;

  return (
    <motion.div
      variants={fadeUp}
      {...hoverLift}
      className="card-interactive flex flex-col gap-2.5 p-4"
    >
      <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-fg-faint">
        <span className={`grid h-7 w-7 shrink-0 place-items-center rounded-lg ${ACCENT_WASH[accent]} ${ACCENT_TEXT[accent]}`}>
          {icon}
        </span>
        <span className="truncate">{label}</span>
      </div>

      <div className="flex items-end justify-between gap-3">
        <div className="text-2xl font-semibold leading-none tnum text-fg">
          {typeof value === 'number' ? (
            <CountUp value={value} format={format ?? ((n) => String(Math.round(n)))} />
          ) : (
            value
          )}
        </div>
        {spark && spark.length > 1 && (
          <Sparkline values={spark} className={ACCENT_TEXT[accent]} width={72} height={24} />
        )}
      </div>

      {(delta || subtext) && (
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          {delta && (
            <span
              className={`chip shrink-0 ${
                neutralDelta ? 'chip-neutral' : good ? 'chip-good' : 'chip-bad'
              }`}
              title="Compared with the previous day"
            >
              {!neutralDelta &&
                (delta.value > 0 ? <ArrowUp size={11} /> : <ArrowDown size={11} />)}
              {delta.format(delta.value)}
            </span>
          )}
          {subtext && <span className="min-w-0 truncate text-xs text-fg-faint">{subtext}</span>}
        </div>
      )}
    </motion.div>
  );
}
