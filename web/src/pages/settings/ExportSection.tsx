import { useState } from 'react';
import { format, subDays } from 'date-fns';
import { Download, FileText } from 'lucide-react';
import { Panel } from '../../components/ui/Card';
import { RevealOnScroll } from '../../components/ui/Reveal';
import { Spinner } from '../../components/ui/Controls';
import { useDownloadWeeklyReport, useExportSessions } from '../../lib/queries';

/** Data export: the full session CSV and the server-rendered Markdown report. */
export default function ExportSection() {
  const exportSessions = useExportSessions();
  const downloadReport = useDownloadWeeklyReport();
  const [exporting, setExporting] = useState(false);

  const handleExport = async () => {
    setExporting(true);
    try {
      const today = format(new Date(), 'yyyy-MM-dd');
      const lastMonth = format(subDays(new Date(), 30), 'yyyy-MM-dd');

      // Every page of /api/sessions in the window is fetched by the mutation.
      const allSessions = await exportSessions.mutateAsync({ from: lastMonth, to: today });

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

  return (
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
              try { await downloadReport.mutateAsync({ from: weekAgo, to: today }); }
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
  );
}
