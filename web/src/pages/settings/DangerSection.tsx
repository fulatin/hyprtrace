import { useState } from 'react';
import { format, subDays } from 'date-fns';
import { AnimatePresence, motion } from 'framer-motion';
import { AlertTriangle, Check, Trash2 } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { Spinner } from '../../components/ui/Controls';
import { easeOut } from '../../lib/motion';
import { useDeleteSessions } from '../../lib/queries';

/** Danger zone: delete tracked sessions in a date range (with confirmation). */
export default function DangerSection() {
  const deleteSessions = useDeleteSessions();

  const days7Ago = format(subDays(new Date(), 7), 'yyyy-MM-dd');
  const today = format(new Date(), 'yyyy-MM-dd');
  const [deleteFrom, setDeleteFrom] = useState(days7Ago);
  const [deleteTo, setDeleteTo] = useState(today);
  const [deleteClass, setDeleteClass] = useState('');
  const [deleting, setDeleting] = useState(false);
  const [deleteMsg, setDeleteMsg] = useState('');

  const handleDeleteSessions = async () => {
    if (!deleteFrom || !deleteTo) {
      setDeleteMsg('Please provide both dates');
      return;
    }
    if (!window.confirm(
      `Delete usage data${deleteClass.trim() ? ` for app "${deleteClass.trim()}"` : ''} from ${deleteFrom} to ${deleteTo}? This cannot be undone.`
    )) {
      return;
    }
    setDeleting(true);
    setDeleteMsg('');
    try {
      const cls = deleteClass.trim() ? deleteClass.trim() : undefined;
      const res = await deleteSessions.mutateAsync({ from: deleteFrom, to: deleteTo, cls });
      setDeleteMsg(`${res.deleted} session(s) deleted`);
    } catch (e) {
      setDeleteMsg('Delete failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setDeleting(false);
    }
  };

  const deleteFailed =
    deleteMsg === 'Delete failed' ||
    deleteMsg.startsWith('Delete failed') ||
    deleteMsg === 'Please provide both dates';

  return (
    <RevealOnScroll>
      <Panel
        title="Delete usage data"
        icon={<Trash2 size={15} className="text-bad" />}
        hint="Cannot be undone"
        className="border-bad/30 bg-bad/5"
      >
        <div className="space-y-4">
          <p className="text-xs text-fg-muted">
            Permanently remove tracked sessions in a date range. A confirmation dialog is shown before anything is
            deleted.
          </p>

          <div className="flex flex-wrap items-end gap-3">
            <label className="block">
              <span className="mb-1 block text-xs text-fg-muted">From</span>
              <input
                type="date"
                value={deleteFrom}
                onChange={(e) => setDeleteFrom(e.target.value)}
                className="input focus:border-bad/60 focus:ring-bad/25"
              />
            </label>
            <label className="block">
              <span className="mb-1 block text-xs text-fg-muted">To</span>
              <input
                type="date"
                value={deleteTo}
                onChange={(e) => setDeleteTo(e.target.value)}
                className="input focus:border-bad/60 focus:ring-bad/25"
              />
            </label>
            <label className="block min-w-[10rem] flex-1">
              <span className="mb-1 block text-xs text-fg-muted">App class (optional)</span>
              <input
                type="text"
                value={deleteClass}
                onChange={(e) => setDeleteClass(e.target.value)}
                placeholder="kitty"
                className="input w-full focus:border-bad/60 focus:ring-bad/25"
              />
            </label>
          </div>

          <div className="flex flex-wrap items-center gap-3">
            <button
              type="button"
              onClick={handleDeleteSessions}
              disabled={deleting}
              className="btn border-bad/30 bg-bad/10 text-bad hover:border-bad/50 hover:bg-bad/20"
            >
              {deleting ? <Spinner size={14} /> : <Trash2 size={14} />}
              {deleting ? 'Deleting...' : 'Delete'}
            </button>
            <AnimatePresence initial={false}>
              {deleteMsg && (
                <motion.span
                  key={deleteMsg}
                  initial={{ opacity: 0, x: -8 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -8 }}
                  transition={easeOut}
                  className={deleteFailed ? 'chip-bad' : 'chip-good'}
                >
                  {deleteFailed ? <AlertTriangle size={12} /> : <Check size={12} />}
                  {deleteMsg}
                </motion.span>
              )}
            </AnimatePresence>
          </div>
        </div>
      </Panel>
    </RevealOnScroll>
  );
}
