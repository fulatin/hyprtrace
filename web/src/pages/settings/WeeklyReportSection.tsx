import { useState } from 'react';
import { FileText } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { SaveButton, Toggle } from '../../components/ui/Controls';
import { useConfig, useUpdateConfig } from '../../lib/queries';

/**
 * Weekly report schedule. Owns the four `weekly_report_*` keys of
 * `/api/config`; its save sends only those, so unsaved edits in the AI or
 * privacy sections are not clobbered.
 */
export default function WeeklyReportSection() {
  const config = useConfig();
  const saveConfig = useUpdateConfig();

  const [weeklyEnabled, setWeeklyEnabled] = useState(config.data?.weekly_report_enabled ?? false);
  const [weeklyDay, setWeeklyDay] = useState(config.data?.weekly_report_day ?? 1);
  const [weeklyHour, setWeeklyHour] = useState(config.data?.weekly_report_hour ?? 9);
  const [weeklyMinute, setWeeklyMinute] = useState(config.data?.weekly_report_minute ?? 0);
  const [savingWeekly, setSavingWeekly] = useState(false);
  const [weeklyMsg, setWeeklyMsg] = useState('');

  const handleSaveWeekly = async () => {
    setSavingWeekly(true);
    setWeeklyMsg('');
    try {
      // Only send the weekly report fields so unsaved AI edits are not clobbered.
      await saveConfig.mutateAsync({
        weekly_report_enabled: weeklyEnabled,
        weekly_report_day: weeklyDay,
        weekly_report_hour: weeklyHour,
        weekly_report_minute: weeklyMinute,
      });
      setWeeklyMsg('Saved');
    } catch (e) {
      setWeeklyMsg('Save failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setSavingWeekly(false);
    }
  };

  return (
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
  );
}
