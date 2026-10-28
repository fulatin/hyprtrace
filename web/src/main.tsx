import React from 'react'
import ReactDOM from 'react-dom/client'
import { QueryClientProvider } from '@tanstack/react-query'
import App from './App'
import { queryClient } from './lib/queries'
import './index.css'

// A redeploy replaces the hashed chunks in /assets. A tab that still has the
// previous index.html (or a long-lived session) can then fail to lazy-load a
// route chunk; Vite reports that as `vite:preloadError`. Reload once so the
// browser picks up the new index.html instead of showing the error boundary.
const CHUNK_RELOAD_KEY = 'hyprtrace.chunk_reload_at'
const CHUNK_RELOAD_COOLDOWN_MS = 30_000

function onPreloadError(event: Event) {
  event.preventDefault()
  try {
    const last = Number(sessionStorage.getItem(CHUNK_RELOAD_KEY) ?? '0')
    if (Date.now() - last > CHUNK_RELOAD_COOLDOWN_MS) {
      sessionStorage.setItem(CHUNK_RELOAD_KEY, String(Date.now()))
      window.location.reload()
    }
  } catch {
    // sessionStorage unavailable (private mode): still recover once.
    window.location.reload()
  }
}

window.addEventListener('vite:preloadError', onPreloadError as EventListener)

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {/* The provider sits above <App> so ThemeProvider → BrowserRouter →
        ErrorBoundary keep their order inside it untouched. */}
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
)
