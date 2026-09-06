import { BrowserRouter, Routes, Route, useLocation } from 'react-router-dom';
import Layout from './components/Layout';
import ErrorBoundary from './components/ErrorBoundary';
import Dashboard from './pages/Dashboard';
import Apps from './pages/Apps';
import Timeline from './pages/Timeline';
import Sessions from './pages/Sessions';
import AIChat from './pages/AIChat';
import Titles from './pages/Titles';
import Settings from './pages/Settings';

export default function App() {
  return (
    <BrowserRouter>
      <AppErrorBoundary />
    </BrowserRouter>
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
          <Route path="/apps" element={<Apps />} />
          <Route path="/timeline" element={<Timeline />} />
          <Route path="/sessions" element={<Sessions />} />
          <Route path="/titles" element={<Titles />} />
          <Route path="/ai" element={<AIChat />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Routes>
    </ErrorBoundary>
  );
}
