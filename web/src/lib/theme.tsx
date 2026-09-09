import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

export type ThemePreference = 'dark' | 'light' | 'system';
export type ResolvedTheme = 'dark' | 'light';

const STORAGE_KEY = 'hyprtrace.theme';

interface ThemeContextValue {
  /** What the user picked (may be "system"). */
  preference: ThemePreference;
  /** What is actually rendered right now. */
  theme: ResolvedTheme;
  setPreference: (next: ThemePreference) => void;
  /** Cycle dark → light → system. */
  cycle: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function readStoredPreference(): ThemePreference {
  if (typeof window === 'undefined') return 'dark';
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (raw === 'dark' || raw === 'light' || raw === 'system') return raw;
  } catch {
    // Private mode / storage disabled: fall through to the default.
  }
  return 'system';
}

function systemTheme(): ResolvedTheme {
  if (typeof window === 'undefined' || !window.matchMedia) return 'dark';
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(readStoredPreference);
  const [system, setSystem] = useState<ResolvedTheme>(systemTheme);

  // Track OS preference changes while the user is on "system".
  useEffect(() => {
    if (!window.matchMedia) return;
    const mq = window.matchMedia('(prefers-color-scheme: light)');
    const onChange = () => setSystem(mq.matches ? 'light' : 'dark');
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);

  const theme: ResolvedTheme = preference === 'system' ? system : preference;

  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove('dark', 'light');
    root.classList.add(theme);
    // Lets the browser colour form controls / scrollbars to match.
    root.style.colorScheme = theme;
  }, [theme]);

  const setPreference = useCallback((next: ThemePreference) => {
    setPreferenceState(next);
    try {
      window.localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Ignore storage failures; the in-memory value still applies.
    }
  }, []);

  const cycle = useCallback(() => {
    setPreferenceState((prev) => {
      const next: ThemePreference =
        prev === 'dark' ? 'light' : prev === 'light' ? 'system' : 'dark';
      try {
        window.localStorage.setItem(STORAGE_KEY, next);
      } catch {
        // Ignore.
      }
      return next;
    });
  }, []);

  const value = useMemo(
    () => ({ preference, theme, setPreference, cycle }),
    [preference, theme, setPreference, cycle],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error('useTheme must be used inside <ThemeProvider>');
  return ctx;
}

/**
 * Inline script that applies the stored theme before first paint, so a light
 * theme user never sees a dark flash on reload. Injected in index.html.
 */
export const THEME_BOOTSTRAP_SCRIPT = `(function(){try{var p=localStorage.getItem('${STORAGE_KEY}')||'system';var m=window.matchMedia&&window.matchMedia('(prefers-color-scheme: light)').matches;var t=p==='system'?(m?'light':'dark'):p;var r=document.documentElement;r.classList.add(t);r.style.colorScheme=t;}catch(e){document.documentElement.classList.add('dark');}})();`;
