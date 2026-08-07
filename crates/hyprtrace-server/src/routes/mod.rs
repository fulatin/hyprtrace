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
        .route("/health", axum::routing::get(data::health))
        .route("/summary", axum::routing::get(data::summary))
        .route("/apps", axum::routing::get(data::app_ranking))
        .route("/timeline", axum::routing::get(data::timeline))
        .route("/sessions", axum::routing::get(data::sessions))
        .route("/app/:class/trend", axum::routing::get(data::app_trend))
        .route("/apps/classes", axum::routing::get(data::app_classes))
        .route("/summary/rebuild", axum::routing::post(data::rebuild_summary))
        .route("/hourly-summary/rebuild", axum::routing::post(data::rebuild_hourly_summary))
        .route("/activity/events", axum::routing::get(data::activity_events))
        .route("/resources", axum::routing::get(data::resources))
        .route("/disruptions", axum::routing::get(data::disruptions))
        .route("/efficiency", axum::routing::get(data::efficiency))
        .route("/categories", axum::routing::get(data::get_categories))
        .route("/categories", axum::routing::put(data::put_categories))
        .route("/ai/models", axum::routing::get(ai::ai_models))
        .route("/ai/tools", axum::routing::get(ai::ai_tools))
        .route("/ai/chat", axum::routing::post(ai::ai_chat))
        .route("/ai/chat/stream", axum::routing::post(ai::chat_stream))
        .route("/ai/chat/stream/text", axum::routing::post(ai::chat_stream_text))
        .route("/ai/chat/agent", axum::routing::post(ai::chat_agent))
        .route("/ai/conversations", axum::routing::get(ai::ai_conversations))
        .route("/ai/conversations", axum::routing::delete(ai::clear_conversations))
        .route("/config", axum::routing::get(config::get_config))
        .route("/config", axum::routing::put(config::update_config))
        // Explicit 404 for unmatched /api/* paths. Without this, axum's nest
        // falls through to the outer SPA fallback and would serve index.html
        // (with a 200) for unknown API paths.
        .fallback(|| async {
            (
                axum::http::StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({"error": "Not found"})),
            )
        })
        .with_state(state)
}
