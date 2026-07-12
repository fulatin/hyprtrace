import { BrowserRouter, Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import ErrorBoundary from './components/ErrorBoundary';
import Dashboard from './pages/Dashboard';
import Apps from './pages/Apps';
import Timeline from './pages/Timeline';
import Sessions from './pages/Sessions';
import AIChat from './pages/AIChat';
import Settings from './pages/Settings';

export default function App() {
  return (
    <ErrorBoundary>
      <BrowserRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route path="/" element={<Dashboard />} />
            <Route path="/apps" element={<Apps />} />
            <Route path="/timeline" element={<Timeline />} />
            <Route path="/sessions" element={<Sessions />} />
            <Route path="/ai" element={<AIChat />} />
            <Route path="/settings" element={<Settings />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </ErrorBoundary>
  );
}