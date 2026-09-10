import { useEffect, useRef, useState } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { AlertTriangle, Check, Loader2, Save } from 'lucide-react';
import { easeOut, spring } from '../../lib/motion';

/* --------------------------------------------------------------------------
   Form controls shared by the Settings sections. They used to live inside
   Settings.tsx; they are generic enough to be reused, so they moved here
   instead of being copied into every section.
   -------------------------------------------------------------------------- */

/** Spinner that animates through framer-motion (survives reduced-motion CSS). */
export function Spinner({ size = 13 }: { size?: number }) {
  return (
    <motion.span
      animate={{ rotate: 360 }}
      transition={{ repeat: Infinity, duration: 0.9, ease: 'linear' }}
      className="inline-flex"
    >
      <Loader2 size={size} />
    </motion.span>
  );
}

/** Animated switch used for every boolean field on this page. */
export function Toggle({
  checked,
  onChange,
  label,
  hint,
  ariaLabel,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label?: string;
  hint?: string;
  ariaLabel?: string;
}) {
  const track = (
    <span
      className={`flex h-5 w-9 shrink-0 items-center rounded-full border transition-colors ${
        checked ? 'justify-end border-accent/40 bg-accent/25 pr-[0.15rem]' : 'justify-start border-line bg-surface-3 pl-[0.15rem]'
      }`}
    >
      <motion.span
        layout
        transition={spring}
        className={`block h-3.5 w-3.5 rounded-full ${checked ? 'bg-accent' : 'bg-fg-faint'}`}
      />
    </span>
  );

  if (!label) {
    return (
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={ariaLabel}
        onClick={() => onChange(!checked)}
        className="inline-flex shrink-0 items-center"
      >
        {track}
      </button>
    );
  }

  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex w-full items-start gap-3 rounded-lg p-1 text-left transition-colors hover:bg-surface-2/60"
    >
      {track}
      <span className="min-w-0">
        <span className="block text-sm text-fg">{label}</span>
        {hint && <span className="mt-0.5 block text-xs text-fg-faint">{hint}</span>}
      </span>
    </button>
  );
}

/**
 * Save button with an animated state machine: idle → saving (spinner) →
 * saved (checkmark chip that fades back after 1.5 s) → error (bad chip).
 * It reads the caller's `saving` / message state, so no API call is added and
 * no extra request is made.
 */
export function SaveButton({
  onClick,
  saving,
  msg,
  label,
  size = 'md',
}: {
  onClick: () => void;
  saving: boolean;
  msg: string;
  label: string;
  size?: 'sm' | 'md';
}) {
  const [feedback, setFeedback] = useState<{ kind: 'saved' | 'error'; text: string } | null>(null);
  const wasSaving = useRef(false);
  const iconSize = size === 'sm' ? 12 : 14;

  useEffect(() => {
    if (wasSaving.current && !saving) {
      if (msg === 'Saved') setFeedback({ kind: 'saved', text: 'Saved' });
      else if (msg) setFeedback({ kind: 'error', text: msg });
      else setFeedback(null);
    }
    wasSaving.current = saving;
  }, [saving, msg]);

  useEffect(() => {
    if (feedback?.kind !== 'saved') return;
    const timer = window.setTimeout(() => setFeedback(null), 1500);
    return () => window.clearTimeout(timer);
  }, [feedback]);

  const state = saving ? 'saving' : feedback?.kind === 'saved' ? 'saved' : 'idle';

  return (
    <div className="flex flex-wrap items-center gap-3">
      <button
        type="button"
        onClick={onClick}
        disabled={saving}
        className={`btn btn-accent ${size === 'sm' ? 'px-3 py-1.5 text-xs' : 'px-4 py-2'}`}
      >
        <AnimatePresence mode="wait" initial={false}>
          <motion.span
            key={state}
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={easeOut}
            className="inline-flex items-center gap-1.5"
          >
            {state === 'saving' ? (
              <>
                <Spinner size={iconSize} />
                Saving...
              </>
            ) : state === 'saved' ? (
              <>
                <Check size={iconSize} />
                {label}
              </>
            ) : (
              <>
                <Save size={iconSize} />
                {label}
              </>
            )}
          </motion.span>
        </AnimatePresence>
      </button>
      <AnimatePresence initial={false}>
        {feedback && (
          <motion.span
            key={feedback.kind}
            initial={{ opacity: 0, x: -8 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -8 }}
            transition={easeOut}
            className={feedback.kind === 'saved' ? 'chip-good' : 'chip-bad'}
          >
            {feedback.kind === 'saved' ? <Check size={12} /> : <AlertTriangle size={12} />}
            {feedback.text}
          </motion.span>
        )}
      </AnimatePresence>
    </div>
  );
}
