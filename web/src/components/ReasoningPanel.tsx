import { useState } from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { Brain, ChevronDown } from 'lucide-react';
import { expand, spring } from '../lib/motion';

interface ReasoningPanelProps {
  /** Accumulated reasoning text of one assistant turn. */
  text: string;
  /** True while the model is still thinking (`part.state === 'streaming'`). */
  streaming: boolean;
}

/**
 * Collapsible "thinking" block for one `reasoning` part of an assistant
 * message.
 *
 * Deliberately collapsed by default — the answer is what the user came for —
 * but the toggle stays live during streaming, so opening it while the model
 * thinks shows the trace as it arrives. The caller renders one per part, so a
 * turn that thinks, calls a tool, then thinks again gets a separate block per
 * step.
 */
export default function ReasoningPanel({ text, streaming }: ReasoningPanelProps) {
  const [open, setOpen] = useState(false);
  const reduced = useReducedMotion();

  return (
    <div className="mb-2 overflow-hidden rounded-lg border border-accent-2/25 bg-accent-2/5">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-label={open ? 'Hide reasoning' : 'Show reasoning'}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-fg-muted transition-colors hover:bg-accent-2/10 hover:text-fg"
      >
        <motion.span
          className={streaming ? 'text-accent-2' : 'text-fg-faint'}
          animate={
            streaming && !reduced ? { opacity: [0.45, 1, 0.45] } : { opacity: 1 }
          }
          transition={
            streaming && !reduced
              ? { duration: 1.6, repeat: Infinity, ease: 'easeInOut' }
              : { duration: 0 }
          }
        >
          <Brain size={13} />
        </motion.span>
        <span className="font-medium">
          {streaming ? 'Thinking…' : 'Thought process'}
        </span>
        {text.length > 0 && (
          <span className="tnum text-fg-faint">{text.length.toLocaleString()} chars</span>
        )}
        <motion.span
          className="ml-auto text-fg-faint"
          animate={{ rotate: open ? 180 : 0 }}
          transition={reduced ? { duration: 0 } : spring}
        >
          <ChevronDown size={13} />
        </motion.span>
      </button>

      <AnimatePresence initial={false}>
        {open && (
          <motion.div
            key="reasoning-body"
            variants={reduced ? undefined : expand}
            initial="hidden"
            animate="show"
            exit="exit"
            className="overflow-hidden border-t border-accent-2/20"
          >
            <p className="max-h-56 overflow-y-auto whitespace-pre-wrap px-3 py-2 text-xs leading-relaxed text-fg-muted">
              {text || 'Waiting for the model…'}
            </p>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
