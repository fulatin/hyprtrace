import { useEffect, useState } from 'react';
import { format } from 'date-fns';
import { api } from '../lib/api';
import type { HourlyBucket } from '../lib/types';
import TimelineChart from '../components/TimelineChart';

export default function Timeline() {
  const [date, setDate] = useState(format(new Date(), 'yyyy-MM-dd'));
  const [data, setData] = useState<HourlyBucket[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    api.timeline(date).then((d) => {
      setData(d);
      setLoading(false);
    });
  }, [date]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">Timeline</h2>
        <input
          type="date"
          value={date}
          onChange={(e) => setDate(e.target.value)}
          className="bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-sm text-gray-200 focus:ring-cyan-500 focus:border-cyan-500"
        />
      </div>

      {loading ? (
        <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 animate-pulse h-64" />
      ) : (
        <TimelineChart data={data} />
      )}
    </div>
  );
}