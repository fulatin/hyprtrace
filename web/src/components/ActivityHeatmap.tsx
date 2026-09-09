import { useEffect, useMemo, useRef, useState } from 'react';
import HeatMap from '@uiw/react-heat-map';
import { motion } from 'framer-motion';
import { CalendarDays } from 'lucide-react';
import type { DailyActivity } from '../lib/types';
import { formatDuration } from '../lib/format';
import { Panel } from './ui/Card';
import { EmptyState } from './ui/Feedback';
import { useTheme } from '../lib/theme';

interface ActivityHeatmapProps {
  data: DailyActivity[];
}

/** Intensity scale keyed by active minutes, tuned per theme. */
const SCALE_DARK = ['#16222e', '#155e75', '#0e7490', '#06b6d4', '#67e8f9'];
const SCALE_LIGHT = ['#e2e8f0', '#a5f3fc', '#67e8f9', '#22d3ee', '#0891b2'];

const LEGEND_LABELS = ['0', '<30m', '<2h', '<4h', '>=4h'];

/**
 * The library picks the colour of the first threshold strictly greater than the
 * count, so a day with 0 minutes would render as the second (active) colour.
 * We therefore set the fill ourselves from the cell's minute count.
 */
function fillFor(count: number, light: boolean): string {
  if (count <= 0) return light ? '#e2e8f0' : '#16222e';
  if (count < 30) return light ? '#a5f3fc' : '#155e75';
  if (count < 120) return light ? '#67e8f9' : '#0e7490';
  if (count < 240) return light ? '#22d3ee' : '#06b6d4';
  return light ? '#0891b2' : '#67e8f9';
}

/** Convert `yyyy-MM-dd` to the `YYYY/M/D` key the heatmap library uses. */
function heatDate(iso: string): string {
  const [y, m, d] = iso.slice(0, 10).split('-').map(Number);
  return `${y}/${m}/${d}`;
}

export default function ActivityHeatmap({ data }: ActivityHeatmapProps) {
  const { theme } = useTheme();
  const scale = theme === 'light' ? SCALE_LIGHT : SCALE_DARK;
  const containerRef = useRef<HTMLDivElement>(null);
  const [rectSize, setRectSize] = useState(11);

  // Size the cells so a full year fits the panel width instead of forcing a
  // horizontal scroll; falls back to scrolling only on very narrow screens.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const SPACE = 3;
    const WEEKS = 54;
    const compute = () => {
      const width = el.clientWidth;
      if (width <= 0) return;
      const size = Math.floor((width - WEEKS * SPACE - 6) / WEEKS);
      setRectSize(Math.max(7, Math.min(13, size)));
    };
    compute();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(compute);
    ro.observe(el);
    return () => ro.disconnect();
    // `data.length` matters: the first render takes the empty-state branch, so
    // the ref is only attached once data arrives — without this dep the effect
    // would never measure the real container.
  }, [data.length]);

  const { value, startDate, endDate, totals } = useMemo(() => {
    if (data.length === 0) {
      return {
        value: [] as { date: string; count: number }[],
        startDate: new Date(),
        endDate: new Date(),
        totals: null,
      };
    }

    // Sort by date so the grid span is correct even if the backend returns
    // days out of order, and key the count by the library's YYYY/M/D format so
    // the rectRender lookup matches a cell's valueItem.date.
    const sorted = [...data].sort((a, b) => a.date.localeCompare(b.date));
    const countByDate = new Map<string, number>();
    for (const d of sorted) {
      countByDate.set(heatDate(d.date), d.total_ms);
    }

    const value = sorted.map((d) => ({
      date: heatDate(d.date),
      count: Math.round(d.total_ms / 60000),
    }));

    const startDate = new Date(heatDate(sorted[0].date));
    const endDate = new Date(heatDate(sorted[sorted.length - 1].date));

    const totalMs = sorted.reduce((a, d) => a + d.total_ms, 0);
    const activeDays = sorted.filter((d) => d.total_ms > 0).length;
    const busiest = sorted.reduce((max, d) => (d.total_ms > max.total_ms ? d : max), sorted[0]);

    return { value, startDate, endDate, totals: { totalMs, activeDays, busiest, countByDate } };
  }, [data]);

  if (!totals || value.length === 0) {
    return (
      <Panel
        title="Yearly activity"
        icon={<CalendarDays size={15} className="text-accent" />}
      >
        <EmptyState
          icon={<CalendarDays size={20} />}
          title="No activity history yet"
          hint="The calendar fills in as days are tracked."
        />
      </Panel>
    );
  }

  return (
    <Panel
      title="Yearly activity"
      icon={<CalendarDays size={15} className="text-accent" />}
      hint={`last ${data.length} days · ${totals.activeDays} active · ${formatDuration(totals.totalMs)} tracked`}
    >
      <div ref={containerRef} className="overflow-x-auto pb-1">
        <HeatMap
          value={value}
          startDate={startDate}
          endDate={endDate}
          rectSize={rectSize}
          space={3}
          // Without an explicit width the library's <svg> falls back to the
          // 300px intrinsic default and only renders ~17 of the 53 weeks.
          style={{ width: '100%' }}
          // Disable the library's built-in legend (its cell gap is fixed at 1px
          // and looks cramped); we render our own with a wider gap below.
          legendCellSize={0}
          panelColors={scale}
          // Accessibility: every cell exposes its date + active time, and the
          // tooltip uses the shared formatter.
          rectRender={(props, valueItem) => {
            const ms = totals.countByDate.get(valueItem.date) ?? 0;
            const minutes = valueItem.count ?? 0;
            return (
              <rect
                {...props}
                fill={fillFor(minutes, theme === 'light')}
                rx={3}
                role="gridcell"
                aria-label={`${valueItem.date}: ${formatDuration(ms)}`}
              >
                <title>{`${valueItem.date}: ${formatDuration(ms)}`}</title>
              </rect>
            );
          }}
        />
      </div>

      {/* Custom legend with a comfortable gap between the intensity cells. */}
      <div className="mt-3 flex flex-wrap items-center justify-end gap-3">
        <span className="text-[10px] text-fg-faint">Less</span>
        <div className="flex items-center gap-2">
          {scale.map((c, i) => (
            <motion.div
              key={c}
              initial={{ opacity: 0, scale: 0.6 }}
              animate={{ opacity: 1, scale: 1 }}
              transition={{ delay: 0.2 + i * 0.05 }}
              className="flex items-center gap-1.5"
            >
              <span className="rounded-sm" style={{ width: 11, height: 11, backgroundColor: c }} />
              <span className="text-[10px] text-fg-faint">{LEGEND_LABELS[i]}</span>
            </motion.div>
          ))}
        </div>
        <span className="text-[10px] text-fg-faint">More</span>
      </div>

      {totals.busiest.total_ms > 0 && (
        <p className="mt-3 text-xs text-fg-faint">
          Busiest day: <span className="text-fg-muted">{totals.busiest.date}</span> (
          {formatDuration(totals.busiest.total_ms)})
        </p>
      )}
    </Panel>
  );
}
