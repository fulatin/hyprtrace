import { motion } from 'framer-motion';
import { AlertTriangle } from 'lucide-react';
import { useState } from 'react';

interface Props {
  message: string;
  onRetry?: () => void;
}

/** Consistent failure state: explains what broke and offers a retry. */
export default function ErrorState({ message, onRetry }: Props) {
  const [expanded, setExpanded] = useState(false);
  const long = message.length > 140;

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.98 }}
      animate={{ opacity: 1, scale: 1 }}
      className="card flex min-h-[16rem] flex-col items-center justify-center gap-3 p-6 text-center"
    >
      <span className="grid h-11 w-11 place-items-center rounded-xl bg-bad/10 text-bad">
        <AlertTriangle size={22} />
      </span>
      <div>
        <p className="text-sm font-medium text-bad">Failed to load data</p>
        <p
          className={`mx-auto mt-1 max-w-md text-xs text-fg-faint ${
            long && !expanded ? 'line-clamp-2' : 'break-all'
          }`}
        >
          {message}
        </p>
        {long && (
          <button
            type="button"
            onClick={() => setExpanded((e) => !e)}
            className="mt-1 text-[11px] text-accent hover:underline"
          >
            {expanded ? 'Show less' : 'Show details'}
          </button>
        )}
      </div>
      {onRetry && (
        <motion.button
          type="button"
          onClick={onRetry}
          whileTap={{ scale: 0.96 }}
          className="btn btn-accent"
        >
          Retry
        </motion.button>
      )}
    </motion.div>
  );
}
