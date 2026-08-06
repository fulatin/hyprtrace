import { useEffect, useState } from 'react';
import { api } from '../lib/api';
import { Wifi, WifiOff, Download, Save, Key, Globe, Cpu, BarChart3, Tags, Plus, Trash2 } from 'lucide-react';
import type { AiModelsResponse, CategoryRule, ConfigResponse, Session } from '../lib/types';

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
      })
      .catch(() => {});

    api.categories()
      .then((res) => {
        setCategoryRules(res.rules);
        setCategoryNames(res.categories);
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
      });
      setSaveMsg('Saved');
      setOpenaiKey('');
      const fresh = await api.getConfig();
      setConfig(fresh);
    } catch (e) {
      setSaveMsg('Save failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setSaving(false);
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

  const aiProviderNames = aiInfo ? Object.keys(aiInfo.providers) : [];

  const handleSaveCategories = async () => {
    setSavingCategories(true);
    setCategoryMsg('');
    try {
      const clean = categoryRules
        .filter((r) => r.pattern.trim() && r.category.trim())
        .map((r) => ({ pattern: r.pattern.trim(), category: r.category.trim(), priority: r.priority ?? 0 }));
      await api.putCategories(clean);
      setCategoryMsg('Saved');
      const fresh = await api.categories();
      setCategoryRules(fresh.rules);
    } catch (e) {
      setCategoryMsg('Save failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setSavingCategories(false);
    }
  };

  const updateRule = (i: number, patch: Partial<CategoryRule>) => {
    setCategoryRules((prev) => prev.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  };

  return (
    <div className="space-y-6 max-w-2xl">
      <h2 className="text-xl font-bold">Settings</h2>

      <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 space-y-6">
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
              className="flex items-center gap-2 bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-xs text-gray-300 hover:bg-gray-700 transition-colors disabled:opacity-50"
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
            <div className="border border-gray-700 rounded-lg p-4 space-y-3">
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
                  className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
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
                  className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
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
                  className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
                />
                <datalist id="openai-model-list">
                  {(aiInfo?.providers?.openai ?? []).map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
              </div>
            </div>

            <div className="border border-gray-700 rounded-lg p-4 space-y-3">
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
                  className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
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
                  className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
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
                className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white rounded-lg px-4 py-2 text-sm transition-colors"
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
          <h3 className="text-sm font-medium text-gray-400 mb-3">Data Export</h3>
          <button
            onClick={handleExport}
            disabled={exporting}
            className="flex items-center gap-2 bg-gray-800 border border-gray-700 rounded-lg px-4 py-2 text-sm text-gray-300 hover:bg-gray-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Download size={14} />
            {exporting ? 'Exporting...' : 'Export Sessions (CSV)'}
          </button>
        </div>

        <div className="border-t border-gray-800 pt-4">
          <h3 className="text-sm font-medium text-gray-400 mb-3 flex items-center gap-2">
            <Tags size={14} /> App Categories
          </h3>
          <p className="text-xs text-gray-500 mb-3">
            Classify apps by window class pattern (<code>%</code> = any sequence, <code>_</code> = one char,
            case-insensitive). Used for efficiency scoring, reports and AI analysis.
          </p>
          <div className="space-y-2">
            {categoryRules.map((rule, i) => (
              <div key={rule.id ?? `new-${i}`} className="flex items-center gap-2">
                <input
                  type="text"
                  value={rule.pattern}
                  onChange={(e) => updateRule(i, { pattern: e.target.value })}
                  placeholder="e.g. minecraft%"
                  className="flex-1 bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500 font-mono"
                />
                <select
                  value={rule.category}
                  onChange={(e) => updateRule(i, { category: e.target.value })}
                  className="bg-gray-800 border border-gray-700 rounded-lg px-2 py-1.5 text-sm text-gray-200 focus:ring-cyan-500"
                >
                  {categoryNames.map((c) => (
                    <option key={c} value={c}>{c}</option>
                  ))}
                </select>
                <button
                  onClick={() => setCategoryRules((prev) => prev.filter((_, idx) => idx !== i))}
                  className="p-1.5 rounded hover:bg-gray-700 text-gray-400 hover:text-red-400 transition-colors"
                  title="Delete rule"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            ))}
            <button
              onClick={() =>
                setCategoryRules((prev) => [...prev, { pattern: '', category: 'other', priority: 0 }])
              }
              className="flex items-center gap-1.5 text-xs text-cyan-400 hover:text-cyan-300 transition-colors"
            >
              <Plus size={12} /> Add rule
            </button>
          </div>
          <div className="flex items-center gap-3 mt-3">
            <button
              onClick={handleSaveCategories}
              disabled={savingCategories}
              className="flex items-center gap-2 bg-cyan-600 hover:bg-cyan-500 disabled:opacity-50 text-white rounded-lg px-4 py-2 text-sm transition-colors"
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
      </div>
    </div>
  );
}
