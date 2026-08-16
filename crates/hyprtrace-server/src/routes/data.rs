use crate::models::{CategoryRule, PaginatedResponse, Project, ProjectRule, ProjectStat, Session};
use crate::routes::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
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
    pub granularity: Option<String>,
}

#[derive(Deserialize)]
pub struct ResourceQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": "0.1.0"
    }))
}

#[derive(Serialize)]
pub struct CategoriesResponse {
    pub rules: Vec<CategoryRule>,
    pub categories: Vec<String>,
}

#[derive(Deserialize)]
pub struct CategoriesUpdate {
    pub rules: Vec<CategoryRule>,
}

pub async fn get_categories(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CategoriesResponse>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.categories() {
        Ok(rules) => Ok(Json(CategoriesResponse {
            rules,
            categories: crate::db::Database::known_categories(),
        })),
        Err(e) => {
            log::error!("Failed to load categories: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn put_categories(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CategoriesUpdate>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.set_categories(&req.rules) {
        Ok(()) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => {
            log::error!("Failed to save categories: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

#[derive(Serialize)]
pub struct ProjectsResponse {
    pub projects: Vec<Project>,
    pub rules: Vec<ProjectRule>,
}

#[derive(Deserialize)]
pub struct ProjectsUpdate {
    pub projects: Vec<Project>,
    pub rules: Vec<ProjectRule>,
}

pub async fn get_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProjectsResponse>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match (db.projects(), db.project_rules()) {
        (Ok(projects), Ok(rules)) => Ok(Json(ProjectsResponse { projects, rules })),
        (Err(e), _) | (_, Err(e)) => {
            log::error!("Failed to load projects: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn put_projects(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProjectsUpdate>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Reject duplicate project names before touching the database so the
    // caller gets a clear 400 instead of a generic 500 from the UNIQUE index.
    let mut seen = std::collections::HashSet::new();
    for p in &req.projects {
        let name = p.name.trim();
        if name.is_empty() {
            continue;
        }
        if !seen.insert(name.to_string()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Duplicate project name: {}", name)})),
            ));
        }
    }

    let db = state.db.lock().await;
    match db.set_projects(&req.projects, &req.rules) {
        Ok(()) => match (db.projects(), db.project_rules()) {
            (Ok(projects), Ok(rules)) => Ok(Json(serde_json::json!({
                "status": "ok",
                "projects": projects,
                "rules": rules,
            }))),
            (Err(e), _) | (_, Err(e)) => {
                log::error!("Failed to reload projects after save: {}", e);
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Internal server error"})),
                ))
            }
        },
        Err(e) => {
            log::error!("Failed to save projects: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal server error"})),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct ProjectStatsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn project_stats(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ProjectStatsQuery>,
) -> Result<Json<Vec<ProjectStat>>, Json<serde_json::Value>> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let from = query.from.unwrap_or_else(|| today.clone());
    let to = query.to.unwrap_or(today);

    let db = state.db.lock().await;
    match db.project_stats(&from, &to) {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            log::error!("Failed to get project stats: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
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

#[derive(Deserialize)]
pub struct DateRangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn app_classes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DateRangeQuery>,
) -> Result<Json<Vec<String>>, Json<serde_json::Value>> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let from = query.from.unwrap_or_else(|| today.clone());
    let to = query.to.unwrap_or(today);

    let db = state.db.lock().await;
    match db.distinct_classes(&from, &to) {
        Ok(classes) => Ok(Json(classes)),
        Err(e) => {
            log::error!("Failed to get app classes: {}", e);
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
    let db = state.db.lock().await;

    if query.granularity.as_deref() == Some("hour") {
        let date = query.from.as_deref().unwrap_or(&today);
        match db.app_trend_hourly(&class, date) {
            Ok(result) => Ok(Json(result)),
            Err(e) => {
                log::error!("Failed to get hourly app trend: {}", e);
                Err(Json(serde_json::json!({"error": "Internal server error"})))
            }
        }
    } else {
        let from = query.from.unwrap_or_else(|| {
            let d = chrono::Utc::now() - chrono::Duration::days(7);
            d.format("%Y-%m-%d").to_string()
        });
        let to = query.to.unwrap_or(today);

        match db.app_daily_trend(&class, &from, &to) {
            Ok(result) => Ok(Json(result)),
            Err(e) => {
                log::error!("Failed to get app trend: {}", e);
                Err(Json(serde_json::json!({"error": "Internal server error"})))
            }
        }
    }
}

#[derive(Deserialize)]
pub struct ActivityEventsQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_events_limit")]
    pub limit: usize,
}

fn default_events_limit() -> usize {
    100
}

pub async fn activity_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ActivityEventsQuery>,
) -> Result<Json<Vec<crate::models::ActivityEvent>>, Json<serde_json::Value>> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let from = query.from.unwrap_or_else(|| today.clone());
    let to = query.to.unwrap_or(today);

    let db = state.db.lock().await;
    match db.activity_events(&from, &to, query.limit) {
        Ok(events) => Ok(Json(events)),
        Err(e) => {
            log::error!("Failed to get activity events: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn rebuild_hourly_summary(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.rebuild_hourly_summary() {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => {
            log::error!("Failed to rebuild hourly_summary: {}", e);
            Err(Json(serde_json::json!({"error": "Failed to rebuild hourly summary"})))
        }
    }
}

pub async fn resources(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ResourceQuery>,
) -> Result<Json<Vec<crate::models::AppResource>>, Json<serde_json::Value>> {    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let from = query.from.unwrap_or_else(|| today.clone());
    let to = query.to.unwrap_or(today);
    let limit = query.limit.clamp(1, 50);

    let db = state.db.lock().await;
    match db.resource_stats(&from, &to, limit) {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            log::error!("Failed to get resource stats: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

#[derive(Deserialize)]
pub struct DisruptionQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

pub async fn disruptions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DisruptionQuery>,
) -> Result<Json<Vec<crate::models::DisruptionEvent>>, Json<serde_json::Value>> {    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let from = query.from.unwrap_or_else(|| today.clone());
    let to = query.to.unwrap_or(today);
    let limit = query.limit.clamp(1, 200);

    let db = state.db.lock().await;
    match db.disruptions(&from, &to, limit) {
        Ok(events) => Ok(Json(events)),
        Err(e) => {
            log::error!("Failed to get disruptions: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

#[derive(Deserialize)]
pub struct EfficiencyQuery {
    pub date: Option<String>,
}

pub async fn efficiency(
    State(state): State<Arc<AppState>>,
    Query(query): Query<EfficiencyQuery>,
) -> Result<Json<crate::models::EfficiencyScore>, Json<serde_json::Value>> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let date = query.date.unwrap_or(today);
    let db = state.db.lock().await;
    match db.efficiency_score(&date) {
        Ok(score) => Ok(Json(score)),
        Err(e) => {
            log::error!("Failed to compute efficiency score: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

#[derive(Serialize)]
pub struct GoalsResponse {
    pub goals: Vec<crate::models::Goal>,
    pub progress: Vec<crate::models::GoalProgress>,
}

#[derive(Deserialize)]
pub struct GoalsUpdate {
    pub goals: Vec<crate::models::Goal>,
}

pub async fn get_goals(
    State(state): State<Arc<AppState>>,
) -> Result<Json<GoalsResponse>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.goal_progress() {
        Ok(progress) => {
            let goals = progress.iter().map(|p| p.goal.clone()).collect();
            Ok(Json(GoalsResponse { goals, progress }))
        }
        Err(e) => {
            log::error!("Failed to load goals: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn put_goals(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GoalsUpdate>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.set_goals(&req.goals) {
        Ok(()) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => {
            log::error!("Failed to save goals: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

#[derive(Deserialize)]
pub struct ReportQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Deserialize)]
pub struct PredictQuery {
    #[serde(default = "default_predict_window")]
    pub window: i64,
}

fn default_predict_window() -> i64 {
    14
}

pub async fn predict(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PredictQuery>,
) -> Result<Json<crate::models::TrendPrediction>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.predict(query.window) {
        Ok(p) => Ok(Json(p)),
        Err(e) => {
            log::error!("Failed to compute prediction: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn current_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::models::CurrentStatus>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.current_status() {
        Ok(s) => Ok(Json(s)),
        Err(e) => {
            log::error!("Failed to get current status: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

#[derive(Deserialize)]
pub struct WorkspaceQuery {
    #[serde(default = "default_ws_days")]
    pub days: i64,
}

fn default_ws_days() -> i64 {
    14
}

pub async fn workspace_recommendations(
    State(state): State<Arc<AppState>>,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Json<Vec<crate::models::WorkspaceRecommendation>>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.workspace_recommendations(query.days) {
        Ok(r) => Ok(Json(r)),
        Err(e) => {
            log::error!("Failed to compute workspace recommendations: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn report(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ReportQuery>,
) -> Result<axum::response::Response, Json<serde_json::Value>> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let to = query.to.unwrap_or_else(|| today.clone());
    let from = query
        .from
        .unwrap_or_else(|| (chrono::Utc::now() - chrono::Duration::days(6)).format("%Y-%m-%d").to_string());

    let db = state.db.lock().await;
    match db.report(&from, &to) {
        Ok(md) => Ok(axum::response::Response::builder()
            .header("Content-Type", "text/markdown; charset=utf-8")
            .header("Content-Disposition", format!("attachment; filename=\"hyprtrace-report-{}-{}.md\"", from, to))
            .body(axum::body::Body::from(md))
            .unwrap()),
        Err(e) => {
            log::error!("Failed to build report: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}
