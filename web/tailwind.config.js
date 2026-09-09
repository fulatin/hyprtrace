/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // Semantic tokens backed by CSS variables (see src/index.css).
        // The `<alpha-value>` placeholder keeps opacity modifiers working,
        // e.g. `bg-surface/60` or `border-accent/30`.
        bg: {
          DEFAULT: 'rgb(var(--c-bg) / <alpha-value>)',
          soft: 'rgb(var(--c-bg-soft) / <alpha-value>)',
        },
        surface: {
          DEFAULT: 'rgb(var(--c-surface) / <alpha-value>)',
          2: 'rgb(var(--c-surface-2) / <alpha-value>)',
          3: 'rgb(var(--c-surface-3) / <alpha-value>)',
        },
        line: {
          DEFAULT: 'rgb(var(--c-line) / <alpha-value>)',
          strong: 'rgb(var(--c-line-strong) / <alpha-value>)',
        },
        fg: {
          DEFAULT: 'rgb(var(--c-fg) / <alpha-value>)',
          muted: 'rgb(var(--c-fg-muted) / <alpha-value>)',
          faint: 'rgb(var(--c-fg-faint) / <alpha-value>)',
        },
        accent: {
          DEFAULT: 'rgb(var(--c-accent) / <alpha-value>)',
          2: 'rgb(var(--c-accent-2) / <alpha-value>)',
          3: 'rgb(var(--c-accent-3) / <alpha-value>)',
        },
        good: 'rgb(var(--c-good) / <alpha-value>)',
        warn: 'rgb(var(--c-warn) / <alpha-value>)',
        bad: 'rgb(var(--c-bad) / <alpha-value>)',
        // Legacy alias kept so older class names keep resolving.
        hypr: {
          primary: '#22d3ee',
          bg: '#030712',
          card: '#111827',
          border: '#1f2937',
        },
      },
      fontFamily: {
        sans: [
          'Inter',
          'ui-sans-serif',
          'system-ui',
          '-apple-system',
          'Segoe UI',
          'Noto Sans SC',
          'sans-serif',
        ],
        mono: [
          'ui-monospace',
          'SFMono-Regular',
          'JetBrains Mono',
          'Menlo',
          'monospace',
        ],
      },
      borderRadius: {
        xl: '0.875rem',
        '2xl': '1.125rem',
      },
      boxShadow: {
        soft: '0 1px 2px rgb(var(--c-shadow) / 0.25), 0 8px 24px -12px rgb(var(--c-shadow) / 0.45)',
        lift: '0 12px 32px -16px rgb(var(--c-shadow) / 0.7)',
        glow: '0 0 24px -4px rgb(var(--c-accent) / var(--glow-opacity))',
      },
      transitionTimingFunction: {
        spring: 'cubic-bezier(0.22, 1.2, 0.36, 1)',
      },
      animation: {
        shimmer: 'shimmer 1.6s infinite',
        float: 'float 3s ease-in-out infinite',
        'ring-pulse': 'ring-pulse 1.8s ease-out infinite',
      },
    },
  },
  plugins: [
    require('@tailwindcss/typography'),
  ],
}
