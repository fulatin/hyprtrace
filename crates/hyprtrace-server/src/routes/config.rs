use crate::ai::AiManager;
use crate::config::{self, Config};
use crate::routes::AppState;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// TOML paths of the config keys this endpoint may update.
const AI_DEFAULT_PROVIDER: &[&str] = &["ai", "default_provider"];
const AI_OLLAMA_BASE_URL: &[&str] = &["ai", "ollama", "base_url"];
const AI_OLLAMA_MODEL: &[&str] = &["ai", "ollama", "default_model"];
const AI_OPENAI_BASE_URL: &[&str] = &["ai", "openai", "base_url"];
const AI_OPENAI_API_KEY: &[&str] = &["ai", "openai", "api_key"];
const AI_OPENAI_MODEL: &[&str] = &["ai", "openai", "default_model"];
const DAEMON_RECORD_TITLES: &[&str] = &["daemon", "record_titles"];
const SERVER_RETENTION_DAYS: &[&str] = &["server", "retention_days"];
const SERVER_WEEKLY_ENABLED: &[&str] = &["server", "weekly_report_enabled"];
const SERVER_WEEKLY_DAY: &[&str] = &["server", "weekly_report_day"];
const SERVER_WEEKLY_HOUR: &[&str] = &["server", "weekly_report_hour"];
const SERVER_WEEKLY_MINUTE: &[&str] = &["server", "weekly_report_minute"];

#[derive(Serialize)]
pub struct ConfigResponse {
    pub openai_url: String,
    pub openai_model: String,
    pub openai_configured: bool,
    pub ollama_url: String,
    pub ollama_model: String,
    pub default_provider: String,
    pub record_titles: bool,
    pub retention_days: u32,
    pub weekly_report_enabled: bool,
    pub weekly_report_day: u32,
    pub weekly_report_hour: u32,
    pub weekly_report_minute: u32,
}

#[derive(Deserialize)]
pub struct ConfigUpdateRequest {
    pub openai_url: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_model: Option<String>,
    pub ollama_url: Option<String>,
    pub ollama_model: Option<String>,
    pub default_provider: Option<String>,
    pub record_titles: Option<bool>,
    pub retention_days: Option<u32>,
    pub weekly_report_enabled: Option<bool>,
    pub weekly_report_day: Option<u32>,
    pub weekly_report_hour: Option<u32>,
    pub weekly_report_minute: Option<u32>,
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<ConfigResponse> {
    let config = state.config.lock().await;
    let ai = state.ai.lock().await;

    Json(ConfigResponse {
        openai_url: config.ai.openai.base_url.clone(),
        openai_model: config.ai.openai.default_model.clone(),
        openai_configured: ai.openai_configured,
        ollama_url: config.ai.ollama.base_url.clone(),
        ollama_model: config.ai.ollama.default_model.clone(),
        default_provider: config.ai.default_provider.clone(),
        record_titles: config.daemon.record_titles,
        retention_days: config.server.retention_days,
        weekly_report_enabled: config.server.weekly_report_enabled,
        weekly_report_day: config.server.weekly_report_day,
        weekly_report_hour: config.server.weekly_report_hour,
        weekly_report_minute: config.server.weekly_report_minute,
    })
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let mut config = state.config.lock().await;

    // Collect only the keys touched by this request as (toml path, value)
    // pairs; `None` means "remove this key" (see the api_key branch below).
    // They are merged into the on-disk TOML document below so that unknown
    // fields (daemon-only settings such as `enable_input_monitor`) survive the
    // save instead of being silently reset.
    let mut updates: Vec<(&[&str], Option<toml_edit::Value>)> = Vec::new();

    if let Some(v) = req.openai_url {
        updates.push((AI_OPENAI_BASE_URL, Some(toml_edit::Value::from(v.clone()))));
        config.ai.openai.base_url = v;
    }
    if let Some(v) = req.openai_api_key {
        // An empty string means "forget this key". It used to be ignored, so a
        // user who had saved a key could never un-configure the provider
        // through the UI. The key is removed from the file rather than stored
        // as `api_key = ""`, which would leave a blank secret sitting on disk.
        if v.is_empty() {
            updates.push((AI_OPENAI_API_KEY, None));
            config.ai.openai.api_key.clear();
        } else {
            updates.push((AI_OPENAI_API_KEY, Some(toml_edit::Value::from(v.clone()))));
            config.ai.openai.api_key = v;
        }
    }
    if let Some(v) = req.openai_model {
        updates.push((AI_OPENAI_MODEL, Some(toml_edit::Value::from(v.clone()))));
        config.ai.openai.default_model = v;
    }
    if let Some(v) = req.ollama_url {
        updates.push((AI_OLLAMA_BASE_URL, Some(toml_edit::Value::from(v.clone()))));
        config.ai.ollama.base_url = v;
    }
    if let Some(v) = req.ollama_model {
        updates.push((AI_OLLAMA_MODEL, Some(toml_edit::Value::from(v.clone()))));
        config.ai.ollama.default_model = v;
    }
    if let Some(v) = req.default_provider {
        updates.push((AI_DEFAULT_PROVIDER, Some(toml_edit::Value::from(v.clone()))));
        config.ai.default_provider = v;
    }
    if let Some(v) = req.record_titles {
        updates.push((DAEMON_RECORD_TITLES, Some(toml_edit::Value::from(v))));
        config.daemon.record_titles = v;
    }
    if let Some(v) = req.retention_days {
        updates.push((SERVER_RETENTION_DAYS, Some(toml_edit::Value::from(v as i64))));
        config.server.retention_days = v;
    }
    if let Some(v) = req.weekly_report_enabled {
        updates.push((SERVER_WEEKLY_ENABLED, Some(toml_edit::Value::from(v))));
        config.server.weekly_report_enabled = v;
    }
    if let Some(v) = req.weekly_report_day {
        updates.push((SERVER_WEEKLY_DAY, Some(toml_edit::Value::from(v as i64))));
        config.server.weekly_report_day = v;
    }
    if let Some(v) = req.weekly_report_hour {
        updates.push((SERVER_WEEKLY_HOUR, Some(toml_edit::Value::from(v as i64))));
        config.server.weekly_report_hour = v;
    }
    if let Some(v) = req.weekly_report_minute {
        updates.push((SERVER_WEEKLY_MINUTE, Some(toml_edit::Value::from(v as i64))));
        config.server.weekly_report_minute = v;
    }

    let config_path = Config::config_path().map_err(|e| {
        Json(serde_json::json!({"error": format!("Failed to determine config path: {}", e)}))
    })?;

    // Read-modify-write the raw TOML document: only the keys listed in
    // `updates` are overwritten; everything else on disk (including fields
    // the server does not model) is preserved as-is.
    if !updates.is_empty() {
        let mut doc = config::load_toml_document(&config_path).map_err(|e| {
            Json(serde_json::json!({"error": format!("Failed to load config: {}", e)}))
        })?;
        for (path, value) in &updates {
            match value {
                Some(v) => config::set_toml_value(&mut doc, path, v.clone()),
                None => {
                    config::remove_toml_value(&mut doc, path);
                }
            }
        }
        config::save_toml_document(&config_path, &doc).map_err(|e| {
            Json(serde_json::json!({"error": format!("Failed to save config: {}", e)}))
        })?;
    }

    drop(config);

    {
        let mut ai = state.ai.lock().await;
        let new_ai = AiManager::from_config(&state.config.lock().await.ai);
        *ai = new_ai;
    }

    Ok(Json(serde_json::json!({"status": "ok"})))
}
