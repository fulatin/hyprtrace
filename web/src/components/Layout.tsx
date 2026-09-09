import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { NavLink, Outlet, useLocation } from 'react-router-dom';
import { AnimatePresence, motion } from 'framer-motion';
import {
  LayoutDashboard,
  BarChart3,
  Clock,
  List,
  FileText,
  Bot,
  Settings,
  Sparkles,
  PanelLeftClose,
  PanelLeftOpen,
  Activity,
} from 'lucide-react';
import ThemeToggle from './ui/ThemeToggle';
import { pageTransition, spring } from '../lib/motion';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/insights', icon: Sparkles, label: 'Insights' },
  { to: '/apps', icon: BarChart3, label: 'Apps' },
  { to: '/timeline', icon: Clock, label: 'Timeline' },
  { to: '/sessions', icon: List, label: 'Sessions' },
  { to: '/titles', icon: FileText, label: 'Documents' },
  { to: '/ai', icon: Bot, label: 'AI Chat' },
  { to: '/settings', icon: Settings, label: 'Settings' },
];

const COLLAPSE_KEY = 'hyprtrace.sidebar.collapsed';

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

  return (
    <div className="relative flex h-screen overflow-hidden bg-bg">
      {/* Ambient depth: two very soft colour washes behind everything. */}
      <div className="pointer-events-none fixed inset-0 overflow-hidden">
        <div className="absolute -left-32 -top-32 h-[420px] w-[420px] rounded-full bg-accent/10 blur-[120px]" />
        <div className="absolute -bottom-40 right-0 h-[380px] w-[380px] rounded-full bg-accent-2/10 blur-[120px]" />
      </div>

      <motion.aside
        animate={{ width: collapsed ? 68 : 236 }}
        transition={spring}
        className="relative z-10 flex flex-shrink-0 flex-col border-r border-line bg-surface/70 backdrop-blur-xl"
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
      </motion.aside>

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

        <main ref={mainRef} className="min-h-0 flex-1 overflow-y-auto">
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
            initial="hidden"
            animate="show"
            className="mx-auto w-full max-w-[1600px] p-6"
          >
            <Outlet />
          </motion.div>
        </main>
      </div>
    </div>
  );
}
