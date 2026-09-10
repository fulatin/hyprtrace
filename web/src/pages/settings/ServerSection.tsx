import { useState } from 'react';
import { RefreshCw, Server, Wifi, WifiOff } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { Spinner } from '../../components/ui/Controls';
import { useHealth, useRebuildHourlySummary } from '../../lib/queries';

/** Server & database: health indicator plus the hourly-summary rebuild. */
export default function ServerSection() {
  const health = useHealth();
  const rebuild = useRebuildHourlySummary();
  const [rebuilding, setRebuilding] = useState(false);

  // The old component kept `status`/`version` in state and set them from the
  // one-shot /api/health call; derived here instead, with the same three states.
  const status = health.isPending ? 'checking' : health.isError ? 'offline' : 'online';
  const version = health.data?.version ?? '';

  const handleRebuildHourly = async () => {
    setRebuilding(true);
    try {
      await rebuild.mutateAsync();
      alert('Hourly summary rebuilt successfully');
    } catch (e) {
      alert('Rebuild failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setRebuilding(false);
    }
  };

  return (
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
  );
}
