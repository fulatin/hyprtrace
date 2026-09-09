import { useEffect, useRef, useState } from 'react';
import { AnimatePresence, motion, useReducedMotion, type Variants } from 'framer-motion';
import { AlertTriangle, Check, ChevronDown, Copy, Loader2, Wrench } from 'lucide-react';
import { easeOut, expand, spring } from '../lib/motion';

interface ToolCallCardProps {
  part: any; // AI SDK tool part: { type: `tool-${name}`, toolCallId, state, input, output?, errorText? }
}

function toolName(part: any): string {
  if (typeof part.type === 'string' && part.type.startsWith('tool-')) {
    return part.type.slice(5);
  }
  return part.toolName ?? 'tool';
}

/**
 * Clipboard write that also survives the app's common case of being served over
 * plain http on a LAN address, where `navigator.clipboard` is unavailable.
 */
async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // Permission denied or insecure context — fall back below.
  }
  try {
    const area = document.createElement('textarea');
    area.value = text;
    area.setAttribute('readonly', '');
    area.style.position = 'fixed';
    area.style.opacity = '0';
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand('copy');
    document.body.removeChild(area);
    return ok;
  } catch {
    return false;
  }
}

export default function ToolCallCard({ part }: ToolCallCardProps) {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const copiedTimer = useRef<number | null>(null);
  const reduced = useReducedMotion();

  const name = toolName(part);
  const state: string = part.state ?? 'input-available';

  const running = state === 'input-streaming' || state === 'input-available';
  const done = state === 'output-available';
  const failed = state === 'output-error' || state === 'output-denied';

  const payload = JSON.stringify(
    {
      ...(part.input !== undefined ? { input: part.input } : {}),
      ...(done ? { output: part.output } : {}),
      ...(failed ? { error: part.errorText } : {}),
    },
    null,
    2,
  );

  useEffect(() => {
    return () => {
      if (copiedTimer.current !== null) window.clearTimeout(copiedTimer.current);
    };
  }, []);

  const handleCopy = async () => {
    const ok = await copyText(payload);
    if (!ok) return;
    setCopied(true);
    if (copiedTimer.current !== null) window.clearTimeout(copiedTimer.current);
    copiedTimer.current = window.setTimeout(() => setCopied(false), 1200);
  };

  // Height animation for the disclosure body; reduced motion keeps the layout
  // change but drops the movement.
  const bodyVariants: Variants = reduced
    ? { hidden: { opacity: 0 }, show: { opacity: 1 }, exit: { opacity: 0 } }
    : expand;

  const badge = running
    ? { cls: 'chip-warn', label: 'running', icon: <Loader2 size={11} className="animate-spin" /> }
    : failed
      ? { cls: 'chip-bad', label: 'error', icon: <AlertTriangle size={11} /> }
      : done
        ? { cls: 'chip-good', label: 'done', icon: <Check size={11} /> }
        : null;

  return (
    <motion.div
      initial={reduced ? { opacity: 0 } : { opacity: 0, y: 8, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={reduced ? { duration: 0 } : easeOut}
      className="my-2 overflow-hidden rounded-lg border border-line bg-surface-3/40 text-xs"
    >
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-surface-3/50"
      >
        <Wrench size={12} className="shrink-0 text-accent" />
        <span className="truncate font-mono text-accent">{name}</span>

        {badge && (
          <AnimatePresence mode="wait" initial={false}>
            <motion.span
              key={badge.label}
              className={badge.cls}
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              transition={easeOut}
            >
              {badge.icon}
              {badge.label}
            </motion.span>
          </AnimatePresence>
        )}

        <motion.span
          className="ml-auto shrink-0 text-fg-faint"
          animate={{ rotate: open ? 180 : 0 }}
          transition={reduced ? { duration: 0 } : spring}
        >
          <ChevronDown size={13} />
        </motion.span>
      </button>

      <AnimatePresence initial={false}>
        {open && (
          <motion.div
            key="body"
            variants={bodyVariants}
            initial="hidden"
            animate="show"
            exit="exit"
            className="overflow-hidden"
          >
            <div className="flex items-center gap-2 border-t border-line px-3 py-1.5">
              <span className="font-mono text-[10px] uppercase tracking-wider text-fg-faint">
                payload
              </span>
              <button
                type="button"
                onClick={handleCopy}
                title={copied ? 'Copied' : 'Copy tool payload'}
                aria-label={copied ? 'Copied' : 'Copy tool payload'}
                className="btn btn-ghost ml-auto px-2 py-0.5"
              >
                <AnimatePresence mode="wait" initial={false}>
                  {copied ? (
                    <motion.span
                      key="copied"
                      className="chip-good"
                      initial={{ opacity: 0, scale: 0.85 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0, scale: 0.85 }}
                      transition={easeOut}
                    >
                      <Check size={11} />
                      Copied
                    </motion.span>
                  ) : (
                    <motion.span
                      key="copy"
                      initial={{ opacity: 0, scale: 0.7 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0, scale: 0.7 }}
                      transition={reduced ? { duration: 0 } : spring}
                    >
                      <Copy size={12} />
                    </motion.span>
                  )}
                </AnimatePresence>
              </button>
            </div>
            <pre className="max-h-64 overflow-auto bg-surface-2/60 px-3 py-2 font-mono text-[11px] leading-relaxed text-fg-muted">
              {payload}
            </pre>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}
