import { motion, type HTMLMotionProps } from 'framer-motion';
import type { ReactNode } from 'react';
import { hoverLift } from '../../lib/motion';

interface CardProps extends HTMLMotionProps<'div'> {
  children: ReactNode;
  /** Adds a hover lift + accent border glow. */
  interactive?: boolean;
  /** Removes the inner padding (for cards that own their layout). */
  flush?: boolean;
}

export function Card({ children, interactive, flush, className = '', ...rest }: CardProps) {
  return (
    <motion.div
      {...(interactive ? hoverLift : {})}
      className={`${interactive ? 'card-interactive' : 'card'} ${flush ? '' : 'p-5'} ${className}`}
      {...rest}
    >
      {children}
    </motion.div>
  );
}

interface PanelProps {
  title: ReactNode;
  icon?: ReactNode;
  hint?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  flush?: boolean;
}

/** Titled panel: the standard container for every chart / list block. */
export function Panel({
  title,
  icon,
  hint,
  actions,
  children,
  className = '',
  flush,
}: PanelProps) {
  return (
    <Card className={className} flush={flush}>
      <div className={`flex flex-wrap items-center gap-x-3 gap-y-2 ${flush ? 'p-5 pb-3' : 'mb-4'}`}>
        <h3 className="panel-title">
          {icon}
          {title}
        </h3>
        {hint && <span className="panel-sub">{hint}</span>}
        {actions && <div className="ml-auto flex items-center gap-2">{actions}</div>}
      </div>
      {children}
    </Card>
  );
}
