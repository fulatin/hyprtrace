import { useEffect, useState } from 'react';
import { format } from 'date-fns';
import { Clock, AppWindow, Hash, Moon } from 'lucide-react';
import { api } from '../lib/api';
import type { TodaySummary, HourlyBucket } from '../lib/types';
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
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    Promise.all([
      api.summary(today).catch(() => null),
      api.timeline(today).catch(() => []),
    ]).then(([s, t]) => {
      setSummary(s);
      setTimeline(t);
      setLoading(false);
    });
  }, [today]);

  if (loading) {
    return (
      <div className="space-y-6">
        <h2 className="text-xl font-bold">Dashboard</h2>
        <div className="grid grid-cols-4 gap-4">
          {[1, 2, 3, 4].map((i) => (
            <div key={i} className="bg-gray-900 border border-gray-800 rounded-xl p-4 h-24 animate-pulse" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">Dashboard</h2>
        <span className="text-sm text-gray-400">{today}</span>
      </div>

      <div className="grid grid-cols-4 gap-4">
        <StatCard
          icon={<Clock size={16} />}
          label="Active Time"
          value={formatDuration(summary?.total_active_ms ?? 0)}
        />
        <StatCard
          icon={<AppWindow size={16} />}
          label="Apps"
          value={String(summary?.app_count ?? 0)}
        />
        <StatCard
          icon={<Hash size={16} />}
          label="Sessions"
          value={String(summary?.session_count ?? 0)}
        />
        <StatCard
          icon={<Moon size={16} />}
          label="Idle Time"
          value={formatDuration(summary?.total_idle_ms ?? 0)}
        />
      </div>

      <div className="grid grid-cols-2 gap-4">
        <AppUsagePie data={summary?.top_apps ?? []} />
        <HourlyHeatmap data={timeline} />
      </div>
    </div>
  );
}