import { motion, useReducedMotion, type Variants } from 'framer-motion';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { fadeUp, staggerContainer } from '../../lib/motion';

/** Staggered container: children wrapped in <RevealItem> appear in sequence. */
export function Reveal({
  children,
  className,
  stagger = 0.05,
  delay = 0,
}: {
  children: ReactNode;
  className?: string;
  stagger?: number;
  delay?: number;
}) {
  const reduced = useReducedMotion();
  if (reduced) return <div className={className}>{children}</div>;
  return (
    <motion.div
      className={className}
      variants={staggerContainer(stagger, delay)}
      initial="hidden"
      animate="show"
    >
      {children}
    </motion.div>
  );
}

export function RevealItem({
  children,
  className,
  variants = fadeUp,
  ...rest
}: {
  children: ReactNode;
  className?: string;
  variants?: Variants;
} & Record<string, unknown>) {
  return (
    <motion.div className={className} variants={variants} {...rest}>
      {children}
    </motion.div>
  );
}

/** Reveals once when scrolled into view (for below-the-fold panels). */
export function RevealOnScroll({
  children,
  className,
  delay = 0,
}: {
  children: ReactNode;
  className?: string;
  delay?: number;
}) {
  const reduced = useReducedMotion();
  const ref = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState(false);

  // Explicit IntersectionObserver instead of `whileInView`: a section that
  // mounts already inside (or above) the viewport must still become visible.
  // `whileInView` only reacts to entering the viewport, which used to leave
  // content permanently transparent after navigating mid-page or refreshing
  // with a restored scroll position.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (el.getBoundingClientRect().top < window.innerHeight * 0.95) {
      setShown(true);
      return;
    }
    if (typeof IntersectionObserver === 'undefined') {
      setShown(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setShown(true);
          observer.disconnect();
        }
      },
      { rootMargin: '0px 0px -60px 0px' },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  if (reduced) return <div className={className}>{children}</div>;

  return (
    <motion.div
      ref={ref}
      className={className}
      initial={false}
      animate={shown ? { opacity: 1, y: 0 } : { opacity: 0, y: 18 }}
      transition={{ duration: 0.45, delay: shown ? delay : 0, ease: [0.22, 1, 0.36, 1] }}
    >
      {children}
    </motion.div>
  );
}
