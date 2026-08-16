import { useEffect, useState } from 'react';
import { format } from 'date-fns';
import { Clock, AppWindow, Hash, Moon, BrainCircuit, BellRing, Copy, Gauge, Target, TrendingUp, FolderKanban } from 'lucide-react';
import { api } from '../lib/api';
import type { TodaySummary, HourlyBucket, DisruptionEvent, EfficiencyScore, GoalProgress, TrendPrediction, ProjectStat } from '../lib/types';
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
  const [efficiency, setEfficiency] = useState<EfficiencyScore | null>(null);
  const [goalProgress, setGoalProgress] = useState<GoalProgress[]>([]);
  const [prediction, setPrediction] = useState<TrendPrediction | null>(null);
  const [projectStats, setProjectStats] = useState<ProjectStat[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    Promise.all([
      api.summary(today).catch(() => null),
      api.timeline(today).catch(() => []),
      api.disruptions(today, today, 30).catch(() => []),
      api.efficiency(today).catch(() => null),
      api.goals().catch(() => ({ goals: [], progress: [] })),
      api.predict(14).catch(() => null),
      api.projectStats(today, today).catch(() => []),
    ]).then(([s, t, d, e, g, p, ps]) => {
      setSummary(s);
      setTimeline(t);
      setDisruptions(d);
      setEfficiency(e);
      setGoalProgress(g.progress ?? []);
      setPrediction(p);
      setProjectStats(ps);
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

      <div className="grid grid-cols-6 gap-4">
        {[
          { icon: <Clock size={16} />, label: "Active Time", value: formatDuration(summary?.total_active_ms ?? 0) },
          { icon: <BrainCircuit size={16} />, label: "Focus Time", value: formatDuration(summary?.total_focused_ms ?? 0), sub: `${Math.round(((summary?.total_focused_ms ?? 0) / Math.max((summary?.total_active_ms ?? 1), 1)) * 100)}% focused` },
          { icon: <AppWindow size={16} />, label: "Apps", value: String(summary?.app_count ?? 0) },
          { icon: <Hash size={16} />, label: "Sessions", value: String(summary?.session_count ?? 0) },
          { icon: <Moon size={16} />, label: "Idle Time", value: formatDuration(summary?.total_idle_ms ?? 0) },
          { icon: <Gauge size={16} />, label: "Efficiency", value: efficiency ? `${efficiency.score}/100` : "—", sub: efficiency ? `${Math.round(efficiency.focus_ratio * 100)}% focus · ${Math.round(efficiency.avg_session_secs / 60)}m/session` : "" },
        ].map((card, i) => (
          <div key={card.label} className="animate-fadeInUp" style={{ animationDelay: `${i * 80}ms` }}>
            <StatCard icon={card.icon} label={card.label} value={card.value} subtext={(card as any).sub} />
          </div>
        ))}
      </div>

      {goalProgress.length > 0 && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 animate-fadeInUp" style={{ animationDelay: "150ms" }}>
          {goalProgress.map((p) => (
            <div key={p.goal.id ?? p.goal.name} className="bg-gray-900 border border-gray-800 rounded-xl p-4">
              <div className="flex items-center justify-between mb-2">
                <span className="text-sm font-medium flex items-center gap-2">
                  <Target size={14} className="text-cyan-400" />
                  {p.goal.name}
                </span>
                <span className="text-xs text-gray-400">{Math.round(p.pct)}%</span>
              </div>
              <div className="w-full h-2 bg-gray-800 rounded-full overflow-hidden">
                <div
                  className={`h-full rounded-full transition-all duration-500 ${p.pct >= 100 ? 'bg-emerald-500' : 'bg-cyan-500'}`}
                  style={{ width: `${Math.min(p.pct, 100)}%` }}
                />
              </div>
              <div className="mt-2 text-xs text-gray-500">
                {Math.round(p.today_ms / 3600000)}h {Math.round((p.today_ms % 3600000) / 60000)}m / {Math.round((p.goal.daily_target_ms || 0) / 3600000)}h
              </div>
            </div>
          ))}
        </div>
      )}

      {prediction && (
        <div className="bg-gray-900 border border-gray-800 rounded-xl p-4 animate-fadeInUp" style={{ animationDelay: "180ms" }}>
          <h3 className="text-sm font-medium text-gray-400 flex items-center gap-2 mb-2">
            <TrendingUp size={14} className="text-emerald-400" />
            Trend Prediction
            <span className="text-xs text-gray-500">based on last {prediction.window_days} days</span>
          </h3>
          <div className="grid grid-cols-3 gap-4 text-sm">
            <div>
              <div className="text-xs text-gray-500">Today so far</div>
              <div className="text-lg font-semibold text-gray-200">{Math.round(prediction.today_ms / 3600000)}h {Math.round((prediction.today_ms % 3600000) / 60000)}m</div>
            </div>
            <div>
              <div className="text-xs text-gray-500">Today projected</div>
              <div className="text-lg font-semibold text-cyan-400">{Math.round(prediction.predicted_today_ms / 3600000)}h {Math.round((prediction.predicted_today_ms % 3600000) / 60000)}m</div>
            </div>
            <div>
              <div className="text-xs text-gray-500">Tomorrow projected</div>
              <div className="text-lg font-semibold text-emerald-400">{Math.round(prediction.predicted_tomorrow_ms / 3600000)}h {Math.round((prediction.predicted_tomorrow_ms % 3600000) / 60000)}m</div>
            </div>
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 gap-4 animate-fadeInUp" style={{ animationDelay: "200ms" }}>
        <AppUsagePie data={summary?.top_apps ?? []} />
        <HourlyHeatmap data={timeline} />
      </div>

      <div className="bg-gray-900 border border-gray-800 rounded-xl p-4 animate-fadeInUp" style={{ animationDelay: "250ms" }}>
        <h3 className="text-sm font-medium text-gray-400 flex items-center gap-2 mb-3">
          <FolderKanban size={14} className="text-cyan-400" />
          Projects
        </h3>
        {projectStats.length === 0 ? (
          <p className="text-xs text-gray-600">No projects configured — add them in Settings to track time by project.</p>
        ) : (
          <div className="space-y-2">
            {projectStats.slice(0, 5).map((p) => (
              <div key={p.project_id ?? 'uncategorized'} className="flex items-center gap-3">
                <span
                  className="w-2.5 h-2.5 rounded-full shrink-0"
                  style={{ backgroundColor: p.color || '#6b7280' }}
                />
                <span className="w-32 text-sm text-gray-200 truncate">{p.name}</span>
                <div className="flex-1 h-2 bg-gray-800 rounded-full overflow-hidden">
                  <div
                    className="h-full rounded-full"
                    style={{ width: `${Math.min(p.percentage, 100)}%`, backgroundColor: p.color || '#6b7280' }}
                  />
                </div>
                <span className="w-20 text-right text-sm text-gray-300">{formatDuration(p.total_ms)}</span>
                <span className="w-14 text-right text-xs text-gray-500">{Math.round(p.percentage)}%</span>
              </div>
            ))}
          </div>
        )}
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