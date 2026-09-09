//! Read-only `/api/insights/*` handlers.
//!
//! Handlers stay thin: parse + clamp the query parameters, hand them to a
//! `Database` method, return its JSON. All formulas live in `crate::insights`.
//!
//! Clamps (contract): limit <= 100, days <= 365, window <= 90,
//! min_ms <= 3_600_000, threshold in 0.5..=5.

use crate::routes::AppState;
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

const MAX_LIMIT: usize = 100;
const MAX_DAYS: i64 = 365;
const MAX_WINDOW: i64 = 90;
const MAX_MIN_MS: i64 = 3_600_000;

#[derive(Deserialize)]
pub struct TransitionsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_min_ms")]
    pub min_ms: i64,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_min_ms() -> i64 {
    3_000
}

fn default_limit() -> usize {
    12
}

#[derive(Deserialize)]
pub struct ForecastQuery {
    pub class: Option<String>,
    #[serde(default = "default_forecast_days")]
    pub days: i64,
    #[serde(default = "default_horizon")]
    pub horizon: i64,
}

fn default_forecast_days() -> i64 {
    30
}

fn default_horizon() -> i64 {
    7
}

#[derive(Deserialize)]
pub struct RangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Deserialize)]
pub struct CooccurrenceQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_window_minutes")]
    pub window_minutes: i64,
    #[serde(default = "default_cooc_limit")]
    pub limit: usize,
}

fn default_window_minutes() -> i64 {
    30
}

fn default_cooc_limit() -> usize {
    10
}

#[derive(Deserialize)]
pub struct AnomaliesQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_anomaly_window")]
    pub window: i64,
    #[serde(default = "default_threshold")]
    pub threshold: f64,
}

fn default_anomaly_window() -> i64 {
    14
}

fn default_threshold() -> f64 {
    2.0
}

/// Resolve the optional `from`/`to` pair; both default to the last 7 local days
/// ending today, and a missing `from` is derived from `to`.
fn resolve_range(from: Option<String>, to: Option<String>) -> (String, String) {
    let to = to.unwrap_or_else(crate::insights::today_local);
    let from = from.unwrap_or_else(|| {
        chrono::NaiveDate::parse_from_str(&to, "%Y-%m-%d")
            .map(|d| (d - chrono::Duration::days(6)).format("%Y-%m-%d").to_string())
            .unwrap_or_else(|_| crate::insights::default_from())
    });
    (from, to)
}

fn internal_error(context: &str, e: anyhow::Error) -> Json<serde_json::Value> {
    log::error!("Failed to compute insights {context}: {e:#}");
    Json(serde_json::json!({"error": "Internal server error"}))
}

pub async fn transitions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TransitionsQuery>,
) -> Result<Json<crate::insights::TransitionsResponse>, Json<serde_json::Value>> {
    let (from, to) = resolve_range(query.from, query.to);
    let min_ms = query.min_ms.clamp(0, MAX_MIN_MS);
    let limit = query.limit.clamp(1, MAX_LIMIT);
    let db = state.db.lock().await;
    db.insight_transitions(&from, &to, min_ms, limit)
        .map(Json)
        .map_err(|e| internal_error("transitions", e))
}

pub async fn forecast(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ForecastQuery>,
) -> Result<Json<crate::insights::ForecastResponse>, Json<serde_json::Value>> {
    let days = query.days.clamp(1, MAX_DAYS);
    let horizon = query.horizon.clamp(0, MAX_DAYS);
    let to = crate::insights::today_local();
    // An empty `class` means "overall", same as an absent parameter.
    let class = query.class.filter(|c| !c.is_empty());
    let db = state.db.lock().await;
    db.insight_forecast(class.as_deref(), &to, days, horizon)
        .map(Json)
        .map_err(|e| internal_error("forecast", e))
}

pub async fn rhythm(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RangeQuery>,
) -> Result<Json<crate::insights::RhythmResponse>, Json<serde_json::Value>> {
    let (from, to) = resolve_range(query.from, query.to);
    let db = state.db.lock().await;
    db.insight_rhythm(&from, &to)
        .map(Json)
        .map_err(|e| internal_error("rhythm", e))
}

pub async fn fragmentation(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RangeQuery>,
) -> Result<Json<crate::insights::FragmentationResponse>, Json<serde_json::Value>> {
    let (from, to) = resolve_range(query.from, query.to);
    let db = state.db.lock().await;
    db.insight_fragmentation(&from, &to)
        .map(Json)
        .map_err(|e| internal_error("fragmentation", e))
}

pub async fn cooccurrence(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CooccurrenceQuery>,
) -> Result<Json<crate::insights::CooccurrenceResponse>, Json<serde_json::Value>> {
    let (from, to) = resolve_range(query.from, query.to);
    let window_minutes = query.window_minutes.clamp(1, 1_440);
    let limit = query.limit.clamp(1, MAX_LIMIT);
    let db = state.db.lock().await;
    db.insight_cooccurrence(&from, &to, window_minutes, limit)
        .map(Json)
        .map_err(|e| internal_error("cooccurrence", e))
}

pub async fn disruption_impact(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RangeQuery>,
) -> Result<Json<crate::insights::DisruptionImpactResponse>, Json<serde_json::Value>> {
    let (from, to) = resolve_range(query.from, query.to);
    let db = state.db.lock().await;
    db.insight_disruption_impact(&from, &to)
        .map(Json)
        .map_err(|e| internal_error("disruption-impact", e))
}

pub async fn anomalies(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AnomaliesQuery>,
) -> Result<Json<crate::insights::AnomaliesResponse>, Json<serde_json::Value>> {
    let (from, to) = resolve_range(query.from, query.to);
    let window = query.window.clamp(1, MAX_WINDOW);
    let threshold = if query.threshold.is_finite() {
        query.threshold.clamp(0.5, 5.0)
    } else {
        2.0
    };
    let db = state.db.lock().await;
    db.insight_anomalies(&from, &to, window, threshold)
        .map(Json)
        .map_err(|e| internal_error("anomalies", e))
}
