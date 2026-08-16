import { PieChart, Pie, Cell, Tooltip, ResponsiveContainer, Legend } from 'recharts';
import type { AppRank, AppMetadata } from '../lib/types';

const COLORS = [
  '#22d3ee', '#8b5cf6', '#10b981', '#f59e0b', '#ef4444',
  '#3b82f6', '#ec4899', '#14b8a6', '#f97316', '#6366f1',
];

interface AppUsagePieProps {
  data: AppRank[];
  metadata?: Record<string, AppMetadata>;
}

export default function AppUsagePie({ data, metadata }: AppUsagePieProps) {
  if (data.length === 0) {
    return (
      <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 flex items-center justify-center h-64 text-gray-400">
        No data available
      </div>
    );
  }

  const displayName = (cls: string) =>
    metadata?.[cls]?.display_name || cls;

  const chartData = data.map((app) => ({
    name: displayName(app.class),
    value: app.total_ms,
  }));

  return (
    <div className="bg-gray-900 border border-gray-800 rounded-xl p-4">
      <h3 className="text-sm font-medium text-gray-400 mb-4">App Usage Distribution</h3>
      <ResponsiveContainer width="100%" height={250}>
        <PieChart>
          <Pie
            data={chartData}
            cx="50%"
            cy="50%"
            innerRadius={60}
            outerRadius={100}
            paddingAngle={2}
            nameKey="name"
            dataKey="value"
          >
            {chartData.map((_entry, index) => (
              <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
            ))}
          </Pie>
          <Tooltip
            contentStyle={{ backgroundColor: '#1f2937', border: '1px solid #374151', borderRadius: '8px', color: '#e5e7eb' }}
            itemStyle={{ color: '#e5e7eb' }}
            labelStyle={{ color: '#e5e7eb' }}
            formatter={(value: number) => {
              const hours = Math.floor(value / 3600000);
              const mins = Math.floor((value % 3600000) / 60000);
              return `${hours}h ${mins}m`;
            }}
          />
          <Legend
            formatter={(value: string) => <span className="text-gray-300 text-xs">{value}</span>}
          />
        </PieChart>
      </ResponsiveContainer>
    </div>
  );
}