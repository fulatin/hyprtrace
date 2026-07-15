import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts';
import type { DailyTrend } from '../lib/types';

interface AppTrendChartProps {
  data: DailyTrend[];
  range?: 'today' | 'week' | 'month';
}

function formatDuration(ms: number): string {
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

export default function AppTrendChart({ data, range }: AppTrendChartProps) {
  if (data.length === 0) {
    return (
      <div className="text-gray-400 text-sm p-4">No trend data available</div>
    );
  }

  const formatXLabel = (dateStr: string) => {
    if (range === 'today') return '';
    return dateStr.slice(5);
  };

  const chartData = data.map((d) => ({
    date: d.date,
    label: formatXLabel(d.date),
    minutes: Math.round(d.total_ms / 60000),
    sessions: d.session_count,
  }));

  return (
    <div className="mt-4 bg-gray-900 border border-gray-800 rounded-xl p-4">
      <div className="flex items-center gap-2 mb-2">
        <h4 className="text-sm font-medium text-gray-400">Daily Trend</h4>
        {range === 'week' && <span className="text-xs text-gray-500">(past 7 days)</span>}
        {range === 'month' && <span className="text-xs text-gray-500">(past 30 days)</span>}
      </div>
      <ResponsiveContainer width="100%" height={180}>
        <LineChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke="#1f2937" />
          <XAxis dataKey="label" tick={{ fill: '#6b7280', fontSize: 10 }} />
          <YAxis tick={{ fill: '#6b7280', fontSize: 10 }} />
          <Tooltip
            contentStyle={{ backgroundColor: '#1f2937', border: '1px solid #374151', borderRadius: '8px', color: '#e5e7eb' }}
            itemStyle={{ color: '#e5e7eb' }}
            labelStyle={{ color: '#e5e7eb' }}
            labelFormatter={(label, payload) => {
              if (payload?.[0]) return (payload[0] as any).payload.date;
              return label;
            }}
            formatter={(value: number) => [formatDuration(value * 60000), 'Active Time']}
          />
          <Line type="monotone" dataKey="minutes" stroke="#22d3ee" strokeWidth={2} dot={{ fill: '#22d3ee', r: 3 }} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}