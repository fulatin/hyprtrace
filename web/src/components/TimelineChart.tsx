import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Legend } from 'recharts';
import type { HourlyBucket } from '../lib/types';

const COLORS = [
  '#22d3ee', '#8b5cf6', '#10b981', '#f59e0b', '#ef4444',
  '#3b82f6', '#ec4899', '#14b8a6', '#f97316', '#6366f1',
];

interface TimelineChartProps {
  data: HourlyBucket[];
}

export default function TimelineChart({ data }: TimelineChartProps) {
  if (data.length === 0) {
    return (
      <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 flex items-center justify-center h-64 text-gray-400">
        No data available
      </div>
    );
  }

  const chartData = data.map((bucket) => ({
    hour: `${bucket.hour}:00`,
    minutes: Math.round(bucket.total_ms / 60000),
    sessions: bucket.session_count,
  }));

  return (
    <div className="bg-gray-900 border border-gray-800 rounded-xl p-4">
      <ResponsiveContainer width="100%" height={350}>
        <BarChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke="#1f2937" />
          <XAxis dataKey="hour" tick={{ fill: '#6b7280', fontSize: 10 }} />
          <YAxis
            tick={{ fill: '#6b7280', fontSize: 10 }}
            label={{ value: 'min', angle: -90, position: 'insideLeft', fill: '#6b7280', fontSize: 10 }}
          />
          <Tooltip
            contentStyle={{ backgroundColor: '#1f2937', border: '1px solid #374151', borderRadius: '8px', color: '#e5e7eb' }}
            itemStyle={{ color: '#e5e7eb' }}
            labelStyle={{ color: '#e5e7eb' }}
            formatter={(value: number, name: string) => {
              if (name === 'minutes') return [`${value} min`, 'Active Time'];
              return [value, name];
            }}
          />
          <Legend
            formatter={(_value: string) => <span className="text-gray-300 text-xs">Active Time (min)</span>}
          />
          <Bar dataKey="minutes" name="Active Time (min)" fill={COLORS[0]} radius={[2, 2, 0, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}