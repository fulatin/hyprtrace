/**
 * API token handling (security review H1).
 *
 * The server can be configured with a static token (`server.auth_token` in
 * config.toml). When set, every `/api/*` request must carry it in the
 * `X-Auth-Token` header. Plain localhost use does not require a token.
 *
 * UX: open the web UI once with the token in the URL —
 * `http://localhost:9420/?token=<secret>` — it is captured below, persisted
 * to localStorage and stripped from the address bar. All subsequent requests
 * (including the AI chat stream) send it automatically.
 */

const TOKEN_KEY = 'hyprtrace_auth_token';

// Capture `?token=` from the URL once on startup.
if (typeof window !== 'undefined') {
  try {
    const params = new URLSearchParams(window.location.search);
    const t = params.get('token');
    if (t) {
      localStorage.setItem(TOKEN_KEY, t);
      params.delete('token');
      const qs = params.toString();
      window.history.replaceState(
        {},
        '',
        window.location.pathname + (qs ? `?${qs}` : '') + window.location.hash,
      );
    }
  } catch {
    // localStorage unavailable (e.g. privacy mode) — degrade gracefully.
  }
}

/** Headers to attach to every API request. Empty when no token is set. */
export function authHeaders(): Record<string, string> {
  try {
    const token = localStorage.getItem(TOKEN_KEY);
    return token ? { 'X-Auth-Token': token } : {};
  } catch {
    return {};
  }
}

/** Remove the persisted token (used when the server rejects it). */
export function clearAuthToken(): void {
  try {
    localStorage.removeItem(TOKEN_KEY);
  } catch {
    // ignore
  }
}
