import { useCallback, useMemo, useState } from 'react';
import {
  AlertTriangle,
  Check,
  Cpu,
  Plug,
  RefreshCw,
  Terminal,
  Wrench,
} from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { Skeleton } from '../../components/ui/Feedback';
import { SaveButton, Spinner, Toggle } from '../../components/ui/Controls';
import {
  useAiModels,
  useAiStatus,
  useAiTools,
  useConfig,
  useUpdateConfig,
} from '../../lib/queries';
import { api, extractApiError } from '../../lib/api';

/**
 * AI runtime configuration.
 *
 * DeepSeek Harness is the only AI runtime now: this section owns the `[ai]`
 * keys of `/api/config` — the `[ai.dsh]` launcher/route settings plus the
 * proactive-insight interval — and shows the live health reported by
 * `GET /api/ai/status` (enabled / ready / bridge / tool count / error).
 *
 * `PUT /api/config` merges only the keys present in the request, so this
 * section sends just `ai` and cannot clobber the weekly-report or privacy
 * values. The advanced `[ai.dsh]` keys that have no field here (`base_url`,
 * `cwd`, `tools_script`, …) are carried over from the loaded config verbatim,
 * so saving in the UI does not drop a hand-written config file.
 */
export default function AiSection() {
  const config = useConfig();
  const models = useAiModels();
  const status = useAiStatus();
  const tools = useAiTools();
  const saveConfig = useUpdateConfig();

  const dsh = config.data?.ai?.dsh;

  const [enabled, setEnabled] = useState(dsh?.enabled ?? false);
  const [command, setCommand] = useState(dsh?.command ?? 'dsh');
  const [profile, setProfile] = useState(dsh?.profile ?? 'sdk');
  const [provider, setProvider] = useState(dsh?.provider ?? '');
  const [model, setModel] = useState(dsh?.model ?? '');
  const [apiKeyEnv, setApiKeyEnv] = useState(dsh?.api_key_env ?? '');
  const [timeoutSecs, setTimeoutSecs] = useState(
    dsh?.request_timeout_seconds ?? 300,
  );
  const [proactiveMinutes, setProactiveMinutes] = useState(
    config.data?.ai?.proactive_interval_minutes ?? 120,
  );
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState('');
  const [showTools, setShowTools] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testMsg, setTestMsg] = useState('');

  const refreshing =
    status.isFetching || models.isFetching || tools.isFetching;

  /** Re-read the live runtime state without booting the harness. */
  const refreshRuntime = useCallback(async () => {
    await Promise.all([
      status.refetch(),
      models.refetch(),
      tools.refetch(),
    ]);
  }, [status, models, tools]);

  /**
   * "Test connection": `GET /api/ai/models?refresh=1` starts the harness if it
   * is not running, so a config that looks right on paper but cannot spawn
   * `dsh` fails here instead of in the first chat turn.
   */
  const testConnection = useCallback(async () => {
    setTesting(true);
    setTestMsg('');
    try {
      await api.aiModels(true);
      await refreshRuntime();
      setTestMsg('Harness answered');
    } catch (e) {
      setTestMsg('Test failed: ' + extractApiError(e));
    } finally {
      setTesting(false);
    }
  }, [refreshRuntime]);

  const handleSave = async () => {
    setSaving(true);
    setSaveMsg('');
    try {
      await saveConfig.mutateAsync({
        ai: {
          proactive_interval_minutes: proactiveMinutes,
          dsh: {
            // Keep every advanced key this form does not edit.
            ...(dsh ?? {}),
            enabled,
            command,
            profile,
            provider,
            model,
            api_key_env: apiKeyEnv,
            request_timeout_seconds: timeoutSecs,
          },
        },
      });
      setSaveMsg('Saved');
      // A save reconfigures the runtime: reflect it immediately.
      await refreshRuntime();
    } catch (e) {
      setSaveMsg('Save failed: ' + extractApiError(e));
    } finally {
      setSaving(false);
    }
  };

  // Model suggestions come from the harness, so the field can be a picker
  // without hiding a hand-written route.
  const modelSuggestions = useMemo(() => {
    const list = models.data?.providers?.dsh ?? [];
    const set = new Set(list);
    if (model) set.add(model);
    return [...set];
  }, [models.data, model]);

  const statusData = status.data;
  const statusError = status.isError ? extractApiError(status.error) : '';

  return (
    <>
      <RevealOnScroll>
        <Panel
          title="DeepSeek Harness"
          icon={<Cpu size={15} className="text-accent" />}
          hint="The only AI runtime"
          actions={
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => void refreshRuntime()}
                disabled={refreshing || testing}
                className="btn text-xs"
                title="Re-read the runtime status, model routes and tool list"
              >
                {refreshing ? <Spinner size={12} /> : <RefreshCw size={12} />}
                {refreshing ? 'Refreshing...' : 'Refresh status'}
              </button>
              <button
                type="button"
                onClick={() => void testConnection()}
                disabled={testing || refreshing}
                className="btn text-xs"
                title="Start the harness if it is not running and re-read its model routes"
              >
                {testing ? <Spinner size={12} /> : <Plug size={12} />}
                {testing ? 'Testing...' : 'Test connection'}
              </button>
            </div>
          }
        >
          {status.isPending ? (
            <div className="space-y-2">
              <Skeleton className="h-4 w-44" />
              <Skeleton className="h-10 w-full" />
            </div>
          ) : statusError ? (
            <div className="flex items-start gap-2 rounded-lg border border-bad/30 bg-bad/10 px-3 py-2.5 text-xs text-bad">
              <AlertTriangle size={14} className="mt-0.5 shrink-0" />
              <div className="min-w-0">
                <p className="font-medium">Cannot read the runtime status</p>
                <p className="mt-0.5 break-words text-fg-muted">{statusError}</p>
                <p className="mt-1 text-fg-faint">
                  The server must expose <code>GET /api/ai/status</code> — a
                  server built before the DSH migration will 404 here.
                </p>
              </div>
            </div>
          ) : statusData ? (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <span
                  className={
                    statusData.enabled ? 'chip-good' : 'chip-neutral'
                  }
                >
                  {statusData.enabled ? <Check size={12} /> : null}
                  {statusData.enabled ? 'Enabled' : 'Disabled'}
                </span>
                <span
                  className={statusData.ready ? 'chip-good' : 'chip-bad'}
                >
                  {statusData.ready ? <Check size={12} /> : <AlertTriangle size={12} />}
                  {statusData.ready ? 'Ready' : 'Not ready'}
                </span>
                <span
                  className={
                    statusData.bridge_connected ? 'chip-good' : 'chip-bad'
                  }
                >
                  {statusData.bridge_connected ? (
                    <Check size={12} />
                  ) : (
                    <AlertTriangle size={12} />
                  )}
                  {statusData.bridge_connected
                    ? 'Bridge connected'
                    : 'Bridge disconnected'}
                </span>
                <span
                  className="chip-neutral tnum"
                  title="Tools exposed by the harness"
                >
                  <Wrench size={12} />
                  {statusData.tool_count} tools
                </span>
              </div>

              <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-fg-muted">
                <span>
                  Route:{' '}
                  <span className="font-mono text-fg">
                    {statusData.provider || '—'} / {statusData.model || '—'}
                  </span>
                </span>
              </div>

              {statusData.error && (
                <div className="flex items-start gap-2 rounded-lg border border-bad/30 bg-bad/10 px-3 py-2.5 text-xs text-bad">
                  <AlertTriangle size={14} className="mt-0.5 shrink-0" />
                  <p className="min-w-0 break-words">{statusData.error}</p>
                </div>
              )}

              {testMsg && (
                <p
                  className={`text-xs ${
                    testMsg.startsWith('Test failed') ? 'text-bad' : 'text-good'
                  }`}
                >
                  {testMsg}
                </p>
              )}

              {statusData.enabled && tools.data && tools.data.tools.length > 0 && (
                <div>
                  <button
                    type="button"
                    onClick={() => setShowTools((v) => !v)}
                    className="text-xs text-accent transition-colors hover:text-accent-2"
                  >
                    {showTools ? 'Hide tools' : `Show tools (${tools.data.tools.length})`}
                  </button>
                  {showTools && (
                    <ul className="mt-2 space-y-1">
                      {tools.data.tools.map((t) => (
                        <li
                          key={t.name}
                          className="rounded-lg border border-line bg-surface-2/60 px-3 py-1.5"
                          title={t.description}
                        >
                          <span className="font-mono text-xs text-fg">{t.name}</span>
                          <span className="ml-2 text-xs text-fg-faint">
                            {t.description}
                          </span>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              )}
            </div>
          ) : null}
        </Panel>
      </RevealOnScroll>

      <RevealOnScroll delay={0.05}>
        <Panel
          title="DSH configuration"
          icon={<Terminal size={15} className="text-accent-2" />}
          hint="Stored in the local config file"
        >
          <div className="space-y-4">
            <Toggle
              checked={enabled}
              onChange={setEnabled}
              label="Enable DeepSeek Harness"
              hint="When disabled the AI runtime reports not-ready and chat requests are rejected."
            />

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <label className="block">
                <span className="mb-1 block text-xs text-fg-muted">Command</span>
                <input
                  type="text"
                  value={command}
                  onChange={(e) => setCommand(e.target.value)}
                  placeholder="dsh"
                  className="input w-full font-mono"
                />
                <span className="mt-1 block text-xs text-fg-faint">
                  Launcher resolved through $PATH.
                </span>
              </label>

              <label className="block">
                <span className="mb-1 block text-xs text-fg-muted">Profile</span>
                <input
                  type="text"
                  value={profile}
                  onChange={(e) => setProfile(e.target.value)}
                  placeholder="sdk"
                  className="input w-full font-mono"
                />
                <span className="mt-1 block text-xs text-fg-faint">
                  DSH profile to boot.
                </span>
              </label>

              <label className="block">
                <span className="mb-1 block text-xs text-fg-muted">Provider</span>
                <input
                  type="text"
                  value={provider}
                  onChange={(e) => setProvider(e.target.value)}
                  placeholder="opencode-go"
                  className="input w-full font-mono"
                />
                <span className="mt-1 block text-xs text-fg-faint">
                  Provider route known to the harness profile.
                </span>
              </label>

              <label className="block">
                <span className="mb-1 block text-xs text-fg-muted">Model</span>
                <input
                  type="text"
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  placeholder="deepseek-v4.1-flash"
                  list="dsh-model-list"
                  className="input w-full font-mono"
                />
                <datalist id="dsh-model-list">
                  {modelSuggestions.map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
                <span className="mt-1 block text-xs text-fg-faint">
                  Default model route for chat turns.
                </span>
              </label>

              <label className="block">
                <span className="mb-1 block text-xs text-fg-muted">
                  API key env
                </span>
                <input
                  type="text"
                  value={apiKeyEnv}
                  onChange={(e) => setApiKeyEnv(e.target.value)}
                  placeholder="OPENCODE_GO_API_KEY"
                  className="input w-full font-mono"
                />
                <span className="mt-1 block text-xs text-fg-faint">
                  Name of the environment variable holding the credential — the
                  secret itself stays out of the config file.
                </span>
              </label>

              <label className="block">
                <span className="mb-1 block text-xs text-fg-muted">
                  Request timeout (seconds)
                </span>
                <input
                  type="number"
                  min={30}
                  value={timeoutSecs}
                  onChange={(e) =>
                    setTimeoutSecs(Math.max(30, Number(e.target.value)))
                  }
                  className="input w-32"
                />
                <span className="mt-1 block text-xs text-fg-faint">
                  Wall-clock budget for one chat turn (minimum 30 s).
                </span>
              </label>
            </div>

            <div className="divider" />

            <label className="block">
              <span className="mb-1 block text-xs text-fg-muted">
                Proactive insights interval (minutes)
              </span>
              <input
                type="number"
                min={0}
                value={proactiveMinutes}
                onChange={(e) =>
                  setProactiveMinutes(Math.max(0, Number(e.target.value)))
                }
                className="input w-32"
              />
              <span className="mt-1 block text-xs text-fg-faint">
                How often the server may generate proactive insight summaries
                (0 = never).
              </span>
            </label>

            <SaveButton
              onClick={handleSave}
              saving={saving}
              msg={saveMsg}
              label="Save AI settings"
            />
          </div>
        </Panel>
      </RevealOnScroll>
    </>
  );
}
