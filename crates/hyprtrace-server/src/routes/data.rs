use crate::models::{PaginatedResponse, Session};
use crate::routes::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct SummaryQuery {
    pub date: Option<String>,
}

#[derive(Deserialize)]
pub struct AppRankingQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

#[derive(Deserialize)]
pub struct DateQuery {
    pub date: Option<String>,
}

#[derive(Deserialize)]
pub struct SessionQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    pub class: Option<String>,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    50
}

#[derive(Deserialize)]
pub struct AppTrendQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": "0.1.0"
    }))
}

pub async fn summary(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SummaryQuery>,
) -> Result<Json<crate::models::TodaySummary>, Json<serde_json::Value>> {
    let date = query.date.unwrap_or_else(|| {
        chrono::Utc::now().format("%Y-%m-%d").to_string()
    });

    let db = state.db.lock().await;
    match db.today_summary(&date) {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            log::error!("Failed to get summary: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn app_ranking(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AppRankingQuery>,
) -> Result<Json<Vec<crate::models::AppRank>>, Json<serde_json::Value>> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let from = query.from.unwrap_or_else(|| today.clone());
    let to = query.to.unwrap_or(today);

    let db = state.db.lock().await;
    match db.app_ranking(&from, &to, query.limit) {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            log::error!("Failed to get app ranking: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DateQuery>,
) -> Result<Json<Vec<crate::models::HourlyBucket>>, Json<serde_json::Value>> {
    let date = query.date.unwrap_or_else(|| {
        chrono::Utc::now().format("%Y-%m-%d").to_string()
    });

    let db = state.db.lock().await;
    match db.hourly_breakdown(&date) {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            log::error!("Failed to get timeline: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
) -> Result<Json<PaginatedResponse<Session>>, Json<serde_json::Value>> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let from = query.from.unwrap_or_else(|| today.clone());
    let to = query.to.unwrap_or(today);

    let db = state.db.lock().await;
    match db.sessions_paginated(&from, &to, query.page, query.per_page, query.class.as_deref()) {
        Ok((data, total)) => Ok(Json(PaginatedResponse {
            data,
            total,
            page: query.page,
            per_page: query.per_page,
        })),
        Err(e) => {
            log::error!("Failed to get sessions: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn rebuild_summary(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.rebuild_daily_summary() {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => {
            log::error!("Failed to rebuild daily_summary: {}", e);
            Err(Json(serde_json::json!({"error": "Failed to rebuild daily summary"})))
        }
    }
}

pub async fn app_trend(
    State(state): State<Arc<AppState>>,
    Path(class): Path<String>,
    Query(query): Query<AppTrendQuery>,
) -> Result<Json<Vec<crate::models::DailyTrend>>, Json<serde_json::Value>> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let from = query.from.unwrap_or_else(|| {
        let d = chrono::Utc::now() - chrono::Duration::days(7);
        d.format("%Y-%m-%d").to_string()
    });
    let to = query.to.unwrap_or(today);

    let db = state.db.lock().await;
    match db.app_daily_trend(&class, &from, &to) {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            log::error!("Failed to get app trend: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}
