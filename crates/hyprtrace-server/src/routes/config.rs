use crate::ai::AiManager;
use crate::config::Config;
use crate::routes::AppState;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct ConfigResponse {
    pub openai_url: String,
    pub openai_model: String,
    pub openai_configured: bool,
    pub ollama_url: String,
    pub ollama_model: String,
    pub default_provider: String,
}

#[derive(Deserialize)]
pub struct ConfigUpdateRequest {
    pub openai_url: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_model: Option<String>,
    pub ollama_url: Option<String>,
    pub ollama_model: Option<String>,
    pub default_provider: Option<String>,
}

pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Json<ConfigResponse> {
    let config = state.config.lock().await;
    let ai = state.ai.lock().await;

    Json(ConfigResponse {
        openai_url: config.ai.openai.base_url.clone(),
        openai_model: config.ai.openai.default_model.clone(),
        openai_configured: ai.openai_configured,
        ollama_url: config.ai.ollama.base_url.clone(),
        ollama_model: config.ai.ollama.default_model.clone(),
        default_provider: config.ai.default_provider.clone(),
    })
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let mut config = state.config.lock().await;

    if let Some(v) = req.openai_url {
        config.ai.openai.base_url = v;
    }
    if let Some(v) = req.openai_api_key {
        if !v.is_empty() {
            config.ai.openai.api_key = v;
        }
    }
    if let Some(v) = req.openai_model {
        config.ai.openai.default_model = v;
    }
    if let Some(v) = req.ollama_url {
        config.ai.ollama.base_url = v;
    }
    if let Some(v) = req.ollama_model {
        config.ai.ollama.default_model = v;
    }
    if let Some(v) = req.default_provider {
        config.ai.default_provider = v;
    }

    let config_path = Config::config_path().map_err(|e| {
        Json(serde_json::json!({"error": format!("Failed to determine config path: {}", e)}))
    })?;

    config.save_to(&config_path).map_err(|e| {
        Json(serde_json::json!({"error": format!("Failed to save config: {}", e)}))
    })?;

    drop(config);

    {
        let mut ai = state.ai.lock().await;
        let new_ai = AiManager::from_config(&state.config.lock().await.ai);
        *ai = new_ai;
    }

    Ok(Json(serde_json::json!({"status": "ok"})))
}
