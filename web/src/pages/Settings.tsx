import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { format, subDays } from 'date-fns';
import {
  AlertTriangle,
  Check,
  Cpu,
  Download,
  FileText,
  FolderKanban,
  Globe,
  Key,
  Loader2,
  Plus,
  RefreshCw,
  Save,
  Server,
  ShieldCheck,
  Tags,
  Target,
  Trash2,
  Wifi,
  WifiOff,
} from 'lucide-react';
import { api } from '../lib/api';
import { Panel } from '../components/ui/Card';
import { RevealOnScroll } from '../components/ui/Reveal';
import { EmptyState, Skeleton, SkeletonPanel } from '../components/ui/Feedback';
import { easeOut, spring } from '../lib/motion';
import type { AiModelsResponse, CategoryRule, ConfigResponse, Goal, Project, ProjectRule, Session } from '../lib/types';

// Extract a human-readable message from an API error thrown by fetchJSON,
// which formats failures as "API Error: <status> <body>".
function extractApiError(e: unknown): string {
  if (e instanceof Error) {
    const idx = e.message.indexOf('{');
    if (idx >= 0) {
      try {
        const parsed = JSON.parse(e.message.slice(idx));
        if (parsed && typeof parsed.error === 'string') {
          return parsed.error;
        }
      } catch {
        /* fall through */
      }
    }
    return e.message;
  }
  return 'Unknown error';
}

/* --------------------------------------------------------------------------
   Section rail (local helper)
   -------------------------------------------------------------------------- */

const SECTIONS = [
  { id: 'status', label: 'Server', icon: Server },
  { id: 'ai', label: 'AI config', icon: Cpu },
  { id: 'categories', label: 'Categories', icon: Tags },
  { id: 'goals', label: 'Goals', icon: Target },
  { id: 'projects', label: 'Projects', icon: FolderKanban },
  { id: 'report', label: 'Weekly report', icon: FileText },
  { id: 'export', label: 'Export', icon: Download },
  { id: 'privacy', label: 'Privacy', icon: ShieldCheck },
  { id: 'danger', label: 'Danger zone', icon: Trash2 },
] as const;

type SectionId = (typeof SECTIONS)[number]['id'];

function SectionNav({
  active,
  onSelect,
  layoutId,
  className = '',
}: {
  active: SectionId;
  onSelect: (id: SectionId) => void;
  layoutId: string;
  className?: string;
}) {
  return (
    <nav aria-label="Settings sections" className={className}>
      <div className="flex gap-1 overflow-x-auto pb-0.5 lg:flex-col lg:overflow-visible lg:pb-0">
        {SECTIONS.map(({ id, label, icon: Icon }) => {
          const isActive = active === id;
          return (
            <button
              key={id}
              type="button"
              onClick={() => onSelect(id)}
              aria-current={isActive ? 'true' : undefined}
              className={`relative flex shrink-0 items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors ${
                isActive ? 'text-fg' : 'text-fg-muted hover:text-fg'
              }`}
            >
              {isActive && (
                <motion.span
                  layoutId={layoutId}
                  className="absolute inset-0 rounded-lg bg-accent/10 ring-1 ring-inset ring-accent/25"
                  transition={spring}
                />
              )}
              <Icon size={15} className={`relative z-10 shrink-0 ${isActive ? 'text-accent' : 'text-fg-faint'}`} />
              <span className="relative z-10 whitespace-nowrap">{label}</span>
            </button>
          );
        })}
      </div>
    </nav>
  );
}

/* --------------------------------------------------------------------------
   Small local primitives (kept in this file: they are Settings-only)
   -------------------------------------------------------------------------- */

/** Spinner that animates through framer-motion (survives reduced-motion CSS). */
function Spinner({ size = 13 }: { size?: number }) {
  return (
    <motion.span
      animate={{ rotate: 360 }}
      transition={{ repeat: Infinity, duration: 0.9, ease: 'linear' }}
      className="inline-flex"
    >
      <Loader2 size={size} />
    </motion.span>
  );
}

