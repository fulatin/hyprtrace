import { useEffect, useState } from 'react';
import { format } from 'date-fns';
import { Clock, AppWindow, Hash, Moon, BrainCircuit, BellRing, Copy } from 'lucide-react';
import { api } from '../lib/api';
import type { TodaySummary, HourlyBucket, DisruptionEvent } from '../lib/types';
import StatCard from '../components/StatCard';
import AppUsagePie from '../components/AppUsagePie';
import HourlyHeatmap from '../components/HourlyHeatmap';

function formatDuration(ms: number): string {
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  if (hours > 0) return `${hours}h ${mins}m`;
  if (mins > 0) return `${mins}m`;
  return `${Math.floor(ms / 1000)}s`;
}

export default function Dashboard() {
  const today = format(new Date(), 'yyyy-MM-dd');
  const [summary, setSummary] = useState<TodaySummary | null>(null);
  const [timeline, setTimeline] = useState<HourlyBucket[]>([]);
  const [disruptions, setDisruptions] = useState<DisruptionEvent[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    Promise.all([
      api.summary(today).catch(() => null),
      api.timeline(today).catch(() => []),
      api.disruptions(today, today, 30).catch(() => []),
    ]).then(([s, t, d]) => {
      setSummary(s);
      setTimeline(t);
      setDisruptions(d);
      setLoading(false);
    });
  }, [today]);

  if (loading) {
    return (
      <div className="space-y-6 animate-fadeIn">
        <h2 className="text-xl font-bold">Dashboard</h2>
        <div className="grid grid-cols-5 gap-4">
          {[1, 2, 3, 4, 5].map((i) => (
            <div key={i} className="bg-gray-900 border border-gray-800 rounded-xl p-4 h-24 animate-pulse" style={{ animationDelay: `${i * 80}ms` }} />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6 animate-fadeIn">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">Dashboard</h2>
        <span className="text-sm text-gray-400">{today}</span>
      </div>

      <div className="grid grid-cols-5 gap-4">
        {[
          { icon: <Clock size={16} />, label: "Active Time", value: formatDuration(summary?.total_active_ms ?? 0) },
          { icon: <BrainCircuit size={16} />, label: "Focus Time", value: formatDuration(summary?.total_focused_ms ?? 0), sub: `${Math.round(((summary?.total_focused_ms ?? 0) / Math.max((summary?.total_active_ms ?? 1), 1)) * 100)}% focused` },
          { icon: <AppWindow size={16} />, label: "Apps", value: String(summary?.app_count ?? 0) },
          { icon: <Hash size={16} />, label: "Sessions", value: String(summary?.session_count ?? 0) },
          { icon: <Moon size={16} />, label: "Idle Time", value: formatDuration(summary?.total_idle_ms ?? 0) },
        ].map((card, i) => (
          <div key={card.label} className="animate-fadeInUp" style={{ animationDelay: `${i * 80}ms` }}>
            <StatCard icon={card.icon} label={card.label} value={card.value} subtext={(card as any).sub} />
          </div>
        ))}
      </div>

      <div className="grid grid-cols-2 gap-4 animate-fadeInUp" style={{ animationDelay: "200ms" }}>
        <AppUsagePie data={summary?.top_apps ?? []} />
        <HourlyHeatmap data={timeline} />
      </div>

      {disruptions.length > 0 && (
        <div className="bg-gray-900 border border-gray-800 rounded-xl p-4 animate-fadeInUp" style={{ animationDelay: "300ms" }}>
          <h3 className="text-sm font-medium text-gray-400 flex items-center gap-2 mb-3">
            <BellRing size={14} className="text-amber-400" />
            Today's Interruptions
            <span className="text-xs text-gray-500">
              {disruptions.filter((d) => d.kind === 'notification').length} notifications · {disruptions.filter((d) => d.kind === 'clipboard').length} copies
            </span>
          </h3>
          <div className="space-y-1.5 max-h-64 overflow-auto">
            {disruptions.slice(0, 12).map((d) => (
              <div key={d.id} className="flex items-center gap-2 text-sm">
                {d.kind === 'notification' ? (
                  <BellRing size={12} className="text-amber-400 shrink-0" />
                ) : (
                  <Copy size={12} className="text-cyan-400 shrink-0" />
                )}
                <span className="text-gray-300 truncate">
                  {d.kind === 'notification'
                    ? `${d.app ?? 'unknown'}: ${d.summary ?? ''}`
                    : 'Clipboard copy'}
                </span>
                <span className="text-xs text-gray-500 ml-auto shrink-0">
                  {new Date(d.occurred_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}