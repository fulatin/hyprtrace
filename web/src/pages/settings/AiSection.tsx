import { useState } from 'react';
import { Check, Cpu, Globe, Key } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { Skeleton } from '../../components/ui/Feedback';
import { SaveButton } from '../../components/ui/Controls';
import { useAiModels, useConfig, useUpdateConfig } from '../../lib/queries';

/**
 * AI providers + API configuration. Owns the five AI fields of `/api/config`;
 * `PUT /api/config` merges only the keys it sends, so this section never
 * touches the daemon/server keys that the privacy and weekly-report sections
 * save.
 */
export default function AiSection() {
  const aiModels = useAiModels();
  const config = useConfig();
  const saveConfig = useUpdateConfig();

  const aiInfo = aiModels.data ?? null;
  const aiLoaded = !aiModels.isPending;

  const [openaiUrl, setOpenaiUrl] = useState(config.data?.openai_url ?? '');
  const [openaiKey, setOpenaiKey] = useState('');
  const [openaiModel, setOpenaiModel] = useState(config.data?.openai_model ?? '');
  const [ollamaUrl, setOllamaUrl] = useState(config.data?.ollama_url ?? '');
  const [ollamaModel, setOllamaModel] = useState(config.data?.ollama_model ?? '');
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState('');

  const aiProviderNames = aiInfo ? Object.keys(aiInfo.providers) : [];

  const handleSaveConfig = async () => {
    setSaving(true);
    setSaveMsg('');
    try {
      await saveConfig.mutateAsync({
        openai_url: openaiUrl,
        openai_api_key: openaiKey || undefined,
        openai_model: openaiModel,
        ollama_url: ollamaUrl,
        ollama_model: ollamaModel,
      });
      setSaveMsg('Saved');
      setOpenaiKey('');
    } catch (e) {
      setSaveMsg('Save failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
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
              {config.data?.openai_configured && (
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
                    placeholder={config.data?.openai_configured ? '•••••••• (leave blank to keep current)' : 'sk-...'}
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
    </>
  );
}