/** Animated switch used for every boolean field on this page. */
function Toggle({
  checked,
  onChange,
  label,
  hint,
  ariaLabel,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label?: string;
  hint?: string;
  ariaLabel?: string;
}) {
  const track = (
    <span
      className={`flex h-5 w-9 shrink-0 items-center rounded-full border transition-colors ${
        checked ? 'justify-end border-accent/40 bg-accent/25 pr-[0.15rem]' : 'justify-start border-line bg-surface-3 pl-[0.15rem]'
      }`}
    >
      <motion.span
        layout
        transition={spring}
        className={`block h-3.5 w-3.5 rounded-full ${checked ? 'bg-accent' : 'bg-fg-faint'}`}
      />
    </span>
  );

  if (!label) {
    return (
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={ariaLabel}
        onClick={() => onChange(!checked)}
        className="inline-flex shrink-0 items-center"
      >
        {track}
      </button>
    );
  }

  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex w-full items-start gap-3 rounded-lg p-1 text-left transition-colors hover:bg-surface-2/60"
    >
      {track}
      <span className="min-w-0">
        <span className="block text-sm text-fg">{label}</span>
        {hint && <span className="mt-0.5 block text-xs text-fg-faint">{hint}</span>}
      </span>
    </button>
  );
}

/**
 * Save button with an animated state machine: idle → saving (spinner) →
 * saved (checkmark chip that fades back after 1.5 s) → error (bad chip).
 * It reads the page's existing `saving` / message state, so no API call is
 * added and no extra request is made.
 */
function SaveButton({
  onClick,
  saving,
  msg,
  label,
  size = 'md',
}: {
  onClick: () => void;
  saving: boolean;
  msg: string;
  label: string;
  size?: 'sm' | 'md';
}) {
  const [feedback, setFeedback] = useState<{ kind: 'saved' | 'error'; text: string } | null>(null);
  const wasSaving = useRef(false);
  const iconSize = size === 'sm' ? 12 : 14;

  useEffect(() => {
    if (wasSaving.current && !saving) {
      if (msg === 'Saved') setFeedback({ kind: 'saved', text: 'Saved' });
      else if (msg) setFeedback({ kind: 'error', text: msg });
      else setFeedback(null);
    }
    wasSaving.current = saving;
  }, [saving, msg]);

  useEffect(() => {
    if (feedback?.kind !== 'saved') return;
    const timer = window.setTimeout(() => setFeedback(null), 1500);
    return () => window.clearTimeout(timer);
  }, [feedback]);

  const state = saving ? 'saving' : feedback?.kind === 'saved' ? 'saved' : 'idle';

  return (
    <div className="flex flex-wrap items-center gap-3">
      <button
        type="button"
        onClick={onClick}
        disabled={saving}
        className={`btn btn-accent ${size === 'sm' ? 'px-3 py-1.5 text-xs' : 'px-4 py-2'}`}
      >
        <AnimatePresence mode="wait" initial={false}>
          <motion.span
            key={state}
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={easeOut}
            className="inline-flex items-center gap-1.5"
          >
            {state === 'saving' ? (
              <>
                <Spinner size={iconSize} />
                Saving...
              </>
            ) : state === 'saved' ? (
              <>
                <Check size={iconSize} />
                {label}
              </>
            ) : (
              <>
                <Save size={iconSize} />
                {label}
              </>
            )}
          </motion.span>
        </AnimatePresence>
      </button>
      <AnimatePresence initial={false}>
        {feedback && (
          <motion.span
            key={feedback.kind}
            initial={{ opacity: 0, x: -8 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -8 }}
            transition={easeOut}
            className={feedback.kind === 'saved' ? 'chip-good' : 'chip-bad'}
          >
            {feedback.kind === 'saved' ? <Check size={12} /> : <AlertTriangle size={12} />}
            {feedback.text}
          </motion.span>
        )}
      </AnimatePresence>
    </div>
  );
}

