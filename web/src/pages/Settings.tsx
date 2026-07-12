import { useEffect, useState } from 'react';
import { api } from '../lib/api';
import { Wifi, WifiOff, Download, Save, Key, Globe, Cpu } from 'lucide-react';
import type { AiModelsResponse, ConfigResponse, Session } from '../lib/types';

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
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState('');

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

      const header = ['ID', 'Class', 'Title', 'Workspace', 'Started At', 'Ended At', 'Duration (ms)'].join(',');
      const rows = allSessions.map((s) =>
        [s.id, `"${s.class}"`, `"${s.title.replace(/"/g, '""')}"`, s.workspace || '', s.started_at, s.ended_at || '', s.duration_ms || ''].join(',')
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
          <p className="text-sm text-gray-300">Path: ~/.local/share/hyprtrace/hyprtrace.db</p>
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
                  className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
                />
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
                  className="w-full bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500"
                />
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
      </div>
    </div>
  );
}
