import { useEffect, useState } from 'react';
import { AnimatePresence } from 'framer-motion';
import { Plus, Tags, Trash2 } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { AnimatedRow } from '../../components/ui/AnimatedRow';
import { SaveButton } from '../../components/ui/Controls';
import { useCategories, useSaveCategories } from '../../lib/queries';
import type { CategoryRule } from '../../lib/types';

/** App categories: an editable list of SQL LIKE class-name rules. */
export default function CategoriesSection() {
  const categories = useCategories();
  const saveCategories = useSaveCategories();
  const [categoryRules, setCategoryRules] = useState<CategoryRule[]>([]);
  const [savingCategories, setSavingCategories] = useState(false);
  const [categoryMsg, setCategoryMsg] = useState('');

  // The draft starts empty and is replaced by the stored rules when they
  // arrive — and again after a save, because `PUT /api/categories` answers with
  // a bare status and the section re-reads the list (server-assigned ids).
  const storedRules = categories.data?.rules;
  useEffect(() => {
    if (storedRules) setCategoryRules(storedRules);
  }, [storedRules]);

  const categoryNames = categories.data?.categories ?? [];

  const handleAddCategory = () => {
    setCategoryRules([...categoryRules, { id: null, pattern: '', category: 'other', priority: 0 }]);
  };

  const handleDeleteCategory = (idx: number) => {
    setCategoryRules(categoryRules.filter((_, i) => i !== idx));
  };

  const handleUpdateCategory = (idx: number, field: 'pattern' | 'category', value: string) => {
    setCategoryRules(categoryRules.map((r, i) => (i === idx ? { ...r, [field]: value } : r)));
  };

  const handleSaveCategories = async () => {
    setSavingCategories(true);
    setCategoryMsg('');
    try {
      const rules = categoryRules.filter((r) => r.pattern.trim());
      await saveCategories.mutateAsync(rules);
      setCategoryMsg('Saved');
    } catch (e) {
      setCategoryMsg('Save failed');
    } finally {
      setSavingCategories(false);
    }
  };

  return (
    <RevealOnScroll>
      <Panel
        title="App categories"
        icon={<Tags size={15} className="text-accent" />}
        hint="SQL LIKE patterns"
      >
        <p className="mb-4 text-xs text-fg-muted">
          Classify apps by class name pattern (SQL LIKE: % matches anything). Higher items take priority.
        </p>

        <div className="mb-4 space-y-2">
          <AnimatePresence initial={false} mode="popLayout">
            {categoryRules.map((rule, i) => (
              <AnimatedRow key={rule.id ?? `new-${i}`} className="flex items-center gap-2">
                <input
                  type="text"
                  value={rule.pattern}
                  onChange={(e) => handleUpdateCategory(i, 'pattern', e.target.value)}
                  placeholder="kitty"
                  aria-label={`Category pattern ${i + 1}`}
                  className="input min-w-0 flex-1 px-2 py-1 text-xs"
                />
                <select
                  value={rule.category}
                  onChange={(e) => handleUpdateCategory(i, 'category', e.target.value)}
                  aria-label={`Category ${i + 1}`}
                  className="input shrink-0 px-2 py-1 text-xs"
                >
                  {categoryNames.map((c) => (
                    <option key={c} value={c}>{c}</option>
                  ))}
                </select>
                <button
                  type="button"
                  onClick={() => handleDeleteCategory(i)}
                  aria-label={`Delete category rule ${i + 1}`}
                  className="btn btn-ghost shrink-0 px-2 py-1 text-fg-faint hover:text-bad"
                >
                  <Trash2 size={13} />
                </button>
              </AnimatedRow>
            ))}
          </AnimatePresence>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <button type="button" onClick={handleAddCategory} className="btn">
            <Plus size={13} />
            Add Rule
          </button>
          <SaveButton
            onClick={handleSaveCategories}
            saving={savingCategories}
            msg={categoryMsg}
            label="Save Categories"
          />
        </div>
      </Panel>
    </RevealOnScroll>
  );
}