/** Row of an editable list: animates in, reflows on reorder, animates out. */
function AnimatedRow({ children, className = '' }: { children: ReactNode; className?: string }) {
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

export default function Settings() {
  const [status, setStatus] = useState<'online' | 'offline' | 'checking'>('checking');
  const [version, setVersion] = useState('');
  const [aiInfo, setAiInfo] = useState<AiModelsResponse | null>(null);
  const [config, setConfig] = useState<ConfigResponse | null>(null);
  const [exporting, setExporting] = useState(false);
  const [booted, setBooted] = useState(false);
  const [aiLoaded, setAiLoaded] = useState(false);

  const [openaiUrl, setOpenaiUrl] = useState('');
  const [openaiKey, setOpenaiKey] = useState('');
  const [openaiModel, setOpenaiModel] = useState('');
  const [ollamaUrl, setOllamaUrl] = useState('');
  const [ollamaModel, setOllamaModel] = useState('');
  const [rebuilding, setRebuilding] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState('');

  const [categoryRules, setCategoryRules] = useState<CategoryRule[]>([]);
  const [categoryNames, setCategoryNames] = useState<string[]>([]);
  const [savingCategories, setSavingCategories] = useState(false);
  const [categoryMsg, setCategoryMsg] = useState('');

  const [goals, setGoals] = useState<Goal[]>([]);
  const [savingGoals, setSavingGoals] = useState(false);
  const [goalMsg, setGoalMsg] = useState('');

  const [recordTitles, setRecordTitles] = useState(true);
  const [savingTitles, setSavingTitles] = useState(false);
  const [titlesMsg, setTitlesMsg] = useState('');

  const [retentionDays, setRetentionDays] = useState(0);
  const days7Ago = format(subDays(new Date(), 7), 'yyyy-MM-dd');
  const today = format(new Date(), 'yyyy-MM-dd');
  const [deleteFrom, setDeleteFrom] = useState(days7Ago);
  const [deleteTo, setDeleteTo] = useState(today);
  const [deleteClass, setDeleteClass] = useState('');
  const [deleting, setDeleting] = useState(false);
  const [deleteMsg, setDeleteMsg] = useState('');

  const [projects, setProjects] = useState<Project[]>([]);
  const [projectRules, setProjectRules] = useState<ProjectRule[]>([]);
  const [savingProjects, setSavingProjects] = useState(false);
  const [projectMsg, setProjectMsg] = useState('');

  const [weeklyEnabled, setWeeklyEnabled] = useState(false);
  const [weeklyDay, setWeeklyDay] = useState(1);
  const [weeklyHour, setWeeklyHour] = useState(9);
  const [weeklyMinute, setWeeklyMinute] = useState(0);
  const [savingWeekly, setSavingWeekly] = useState(false);
  const [weeklyMsg, setWeeklyMsg] = useState('');

  // Section rail: one ref per section, kept stable so scrolling only reads.
  const [activeSection, setActiveSection] = useState<SectionId>('status');
  const sectionRefs = useRef<Partial<Record<SectionId, HTMLElement | null>>>({});
  const setSectionRef = useMemo(() => {
    const map = {} as Record<SectionId, (el: HTMLElement | null) => void>;
    for (const s of SECTIONS) {
      map[s.id] = (el) => {
        sectionRefs.current[s.id] = el;
      };
    }
    return map;
  }, []);

  useEffect(() => {
    api.health()
      .then((res) => {
        setStatus('online');
        setVersion(res.version || '');
      })
      .catch(() => setStatus('offline'));

    api.aiModels()
      .then((res) => setAiInfo(res))
      .catch(() => {})
      .finally(() => setAiLoaded(true));

    api.getConfig()
      .then((c) => {
        setConfig(c);
        setOpenaiUrl(c.openai_url);
        setOpenaiModel(c.openai_model);
        setOllamaUrl(c.ollama_url);
        setOllamaModel(c.ollama_model);
        setRecordTitles(c.record_titles ?? true);
        setRetentionDays(c.retention_days ?? 0);
        setWeeklyEnabled(c.weekly_report_enabled);
        setWeeklyDay(c.weekly_report_day);
        setWeeklyHour(c.weekly_report_hour);
        setWeeklyMinute(c.weekly_report_minute);
      })
      .catch(() => {})
      .finally(() => setBooted(true));

    api.categories()
      .then((res) => {
        setCategoryRules(res.rules);
        setCategoryNames(res.categories);
      })
      .catch(() => {});

    api.goals()
      .then((res) => setGoals(res.goals))
      .catch(() => {});

    api.projects()
      .then((res) => {
        setProjects(res.projects);
        setProjectRules(res.rules);
      })
      .catch(() => {});
  }, []);

  // Highlight whichever section sits at the top of the scrollport. The page
  // scrolls inside Layout's <main>, so listen in the capture phase instead of
  // assuming window scrolling.
  useEffect(() => {
    const compute = () => {
      let current: SectionId = SECTIONS[0].id;
      for (const s of SECTIONS) {
        const el = sectionRefs.current[s.id];
        if (el && el.getBoundingClientRect().top <= 160) current = s.id;
      }
      setActiveSection((prev) => (prev === current ? prev : current));
    };
    compute();
    document.addEventListener('scroll', compute, true);
    window.addEventListener('resize', compute);
    return () => {
      document.removeEventListener('scroll', compute, true);
      window.removeEventListener('resize', compute);
    };
  }, []);

  const scrollToSection = (id: SectionId) => {
    setActiveSection(id);
    const el = sectionRefs.current[id];
    if (!el) return;
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  const handleSaveConfig = async () => {
    setSaving(true);
    setSaveMsg('');
    try {
      await api.updateConfig({
        openai_url: openaiUrl,
        openai_api_key: openaiKey || undefined,
        openai_model: openaiModel,
        ollama_url: ollamaUrl,
        ollama_model: ollamaModel,
        retention_days: retentionDays,
      });
      setSaveMsg('Saved');
      setOpenaiKey('');
      const fresh = await api.getConfig();
      setConfig(fresh);
      setRetentionDays(fresh.retention_days ?? 0);
    } catch (e) {
      setSaveMsg('Save failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setSaving(false);
    }
  };

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
      const res = await api.deleteSessions(deleteFrom, deleteTo, cls);
      setDeleteMsg(`${res.deleted} session(s) deleted`);
    } catch (e) {
      setDeleteMsg('Delete failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setDeleting(false);
    }
  };

  const handleRebuildHourly = async () => {
    setRebuilding(true);
    try {
      await api.rebuildHourlySummary();
      alert('Hourly summary rebuilt successfully');
    } catch (e) {
      alert('Rebuild failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setRebuilding(false);
    }
  };

  const handleExport = async () => {
    setExporting(true);
    try {
      const today = format(new Date(), 'yyyy-MM-dd');
      const lastMonth = format(subDays(new Date(), 30), 'yyyy-MM-dd');

      let allSessions: Session[] = [];
      let page = 1;
      const perPage = 200;
      let totalFetched = 0;

      while (true) {
        const res = await api.sessions(lastMonth, today, page, perPage);
        allSessions = allSessions.concat(res.data);
        totalFetched += res.data.length;
        if (totalFetched >= res.total || res.data.length === 0) break;
        page++;
      }

      // RFC 4180: quote every field and double any quote inside it. The
      // previous version quoted `title` and `class` but only escaped quotes in
      // `title`, so a class containing `"` broke the row; every other field was
      // left bare, so a comma in a workspace name or window title silently
      // shifted the columns. Quoting also keeps embedded newlines inside a
      // field instead of starting a new record.
      //
      // The leading `'` is formula-injection defence: a field starting with
      // `=`, `+`, `-` or `@` is evaluated as a formula when the file is opened
      // in Excel, LibreOffice or Sheets. Window titles are attacker-controlled
      // — any web page chooses its own — so this is a real path, not hygiene.
      //
      // `||` is replaced by `??` throughout: a duration of 0, or a workspace
      // literally named "0", is data rather than a missing value.
      const cell = (v: unknown): string => {
        const raw = v === null || v === undefined ? '' : String(v);
        const neutralised = /^[=+\-@\t\r]/.test(raw) ? `'${raw}` : raw;
        return `"${neutralised.replace(/"/g, '""')}"`;
      };
      const columns = ['ID', 'Class', 'Title', 'Workspace', 'Started At', 'Ended At', 'Duration (ms)', 'Activity State', 'Focus (ms)'];
      const rows = allSessions.map((s) =>
        [
          s.id,
          s.class,
          s.title,
          s.workspace ?? '',
          s.started_at,
          s.ended_at ?? '',
          s.duration_ms ?? '',
          s.activity_state ?? '',
          s.focused_ms ?? '',
        ]
          .map(cell)
          .join(',')
      );
      // CRLF is what RFC 4180 specifies and what Excel expects. The BOM is
      // what stops Excel from decoding the file as the local codepage, which
      // turns any non-ASCII window title into mojibake.
      const csv = '\uFEFF' + [columns.map(cell).join(','), ...rows].join('\r\n');

      const blob = new Blob([csv], { type: 'text/csv' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `hyprtrace-export-${today}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      alert('Export failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setExporting(false);
    }
  };

  const handleAddCategory = () => {
    setCategoryRules([...categoryRules, { pattern: '', category: 'other', priority: 0 }]);
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
      await api.putCategories(rules);
      const fresh = await api.categories();
      setCategoryRules(fresh.rules);
      setCategoryMsg('Saved');
    } catch (e) {
      setCategoryMsg('Save failed');
    } finally {
      setSavingCategories(false);
    }
  };

  const handleAddGoal = () => {
    setGoals([...goals, { name: '', target_type: 'all', target_key: '', daily_target_ms: 4 * 3600000, enabled: true }]);
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
      await api.putGoals(list);
      setGoalMsg('Saved');
    } catch (e) {
      setGoalMsg('Save failed');
    } finally {
      setSavingGoals(false);
    }
  };

  const handleAddProject = () => {
    setProjects([...projects, { id: undefined, name: '', color: '#22d3ee', sort_order: projects.length }]);
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
      { id: undefined, project_id: first.id ?? -1, pattern: '', priority: 0 },
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
      const projs = projects
        .map((p, i) => ({ ...p, sort_order: i }))
        .filter((p) => p.name.trim());
      const rules = projectRules.filter((r) => r.pattern.trim());

      const usesTempIds = rules.some((r) => r.project_id < 0);
      let saved: { status: string; projects: Project[]; rules: ProjectRule[] };

      if (usesTempIds) {
        // Persist the projects first to obtain their real ids.
        const first = await api.putProjects(projs, []);
        const nameToId = new Map<string, number>();
        first.projects.forEach((p) => {
          if (p.id != null) nameToId.set(p.name, p.id);
        });

        // Resolve every rule's project to the newly-saved id. Negative temp
        // ids are an index into `projects` (-1 → projects[0], ...); existing
        // ids are looked up by name because a full replace renumbers rows.
        const finalRules = rules.map((r) => {
          let ref: Project | undefined;
          if (r.project_id < 0) {
            const idx = -r.project_id - 1;
            ref = idx >= 0 && idx < projects.length ? projects[idx] : undefined;
          } else {
            ref = projects.find((p) => p.id === r.project_id);
          }
          // Prefer the freshly-saved id (by name); fall back to any id we
          // already have, else leave the rule's id as-is for the server to drop.
          const realId = ref ? nameToId.get(ref.name) ?? ref.id : undefined;
          return { ...r, project_id: realId ?? r.project_id };
        });

        // Save again with the persisted projects (carrying real ids) so the
        // remapped rules resolve against the projects that actually exist.
        saved = await api.putProjects(first.projects, finalRules);
      } else {
        saved = await api.putProjects(projs, rules);
      }

      setProjects(saved.projects);
      setProjectRules(saved.rules);
      setProjectMsg('Saved');
    } catch (e) {
      setProjectMsg('Save failed: ' + extractApiError(e));
    } finally {
      setSavingProjects(false);
    }
  };

  const handleSaveWeekly = async () => {
    setSavingWeekly(true);
    setWeeklyMsg('');
    try {
      // Only send the weekly report fields so unsaved AI edits are not clobbered.
      await api.updateConfig({
        weekly_report_enabled: weeklyEnabled,
        weekly_report_day: weeklyDay,
        weekly_report_hour: weeklyHour,
        weekly_report_minute: weeklyMinute,
      });
      setWeeklyMsg('Saved');
      const fresh = await api.getConfig();
      setConfig(fresh);
    } catch (e) {
      setWeeklyMsg('Save failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setSavingWeekly(false);
    }
  };

  const handleSaveTitles = async () => {
    setSavingTitles(true);
    setTitlesMsg('');
    try {
      await api.updateConfig({ record_titles: recordTitles });
      setTitlesMsg('Saved');
      const fresh = await api.getConfig();
      setConfig(fresh);
      setRecordTitles(fresh.record_titles ?? true);
    } catch (e) {
      setTitlesMsg('Save failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setSavingTitles(false);
    }
  };

  const aiProviderNames = aiInfo ? Object.keys(aiInfo.providers) : [];
  const deleteFailed =
    deleteMsg === 'Delete failed' ||
    deleteMsg.startsWith('Delete failed') ||
    deleteMsg === 'Please provide both dates';

  if (!booted) {
    return (
      <div className="max-w-3xl space-y-6">
        <div className="space-y-2">
          <Skeleton className="h-6 w-32" />
          <Skeleton className="h-4 w-72" />
        </div>
        <SkeletonPanel height="h-24" />
        <SkeletonPanel height="h-56" />
        <SkeletonPanel height="h-40" />
      </div>
    );
  }

  return (
    <div className="max-w-5xl">
      <header className="mb-6">
        <h2 className="text-xl font-bold">Settings</h2>
        <p className="mt-1 text-sm text-fg-muted">
          AI providers, categories, projects, goals, reporting and local data controls.
        </p>
      </header>

      {/* Narrow screens: sticky top tabs. Desktop: sticky left rail (below). */}
      <SectionNav
        active={activeSection}
        onSelect={scrollToSection}
        layoutId="settings-tabs-pill"
        className="sticky top-0 z-20 mb-6 border-b border-line bg-bg/85 py-2 backdrop-blur-xl lg:hidden"
      />

      <div className="grid gap-6 lg:grid-cols-[12.5rem_minmax(0,1fr)] lg:gap-8">
        <SectionNav
          active={activeSection}
          onSelect={scrollToSection}
          layoutId="settings-rail-pill"
          className="hidden lg:sticky lg:top-4 lg:block lg:self-start"
        />

        <div className="min-w-0 space-y-6">
          {/* ---------------------------------------------------------- Server */}
          <div ref={setSectionRef.status} className="scroll-mt-16 lg:scroll-mt-6">
            <RevealOnScroll>
              <Panel
                title="Server & database"
                icon={<Server size={15} className="text-accent" />}
                hint="Local daemon and SQLite store"
              >
                <div className="space-y-4">
                  <div className="flex flex-wrap items-center gap-3">
                    {status === 'online' ? (
                      <>
                        <span className="grid h-9 w-9 place-items-center rounded-xl bg-good/10 text-good">
                          <Wifi size={17} />
                        </span>
                        <div>
                          <p className="text-sm font-medium text-good">Online</p>
                          <p className="text-xs text-fg-faint">{version ? `v${version}` : 'version unknown'}</p>
                        </div>
                      </>
                    ) : status === 'offline' ? (
                      <>
                        <span className="grid h-9 w-9 place-items-center rounded-xl bg-bad/10 text-bad">
                          <WifiOff size={17} />
                        </span>
                        <div>
                          <p className="text-sm font-medium text-bad">Offline</p>
                          <p className="text-xs text-fg-faint">The daemon is not responding</p>
                        </div>
                      </>
                    ) : (
                      <>
                        <span className="grid h-9 w-9 place-items-center rounded-xl bg-surface-2 text-fg-faint">
                          <Spinner size={16} />
                        </span>
                        <p className="text-sm text-fg-muted">Checking...</p>
                      </>
                    )}
                  </div>

                  <div className="divider" />

                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <div>
                      <p className="text-xs text-fg-muted">Database</p>
                      <p className="mt-0.5 font-mono text-xs text-fg-faint">~/.local/share/hyprtrace/hyprtrace.db</p>
                    </div>
                    <button type="button" onClick={handleRebuildHourly} disabled={rebuilding} className="btn">
                      {rebuilding ? <Spinner /> : <RefreshCw size={13} />}
                      {rebuilding ? 'Rebuilding...' : 'Rebuild Hourly Summary'}
                    </button>
                  </div>
                </div>
              </Panel>
            </RevealOnScroll>
          </div>

          {/* ------------------------------------------------------- AI config */}
          <div ref={setSectionRef.ai} className="scroll-mt-16 space-y-6 lg:scroll-mt-6">
            <RevealOnScroll>
              <Panel
                title="AI providers"
                icon={<Cpu size={15} className="text-accent" />}
                hint="Available to AI Chat"
              >
                {aiInfo ? (
                  <div className="space-y-3">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="chip-accent">{aiInfo.default}</span>
                      <span className="text-xs text-fg-faint">default provider</span>
                    </div>
                    {aiProviderNames.length > 0 ? (
                      <ul className="space-y-1.5">
                        {aiProviderNames.map((name) => (
                          <li
                            key={name}
                            className="flex items-center justify-between gap-3 rounded-lg border border-line bg-surface-2/60 px-3 py-2"
                          >
                            <span className="text-sm text-fg">{name}</span>
                            <span className="chip-neutral tnum">{aiInfo.providers[name]?.length || 0} models</span>
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p className="text-sm text-fg-muted">No AI providers available</p>
                    )}
                  </div>
                ) : !aiLoaded ? (
                  <div className="space-y-2">
                    <Skeleton className="h-4 w-44" />
                    <Skeleton className="h-10 w-full" />
                  </div>
                ) : (
                  <p className="text-sm text-fg-muted">No AI providers available</p>
                )}
              </Panel>
            </RevealOnScroll>

            <RevealOnScroll delay={0.05}>
              <Panel
                title="API configuration"
                icon={<Key size={15} className="text-accent-2" />}
                hint="Stored in the local config file"
              >
                <div className="space-y-4">
                  <div className="rounded-xl border border-line bg-surface-2/60 p-4">
                    <h4 className="mb-3 flex items-center gap-2 text-xs font-medium text-accent">
                      <Cpu size={13} /> OpenAI Compatible
                    </h4>
                    {config?.openai_configured && (
                      <span className="chip-good mb-3">
                        <Check size={12} /> Configured
                      </span>
                    )}

                    <div className="space-y-3">
                      <label className="block">
                        <span className="mb-1 flex items-center gap-1 text-xs text-fg-muted">
                          <Globe size={12} /> API Base URL
                        </span>
                        <input
                          type="text"
                          value={openaiUrl}
                          onChange={(e) => setOpenaiUrl(e.target.value)}
                          placeholder="https://api.openai.com/v1"
                          className="input w-full"
                        />
                      </label>

                      <label className="block">
                        <span className="mb-1 flex items-center gap-1 text-xs text-fg-muted">
                          <Key size={12} /> API Key
                        </span>
                        <input
                          type="password"
                          value={openaiKey}
                          onChange={(e) => setOpenaiKey(e.target.value)}
                          placeholder={config?.openai_configured ? '•••••••• (leave blank to keep current)' : 'sk-...'}
                          className="input w-full"
                        />
                      </label>

                      <label className="block">
                        <span className="mb-1 flex items-center gap-1 text-xs text-fg-muted">
                          <Cpu size={12} /> Model
                        </span>
                        <input
                          type="text"
                          value={openaiModel}
                          onChange={(e) => setOpenaiModel(e.target.value)}
                          placeholder="gpt-4o-mini"
                          list="openai-model-list"
                          className="input w-full"
                        />
                      </label>
                      <datalist id="openai-model-list">
                        {(aiInfo?.providers?.openai ?? []).map((m) => (
                          <option key={m} value={m} />
                        ))}
                      </datalist>
                    </div>
                  </div>

                  <div className="rounded-xl border border-line bg-surface-2/60 p-4">
                    <h4 className="mb-3 flex items-center gap-2 text-xs font-medium text-accent-2">
                      <Cpu size={13} /> Ollama
                    </h4>

                    <div className="space-y-3">
                      <label className="block">
                        <span className="mb-1 flex items-center gap-1 text-xs text-fg-muted">
                          <Globe size={12} /> API Base URL
                        </span>
                        <input
                          type="text"
                          value={ollamaUrl}
                          onChange={(e) => setOllamaUrl(e.target.value)}
                          placeholder="http://localhost:11434"
                          className="input w-full"
                        />
                      </label>

                      <label className="block">
                        <span className="mb-1 flex items-center gap-1 text-xs text-fg-muted">
                          <Cpu size={12} /> Model
                        </span>
                        <input
                          type="text"
                          value={ollamaModel}
                          onChange={(e) => setOllamaModel(e.target.value)}
                          placeholder="qwen2.5:7b"
                          list="ollama-model-list"
                          className="input w-full"
                        />
                      </label>
                      <datalist id="ollama-model-list">
                        {(aiInfo?.providers?.ollama ?? []).map((m) => (
                          <option key={m} value={m} />
                        ))}
                      </datalist>
                    </div>
                  </div>

                  <SaveButton onClick={handleSaveConfig} saving={saving} msg={saveMsg} label="Save Config" />
                </div>
              </Panel>
            </RevealOnScroll>
          </div>

          {/* ----------------------------------------------------- Categories */}
          <div ref={setSectionRef.categories} className="scroll-mt-16 lg:scroll-mt-6">
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
          </div>

          {/* ---------------------------------------------------------- Goals */}
          <div ref={setSectionRef.goals} className="scroll-mt-16 lg:scroll-mt-6">
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
          </div>

          {/* ------------------------------------------------------- Projects */}
          <div ref={setSectionRef.projects} className="scroll-mt-16 lg:scroll-mt-6">
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
          </div>

          {/* --------------------------------------------------- Weekly report */}
          <div ref={setSectionRef.report} className="scroll-mt-16 lg:scroll-mt-6">
            <RevealOnScroll>
              <Panel
                title="Weekly report"
                icon={<FileText size={15} className="text-accent" />}
                hint="Markdown summary every week"
              >
                <p className="mb-4 text-xs text-fg-muted">
                  Generate a Markdown report of the last 7 days on a chosen weekday and send a desktop notification.
                </p>

                <div className="space-y-4">
                  <Toggle
                    checked={weeklyEnabled}
                    onChange={setWeeklyEnabled}
                    label="Enable weekly report"
                  />

                  <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                    <label className="block">
                      <span className="mb-1 block text-xs text-fg-muted">Day</span>
                      <select
                        value={weeklyDay}
                        onChange={(e) => setWeeklyDay(Number(e.target.value))}
                        className="input w-full"
                      >
                        <option value={1}>Monday</option>
                        <option value={2}>Tuesday</option>
                        <option value={3}>Wednesday</option>
                        <option value={4}>Thursday</option>
                        <option value={5}>Friday</option>
                        <option value={6}>Saturday</option>
                        <option value={7}>Sunday</option>
                      </select>
                    </label>
                    <label className="block">
                      <span className="mb-1 block text-xs text-fg-muted">Hour (0-23)</span>
                      <input
                        type="number"
                        min={0}
                        max={23}
                        value={weeklyHour}
                        onChange={(e) => setWeeklyHour(Math.max(0, Math.min(23, Number(e.target.value))))}
                        className="input w-full"
                      />
                    </label>
                    <label className="block">
                      <span className="mb-1 block text-xs text-fg-muted">Minute (0-59)</span>
                      <input
                        type="number"
                        min={0}
                        max={59}
                        value={weeklyMinute}
                        onChange={(e) => setWeeklyMinute(Math.max(0, Math.min(59, Number(e.target.value))))}
                        className="input w-full"
                      />
                    </label>
                  </div>

                  <SaveButton
                    onClick={handleSaveWeekly}
                    saving={savingWeekly}
                    msg={weeklyMsg}
                    label="Save weekly report settings"
                  />
                </div>
              </Panel>
            </RevealOnScroll>
          </div>

          {/* --------------------------------------------------------- Export */}
          <div ref={setSectionRef.export} className="scroll-mt-16 lg:scroll-mt-6">
            <RevealOnScroll>
              <Panel
                title="Data export"
                icon={<Download size={15} className="text-accent-2" />}
                hint="CSV and Markdown"
              >
                <div className="flex flex-wrap gap-2">
                  <button type="button" onClick={handleExport} disabled={exporting} className="btn">
                    {exporting ? <Spinner size={14} /> : <Download size={14} />}
                    {exporting ? 'Exporting...' : 'Export Sessions (CSV)'}
                  </button>
                  <button
                    type="button"
                    onClick={async () => {
                      const today = format(new Date(), 'yyyy-MM-dd');
                      const weekAgo = format(subDays(new Date(), 6), 'yyyy-MM-dd');
                      try { await api.report(weekAgo, today); }
                      catch (e) { alert('Report failed'); }
                    }}
                    className="btn"
                  >
                    <FileText size={14} />
                    Download Weekly Report (MD)
                  </button>
                </div>
              </Panel>
            </RevealOnScroll>
          </div>

          {/* -------------------------------------------------------- Privacy */}
          <div ref={setSectionRef.privacy} className="scroll-mt-16 lg:scroll-mt-6">
            <RevealOnScroll>
              <Panel
                title="Privacy & retention"
                icon={<ShieldCheck size={15} className="text-good" />}
                hint="Stays on this machine"
              >
                <div className="space-y-4">
                  <div>
                    <Toggle
                      checked={recordTitles}
                      onChange={setRecordTitles}
                      label="Record window titles"
                      hint="When disabled, new sessions store no window title (only the app class is kept). Takes effect after the daemon restarts."
                    />
                    <div className="mt-3">
                      <SaveButton
                        size="sm"
                        onClick={handleSaveTitles}
                        saving={savingTitles}
                        msg={titlesMsg}
                        label="Save privacy settings"
                      />
                    </div>
                  </div>

                  <div className="divider" />

                  <div>
                    <label className="block">
                      <span className="mb-1 block text-xs text-fg-muted">Retention days</span>
                      <div className="flex items-center gap-2">
                        <input
                          type="number"
                          min={0}
                          value={retentionDays}
                          onChange={(e) => setRetentionDays(Math.max(0, Number(e.target.value)))}
                          className="input w-24"
                        />
                        <span className="text-xs text-fg-faint">0 = keep forever</span>
                      </div>
                    </label>
                    <p className="mt-1 text-xs text-fg-faint">
                      Sessions older than this many days are deleted automatically (saved with the config above).
                    </p>
                  </div>
                </div>
              </Panel>
            </RevealOnScroll>
          </div>

          {/* --------------------------------------------------- Danger zone */}
          <div ref={setSectionRef.danger} className="scroll-mt-16 lg:scroll-mt-6">
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
          </div>
        </div>
      </div>
    </div>
  );
}
