import { Suspense, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { NavLink, Outlet, useLocation } from 'react-router-dom';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import {
  LayoutDashboard,
  BarChart3,
  Clock,
  List,
  FileText,
  Search,
  Bot,
  Settings,
  Sparkles,
  PanelLeftClose,
  PanelLeftOpen,
  Activity,
} from 'lucide-react';
import ThemeToggle from './ui/ThemeToggle';
import { SkeletonPanel, SkeletonStats } from './ui/Feedback';
import { pageTransition, spring } from '../lib/motion';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/insights', icon: Sparkles, label: 'Insights' },
  { to: '/apps', icon: BarChart3, label: 'Apps' },
  { to: '/timeline', icon: Clock, label: 'Timeline' },
  { to: '/sessions', icon: List, label: 'Sessions' },
  { to: '/titles', icon: FileText, label: 'Documents' },
  { to: '/search', icon: Search, label: 'Search' },
  { to: '/ai', icon: Bot, label: 'AI Chat' },
  { to: '/settings', icon: Settings, label: 'Settings' },
];

const COLLAPSE_KEY = 'hyprtrace.sidebar.collapsed';

/**
 * Skeleton for a route whose lazy chunk is still being fetched. It lives inside
 * the animated page container, so the shell (sidebar, header, ambient wash)
 * never unmounts while the route loads.
 */
function RouteFallback() {
  return (
    <div className="space-y-5" aria-busy="true" aria-live="polite">
      <div className="skeleton h-8 w-56" />
      <SkeletonStats count={4} />
      <div className="grid gap-5 lg:grid-cols-2">
        <SkeletonPanel height="h-52" />
        <SkeletonPanel height="h-52" />
      </div>
    </div>
  );
}

