import { memo, useEffect, useMemo, useRef, useState } from 'react';
import HeatMap from '@uiw/react-heat-map';
import { motion } from 'framer-motion';
import { CalendarDays } from 'lucide-react';
import type { DailyActivity } from '../lib/types';
import { formatDuration } from '../lib/format';
import { HEAT_BUCKETS, heatDate, heatFill, heatLabel, heatScale } from '../lib/heatmap';
import { Panel } from './ui/Card';
import { EmptyState } from './ui/Feedback';
import { useTheme } from '../lib/theme';

interface ActivityHeatmapProps {
  data: DailyActivity[];
}

export default memo(function ActivityHeatmap({ data }: ActivityHeatmapProps) {
  const { theme } = useTheme();
  // Cells, legend and screen-reader text all read the same palette table
  // (`lib/heatmap.ts`); changing a colour there updates everything at once.
  const light = theme === 'light';
  const scale = heatScale(light ? 'light' : 'dark');
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
    // Keyed by the library's `YYYY/M/D` cell key, holding both the original
    // ISO date (for the label) and the measured milliseconds (for the fill).
    const byDate = new Map<string, { iso: string; ms: number }>();
    for (const d of sorted) {
      byDate.set(heatDate(d.date), { iso: d.date.slice(0, 10), ms: d.total_ms });
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

    return { value, startDate, endDate, totals: { totalMs, activeDays, busiest, byDate } };
  }, [data]);

  // Stable element identity: while its inputs are unchanged React bails out of
  // re-rendering this subtree, so a parent re-render does not make the library
  // rebuild its ~371 <rect> cells. Only a resize (rectSize) or theme change
  // produces a new element.
  const heatMap = useMemo(() => {
    if (!totals) return null;
    const { byDate } = totals;
    return (
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
        // Accessibility: each cell is focusable and announces its date plus
        // active time (or "no activity"), so the heatmap is usable with a
        // keyboard and a screen reader (issue #19). The fill comes from the
        // shared palette table; `panelColors` from the library is not used
        // because `rectRender` sets the fill itself.
        rectRender={(props, valueItem) => {
          const day = byDate.get(valueItem.date);
          const ms = day?.ms ?? 0;
          const minutes = valueItem.count ?? 0;
          const label = heatLabel(day?.iso ?? valueItem.date, ms, formatDuration);
          return (
            <rect
              {...props}
              fill={heatFill(minutes, light ? 'light' : 'dark')}
              rx={3}
              role="gridcell"
              tabIndex={0}
              aria-label={label}
            >
              <title>{label}</title>
            </rect>
          );
        }}
      />
    );
  }, [value, startDate, endDate, rectSize, scale, light, totals]);

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
      <div
        ref={containerRef}
        className="overflow-x-auto pb-1"
        role="grid"
        aria-label="Yearly activity heatmap, one cell per day"
      >
        {heatMap}
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
              <span className="text-[10px] text-fg-faint">{HEAT_BUCKETS[i].label}</span>
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
});
