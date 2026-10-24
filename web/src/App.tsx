import { lazy } from 'react';
import { BrowserRouter, Routes, Route, useLocation } from 'react-router-dom';
import Layout from './components/Layout';
import ErrorBoundary from './components/ErrorBoundary';
import { ThemeProvider } from './lib/theme';

/**
 * Route-level code splitting.
 *
 * Every page used to be a static import of the entry chunk, so first paint
 * downloaded, parsed and compiled all nine pages *and* vendor code only some of
 * them touch — recharts (~434 KB raw) and streamdown's markdown pipeline
 * (~473 KB raw) — and index.html `modulepreload`ed both even on a visit that
 * renders neither. Lazy routes let the browser fetch exactly the route being
 * shown; the heavy vendors move into the chunk graph of the pages importing them.
 *
 * `Layout` stays eager: it is the shell, and keeping it mounted is what lets the
 * Suspense fallback (see Layout.tsx) show a page skeleton without the sidebar
 * and header flashing away while the chunk is fetched.
 */
const Dashboard = lazy(() => import('./pages/Dashboard'));
const Insights = lazy(() => import('./pages/Insights'));
const Apps = lazy(() => import('./pages/Apps'));
const Timeline = lazy(() => import('./pages/Timeline'));
const Sessions = lazy(() => import('./pages/Sessions'));
const AIChat = lazy(() => import('./pages/AIChat'));
const Titles = lazy(() => import('./pages/Titles'));
const Search = lazy(() => import('./pages/Search'));
const Settings = lazy(() => import('./pages/Settings'));

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <AppErrorBoundary />
      </BrowserRouter>
    </ThemeProvider>
  );
}

/** The error boundary sits inside the router so it can reset on navigation. */
function AppErrorBoundary() {
  const location = useLocation();
  return (
    <ErrorBoundary resetKeys={[location.pathname]}>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Dashboard />} />
          <Route path="/insights" element={<Insights />} />
          <Route path="/apps" element={<Apps />} />
          <Route path="/timeline" element={<Timeline />} />
          <Route path="/sessions" element={<Sessions />} />
          <Route path="/titles" element={<Titles />} />
          <Route path="/search" element={<Search />} />
          <Route path="/ai" element={<AIChat />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Routes>
    </ErrorBoundary>
  );
}
