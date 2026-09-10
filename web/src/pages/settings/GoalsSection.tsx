import { useEffect, useState } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { Plus, Target, Trash2 } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { AnimatedRow } from '../../components/ui/AnimatedRow';
import { SaveButton, Toggle } from '../../components/ui/Controls';
import { easeOut } from '../../lib/motion';
import { useGoals, useSaveGoals } from '../../lib/queries';
import type { Goal } from '../../lib/types';

/** Daily goals: name, target scope, hours per day and an on/off switch per row. */
export default function GoalsSection() {
  const goalsQuery = useGoals();
  const saveGoals = useSaveGoals();
  const [goals, setGoals] = useState<Goal[]>([]);
  const [savingGoals, setSavingGoals] = useState(false);
  const [goalMsg, setGoalMsg] = useState('');

  const storedGoals = goalsQuery.data?.goals;
  useEffect(() => {
    if (storedGoals) setGoals(storedGoals);
  }, [storedGoals]);

  const handleAddGoal = () => {
    setGoals([...goals, { id: null, name: '', target_type: 'all', target_key: '', daily_target_ms: 4 * 3600000, enabled: true }]);
  };

  const handleDeleteGoal = (idx: number) => {
    setGoals(goals.filter((_, i) => i !== idx));
  };

  const handleUpdateGoal = (idx: number, field: keyof Goal, value: string | number | boolean) => {
    setGoals(goals.map((g, i) => (i === idx ? { ...g, [field]: value } : g)));
  };

  const handleSaveGoals = async () => {
    setSavingGoals(true);
    setGoalMsg('');
    try {
      const list = goals.filter((g) => g.name.trim());
      await saveGoals.mutateAsync(list);
      setGoalMsg('Saved');
    } catch (e) {
      setGoalMsg('Save failed');
    } finally {
      setSavingGoals(false);
    }
  };

  return (
    <RevealOnScroll>
      <Panel
        title="Daily goals"
        icon={<Target size={15} className="text-good" />}
        hint="Active-time targets"
      >
        <p className="mb-4 text-xs text-fg-muted">
          Set daily active-time targets. The daemon notifies you at 50% and 100% progress, and reminds you to take a
          break after long focused stretches.
        </p>

        <div className="mb-4 space-y-2">
          <AnimatePresence initial={false} mode="popLayout">
            {goals.map((goal, i) => (
              <AnimatedRow key={goal.id ?? `new-${i}`} className="flex flex-wrap items-center gap-2">
                <input
                  type="text"
                  value={goal.name}
                  onChange={(e) => handleUpdateGoal(i, 'name', e.target.value)}
                  placeholder="Deep work"
                  aria-label={`Goal name ${i + 1}`}
                  className="input min-w-0 flex-1 px-2 py-1 text-xs"
                />
                <select
                  value={goal.target_type}
                  onChange={(e) => handleUpdateGoal(i, 'target_type', e.target.value)}
                  aria-label={`Goal target type ${i + 1}`}
                  className="input shrink-0 px-2 py-1 text-xs"
                >
                  <option value="all">All apps</option>
                  <option value="class">Specific app</option>
                </select>
                <AnimatePresence initial={false}>
                  {goal.target_type === 'class' && (
                    <motion.input
                      type="text"
                      value={goal.target_key ?? ''}
                      onChange={(e) => handleUpdateGoal(i, 'target_key', e.target.value)}
                      placeholder="kitty"
                      aria-label={`Goal target app ${i + 1}`}
                      initial={{ opacity: 0, width: 0 }}
                      animate={{ opacity: 1, width: '6rem' }}
                      exit={{ opacity: 0, width: 0 }}
                      transition={easeOut}
                      className="input shrink-0 px-2 py-1 text-xs"
                    />
                  )}
                </AnimatePresence>
                <input
                  type="number"
                  value={Math.round((goal.daily_target_ms || 0) / 3600000)}
                  onChange={(e) => handleUpdateGoal(i, 'daily_target_ms', Number(e.target.value) * 3600000)}
                  min={1}
                  aria-label={`Goal hours ${i + 1}`}
                  className="input w-16 shrink-0 px-2 py-1 text-xs"
                />
                <span className="text-xs text-fg-faint">h</span>
                <Toggle
                  checked={goal.enabled}
                  onChange={(next) => handleUpdateGoal(i, 'enabled', next)}
                  ariaLabel={`Enable goal ${i + 1}`}
                />
                <button
                  type="button"
                  onClick={() => handleDeleteGoal(i)}
                  aria-label={`Delete goal ${i + 1}`}
                  className="btn btn-ghost shrink-0 px-2 py-1 text-fg-faint hover:text-bad"
                >
                  <Trash2 size={13} />
                </button>
              </AnimatedRow>
            ))}
          </AnimatePresence>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <button type="button" onClick={handleAddGoal} className="btn">
            <Plus size={13} />
            Add Goal
          </button>
          <SaveButton onClick={handleSaveGoals} saving={savingGoals} msg={goalMsg} label="Save Goals" />
        </div>
      </Panel>
    </RevealOnScroll>
  );
}
