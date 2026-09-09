import type { Transition, Variants } from 'framer-motion';

/**
 * Shared motion language for the whole app. Keeping every timing here means
 * new panels animate consistently instead of each page inventing its own.
 */

/** Snappy, slightly overshooting spring used for interactive elements. */
export const spring: Transition = {
  type: 'spring',
  stiffness: 380,
  damping: 30,
  mass: 0.8,
};

/** Softer spring for larger surfaces (cards, panels, drawers). */
export const softSpring: Transition = {
  type: 'spring',
  stiffness: 220,
  damping: 26,
};

export const easeOut: Transition = { duration: 0.32, ease: [0.22, 1, 0.36, 1] };

/** Container that reveals children one after another. */
export const staggerContainer = (stagger = 0.05, delay = 0): Variants => ({
  hidden: {},
  show: {
    transition: { staggerChildren: stagger, delayChildren: delay },
  },
});

export const fadeUp: Variants = {
  hidden: { opacity: 0, y: 14 },
  show: { opacity: 1, y: 0, transition: easeOut },
};

export const fadeIn: Variants = {
  hidden: { opacity: 0 },
  show: { opacity: 1, transition: { duration: 0.28 } },
};

export const scaleIn: Variants = {
  hidden: { opacity: 0, scale: 0.96 },
  show: { opacity: 1, scale: 1, transition: softSpring },
};

/** Page-level transition used by the router outlet.
 *
 * Entrance only, deliberately: an exit animation keeps the previous route
 * mounted, and because <Outlet/> is driven by the live router context it
 * re-renders the NEW page inside the exiting container (double mount). If a
 * nested AnimatePresence is mid-flight the exit can stall, leaving the content
 * area empty until a refresh. See Layout.tsx. */
export const pageTransition: Variants = {
  hidden: { opacity: 0, y: 10, filter: 'blur(4px)' },
  show: {
    opacity: 1,
    y: 0,
    filter: 'blur(0px)',
    transition: { duration: 0.3, ease: [0.22, 1, 0.36, 1] },
  },
};

/** Rows/items inside a list. */
export const listItem: Variants = {
  hidden: { opacity: 0, x: -8 },
  show: { opacity: 1, x: 0, transition: easeOut },
};

/** Expanded detail panel (e.g. a selected app's trend). */
export const expand: Variants = {
  hidden: { opacity: 0, height: 0 },
  show: {
    opacity: 1,
    height: 'auto',
    transition: { height: softSpring, opacity: { duration: 0.2 } },
  },
  exit: { opacity: 0, height: 0, transition: { duration: 0.2 } },
};

export const hoverLift = {
  whileHover: { y: -3 },
  whileTap: { scale: 0.995 },
  transition: spring,
};

/** Progress/bar fill: animates from 0 on mount, then eases to new values. */
export const barTransition: Transition = {
  type: 'spring',
  stiffness: 90,
  damping: 20,
  mass: 1,
};
