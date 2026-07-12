import type { ReactNode } from 'react';

interface StatCardProps {
  icon: ReactNode;
  label: string;
  value: string;
  subtext?: string;
}

export default function StatCard({ icon, label, value, subtext }: StatCardProps) {
  return (
    <div className="bg-gray-900 border border-gray-800 rounded-xl p-4 flex flex-col gap-2">
      <div className="flex items-center gap-2 text-gray-400 text-sm">
        {icon}
        <span>{label}</span>
      </div>
      <div className="text-2xl font-bold text-gray-100">{value}</div>
      {subtext && <div className="text-xs text-gray-400">{subtext}</div>}
    </div>
  );
}