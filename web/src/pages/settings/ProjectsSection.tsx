import { useEffect, useState } from 'react';
import { AnimatePresence } from 'framer-motion';
import { FolderKanban, Plus, Trash2 } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { EmptyState } from '../../components/ui/Feedback';
import { AnimatedRow } from '../../components/ui/AnimatedRow';
import { SaveButton } from '../../components/ui/Controls';
import { extractApiError } from '../../lib/api';
import { useProjects, useSaveProjects } from '../../lib/queries';
import type { Project, ProjectRule } from '../../lib/types';

/** Projects and the SQL LIKE rules that attribute app usage to them. */
export default function ProjectsSection() {
  const projectsQuery = useProjects();
  const saveProjects = useSaveProjects();
  const [projects, setProjects] = useState<Project[]>([]);
  const [projectRules, setProjectRules] = useState<ProjectRule[]>([]);
  const [savingProjects, setSavingProjects] = useState(false);
  const [projectMsg, setProjectMsg] = useState('');

  // Seed the drafts when the stored projects arrive; the mutation below refreshes
  // the same cache entry from its own response, which re-seeds them the same way
  // the old handler did with `saved.projects` / `saved.rules`.
  const stored = projectsQuery.data;
  useEffect(() => {
    if (!stored) return;
    setProjects(stored.projects);
    setProjectRules(stored.rules);
  }, [stored]);

  const handleAddProject = () => {
    setProjects([...projects, { id: null, name: '', color: '#22d3ee', sort_order: projects.length }]);
  };

  const handleUpdateProject = (idx: number, field: 'name' | 'color', value: string) => {
    setProjects(projects.map((p, i) => (i === idx ? { ...p, [field]: value } : p)));
  };

  const handleDeleteProject = (idx: number) => {
    const target = projects[idx];
    const remaining = projects.filter((_, i) => i !== idx);
    setProjects(remaining);
    // Drop rules that reference the removed project (by real id or temp id).
    const removedIds = new Set<number>();
    if (target?.id != null) removedIds.add(target.id);
    removedIds.add(-(idx + 1));
    setProjectRules(projectRules.filter((r) => !removedIds.has(r.project_id)));
  };

  const handleAddProjectRule = () => {
    const first = projects[0];
    if (!first) return;
    // Use a negative temp id for unsaved projects: -1 → projects[0], -2 → projects[1], ...
    setProjectRules([
      ...projectRules,
      { id: null, project_id: first.id ?? -1, pattern: '', priority: 0 },
    ]);
  };

  const handleUpdateProjectRule = (idx: number, field: 'project_id' | 'pattern' | 'priority', value: string) => {
    setProjectRules(projectRules.map((r, i) =>
      i === idx
        ? { ...r, [field]: field === 'project_id' || field === 'priority' ? Number(value) : value }
        : r
    ));
  };

  const handleDeleteProjectRule = (idx: number) => {
    setProjectRules(projectRules.filter((_, i) => i !== idx));
  };

  // Stable select value for a project: real id when saved, negative temp id otherwise.
  const projectSelectValue = (p: Project, i: number): number => p.id ?? -(i + 1);

  const handleSaveProjects = async () => {
    setSavingProjects(true);
    setProjectMsg('');
    try {
      // The mutation filters the drafts, resolves temp project ids and answers
      // with what the server stored — the same `saved` pair the old handler
      // put back into state.
      const saved = await saveProjects.mutateAsync({ projects, rules: projectRules });
      setProjects(saved.projects);
      setProjectRules(saved.rules);
      setProjectMsg('Saved');
    } catch (e) {
      setProjectMsg('Save failed: ' + extractApiError(e));
    } finally {
      setSavingProjects(false);
    }
  };

  return (
    <RevealOnScroll>
      <Panel
        title="Projects"
        icon={<FolderKanban size={15} className="text-accent-2" />}
        hint="Attribute usage to your own projects"
      >
        <p className="mb-4 text-xs text-fg-muted">
          Attribute app usage to user-defined projects (e.g. 课设, Open Source). Rules use SQL LIKE patterns (% matches
          anything); higher priority wins.
        </p>

        <div className="mb-4 space-y-2">
          <AnimatePresence initial={false} mode="popLayout">
            {projects.map((project, i) => (
              <AnimatedRow key={project.id ?? `new-${i}`} className="flex items-center gap-2">
                <input
                  type="color"
                  value={project.color || '#22d3ee'}
                  onChange={(e) => handleUpdateProject(i, 'color', e.target.value)}
                  className="input h-8 w-9 shrink-0 p-0.5"
                  title="Project color"
                  aria-label={`Project color ${i + 1}`}
                />
                <input
                  type="text"
                  value={project.name}
                  onChange={(e) => handleUpdateProject(i, 'name', e.target.value)}
                  placeholder="课设"
                  aria-label={`Project name ${i + 1}`}
                  className="input min-w-0 flex-1 px-2 py-1 text-xs"
                />
                <button
                  type="button"
                  onClick={() => handleDeleteProject(i)}
                  className="btn btn-ghost shrink-0 px-2 py-1 text-fg-faint hover:text-bad"
                  title="Delete project"
                  aria-label={`Delete project ${i + 1}`}
                >
                  <Trash2 size={13} />
                </button>
              </AnimatedRow>
            ))}
          </AnimatePresence>
          {projects.length === 0 && (
            <EmptyState
              icon={<FolderKanban size={18} />}
              title="No projects yet"
              hint="Add one to group your app time."
            />
          )}
        </div>

        <div className="mb-4 space-y-2">
          <AnimatePresence initial={false} mode="popLayout">
            {projectRules.map((rule, i) => (
              <AnimatedRow key={rule.id ?? `new-${i}`} className="flex items-center gap-2">
                <select
                  value={rule.project_id}
                  onChange={(e) => handleUpdateProjectRule(i, 'project_id', e.target.value)}
                  aria-label={`Rule project ${i + 1}`}
                  className="input shrink-0 px-2 py-1 text-xs"
                >
                  {projects.map((p, pi) => (
                    <option key={p.id ?? `new-${pi}`} value={projectSelectValue(p, pi)}>
                      {p.name || '(unnamed)'}
                    </option>
                  ))}
                </select>
                <input
                  type="text"
                  value={rule.pattern}
                  onChange={(e) => handleUpdateProjectRule(i, 'pattern', e.target.value)}
                  placeholder="code% (app class pattern)"
                  aria-label={`Rule pattern ${i + 1}`}
                  className="input min-w-0 flex-1 px-2 py-1 text-xs"
                />
                <input
                  type="number"
                  value={rule.priority}
                  onChange={(e) => handleUpdateProjectRule(i, 'priority', e.target.value)}
                  min={0}
                  className="input w-16 shrink-0 px-2 py-1 text-xs"
                  title="Priority"
                  aria-label={`Rule priority ${i + 1}`}
                />
                <button
                  type="button"
                  onClick={() => handleDeleteProjectRule(i)}
                  className="btn btn-ghost shrink-0 px-2 py-1 text-fg-faint hover:text-bad"
                  title="Delete rule"
                  aria-label={`Delete rule ${i + 1}`}
                >
                  <Trash2 size={13} />
                </button>
              </AnimatedRow>
            ))}
          </AnimatePresence>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <button type="button" onClick={handleAddProject} className="btn">
            <Plus size={13} />
            Add Project
          </button>
          <button type="button" onClick={handleAddProjectRule} className="btn">
            <Plus size={13} />
            Add Rule
          </button>
          <SaveButton
            onClick={handleSaveProjects}
            saving={savingProjects}
            msg={projectMsg}
            label="Save Projects"
          />
        </div>
      </Panel>
    </RevealOnScroll>
  );
}
