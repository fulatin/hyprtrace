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
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    if let Some(rejection) = origin_rejection(origin) {
        log::warn!(
            "Rejected cross-origin request: Origin={}",
            origin.unwrap_or_default()
        );
        return (
            rejection,
            Json(serde_json::json!({"error": "Cross-origin requests are not allowed"})),
        )
            .into_response();
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

/// The status a request's `Origin` header must be rejected with, if any.
///
/// Split out of the middleware so the rejection→403 mapping itself is
/// testable: `guard` returns exactly this status for a foreign origin.
fn origin_rejection(origin: Option<&str>) -> Option<StatusCode> {
    match origin {
        Some(origin) if !is_loopback_origin(origin) => Some(StatusCode::FORBIDDEN),
        // No `Origin` means a non-browser client (curl, scripts); those are
        // unaffected by the cross-origin guard.
        _ => None,
    }
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

    // `localhost` and `*.localhost` are loopback by convention. Anything else
    // must be an IP literal that is itself a loopback address. Crucially this
    // rejects `127.evil.com`: it *starts with* "127." but is a real,
    // non-loopback domain name — a naive prefix match would let it through.
    if host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
    {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
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

/// Constant-time equality for API tokens.
///
/// It compares a *fixed* number of bytes instead of iterating to the longer
/// input's length: the latter still leaks the expected token's length through
/// the loop count when an attacker sends a short token (issue #15 asks for
/// "same order of magnitude" cost, this gives a constant one for any token
/// that fits the cap). Tokens longer than 256 bytes — far beyond any realistic
/// `server.auth_token` — are rejected outright; that branch only leaks the
/// fact that the input is unusually long.
fn constant_time_eq(a: &str, b: &str) -> bool {
    const MAX_TOKEN_LEN: usize = 256;
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() > MAX_TOKEN_LEN || b_bytes.len() > MAX_TOKEN_LEN {
        return false;
    }
    let mut diff = (a_bytes.len() != b_bytes.len()) as u8;
    for i in 0..MAX_TOKEN_LEN {
        let x = a_bytes.get(i).copied().unwrap_or(0);
        let y = b_bytes.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0
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
            "http://foo.localhost:5173",
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
    fn loopback_prefix_domains_are_rejected() {
        // Security regression (issue #15): "127." as a string prefix must not
        // make a non-loopback domain look like a loopback host.
        for bad in [
            "http://127.evil.com",
            "http://127.evil.com:8080",
            "http://127.0.0.1.evil.com",
            "http://2130706433", // decimal-encoded 127.0.0.1, not a valid host
        ] {
            assert!(!is_loopback_origin(bad), "must reject {bad}");
        }
    }

    #[test]
    fn foreign_origins_map_to_403_and_loopback_ones_do_not() {
        // The middleware returns exactly this status for a rejected Origin, so
        // asserting it here is the 403 contract of issue #15.
        for bad in [
            "http://127.evil.com",
            "http://127.0.0.1.evil.com",
            "http://2130706433",
            "https://evil.com",
            "http://192.168.1.5:9420",
            "null",
        ] {
            assert_eq!(
                origin_rejection(Some(bad)),
                Some(StatusCode::FORBIDDEN),
                "{bad} must be rejected with 403"
            );
        }
        for ok in [
            "http://127.0.0.1:9420",
            "http://[::1]:9420",
            "http://localhost:5173",
        ] {
            assert_eq!(origin_rejection(Some(ok)), None, "{ok} must be allowed");
        }
        // Requests without an Origin (curl, scripts) are not cross-origin.
        assert_eq!(origin_rejection(None), None);
    }

    #[test]
    fn constant_time_eq_basics() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "a"));
        // Fixed-length comparison: the cost does not depend on how long the
        // expected value is, and overly long inputs are rejected.
        let long = "x".repeat(256);
        assert!(constant_time_eq(&long, &long));
        assert!(!constant_time_eq(&long, &"x".repeat(255)));
        assert!(!constant_time_eq(&"x".repeat(257), &"x".repeat(257)));
    }
}
