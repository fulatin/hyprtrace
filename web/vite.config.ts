import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev server proxies /api to the running hyprtrace-server. Override the target
// with VITE_API_TARGET to point at a throwaway instance (e.g. a test server on
// another port holding a copy of the database).
const apiTarget = process.env.VITE_API_TARGET ?? 'http://127.0.0.1:9420'

export default defineConfig({
  plugins: [react()],
  build: {
    // Split the heavy third-party libraries so the app shell stays small and
    // vendor code can be cached across deploys.
    rollupOptions: {
      output: {
        manualChunks: {
          react: ['react', 'react-dom', 'react-router-dom'],
          charts: ['recharts'],
          motion: ['framer-motion'],
          markdown: ['streamdown'],
        },
      },
    },
  },
  server: {
    proxy: {
      '/api': apiTarget,
    },
  },
})
