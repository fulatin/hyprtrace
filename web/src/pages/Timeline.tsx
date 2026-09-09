import { useEffect, useMemo, useState } from 'react';
import { format } from 'date-fns';
import { motion, useReducedMotion } from 'framer-motion';
import { AppWindow, CalendarOff, Clock, Hourglass, Timer } from 'lucide-react';
import { api } from '../lib/api';
import type { Session } from '../lib/types';
import ErrorState from '../components/ErrorState';
import StatCard from '../components/StatCard';
import { Panel } from '../components/ui/Card';
import { EmptyState, SkeletonPanel } from '../components/ui/Feedback';
import { Reveal, RevealItem } from '../components/ui/Reveal';
import { formatDuration, formatHour } from '../lib/format';
import { listItem, spring } from '../lib/motion';

// Stable per-app series palette. These are data-series colours (not UI chrome),
// so they stay the same hex in both themes — the hash below keeps every class
// pinned to one colour across reloads and pages.
const CATEGORY_COLORS = [
  '#22d3ee', '#a78bfa', '#34d399', '#f472b6', '#fbbf24',
  '#60a5fa', '#f87171', '#4ade80', '#e879f9', '#38bdf8',
];

// Padding (minutes) added on both sides of the active time range.
const AXIS_PADDING = 15;
const DAY_MINUTES = 1440;
// The "paint left to right" stagger is never allowed to run longer than this.
const MAX_BLOCK_DELAY = 0.4;

function colorForClass(cls: string): string {
  let h = 0;
  for (let i = 0; i < cls.length; i++) h = (h * 31 + cls.charCodeAt(i)) >>> 0;
  return CATEGORY_COLORS[h % CATEGORY_COLORS.length];
}

function toMinutes(iso: string): number {
  const d = new Date(iso);
  return d.getHours() * 60 + d.getMinutes();
}

// Minutes of the current moment, clamped to [0, 1440]. Used as the end point
// of an ongoing (not-yet-ended) session so its block doesn't stretch to 23:59.
function nowMinutes(): number {
  return Math.max(0, Math.min(toMinutes(new Date().toISOString()), DAY_MINUTES));
}

function formatDur(ms: number): string {
  const m = Math.round(ms / 60000);
  if (m >= 60) return `${Math.floor(m / 60)}h ${m % 60}m`;
  return `${m}m`;
}

/** "09:15" for an axis position expressed in minutes past midnight. */
function minutesLabel(mins: number): string {
  const h = Math.floor(mins / 60) % 24;
  const m = Math.round(mins % 60);
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
}

/** Summary duration: stored value, else the span between start and end/now. */
function sessionMs(s: Session): number {
  if (s.duration_ms !== null) return s.duration_ms;
  const start = new Date(s.started_at).getTime();
  const end = s.ended_at ? new Date(s.ended_at).getTime() : Date.now();
  return Math.max(0, end - start);
}

const formatCount = (n: number) => String(Math.round(n));

interface TimelineBlock {
  id: number;
  left: number;
  width: number;
  mins: number;
  title: string;
}

interface TimelineRow {
  cls: string;
  color: string;
  count: number;
  totalMs: number;
  blocks: TimelineBlock[];
}

