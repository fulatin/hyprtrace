mod ai;
mod config;
mod data;

use crate::ai::AiManager;
use crate::config::Config;
use crate::db::Database;
use axum::Router;
use std::sync::Arc;

pub struct AppState {
    pub db: tokio::sync::Mutex<Database>,
    pub config: tokio::sync::Mutex<Config>,
    pub ai: tokio::sync::Mutex<AiManager>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", axum::routing::get(data::health))
        .route("/api/summary", axum::routing::get(data::summary))
        .route("/api/apps", axum::routing::get(data::app_ranking))
        .route("/api/timeline", axum::routing::get(data::timeline))
        .route("/api/sessions", axum::routing::get(data::sessions))
        .route("/api/app/:class/trend", axum::routing::get(data::app_trend))
        .route("/api/ai/models", axum::routing::get(ai::ai_models))
        .route("/api/ai/chat", axum::routing::post(ai::ai_chat))
        .route("/api/ai/conversations", axum::routing::get(ai::ai_conversations))
        .route("/api/ai/conversations", axum::routing::delete(ai::clear_conversations))
        .route("/api/config", axum::routing::get(config::get_config))
        .route("/api/config", axum::routing::put(config::update_config))
        .with_state(state)
}