export default function Layout() {
  const location = useLocation();
  const mainRef = useRef<HTMLElement>(null);
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return window.localStorage.getItem(COLLAPSE_KEY) === '1';
    } catch {
      return false;
    }
  });

  // Start every page at its top instead of inheriting the previous page's
  // scroll offset (which could leave the new page's first sections above the
  // viewport, so their scroll-reveal never fired and the page looked empty).
  useLayoutEffect(() => {
    mainRef.current?.scrollTo({ top: 0, behavior: 'auto' });
  }, [location.pathname]);

  useEffect(() => {
    try {
      window.localStorage.setItem(COLLAPSE_KEY, collapsed ? '1' : '0');
    } catch {
      // Storage unavailable — the in-memory state still applies.
    }
  }, [collapsed]);

  const active = navItems.find((item) =>
    item.to === '/' ? location.pathname === '/' : location.pathname.startsWith(item.to),
  );

  // The CSS reduced-motion rule in index.css only covers CSS animations; a
  // framer transition is JavaScript and would still fade the whole page in.
  // `Reveal`/`CountUp` already honour the preference — the route transition is
  // the largest motion in the app, so it must too.
  const reducedMotion = useReducedMotion();

  return (
    <div className="relative flex h-screen overflow-hidden bg-bg">
      {/* Ambient depth: two very soft colour washes behind everything.
          Pre-baked radial gradients (see .wash-* in index.css) instead of a
          `blur(120px)` filter on a solid disc: visually equivalent, but no
          420px layer has to be rasterised through a filter before the first
          frame — and the whole chrome sits on top of it. */}
      <div className="pointer-events-none fixed inset-0 overflow-hidden">
        <div className="wash-accent absolute -left-32 -top-32 h-[420px] w-[420px] rounded-full" />
        <div className="wash-accent-2 absolute -bottom-40 right-0 h-[380px] w-[380px] rounded-full" />
      </div>

      {/*
        The width is a plain CSS transition, not a framer spring.
        Collapsing the sidebar re-lays-out the whole content pane, and therefore
        resizes every ResponsiveContainer chart — that is unavoidable. What is
        avoidable is spending main-thread frames on JS animation bookkeeping
        (plus framer's projection tree, which `layoutId="nav-pill"` keeps active)
        while the browser is already re-laying-out and re-rendering charts on the
        same frames. The stylesheet transition keeps the same easing and lets the
        style/layout phase own it. `overflow-hidden` clips the labels that are
        still fading out while the rail narrows.
      */}
      <aside
        style={{ width: collapsed ? 68 : 236 }}
        className="relative z-10 flex flex-shrink-0 flex-col overflow-hidden border-r border-line bg-surface/70 backdrop-blur-xl transition-[width] duration-200 ease-spring"
      >
        <div className="flex h-16 items-center gap-2.5 border-b border-line px-4">
          <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-gradient-to-br from-accent to-accent-2 text-bg shadow-glow">
            <Activity size={18} strokeWidth={2.5} />
          </div>
          <AnimatePresence initial={false}>
            {!collapsed && (
              <motion.div
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: -8 }}
                transition={{ duration: 0.18 }}
                className="min-w-0"
              >
                <div className="truncate text-sm font-semibold leading-tight text-gradient">
                  HyprTrace
                </div>
                <div className="truncate text-[10px] uppercase tracking-wider text-fg-faint">
                  Window time tracker
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        <nav className="flex-1 space-y-1 overflow-y-auto p-2.5">
          {navItems.map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              title={collapsed ? label : undefined}
              className="group relative flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors"
            >
              {({ isActive }) => (
                <>
                  {isActive && (
                    <motion.span
                      layoutId="nav-pill"
                      className="absolute inset-0 rounded-lg bg-accent/10 ring-1 ring-inset ring-accent/25"
                      transition={spring}
                    />
                  )}
                  <Icon
                    size={18}
                    className={`relative z-10 shrink-0 transition-colors ${
                      isActive ? 'text-accent' : 'text-fg-faint group-hover:text-fg-muted'
                    }`}
                  />
                  <AnimatePresence initial={false}>
                    {!collapsed && (
                      <motion.span
                        initial={{ opacity: 0, x: -6 }}
                        animate={{ opacity: 1, x: 0 }}
                        exit={{ opacity: 0, x: -6 }}
                        transition={{ duration: 0.16 }}
                        className={`relative z-10 truncate ${
                          isActive ? 'font-medium text-fg' : 'text-fg-muted group-hover:text-fg'
                        }`}
                      >
                        {label}
                      </motion.span>
                    )}
                  </AnimatePresence>
                </>
              )}
            </NavLink>
          ))}
        </nav>

        <div className="space-y-2 border-t border-line p-2.5">
          <div className={collapsed ? 'flex justify-center' : ''}>
            <ThemeToggle compact={collapsed} />
          </div>
          <div className="flex items-center justify-between gap-2 px-1">
            {!collapsed && <span className="text-[10px] text-fg-faint">v0.1.0</span>}
            <button
              type="button"
              onClick={() => setCollapsed((c) => !c)}
              className="btn btn-ghost ml-auto px-2"
              title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
              aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            >
              {collapsed ? <PanelLeftOpen size={15} /> : <PanelLeftClose size={15} />}
            </button>
          </div>
        </div>
      </aside>

      <div className="relative z-10 flex min-w-0 flex-1 flex-col">
        {/* Slim context bar: always shows which section is open. */}
        <header className="flex h-16 shrink-0 items-center gap-3 border-b border-line bg-surface/60 px-6 backdrop-blur-xl">
          <div className="flex items-center gap-2">
            {active && <active.icon size={16} className="text-accent" />}
            <span className="text-sm font-medium text-fg">{active?.label ?? 'HyprTrace'}</span>
          </div>
          <div className="ml-auto flex items-center gap-3">
            <span className="hidden items-center gap-1.5 text-xs text-fg-faint sm:flex">
              <span className="relative flex h-1.5 w-1.5">
                <span className="absolute inline-flex h-full w-full animate-ring-pulse rounded-full bg-good/70" />
                <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-good" />
              </span>
              live tracking
            </span>
          </div>
        </header>

        {/*
          `scrollbar-gutter: stable` reserves the scrollbar's width for good.
          Without it the content pane gets ~8px wider every time a page's
          skeleton is replaced by tall content (or a filter narrows a list),
          which resizes every ResponsiveContainer chart mid-entrance — the
          classic "the dashboard twitches right after loading" artefact.
        */}
        <main
          ref={mainRef}
          className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]"
        >
          {/*
            Entrance-only page transition, on purpose. An exit animation has to
            keep the previous route mounted, and because <Outlet/> is driven by
            the live router context it re-renders the *new* page inside that
            exiting container: the page mounts twice, and if a nested
            AnimatePresence (Sessions rows, AI messages, Settings lists) is
            mid-flight the exit never completes, so the new page never mounts
            and the content area stays empty until a manual refresh.
            Animating only the entrance makes navigation impossible to block.
          */}
          <motion.div
            key={location.pathname}
            variants={pageTransition}
            initial={reducedMotion ? false : 'hidden'}
            animate="show"
            className="mx-auto w-full max-w-[1600px] p-6"
          >
            {/* Routes are lazy (see App.tsx); the boundary sits inside the
                animated container so the shell stays put while a chunk loads. */}
            <Suspense fallback={<RouteFallback />}>
              <Outlet />
            </Suspense>
          </motion.div>
        </main>
      </div>
    </div>
  );
}
