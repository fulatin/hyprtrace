import { NavLink, Outlet } from 'react-router-dom';
import {
  LayoutDashboard,
  BarChart3,
  Clock,
  List,
  Bot,
  Settings,
} from 'lucide-react';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/apps', icon: BarChart3, label: 'Apps' },
  { to: '/timeline', icon: Clock, label: 'Timeline' },
  { to: '/sessions', icon: List, label: 'Sessions' },
  { to: '/ai', icon: Bot, label: 'AI Chat' },
  { to: '/settings', icon: Settings, label: 'Settings' },
];

export default function Layout() {
  return (
    <div className="flex h-screen bg-gray-950">
      <aside className="w-60 flex-shrink-0 bg-gray-900 border-r border-gray-800 flex flex-col">
        <div className="p-4 border-b border-gray-800">
          <h1 className="text-xl font-bold text-cyan-400">HyprTrace</h1>
          <p className="text-xs text-gray-400 mt-1">Window Time Tracker</p>
        </div>

        <nav className="flex-1 p-2 space-y-1">
          {navItems.map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
                  isActive
                    ? 'bg-gray-800 text-cyan-400'
                    : 'text-gray-400 hover:bg-gray-800 hover:text-gray-200'
                }`
              }
            >
              <Icon size={18} />
              {label}
            </NavLink>
          ))}
        </nav>

        <div className="p-4 border-t border-gray-800">
          <p className="text-xs text-gray-500">v0.1.0</p>
        </div>
      </aside>

      <main className="flex-1 overflow-auto p-6">
        <Outlet />
      </main>
    </div>
  );
}