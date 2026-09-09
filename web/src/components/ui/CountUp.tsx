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

  useEffect(() => {
    if (reduced) {
      if (ref.current) ref.current.textContent = format(value);
      motionValue.set(value);
      return;
    }
    const controls = animate(motionValue, value, {
      duration,
      ease: [0.22, 1, 0.36, 1],
      onUpdate: (latest) => {
        if (ref.current) ref.current.textContent = format(latest);
      },
    });
    return () => controls.stop();
  }, [value, duration, format, motionValue, reduced]);

  return (
    <span ref={ref} className={className}>
      {format(reduced ? value : 0)}
    </span>
  );
}
