import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev server proxies /api to the running hyprtrace-server. Override the target
// with VITE_API_TARGET to point at a throwaway instance (e.g. a test server on
// another port holding a copy of the database).
const apiTarget = process.env.VITE_API_TARGET ?? 'http://127.0.0.1:9420'

export default defineConfig({
  plugins: [react()],
  build: {
    // The 500 kB heuristic is about what the browser must parse before it can
    // paint. The only chunk above it is `mermaid`, which streamdown imports
    // dynamically and only when a chat message actually contains a diagram —
    // it is never in any route's first-paint graph (see `grep modulepreload
    // dist/index.html`). Raising the limit keeps real regressions visible
    // instead of training everyone to ignore a warning that is always there.
    chunkSizeWarningLimit: 700,
    // Only the two vendor groups every route needs are pinned. `react` and
    // `motion` are static imports of the entry (Layout renders framer-motion),
    // so they are preloaded once and cached across deploys.
    //
    // Charts, markdown and the AI SDK are deliberately NOT listed here. Pinning
    // them with the object form of manualChunks pulls their whole dependency
    // subtree into one chunk — including Vite's own `__vitePreload` helper,
    // which the entry then has to import statically: the entry ended up
    // statically importing the 473 KB markdown chunk, and index.html
    // modulepreloaded recharts + streamdown on every route again, exactly the
    // problem the lazy routes were meant to fix. Left alone, Rollup splits them
    // along the dynamic-import boundaries, so a chunk is fetched when (and only
    // when) a route that imports it is rendered.
    rollupOptions: {
      output: {
        manualChunks: {
          react: ['react', 'react-dom', 'react-router-dom'],
          motion: ['framer-motion'],
        },
      },
    },
  },
  server: {
    proxy: {
      '/api': apiTarget,
    },
  },
  // `vite preview` serves the production build out of dist/. Without a proxy
  // here every /api call 404s, which makes it useless for eyeballing the real
  // chunk graph (the dev server does not exercise the lazy route split). Same
  // target as the dev server so previews hit the live or isolated instance.
  preview: {
    proxy: {
      '/api': apiTarget,
    },
  },
})
