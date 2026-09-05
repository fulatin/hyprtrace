//! Server-side API guard (security review H1).
//!
//! Threat model: the server listens on a local port and has no user accounts.
//! Two layers protect the API without breaking the same-origin web UI:
//!
//! 1. **Origin allow-list** (always on): any request carrying an `Origin`
//!    header whose host is not loopback is rejected with 403. Malicious web
//!    pages always send their own origin on cross-site `fetch` calls, so this
//!    blocks the drive-by read/write chain server-side — stronger than the
//!    removed permissive CORS layer, because it does not depend on the
//!    browser honoring CORS response headers. Non-browser clients (curl,
//!    scripts) send no `Origin` and are unaffected.
//!
//! 2. **Optional static token** (`server.auth_token` in config.toml): when
//!    set, every `/api/*` request except `/api/health` must present the token
//!    via `X-Auth-Token: <token>` or `Authorization: Bearer <token>`, else
//!    401. Off by default; intended for users who bind the server to a
//!    non-loopback address (LAN access).

use crate::routes::AppState;
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

/// Health endpoint is exempt from the token check so monitoring pings keep
/// working. The middleware runs inside the `/api` nest, where the prefix has
/// already been stripped — accept both forms to stay correct at any layer.
const HEALTH_PATHS: [&str; 2] = ["/health", "/api/health"];

pub async fn guard(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();

    // Layer 1: Origin allow-list (blocks cross-site browser requests).
    if let Some(origin) = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
    {
        if !is_loopback_origin(origin) {
            log::warn!("Rejected cross-origin request: Origin={}", origin);
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Cross-origin requests are not allowed"})),
            )
                .into_response();
        }
    }

    // Layer 2: static token (only enforced when configured).
    let expected = state.config.lock().await.server.auth_token.clone();
    if let Some(expected) = expected.filter(|t| !t.is_empty()) {
        let is_health = HEALTH_PATHS.contains(&path.as_str());
        if !is_health && !request_has_valid_token(&req, &expected) {
            log::warn!("Rejected unauthenticated request to {}", path);
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Missing or invalid API token"})),
            )
                .into_response();
        }
    }

    next.run(req).await
}

/// True if the `Origin` header value (e.g. `http://localhost:5173`) points at
/// a loopback host. Any port is allowed so the Vite dev server and other
/// local tooling keep working.
fn is_loopback_origin(origin: &str) -> bool {
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or(origin);

    // Host part ends at the first ':' (port) or '/'. IPv6 origins are
    // bracketed (`http://[::1]:9420`), so the host ends at ']' instead.
    let host = if let Some(stripped) = rest.strip_prefix('[') {
        match stripped.find(']') {
            Some(end) => &stripped[..end],
            None => return false,
        }
    } else {
        rest.split([':', '/']).next().unwrap_or(rest)
    };

    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.")
}

fn request_has_valid_token(req: &Request, expected: &str) -> bool {
    let header_token = req
        .headers()
        .get("x-auth-token")
        .and_then(|v| v.to_str().ok());

    let bearer_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    header_token
        .into_iter()
        .chain(bearer_token)
        .any(|t| constant_time_eq(t, expected))
}

/// Length-independent comparison to avoid trivially leaking the token through
/// response-time differences.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_origins_are_allowed() {
        for ok in [
            "http://localhost:9420",
            "http://localhost:5173",
            "http://127.0.0.1:9420",
            "http://127.0.0.1",
            "http://[::1]:9420",
            "https://localhost",
        ] {
            assert!(is_loopback_origin(ok), "should allow {ok}");
        }
    }

    #[test]
    fn foreign_origins_are_rejected() {
        for bad in [
            "https://evil.com",
            "http://evil.com:8080",
            "https://localhost.evil.com",
            "http://192.168.1.5:9420",
            "null",
        ] {
            assert!(!is_loopback_origin(bad), "should reject {bad}");
        }
    }

    #[test]
    fn constant_time_eq_basics() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "a"));
    }
}
