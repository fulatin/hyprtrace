import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Cell } from 'recharts';
import type { AppRank } from '../lib/types';

const COLORS = [
  '#22d3ee', '#8b5cf6', '#10b981', '#f59e0b', '#ef4444',
  '#3b82f6', '#ec4899', '#14b8a6', '#f97316', '#6366f1',
];

interface AppRankingBarProps {
  data: AppRank[];
}

function formatDuration(ms: number): string {
  const hours = Math.floor(ms / 3600000);
  const mins = Math.floor((ms % 3600000) / 60000);
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

export default function AppRankingBar({ data }: AppRankingBarProps) {
  if (data.length === 0) {
    return (
      <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 flex items-center justify-center h-48 text-gray-400">
        No data available
      </div>
    );
  }

  const chartData = data.map((app) => ({
    name: app.class,
    minutes: Math.round(app.total_ms / 60000),
    percentage: app.percentage,
    display: formatDuration(app.total_ms),
  }));

  return (
    <div className="bg-gray-900 border border-gray-800 rounded-xl p-4">
      <ResponsiveContainer width="100%" height={data.length * 40 + 40}>
        <BarChart data={chartData} layout="vertical" margin={{ left: 80, right: 40 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#1f2937" horizontal={false} />
          <XAxis type="number" tick={{ fill: '#6b7280', fontSize: 10 }} />
          <YAxis
            type="category"
            dataKey="name"
            tick={{ fill: '#d1d5db', fontSize: 12 }}
            width={70}
          />
          <Tooltip
            contentStyle={{ backgroundColor: '#1f2937', border: '1px solid #374151', borderRadius: '8px', color: '#e5e7eb' }}
            itemStyle={{ color: '#e5e7eb' }}
            labelStyle={{ color: '#e5e7eb' }}
            formatter={(value: number) => [`${value} min`, 'Active Time']}
          />
          <Bar dataKey="minutes" radius={[0, 4, 4, 0]}>
            {chartData.map((_entry, index) => (
              <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}