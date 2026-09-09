import { motion, useReducedMotion } from 'framer-motion';
import { useMemo, useState } from 'react';
import { Moon, Sunrise } from 'lucide-react';
import type { RhythmResponse } from '../../lib/types';
import { formatDuration, formatHour } from '../../lib/format';
import { WEEKDAYS } from './shared';

interface RhythmHeatmapProps {
  data: RhythmResponse;
}

/**
 * Average day-of-week × hour-of-day intensity grid. Each cell is the mean time
 * tracked in that hour on matching weekdays, so it reads as "what a typical
 * Tuesday looks like" rather than a raw sum.
 */
export default function RhythmHeatmap({ data }: RhythmHeatmapProps) {
  const reduced = useReducedMotion();
  const [hover, setHover] = useState<{ weekday: number; hour: number } | null>(null);

  const { grid, peak } = useMemo(() => {
    const g = new Map<string, { avg_ms: number; days: number; total_ms: number }>();
    for (const c of data.cells) g.set(`${c.weekday}-${c.hour}`, c);
    let peak: { weekday: number; hour: number; avg_ms: number; days: number } | null = null;
    for (const c of data.cells) {
      if (!peak || c.avg_ms > peak.avg_ms) peak = c;
    }
    return { grid: g, peak };
  }, [data]);

  const max = Math.max(data.max_avg_ms, 1);
  const hours = Array.from({ length: 24 }, (_, i) => i);

  const readout = hover
    ? {
        ...hover,
        cell: grid.get(`${hover.weekday}-${hover.hour}`),
      }
    : null;

  if (data.cells.length === 0) return null;

  return (
    <div>
      <div className="mb-2 flex h-5 flex-wrap items-center gap-x-3 gap-y-1 text-xs">
        {readout ? (
          <>
            <span className="font-medium text-fg">
              {WEEKDAYS[readout.weekday]} {formatHour(readout.hour)}
            </span>
            {readout.cell ? (
              <>
                <span className="tnum text-accent">{formatDuration(readout.cell.avg_ms)}</span>
                <span className="text-fg-faint">
                  average over {readout.cell.days} {readout.cell.days === 1 ? 'day' : 'days'} ·{' '}
                  {formatDuration(readout.cell.total_ms)} total
                </span>
              </>
            ) : (
              <span className="text-fg-faint">no activity</span>
            )}
          </>
        ) : (
          <span className="text-fg-faint">
            Hover a cell for that hour's average. Darker = more time tracked.
          </span>
        )}
        {peak && !hover && (
          <span className="ml-auto flex items-center gap-1.5 text-fg-faint">
            <Sunrise size={12} className="text-warn" />
            Peak: {WEEKDAYS[peak.weekday]} {formatHour(peak.hour)} · {formatDuration(peak.avg_ms)} avg
          </span>
        )}
      </div>

      <div className="overflow-x-auto">
        <div className="min-w-[620px]">
          {/* Hour axis */}
          <div className="mb-1 ml-10 flex">
            {hours.map((h) => (
              <div key={h} className="flex-1 text-center text-[9px] text-fg-faint">
                {h % 3 === 0 ? h : ''}
              </div>
            ))}
          </div>

          {WEEKDAYS.map((day, wd) => (
            <div key={day} className="mb-1 flex items-center">
              <div className="w-10 shrink-0 pr-2 text-right text-[10px] text-fg-muted">{day}</div>
              <div className="flex flex-1 gap-[2px]">
                {hours.map((h) => {
                  const cell = grid.get(`${wd}-${h}`);
                  const ratio = cell ? cell.avg_ms / max : 0;
                  const isNight = h >= 23 || h <= 5;
                  return (
                    <motion.div
                      key={h}
                      initial={reduced ? undefined : { opacity: 0, scale: 0.6 }}
                      animate={{ opacity: 1, scale: 1 }}
                      transition={{ delay: reduced ? 0 : Math.min(0.5, (wd * 24 + h) * 0.0016) }}
                      onMouseEnter={() => setHover({ weekday: wd, hour: h })}
                      onMouseLeave={() => setHover(null)}
                      className="h-6 flex-1 cursor-default rounded-[3px] transition-transform hover:scale-125 hover:ring-1 hover:ring-accent/60"
                      style={{
                        backgroundColor: cell
                          ? `rgb(var(--c-accent) / ${(0.1 + ratio * 0.9).toFixed(3)})`
                          : isNight
                            ? 'rgb(var(--c-surface-3))'
                            : 'rgb(var(--c-surface-2))',
                      }}
                      title={`${day} ${formatHour(h)} — ${cell ? formatDuration(cell.avg_ms) + ' avg' : 'no data'}`}
                    />
                  );
                })}
              </div>
            </div>
          ))}

          <div className="mt-3 flex items-center justify-between text-[10px] text-fg-faint">
            <span className="flex items-center gap-1.5">
              <Moon size={11} />
              Night hours are outlined in the background
            </span>
            <div className="flex items-center gap-1.5">
              <span>less</span>
              {[0.1, 0.35, 0.6, 0.85, 1].map((r) => (
                <span
                  key={r}
                  className="h-3 w-3 rounded-[3px]"
                  style={{ backgroundColor: `rgb(var(--c-accent) / ${r})` }}
                />
              ))}
              <span>more</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
