import { useEffect, useState } from 'react';
import { format, subDays } from 'date-fns';
import { api } from '../lib/api';
import type { AppRank, DailyTrend } from '../lib/types';
import AppRankingBar from '../components/AppRankingBar';
import AppTrendChart from '../components/AppTrendChart';

type Range = 'today' | 'week' | 'month';

export default function Apps() {
  const [range, setRange] = useState<Range>('today');
  const [data, setData] = useState<AppRank[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedApp, setSelectedApp] = useState<string | null>(null);
  const [trend, setTrend] = useState<DailyTrend[]>([]);

  const getDateRange = () => {
    const today = format(new Date(), 'yyyy-MM-dd');
    switch (range) {
      case 'today':
        return { from: today, to: today };
      case 'week':
        return { from: format(subDays(new Date(), 7), 'yyyy-MM-dd'), to: today };
      case 'month':
        return { from: format(subDays(new Date(), 30), 'yyyy-MM-dd'), to: today };
    }
  };

  useEffect(() => {
    setLoading(true);
    const { from, to } = getDateRange();
    api.appRanking(from, to, 15).then((d) => {
      setData(d);
      setLoading(false);
    });
  }, [range]);

  useEffect(() => {
    if (!selectedApp) {
      setTrend([]);
      return;
    }
    const { from, to } = getDateRange();
    api.appTrend(selectedApp, from, to).then(setTrend);
  }, [selectedApp, range]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">App Ranking</h2>
        <div className="flex gap-2">
          {(['today', 'week', 'month'] as Range[]).map((r) => (
            <button
              key={r}
              onClick={() => { setRange(r); setSelectedApp(null); }}
              className={`px-3 py-1 rounded-lg text-sm transition-colors ${
                range === r
                  ? 'bg-cyan-600 text-white'
                  : 'bg-gray-800 text-gray-400 hover:bg-gray-700'
              }`}
            >
              {r.charAt(0).toUpperCase() + r.slice(1)}
            </button>
          ))}
        </div>
      </div>

      {loading ? (
        <div className="bg-gray-900 border border-gray-800 rounded-xl p-6 animate-pulse h-64" />
      ) : (
        <>
          <div onClick={() => setSelectedApp(null)}>
            <AppRankingBar data={data} />
          </div>
          {selectedApp && (
            <div>
              <h3 className="text-sm font-medium text-gray-400 mb-2">{selectedApp} - {range === 'today' ? 'Today' : range === 'week' ? '7-Day' : '30-Day'} Trend</h3>
              <AppTrendChart data={trend} range={range} />
            </div>
          )}
          <div className="space-y-2">
            {data.map((app, i) => (
              <div
                key={app.class}
                onClick={() => setSelectedApp(app.class)}
                className={`flex items-center justify-between p-3 rounded-lg cursor-pointer transition-colors ${
                  selectedApp === app.class ? 'bg-gray-800 border border-cyan-500/30' : 'bg-gray-900 border border-gray-800 hover:bg-gray-800'
                }`}
              >
                <div className="flex items-center gap-3">
                  <span className="text-xs text-gray-400 w-6">{i + 1}</span>
                  <span className="text-sm font-medium">{app.class}</span>
                </div>
                <div className="flex items-center gap-4">
                  <span className="text-xs text-gray-400">{app.percentage.toFixed(1)}%</span>
                  <span className="text-sm text-cyan-400">
                    {Math.floor(app.total_ms / 3600000)}h {Math.floor((app.total_ms % 3600000) / 60000)}m
                  </span>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}