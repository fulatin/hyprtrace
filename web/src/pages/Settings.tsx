import { useEffect, useState } from 'react';
import { api } from '../lib/api';
import { Wifi, WifiOff, Download, Save, Key, Globe, Cpu, BarChart3, Tags, Plus, Trash2, Target, FileText, FolderKanban } from 'lucide-react';
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

export default function Settings() {
  const [status, setStatus] = useState<'online' | 'offline' | 'checking'>('checking');
  const [version, setVersion] = useState('');
  const [aiInfo, setAiInfo] = useState<AiModelsResponse | null>(null);
  const [config, setConfig] = useState<ConfigResponse | null>(null);
  const [exporting, setExporting] = useState(false);

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

  const [retentionDays, setRetentionDays] = useState(0);
  const days7Ago = new Date(Date.now() - 7 * 86400000).toISOString().slice(0, 10);
  const today = new Date().toISOString().slice(0, 10);
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

  useEffect(() => {
    api.health()
      .then((res) => {
        setStatus('online');
        setVersion(res.version || '');
      })
      .catch(() => setStatus('offline'));

    api.aiModels()
      .then((res) => setAiInfo(res))
      .catch(() => {});

    api.getConfig()
      .then((c) => {
        setConfig(c);
        setOpenaiUrl(c.openai_url);
        setOpenaiModel(c.openai_model);
        setOllamaUrl(c.ollama_url);
        setOllamaModel(c.ollama_model);
        setRetentionDays(c.retention_days ?? 0);
        setWeeklyEnabled(c.weekly_report_enabled);
        setWeeklyDay(c.weekly_report_day);
        setWeeklyHour(c.weekly_report_hour);
        setWeeklyMinute(c.weekly_report_minute);
      })
      .catch(() => {});

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
      const today = new Date().toISOString().slice(0, 10);
      const lastMonth = new Date(Date.now() - 30 * 86400000).toISOString().slice(0, 10);

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

      const header = ['ID', 'Class', 'Title', 'Workspace', 'Started At', 'Ended At', 'Duration (ms)', 'Activity State', 'Focus (ms)'].join(',');
      const rows = allSessions.map((s) =>
        [s.id, `"${s.class}"`, `"${s.title.replace(/"/g, '""')}"`, s.workspace || '', s.started_at, s.ended_at || '', s.duration_ms || '', s.activity_state || '', s.focused_ms || ''].join(',')
      );
      const csv = [header, ...rows].join('\n');

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

  const aiProviderNames = aiInfo ? Object.keys(aiInfo.providers) : [];

  return (
    <div className="space-y-6 max-w-2xl">
      <h2 className="text-xl font-bold">Settings</h2>

      <div className="bg-gray-900 border border-gray-800 rounded-lg p-6 space-y-6">
        <div>
          <h3 className="text-sm font-medium text-gray-400 mb-3">Server Status</h3>
          <div className="flex items-center gap-3">
            {status === 'online' ? (
              <>
                <Wifi size={18} className="text-emerald-400" />
                <span className="text-emerald-400 text-sm">Online</span>
                {version && <span className="text-xs text-gray-400">v{version}</span>}
              </>
            ) : status === 'offline' ? (
              <>
                <WifiOff size={18} className="text-red-400" />
                <span className="text-red-400 text-sm">Offline</span>
              </>
            ) : (
              <span className="text-gray-400 text-sm">Checking...</span>
            )}
          </div>
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3">Database</h3>
          <p className="text-sm text-gray-300 mb-3">Path: ~/.local/share/hyprtrace/hyprtrace.db</p>
          <div className="flex gap-2">
            <button
              onClick={handleRebuildHourly}
              disabled={rebuilding}
              className="flex items-center gap-2 bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-xs text-gray-300 hover:bg-gray-700 transition-colors disabled:opacity-50"
            >
              <BarChart3 size={12} />
              {rebuilding ? 'Rebuilding...' : 'Rebuild Hourly Summary'}
            </button>
          </div>
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3">AI Providers</h3>
          {aiInfo ? (
            <div className="space-y-1">
              <p className="text-sm text-gray-300">
                Default: {aiInfo.default}
              </p>
              {aiProviderNames.length > 0 ? (
                <ul className="text-sm text-gray-400 space-y-0.5 ml-4 list-disc">
                  {aiProviderNames.map((name) => (
                    <li key={name}>
                      {name} ({aiInfo.providers[name]?.length || 0} models)
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-sm text-gray-400">No AI providers available</p>
              )}
            </div>
          ) : (
            <p className="text-sm text-gray-400">Loading AI provider info...</p>
          )}
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-4">API Configuration</h3>

          <div className="space-y-4">
            <div className="border border-gray-700 rounded-md p-4 space-y-3">
              <h4 className="text-xs font-medium text-cyan-400 flex items-center gap-2">
                <Cpu size={14} /> OpenAI Compatible
              </h4>
              {config?.openai_configured && (
                <p className="text-xs text-emerald-400">Configured</p>
              )}

              <div>
                <label className="text-xs text-gray-400 flex items-center gap-1 mb-1">
                  <Globe size={12} /> API Base URL
                </label>
                <input
                  type="text"
                  value={openaiUrl}
                  onChange={(e) => setOpenaiUrl(e.target.value)}
                  placeholder="https://api.openai.com/v1"
                  className="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
                />
              </div>

              <div>
                <label className="text-xs text-gray-400 flex items-center gap-1 mb-1">
                  <Key size={12} /> API Key
                </label>
                <input
                  type="password"
                  value={openaiKey}
                  onChange={(e) => setOpenaiKey(e.target.value)}
                  placeholder={config?.openai_configured ? '•••••••• (leave blank to keep current)' : 'sk-...'}
                  className="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
                />
              </div>

              <div>
                <label className="text-xs text-gray-400 flex items-center gap-1 mb-1">
                  <Cpu size={12} /> Model
                </label>
                <input
                  type="text"
                  value={openaiModel}
                  onChange={(e) => setOpenaiModel(e.target.value)}
                  placeholder="gpt-4o-mini"
                  list="openai-model-list"
                  className="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
                />
                <datalist id="openai-model-list">
                  {(aiInfo?.providers?.openai ?? []).map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
              </div>
            </div>

            <div className="border border-gray-700 rounded-md p-4 space-y-3">
              <h4 className="text-xs font-medium text-purple-400 flex items-center gap-2">
                <Cpu size={14} /> Ollama
              </h4>

              <div>
                <label className="text-xs text-gray-400 flex items-center gap-1 mb-1">
                  <Globe size={12} /> API Base URL
                </label>
                <input
                  type="text"
                  value={ollamaUrl}
                  onChange={(e) => setOllamaUrl(e.target.value)}
                  placeholder="http://localhost:11434"
                  className="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
                />
              </div>

              <div>
                <label className="text-xs text-gray-400 flex items-center gap-1 mb-1">
                  <Cpu size={12} /> Model
                </label>
                <input
                  type="text"
                  value={ollamaModel}
                  onChange={(e) => setOllamaModel(e.target.value)}
                  placeholder="qwen2.5:7b"
                  list="ollama-model-list"
                  className="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
                />
                <datalist id="ollama-model-list">
                  {(aiInfo?.providers?.ollama ?? []).map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
              </div>
            </div>

            <div className="flex items-center gap-3">
              <button
                onClick={handleSaveConfig}
                disabled={saving}
                className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm transition-colors"
              >
                <Save size={14} />
                {saving ? 'Saving...' : 'Save Config'}
              </button>
              {saveMsg && (
                <span className={`text-xs ${saveMsg === 'Saved' ? 'text-emerald-400' : 'text-red-400'}`}>
                  {saveMsg}
                </span>
              )}
            </div>
          </div>
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3 flex items-center gap-2">
            <Tags size={14} />
            App Categories
          </h3>
          <p className="text-xs text-gray-500 mb-3">Classify apps by class name pattern (SQL LIKE: % matches anything). Higher items take priority.</p>
          <div className="space-y-2 mb-4">
            {categoryRules.map((rule, i) => (
              <div key={i} className="flex items-center gap-2">
                <input
                  type="text"
                  value={rule.pattern}
                  onChange={(e) => handleUpdateCategory(i, 'pattern', e.target.value)}
                  placeholder="kitty"
                  className="flex-1 bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 placeholder-gray-500 focus:ring-cyan-500"
                />
                <select
                  value={rule.category}
                  onChange={(e) => handleUpdateCategory(i, 'category', e.target.value)}
                  className="bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 focus:ring-cyan-500"
                >
                  {categoryNames.map((c) => (
                    <option key={c} value={c}>{c}</option>
                  ))}
                </select>
                <button
                  onClick={() => handleDeleteCategory(i)}
                  className="p-1 text-gray-500 hover:text-red-400 transition-colors"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={handleAddCategory}
              className="flex items-center gap-1 bg-gray-800 border border-gray-700 rounded-md px-3 py-1 text-xs text-gray-300 hover:bg-gray-700 transition-colors"
            >
              <Plus size={12} />
              Add Rule
            </button>
            <button
              onClick={handleSaveCategories}
              disabled={savingCategories}
              className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm transition-colors"
            >
              <Save size={14} />
              {savingCategories ? 'Saving...' : 'Save Categories'}
            </button>
            {categoryMsg && (
              <span className={`text-xs ${categoryMsg === 'Saved' ? 'text-emerald-400' : 'text-red-400'}`}>
                {categoryMsg}
              </span>
            )}
          </div>
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3 flex items-center gap-2">
            <Target size={14} />
            Daily Goals
          </h3>
          <p className="text-xs text-gray-500 mb-3">Set daily active-time targets. The daemon notifies you at 50% and 100% progress, and reminds you to take a break after long focused stretches.</p>
          <div className="space-y-2 mb-4">
            {goals.map((goal, i) => (
              <div key={i} className="flex items-center gap-2">
                <input
                  type="text"
                  value={goal.name}
                  onChange={(e) => handleUpdateGoal(i, 'name', e.target.value)}
                  placeholder="Deep work"
                  className="flex-1 bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 placeholder-gray-500 focus:ring-cyan-500"
                />
                <select
                  value={goal.target_type}
                  onChange={(e) => handleUpdateGoal(i, 'target_type', e.target.value)}
                  className="bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200"
                >
                  <option value="all">All apps</option>
                  <option value="class">Specific app</option>
                </select>
                {goal.target_type === 'class' && (
                  <input
                    type="text"
                    value={goal.target_key ?? ''}
                    onChange={(e) => handleUpdateGoal(i, 'target_key', e.target.value)}
                    placeholder="kitty"
                    className="w-24 bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200"
                  />
                )}
                <input
                  type="number"
                  value={Math.round((goal.daily_target_ms || 0) / 3600000)}
                  onChange={(e) => handleUpdateGoal(i, 'daily_target_ms', Number(e.target.value) * 3600000)}
                  min={1}
                  className="w-16 bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200"
                />
                <span className="text-xs text-gray-500">h</span>
                <input
                  type="checkbox"
                  checked={goal.enabled}
                  onChange={(e) => handleUpdateGoal(i, 'enabled', e.target.checked)}
                  className="accent-cyan-500"
                />
                <button
                  onClick={() => handleDeleteGoal(i)}
                  className="p-1 text-gray-500 hover:text-red-400 transition-colors"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={handleAddGoal}
              className="flex items-center gap-1 bg-gray-800 border border-gray-700 rounded-md px-3 py-1 text-xs text-gray-300 hover:bg-gray-700 transition-colors"
            >
              <Plus size={12} />
              Add Goal
            </button>
            <button
              onClick={handleSaveGoals}
              disabled={savingGoals}
              className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm transition-colors"
            >
              <Save size={14} />
              {savingGoals ? 'Saving...' : 'Save Goals'}
            </button>
            {goalMsg && (
              <span className={`text-xs ${goalMsg === 'Saved' ? 'text-emerald-400' : 'text-red-400'}`}>
                {goalMsg}
              </span>
            )}
          </div>
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3 flex items-center gap-2">
            <FolderKanban size={14} />
            Projects
          </h3>
          <p className="text-xs text-gray-500 mb-3">Attribute app usage to user-defined projects (e.g. 课设, Open Source). Rules use SQL LIKE patterns (% matches anything); higher priority wins.</p>

          <div className="space-y-2 mb-4">
            {projects.map((project, i) => (
              <div key={i} className="flex items-center gap-2">
                <input
                  type="color"
                  value={project.color || '#22d3ee'}
                  onChange={(e) => handleUpdateProject(i, 'color', e.target.value)}
                  className="w-9 h-8 bg-gray-800 border border-gray-700 rounded-md p-0.5"
                  title="Project color"
                />
                <input
                  type="text"
                  value={project.name}
                  onChange={(e) => handleUpdateProject(i, 'name', e.target.value)}
                  placeholder="课设"
                  className="flex-1 bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 placeholder-gray-500 focus:ring-cyan-500"
                />
                <button
                  onClick={() => handleDeleteProject(i)}
                  className="p-1 text-gray-500 hover:text-red-400 transition-colors"
                  title="Delete project"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
            {projects.length === 0 && (
              <p className="text-xs text-gray-600">No projects yet — add one to group your app time.</p>
            )}
          </div>

          <div className="space-y-2 mb-4">
            {projectRules.map((rule, i) => (
              <div key={i} className="flex items-center gap-2">
                <select
                  value={rule.project_id}
                  onChange={(e) => handleUpdateProjectRule(i, 'project_id', e.target.value)}
                  className="bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 focus:ring-cyan-500"
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
                  className="flex-1 bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 placeholder-gray-500 focus:ring-cyan-500"
                />
                <input
                  type="number"
                  value={rule.priority}
                  onChange={(e) => handleUpdateProjectRule(i, 'priority', e.target.value)}
                  min={0}
                  className="w-16 bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200"
                  title="Priority"
                />
                <button
                  onClick={() => handleDeleteProjectRule(i)}
                  className="p-1 text-gray-500 hover:text-red-400 transition-colors"
                  title="Delete rule"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))}
          </div>

          <div className="flex items-center gap-3">
            <button
              onClick={handleAddProject}
              className="flex items-center gap-1 bg-gray-800 border border-gray-700 rounded-md px-3 py-1 text-xs text-gray-300 hover:bg-gray-700 transition-colors"
            >
              <Plus size={12} />
              Add Project
            </button>
            <button
              onClick={handleAddProjectRule}
              className="flex items-center gap-1 bg-gray-800 border border-gray-700 rounded-md px-3 py-1 text-xs text-gray-300 hover:bg-gray-700 transition-colors"
            >
              <Plus size={12} />
              Add Rule
            </button>
            <button
              onClick={handleSaveProjects}
              disabled={savingProjects}
              className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm transition-colors"
            >
              <Save size={14} />
              {savingProjects ? 'Saving...' : 'Save Projects'}
            </button>
            {projectMsg && (
              <span className={`text-xs ${projectMsg === 'Saved' ? 'text-emerald-400' : 'text-red-400'}`}>
                {projectMsg}
              </span>
            )}
          </div>
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3 flex items-center gap-2">
            <FileText size={14} />
            Weekly Report
          </h3>
          <p className="text-xs text-gray-500 mb-3">Generate a Markdown report of the last 7 days on a chosen weekday and send a desktop notification.</p>
          <div className="space-y-3">
            <label className="flex items-center gap-2 text-sm text-gray-300">
              <input
                type="checkbox"
                checked={weeklyEnabled}
                onChange={(e) => setWeeklyEnabled(e.target.checked)}
                className="accent-cyan-500"
              />
              Enable weekly report
            </label>

            <div className="grid grid-cols-3 gap-3">
              <div>
                <label className="text-xs text-gray-400 mb-1 block">Day</label>
                <select
                  value={weeklyDay}
                  onChange={(e) => setWeeklyDay(Number(e.target.value))}
                  className="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 focus:ring-cyan-500 focus:border-cyan-500"
                >
                  <option value={1}>Monday</option>
                  <option value={2}>Tuesday</option>
                  <option value={3}>Wednesday</option>
                  <option value={4}>Thursday</option>
                  <option value={5}>Friday</option>
                  <option value={6}>Saturday</option>
                  <option value={7}>Sunday</option>
                </select>
              </div>
              <div>
                <label className="text-xs text-gray-400 mb-1 block">Hour (0-23)</label>
                <input
                  type="number"
                  min={0}
                  max={23}
                  value={weeklyHour}
                  onChange={(e) => setWeeklyHour(Math.max(0, Math.min(23, Number(e.target.value))))}
                  className="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 focus:ring-cyan-500 focus:border-cyan-500"
                />
              </div>
              <div>
                <label className="text-xs text-gray-400 mb-1 block">Minute (0-59)</label>
                <input
                  type="number"
                  min={0}
                  max={59}
                  value={weeklyMinute}
                  onChange={(e) => setWeeklyMinute(Math.max(0, Math.min(59, Number(e.target.value))))}
                  className="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-1.5 text-sm text-gray-200 focus:ring-cyan-500 focus:border-cyan-500"
                />
              </div>
            </div>

            <div className="flex items-center gap-3">
              <button
                onClick={handleSaveWeekly}
                disabled={savingWeekly}
                className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm transition-colors"
              >
                <Save size={14} />
                {savingWeekly ? 'Saving...' : 'Save weekly report settings'}
              </button>
              {weeklyMsg && (
                <span className={`text-xs ${weeklyMsg === 'Saved' ? 'text-emerald-400' : 'text-red-400'}`}>
                  {weeklyMsg}
                </span>
              )}
            </div>
          </div>
        </div>



        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3">Data Export</h3>
          <div className="flex gap-2 flex-wrap">
            <button
              onClick={handleExport}
              disabled={exporting}
              className="flex items-center gap-2 bg-gray-800 border border-gray-700 rounded-md px-4 py-2 text-sm text-gray-300 hover:bg-gray-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Download size={14} />
              {exporting ? 'Exporting...' : 'Export Sessions (CSV)'}
            </button>
            <button
              onClick={async () => {
                const today = new Date().toISOString().slice(0, 10);
                const weekAgo = new Date(Date.now() - 6 * 86400000).toISOString().slice(0, 10);
                try { await api.report(weekAgo, today); }
                catch (e) { alert('Report failed'); }
              }}
              className="flex items-center gap-2 bg-gray-800 border border-gray-700 rounded-md px-4 py-2 text-sm text-gray-300 hover:bg-gray-700 transition-colors"
            >
              <FileText size={14} />
              Download Weekly Report (MD)
            </button>
          </div>
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3 flex items-center gap-2">
            <Trash2 size={14} />
            Data Privacy
          </h3>

          <div className="space-y-4">
            <div>
              <p className="text-xs font-medium text-gray-300 mb-2">Delete usage data</p>
              <div className="flex items-center gap-2 flex-wrap">
                <label className="text-xs text-gray-400">From</label>
                <input
                  type="date"
                  value={deleteFrom}
                  onChange={(e) => setDeleteFrom(e.target.value)}
                  className="bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 focus:ring-red-500 focus:border-red-500"
                />
                <label className="text-xs text-gray-400">To</label>
                <input
                  type="date"
                  value={deleteTo}
                  onChange={(e) => setDeleteTo(e.target.value)}
                  className="bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 focus:ring-red-500 focus:border-red-500"
                />
              </div>
              <div className="mt-2 flex items-center gap-2">
                <label className="text-xs text-gray-400">App class (optional)</label>
                <input
                  type="text"
                  value={deleteClass}
                  onChange={(e) => setDeleteClass(e.target.value)}
                  placeholder="kitty"
                  className="bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 placeholder-gray-500 focus:ring-red-500 focus:border-red-500"
                />
              </div>
              <div className="mt-3 flex items-center gap-3">
                <button
                  onClick={handleDeleteSessions}
                  disabled={deleting}
                  className="flex items-center gap-2 bg-red-600 hover:bg-red-500 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm transition-colors"
                >
                  <Trash2 size={14} />
                  {deleting ? 'Deleting...' : 'Delete'}
                </button>
                {deleteMsg && (
                  <span className={`text-xs ${deleteMsg === 'Delete failed' || deleteMsg.startsWith('Delete failed') || deleteMsg === 'Please provide both dates' ? 'text-red-400' : 'text-emerald-400'}`}>
                    {deleteMsg}
                  </span>
                )}
              </div>
            </div>

            <div className="border-t border-gray-800 pt-4">
              <p className="text-xs font-medium text-gray-300 mb-2">Retention</p>
              <div className="flex items-center gap-2">
                <label className="text-xs text-gray-400">Retention days</label>
                <input
                  type="number"
                  min={0}
                  value={retentionDays}
                  onChange={(e) => setRetentionDays(Math.max(0, Number(e.target.value)))}
                  className="w-24 bg-gray-800 border border-gray-700 rounded-md px-2 py-1 text-xs text-gray-200 focus:ring-cyan-500 focus:border-cyan-500"
                />
                <span className="text-xs text-gray-500">0 = keep forever</span>
              </div>
              <p className="text-xs text-gray-500 mt-1">Sessions older than this many days are deleted automatically (saved with the config above).</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
