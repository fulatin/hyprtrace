import { useEffect, useRef } from 'react';
import { animate, useMotionValue, useReducedMotion } from 'framer-motion';

interface CountUpProps {
  /** Target value; the number eases from its current rendered value to this. */
  value: number;
  /** Render the animated raw number as a string (e.g. "3h 12m"). */
  format: (n: number) => string;
  duration?: number;
  className?: string;
}

/**
 * Animates a number by writing text directly into the DOM node on each frame.
 * Going through React state would re-render the whole card 60×/s, which is
 * wasteful for a dashboard with a dozen live counters.
 */
export default function CountUp({ value, format, duration = 0.9, className }: CountUpProps) {
  const ref = useRef<HTMLSpanElement>(null);
  const motionValue = useMotionValue(0);
  const reduced = useReducedMotion();

  // `format` is only ever read while writing text, so it lives in a ref: a
  // parent that passes a fresh inline arrow (StatCard's fallback, Dashboard's
  // `cards` literals) would otherwise be a new dependency on every render and
  // stop/restart the tween from the current value — the visible jitter. Only a
  // new target `value` (or a reduced-motion change) may restart the animation.
  const formatRef = useRef(format);
  useEffect(() => {
    formatRef.current = format;
  });

  useEffect(() => {
    if (reduced) {
      if (ref.current) ref.current.textContent = formatRef.current(value);
      motionValue.set(value);
      return;
    }
    const controls = animate(motionValue, value, {
      duration,
      ease: [0.22, 1, 0.36, 1],
      onUpdate: (latest) => {
        if (ref.current) ref.current.textContent = formatRef.current(latest);
      },
    });
    return () => controls.stop();
  }, [value, duration, motionValue, reduced]);

  return (
    <span ref={ref} className={className}>
      {format(reduced ? value : 0)}
    </span>
  );
}
