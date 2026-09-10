import { motion } from 'framer-motion';
import type { ReactNode } from 'react';
import { easeOut } from '../../lib/motion';

/** Row of an editable list: animates in, reflows on reorder, animates out. */
export function AnimatedRow({ children, className = '' }: { children: ReactNode; className?: string }) {
  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, x: 16, transition: { duration: 0.18, ease: 'easeIn' } }}
      transition={easeOut}
      className={className}
    >
      {children}
    </motion.div>
  );
}
