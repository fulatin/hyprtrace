import { AnimatePresence, motion } from 'framer-motion';
import { Monitor, Moon, Sun } from 'lucide-react';
import { useTheme } from '../../lib/theme';
import { spring } from '../../lib/motion';

const ICONS = { dark: Moon, light: Sun, system: Monitor } as const;
const LABELS = { dark: 'Dark', light: 'Light', system: 'System' } as const;

/** Cycles dark → light → system. The icon morphs between states. */
export default function ThemeToggle({ compact = false }: { compact?: boolean }) {
  const { preference, cycle } = useTheme();
  const Icon = ICONS[preference];

  return (
    <button
      type="button"
      onClick={cycle}
      className="btn btn-ghost gap-2"
      title={`Theme: ${LABELS[preference]} (click to switch)`}
      aria-label={`Switch theme, currently ${LABELS[preference]}`}
    >
      <span className="relative grid h-4 w-4 place-items-center">
        <AnimatePresence mode="wait" initial={false}>
          <motion.span
            key={preference}
            initial={{ opacity: 0, rotate: -60, scale: 0.6 }}
            animate={{ opacity: 1, rotate: 0, scale: 1 }}
            exit={{ opacity: 0, rotate: 60, scale: 0.6 }}
            transition={spring}
            className="absolute"
          >
            <Icon size={15} />
          </motion.span>
        </AnimatePresence>
      </span>
      {!compact && <span className="text-xs">{LABELS[preference]}</span>}
    </button>
  );
}
