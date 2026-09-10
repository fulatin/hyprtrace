import { useEffect, useMemo, useState } from 'react';
import { format } from 'date-fns';
import { motion, useReducedMotion } from 'framer-motion';
import { CalendarRange, Search as SearchIcon, SearchX } from 'lucide-react';
import { api } from '../lib/api';
import type { AppMetadata, SearchHit } from '../lib/types';
import AppName from '../components/AppName';
import { EmptyState, SkeletonPanel } from '../components/ui/Feedback';
import { Reveal, RevealItem } from '../components/ui/Reveal';
import { formatSpan } from '../lib/format';
import { listItem, staggerContainer } from '../lib/motion';

/** How long typing settles before a request goes out. */
const DEBOUNCE_MS = 250;
const LIMIT = 100;

/**
 * The hit's local calendar date. The API stores UTC timestamps, and grouping by
 * the *local* day is what the rest of the app does (the summaries are bucketed
 * that way), so the browser's own timezone is the right one to read them in.
 */
function localDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso.slice(0, 10) : format(d, 'yyyy-MM-dd');
}

function localTime(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? '' : format(d, 'HH:mm');
}

export default function SearchPage() {
  const [query, setQuery] = useState('');
  const [settled, setSettled] = useState('');
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [appMetadata, setAppMetadata] = useState<Record<string, AppMetadata>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const reduced = useReducedMotion();

  // Typing must not fire a request per keystroke; the index is fast but the
  // point is a search, not a keystroke echo.
  useEffect(() => {
    const timer = setTimeout(() => setSettled(query.trim()), DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    if (!settled) {
      setHits([]);
      setError('');
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError('');
    api
      .search(settled, from, to, LIMIT)
      .then((res) => {
        if (cancelled) return;
        // The server answers 200 with `{"error": ...}` for its own failures;
        // `fetchJSON` only throws on a non-2xx status, so an object instead of
        // an array has to be treated as a failure here.
        if (!Array.isArray(res)) {
          setHits([]);
          setError('Search failed.');
          return;
        }
        setHits(res);
      })
      .catch(() => {
        if (!cancelled) {
          setHits([]);
          setError('Search failed.');
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [settled, from, to]);

  // Friendly names + icons for the apps present in the results.
  useEffect(() => {
    const unique = Array.from(new Set(hits.map((h) => h.class)));
    if (unique.length === 0) {
      setAppMetadata({});
      return;
    }
    api
      .appsMetadata(unique)
      .then((res) => setAppMetadata(res.entries))
      .catch(() => setAppMetadata({}));
  }, [hits]);

  // Group by local day: "when did I see this" is answered by the day first.
  const days = useMemo(() => {
    const byDay = new Map<string, SearchHit[]>();
    for (const hit of hits) {
      const day = localDate(hit.started_at);
      const bucket = byDay.get(day);
      if (bucket) bucket.push(hit);
      else byDay.set(day, [hit]);
    }
    return Array.from(byDay.entries()).sort((a, b) => (a[0] < b[0] ? 1 : -1));
  }, [hits]);

  const totalMs = useMemo(
    () => hits.reduce((sum, hit) => sum + (hit.duration_ms ?? 0), 0),
    [hits]
  );

  return (
    <div className="space-y-6">
      <Reveal className="flex flex-wrap items-center justify-between gap-3">
        <RevealItem>
          <h2 className="flex items-center gap-2 text-xl font-bold">
            <SearchIcon size={20} className="text-accent" />
            Search
          </h2>
          <p className="mt-1 text-xs text-fg-muted">
            Find when you worked on a file, a tab or a page. Matches anywhere in the title, and
            several words all have to appear.
          </p>
        </RevealItem>

        <RevealItem className="flex flex-wrap items-center gap-2">
          <CalendarRange size={14} className="text-fg-faint" />
          <input
            type="date"
            value={from}
            onChange={(e) => setFrom(e.target.value)}
            className="input"
            aria-label="From date"
          />
          <span className="text-xs text-fg-faint">→</span>
          <input
            type="date"
            value={to}
            onChange={(e) => setTo(e.target.value)}
            className="input"
            aria-label="To date"
          />
          {(from || to) && (
            <button
              type="button"
              className="btn-ghost text-xs"
              onClick={() => {
                setFrom('');
                setTo('');
              }}
            >
              All time
            </button>
          )}
        </RevealItem>
      </Reveal>

      <div className="card card-pad">
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search window titles…"
          className="input w-full text-base"
          aria-label="Search window titles"
          autoFocus
        />
        {settled && !loading && !error && (
          <p className="mt-2 text-xs text-fg-faint">
            {hits.length === 0
              ? 'No matching titles.'
              : `${hits.length}${hits.length === LIMIT ? '+' : ''} match${hits.length === 1 ? '' : 'es'} · ${formatSpan(totalMs)} tracked`}
          </p>
        )}
      </div>

      {error && <EmptyState icon={<SearchX size={22} />} title="Search failed" hint={error} />}

      {!settled && !error && (
        <EmptyState
          icon={<SearchIcon size={22} />}
          title="Start typing"
          hint="Titles are indexed in full, so a file name, a page title or a fragment all work. Dates are optional — leave them empty to search everything."
        />
      )}

      {loading && <SkeletonPanel height="h-32" />}

      {!loading && !error && settled && hits.length === 0 && (
        <EmptyState
          icon={<SearchX size={22} />}
          title="Nothing found"
          hint="Try fewer or shorter words. The match is a substring, so part of a word is enough."
        />
      )}

      {!loading &&
        days.map(([day, dayHits]) => (
          <section key={day} className="space-y-2">
            <div className="flex items-baseline justify-between">
              <h3 className="text-sm font-semibold text-fg-muted">{day}</h3>
              <span className="text-xs text-fg-faint">
                {dayHits.length} session{dayHits.length === 1 ? '' : 's'}
              </span>
            </div>
            <motion.ul
              className="card divide-y divide-line"
              variants={reduced ? undefined : staggerContainer(dayHits.length)}
              initial="hidden"
              animate="show"
            >
              {dayHits.map((hit) => (
                <motion.li
                  key={hit.id}
                  variants={reduced ? undefined : listItem}
                  className="flex items-center gap-3 px-4 py-2.5"
                >
                  <span className="tnum w-12 shrink-0 text-xs text-fg-faint">
                    {localTime(hit.started_at)}
                  </span>
                  <span className="hidden w-40 shrink-0 sm:block">
                    <AppName cls={hit.class} metadata={appMetadata[hit.class]} />
                  </span>
                  <span className="min-w-0 flex-1 truncate text-sm" title={hit.title}>
                    {hit.snippet || hit.title}
                  </span>
                  <span className="tnum shrink-0 text-xs text-fg-muted">
                    {formatSpan(hit.duration_ms ?? 0)}
                  </span>
                </motion.li>
              ))}
            </motion.ul>
          </section>
        ))}
    </div>
  );
}
