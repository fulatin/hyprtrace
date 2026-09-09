import { motion } from 'framer-motion';
import { spring } from '../../lib/motion';

interface SegmentedControlProps<T extends string> {
  options: { value: T; label: string; icon?: React.ReactNode }[];
  value: T;
  onChange: (next: T) => void;
  /** Unique id so multiple controls on one page don't share a layout animation. */
  layoutId: string;
  size?: 'sm' | 'md';
  className?: string;
}

/** Segmented control whose active pill slides between options (shared layout). */
export default function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  layoutId,
  size = 'md',
  className = '',
}: SegmentedControlProps<T>) {
  const pad = size === 'sm' ? 'px-2.5 py-1 text-xs' : 'px-3 py-1.5 text-sm';
  return (
    <div
      role="tablist"
      className={`inline-flex items-center gap-1 rounded-lg border border-line bg-surface-2 p-1 ${className}`}
    >
      {options.map((opt) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            role="tab"
            aria-selected={active}
            type="button"
            onClick={() => onChange(opt.value)}
            className={`relative inline-flex items-center gap-1.5 rounded-md font-medium transition-colors ${pad} ${
              active ? 'text-bg' : 'text-fg-muted hover:text-fg'
            }`}
          >
            {active && (
              <motion.span
                layoutId={layoutId}
                className="absolute inset-0 rounded-md bg-accent"
                transition={spring}
              />
            )}
            <span className="relative z-10 flex items-center gap-1.5">
              {opt.icon}
              {opt.label}
            </span>
          </button>
        );
      })}
    </div>
  );
}
