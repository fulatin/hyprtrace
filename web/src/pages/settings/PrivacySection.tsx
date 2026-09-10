import { useState } from 'react';
import { ShieldCheck } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { SaveButton, Toggle } from '../../components/ui/Controls';
import { useConfig, useUpdateConfig } from '../../lib/queries';

/**
 * Privacy & retention. This section owns BOTH config keys it shows —
 * `daemon.record_titles` and `server.retention_days` — and its save sends both.
 *
 * Before the split the retention input lived here but was only written by the
 * API-configuration section's "Save Config" button (which sent
 * `retention_days`), so typing a retention window and pressing this panel's
 * save dropped the edit. `PUT /api/config` merges only the keys present in the
 * request, so sending both here cannot clobber the AI or weekly-report values.
 */
export default function PrivacySection() {
  const config = useConfig();
  const saveConfig = useUpdateConfig();

  const [recordTitles, setRecordTitles] = useState(config.data?.record_titles ?? true);
  const [retentionDays, setRetentionDays] = useState(config.data?.retention_days ?? 0);
  const [savingPrivacy, setSavingPrivacy] = useState(false);
  const [privacyMsg, setPrivacyMsg] = useState('');

  const handleSavePrivacy = async () => {
    setSavingPrivacy(true);
    setPrivacyMsg('');
    try {
      await saveConfig.mutateAsync({ record_titles: recordTitles, retention_days: retentionDays });
      setPrivacyMsg('Saved');
    } catch (e) {
      setPrivacyMsg('Save failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setSavingPrivacy(false);
    }
  };

  return (
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
                onClick={handleSavePrivacy}
                saving={savingPrivacy}
                msg={privacyMsg}
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
              Sessions older than this many days are deleted automatically (saved with the privacy settings above).
            </p>
          </div>
        </div>
      </Panel>
    </RevealOnScroll>
  );
}
