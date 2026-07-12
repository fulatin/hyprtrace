import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts';
import type { HourlyBucket } from '../lib/types';

interface HourlyHeatmapProps {
  data: HourlyBucket[];
}

export default function HourlyHeatmap({ data }: HourlyHeatmapProps) {
  if (data.length === 0) {
    return (
      <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 flex items-center justify-center h-48 text-gray-400">
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
      <h3 className="text-sm font-medium text-gray-400 mb-4">24h Activity</h3>
      <ResponsiveContainer width="100%" height={200}>
        <BarChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke="#1f2937" />
          <XAxis
            dataKey="hour"
            tick={{ fill: '#6b7280', fontSize: 10 }}
            interval={2}
          />
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
          <Bar dataKey="minutes" fill="#22d3ee" radius={[2, 2, 0, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}