export default function Timeline() {
  const [date, setDate] = useState(format(new Date(), 'yyyy-MM-dd'));
  const [sessions, setSessions] = useState<Session[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const reduced = useReducedMotion();

  useEffect(() => {
    setLoading(true);
    setError(null);
    // Fetch the whole day (large per_page) ordered by start time.
    api.sessions(date, date, 1, 2000).then((res) => {
      const sorted = [...res.data].sort((a, b) => a.started_at.localeCompare(b.started_at));
      setSessions(sorted);
      setLoading(false);
    }).catch((e) => {
      setError(e instanceof Error ? e.message : 'Unknown error');
      setLoading(false);
    });
  }, [date, reloadKey]);

  // Group sessions by app class so each app occupies a single row with all of
  // its time blocks, instead of one row per session.
  const groups = useMemo(() => {
    const map = new Map<string, Session[]>();
    for (const s of sessions) {
      const arr = map.get(s.class) ?? [];
      arr.push(s);
      map.set(s.class, arr);
    }
    return Array.from(map.entries());
  }, [sessions]);

  // Dynamic horizontal axis: only show the window where the user was actually
  // active (clamped to the day), padded slightly on each side. Falls back to
  // the full 24h when there are no sessions.
  const axis = useMemo(() => {
    if (sessions.length === 0) {
      return { start: 0, end: DAY_MINUTES };
    }
    let minStart = DAY_MINUTES;
    let maxEnd = 0;
    for (const s of sessions) {
      const startM = Math.max(0, Math.min(toMinutes(new Date(s.started_at).toISOString()), DAY_MINUTES));
      const endM = s.ended_at
        ? Math.max(0, Math.min(toMinutes(new Date(s.ended_at).toISOString()), DAY_MINUTES))
        : nowMinutes();
      if (startM < minStart) minStart = startM;
      if (endM > maxEnd) maxEnd = endM;
    }
    let start = Math.max(0, minStart - AXIS_PADDING);
    let end = Math.min(DAY_MINUTES, maxEnd + AXIS_PADDING);
    // Guarantee a minimum visible span so tiny windows stay readable.
    const MIN_SPAN = 60;
    if (end - start < MIN_SPAN) {
      const mid = (start + end) / 2;
      start = Math.max(0, mid - MIN_SPAN / 2);
      end = Math.min(DAY_MINUTES, mid + MIN_SPAN / 2);
    }
    return { start, end };
  }, [sessions]);

  const axisLen = Math.max(axis.end - axis.start, 1);
  const isToday = date === format(new Date(), 'yyyy-MM-dd');
  const nowM = nowMinutes();
  const showNow = isToday && nowM >= axis.start && nowM <= axis.end;

  // Hour tick marks within the active window.
  const hourTicks = useMemo(() => {
    const ticks: number[] = [];
    for (let h = Math.floor(axis.start / 60); h * 60 <= axis.end; h++) {
      ticks.push(h);
    }
    return ticks;
  }, [axis]);

  // Gridline positions: every 30 min, with the hour marks emphasised in JSX.
  const gridMinutes = useMemo(() => {
    const lines: number[] = [];
    for (let m = Math.floor(axis.start / 30) * 30; m <= axis.end; m += 30) {
      lines.push(m);
    }
    return lines;
  }, [axis]);

  // Geometry for every block, computed once per day/axis change. The maths is
  // the original one: clamped to day bounds, ongoing sessions end at "now".
  const rows = useMemo<TimelineRow[]>(() => groups.map(([cls, items]) => {
    const color = colorForClass(cls);
    let totalMs = 0;
    const blocks: TimelineBlock[] = [];
    for (const s of items) {
      const start = new Date(s.started_at);
      const startM = Math.max(0, Math.min(toMinutes(start.toISOString()), DAY_MINUTES));
      const endM = s.ended_at
        ? Math.max(0, Math.min(toMinutes(new Date(s.ended_at).toISOString()), DAY_MINUTES))
        : nowMinutes();
      const ms = sessionMs(s);
      totalMs += ms;
      if (endM <= startM) continue;
      const left = ((startM - axis.start) / axisLen) * 100;
      const width = ((endM - startM) / axisLen) * 100;
      const mins = Math.round((s.duration_ms ?? 0) / 60000);
      blocks.push({ id: s.id, left, width, mins, title: `${cls} — ${s.title} — ${formatDur(ms)}` });
    }
    return { cls, color, count: items.length, totalMs, blocks };
  }), [groups, axis, axisLen]);

  // Running index of each row's first block, so the left-to-right stagger is
  // continuous across rows instead of restarting on every row.
  const rowOffsets = useMemo(() => {
    const offsets: number[] = [];
    let acc = 0;
    for (const [, items] of groups) {
      offsets.push(acc);
      acc += items.length;
    }
    return offsets;
  }, [groups]);

  const blockCount = groups.reduce((n, entry) => n + entry[1].length, 0);
  const blockStep = Math.min(0.035, MAX_BLOCK_DELAY / Math.max(1, blockCount));

  // Summary strip — derived from the sessions already in memory.
  const summary = useMemo(() => {
    let totalMs = 0;
    let longest: { cls: string; ms: number } | null = null;
    const byHour = new Map<number, number>();
    for (const s of sessions) {
      const ms = sessionMs(s);
      totalMs += ms;
      if (!longest || ms > longest.ms) longest = { cls: s.class, ms };
      const hour = new Date(s.started_at).getHours();
      byHour.set(hour, (byHour.get(hour) ?? 0) + ms);
    }
    let busiest: { hour: number; ms: number } | null = null;
    for (const [hour, ms] of byHour) {
      if (!busiest || ms > busiest.ms) busiest = { hour, ms };
    }
    return { totalMs, appCount: groups.length, longest, busiest };
  }, [sessions, groups]);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="flex items-center gap-2 text-xl font-bold">
            <Clock size={20} className="text-accent" />
            Session Timeline
          </h2>
          <p className="mt-1 text-xs text-fg-muted">
            One row per app, each block is a session.
            {sessions.length > 0 && ` ${sessions.length} sessions · ${groups.length} apps.`}
          </p>
        </div>
        <label className="flex items-center gap-2 text-xs text-fg-muted">
          Day
          <input
            type="date"
            value={date}
            onChange={(e) => setDate(e.target.value)}
            className="input"
          />
        </label>
      </div>

      {loading ? (
        <SkeletonPanel />
      ) : error ? (
        <ErrorState message={error} onRetry={() => setReloadKey((k) => k + 1)} />
      ) : rows.length === 0 ? (
        <EmptyState
          icon={<CalendarOff size={22} />}
          title="No activity recorded this day"
          hint="The timeline will appear here once sessions exist."
        />
      ) : (
        <>
          <Reveal className="grid grid-cols-2 gap-4 lg:grid-cols-4">
            <StatCard
              icon={<Clock size={15} />}
              label="Active time"
              value={summary.totalMs}
              format={formatDuration}
              subtext={isToday ? 'so far today' : 'total for the day'}
              accent="accent"
            />
            <StatCard
              icon={<AppWindow size={15} />}
              label="Apps"
              value={summary.appCount}
              format={formatCount}
              subtext={`${sessions.length} sessions`}
              accent="accent-2"
            />
            <StatCard
              icon={<Hourglass size={15} />}
              label="Longest block"
              value={summary.longest?.ms ?? 0}
              format={formatDuration}
              subtext={summary.longest?.cls ?? '—'}
              accent="good"
            />
            <StatCard
              icon={<Timer size={15} />}
              label="Busiest hour"
              value={summary.busiest ? formatHour(summary.busiest.hour) : '—'}
              subtext={summary.busiest ? formatDuration(summary.busiest.ms) : 'no data'}
              accent="warn"
            />
          </Reveal>

          <Panel
            title="Timeline"
            icon={<Clock size={14} />}
            hint={`active window ${minutesLabel(axis.start)} – ${minutesLabel(axis.end)}`}
            flush
          >
            <div className="overflow-x-auto px-5 pb-5">
              <div className="flex">
                {/* Row labels */}
                <div className="shrink-0 w-52 pr-3">
                  {/* Spacer matching the hour-label row so app rows align */}
                  <div className="h-6 mb-1" />
                  <Reveal stagger={0.045}>
                    {rows.map((row) => (
                      <RevealItem
                        key={row.cls}
                        variants={listItem}
                        className="flex h-7 items-center gap-1.5 overflow-hidden"
                        title={row.cls}
                      >
                        <span
                          className="h-1.5 w-1.5 shrink-0 rounded-full"
                          style={{ backgroundColor: row.color }}
                        />
                        <span className="truncate text-xs text-fg-muted">{row.cls}</span>
                        <span className="ml-auto shrink-0 text-[10px] tnum text-fg-faint">
                          {row.count}×
                        </span>
                        <span className="shrink-0 text-[10px] tnum text-fg-faint">
                          {formatDuration(row.totalMs)}
                        </span>
                      </RevealItem>
                    ))}
                  </Reveal>
                </div>

                {/* Chart area: dynamic active-time axis */}
                <div className="flex-1 min-w-[480px]">
                  {/* Hour labels in normal flow (not clipped) */}
                  <div className="relative h-6 mb-1">
                    {hourTicks.map((h) => {
                      const hh = h % 24;
                      const left = (((h * 60 - axis.start) / axisLen) * 100);
                      return (
                        <div
                          key={`l${h}`}
                          className="absolute -translate-x-1/2 text-[10px] text-fg-faint whitespace-nowrap"
                          style={{ left: `${left}%` }}
                        >
                          {hh}:00
                        </div>
                      );
                    })}
                  </div>

                  {/* Time blocks, one row per app class */}
                  <div className="relative">
                    {/* Gridlines: every 30 min faint, every hour stronger */}
                    {gridMinutes.map((m) => (
                      <div
                        key={`g${m}`}
                        className={`pointer-events-none absolute top-0 bottom-0 border-l ${
                          m % 60 === 0 ? 'border-line-strong' : 'border-line'
                        }`}
                        style={{ left: `${((m - axis.start) / axisLen) * 100}%` }}
                      />
                    ))}

                    {rows.map((row, rowIdx) => (
                      <motion.div
                        key={row.cls}
                        className="relative h-7"
                        initial={reduced ? false : { opacity: 0 }}
                        animate={{ opacity: 1 }}
                        transition={{
                          duration: 0.3,
                          ease: [0.22, 1, 0.36, 1],
                          delay: reduced ? 0 : Math.min(rowIdx * 0.04, 0.2),
                        }}
                      >
                        {row.blocks.map((b, i) => {
                          const idx = (rowOffsets[rowIdx] ?? 0) + i;
                          return (
                            <motion.div
                              key={b.id}
                              className="absolute top-1 h-5 rounded-sm flex items-center overflow-hidden"
                              style={{
                                left: `${b.left}%`,
                                width: `${b.width}%`,
                                transformOrigin: 'left',
                                backgroundColor: `${row.color}33`,
                                border: `1px solid ${row.color}66`,
                              }}
                              title={b.title}
                              initial={reduced ? false : { scaleX: 0, opacity: 0 }}
                              animate={{ scaleX: 1, opacity: 0.88 }}
                              transition={{
                                duration: 0.34,
                                ease: [0.22, 1, 0.36, 1],
                                delay: reduced ? 0 : Math.min(idx * blockStep, MAX_BLOCK_DELAY),
                              }}
                              whileHover={{ y: -2, opacity: 1, zIndex: 20, transition: spring }}
                            >
                              <span className="text-[9px] text-fg px-1 truncate">
                                {b.width > 3 ? `${b.mins}m` : ''}
                              </span>
                            </motion.div>
                          );
                        })}
                      </motion.div>
                    ))}

                    {/* "Now" line — today only, gently pulsing. */}
                    {showNow && (
                      <motion.div
                        className="pointer-events-none absolute bottom-0 top-0 z-10 w-px bg-accent"
                        style={{ left: `${((nowM - axis.start) / axisLen) * 100}%` }}
                        initial={reduced ? false : { opacity: 0, scaleY: 0.7 }}
                        animate={{ opacity: 1, scaleY: 1 }}
                        transition={{ duration: 0.35, ease: [0.22, 1, 0.36, 1] }}
                      >
                        <motion.span
                          className="absolute -left-[3px] -top-0.5 h-1.5 w-1.5 rounded-full bg-accent shadow-glow"
                          animate={reduced ? undefined : { scale: [1, 1.9, 1], opacity: [1, 0.5, 1] }}
                          transition={reduced ? undefined : { duration: 2.2, repeat: Infinity, ease: 'easeInOut' }}
                        />
                      </motion.div>
                    )}
                  </div>
                </div>
              </div>
            </div>
          </Panel>
        </>
      )}
    </div>
  );
}
