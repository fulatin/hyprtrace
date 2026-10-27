//! Config read/update endpoints.
//!
//! The AI section is DeepSeek Harness only: there is no provider/key
//! configuration for in-process providers any more. Edits are merged into the
//! on-disk TOML document so unknown/daemon-only keys survive.

use crate::ai::service::AiStatus;
use crate::config::{self, Config, DshConfig};
use crate::routes::AppState;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// TOML paths this endpoint may update.
const AI_PROACTIVE: &[&str] = &["ai", "proactive_interval_minutes"];
const AI_DSH_ENABLED: &[&str] = &["ai", "dsh", "enabled"];
const AI_DSH_COMMAND: &[&str] = &["ai", "dsh", "command"];
const AI_DSH_PROFILE: &[&str] = &["ai", "dsh", "profile"];
const AI_DSH_PROVIDER: &[&str] = &["ai", "dsh", "provider"];
const AI_DSH_MODEL: &[&str] = &["ai", "dsh", "model"];
const AI_DSH_API_KEY_ENV: &[&str] = &["ai", "dsh", "api_key_env"];
const AI_DSH_TIMEOUT: &[&str] = &["ai", "dsh", "request_timeout_seconds"];
const DAEMON_RECORD_TITLES: &[&str] = &["daemon", "record_titles"];
const SERVER_RETENTION_DAYS: &[&str] = &["server", "retention_days"];
const SERVER_WEEKLY_ENABLED: &[&str] = &["server", "weekly_report_enabled"];
const SERVER_WEEKLY_DAY: &[&str] = &["server", "weekly_report_day"];
const SERVER_WEEKLY_HOUR: &[&str] = &["server", "weekly_report_hour"];
const SERVER_WEEKLY_MINUTE: &[&str] = &["server", "weekly_report_minute"];

#[derive(Serialize)]
pub struct AiConfigView {
    pub proactive_interval_minutes: u64,
    pub dsh: DshConfig,
}

#[derive(Serialize)]
pub struct ConfigResponse {
    pub ai: AiConfigView,
    pub ai_status: AiStatus,
    pub record_titles: bool,
    pub retention_days: u32,
    pub weekly_report_enabled: bool,
    pub weekly_report_day: u32,
    pub weekly_report_hour: u32,
    pub weekly_report_minute: u32,
}

/// Partial DSH update: every present field is written, absent fields keep
/// their current value.
#[derive(Deserialize, Default)]
pub struct DshUpdate {
    pub enabled: Option<bool>,
    pub command: Option<String>,
    pub profile: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
    pub request_timeout_seconds: Option<u64>,
}

#[derive(Deserialize, Default)]
pub struct AiUpdate {
    pub proactive_interval_minutes: Option<u64>,
    pub dsh: Option<DshUpdate>,
}

#[derive(Deserialize)]
pub struct ConfigUpdateRequest {
    #[serde(default)]
    pub ai: Option<AiUpdate>,
    pub record_titles: Option<bool>,
    pub retention_days: Option<u32>,
    pub weekly_report_enabled: Option<bool>,
    pub weekly_report_day: Option<u32>,
    pub weekly_report_hour: Option<u32>,
    pub weekly_report_minute: Option<u32>,
}

pub async fn get_config(State(state): State<Arc<AppState>>) -> Json<ConfigResponse> {
    let (ai_view, record_titles, retention_days, weekly) = {
        let config = state.config.lock().await;
        (
            AiConfigView {
                proactive_interval_minutes: config.ai.proactive_interval_minutes,
                dsh: config.ai.dsh.clone(),
            },
            config.daemon.record_titles,
            config.server.retention_days,
            (
                config.server.weekly_report_enabled,
                config.server.weekly_report_day,
                config.server.weekly_report_hour,
                config.server.weekly_report_minute,
            ),
        )
    };
    // Status is collected outside the config lock so a slow harness probe can
    // never block a concurrent config write.
    let ai_status = state.ai.status().await;
    Json(ConfigResponse {
        ai: ai_view,
        ai_status,
        record_titles,
        retention_days,
        weekly_report_enabled: weekly.0,
        weekly_report_day: weekly.1,
        weekly_report_hour: weekly.2,
        weekly_report_minute: weekly.3,
    })
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConfigUpdateRequest>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let mut config = state.config.lock().await;

    // Collect only the keys touched by this request as (toml path, value)
    // pairs; they are merged into the on-disk TOML document below so that
    // unknown fields (daemon-only settings, forward-compatible keys) survive.
    let mut updates: Vec<(&[&str], toml_edit::Value)> = Vec::new();

    if let Some(ai) = req.ai {
        if let Some(minutes) = ai.proactive_interval_minutes {
            updates.push((AI_PROACTIVE, toml_edit::Value::from(minutes as i64)));
            config.ai.proactive_interval_minutes = minutes;
        }
        if let Some(dsh) = ai.dsh {
            if let Some(value) = dsh.enabled {
                updates.push((AI_DSH_ENABLED, toml_edit::Value::from(value)));
                config.ai.dsh.enabled = value;
            }
            if let Some(value) = dsh.command {
                updates.push((AI_DSH_COMMAND, toml_edit::Value::from(value.clone())));
                config.ai.dsh.command = value;
            }
            if let Some(value) = dsh.profile {
                updates.push((AI_DSH_PROFILE, toml_edit::Value::from(value.clone())));
                config.ai.dsh.profile = value;
            }
            if let Some(value) = dsh.provider {
                updates.push((AI_DSH_PROVIDER, toml_edit::Value::from(value.clone())));
                config.ai.dsh.provider = value;
            }
            if let Some(value) = dsh.model {
                updates.push((AI_DSH_MODEL, toml_edit::Value::from(value.clone())));
                config.ai.dsh.model = value;
            }
            if let Some(value) = dsh.api_key_env {
                updates.push((AI_DSH_API_KEY_ENV, toml_edit::Value::from(value.clone())));
                config.ai.dsh.api_key_env = value;
            }
            if let Some(value) = dsh.request_timeout_seconds {
                updates.push((AI_DSH_TIMEOUT, toml_edit::Value::from(value as i64)));
                config.ai.dsh.request_timeout_seconds = value;
            }
        }
    }

    if let Some(value) = req.record_titles {
        updates.push((DAEMON_RECORD_TITLES, toml_edit::Value::from(value)));
        config.daemon.record_titles = value;
    }
    if let Some(value) = req.retention_days {
        updates.push((SERVER_RETENTION_DAYS, toml_edit::Value::from(value as i64)));
        config.server.retention_days = value;
    }
    if let Some(value) = req.weekly_report_enabled {
        updates.push((SERVER_WEEKLY_ENABLED, toml_edit::Value::from(value)));
        config.server.weekly_report_enabled = value;
    }
    if let Some(value) = req.weekly_report_day {
        updates.push((SERVER_WEEKLY_DAY, toml_edit::Value::from(value as i64)));
        config.server.weekly_report_day = value;
    }
    if let Some(value) = req.weekly_report_hour {
        updates.push((SERVER_WEEKLY_HOUR, toml_edit::Value::from(value as i64)));
        config.server.weekly_report_hour = value;
    }
    if let Some(value) = req.weekly_report_minute {
        updates.push((SERVER_WEEKLY_MINUTE, toml_edit::Value::from(value as i64)));
        config.server.weekly_report_minute = value;
    }

    let config_path = Config::config_path().map_err(|e| {
        Json(serde_json::json!({"error": format!("Failed to determine config path: {}", e)}))
    })?;

    if !updates.is_empty() {
        let mut doc = config::load_toml_document(&config_path).map_err(|e| {
            Json(serde_json::json!({"error": format!("Failed to load config: {}", e)}))
        })?;
        for (path, value) in &updates {
            config::set_toml_value(&mut doc, path, value.clone());
        }
        config::save_toml_document(&config_path, &doc).map_err(|e| {
            Json(serde_json::json!({"error": format!("Failed to save config: {}", e)}))
        })?;
    }

    let dsh_config = config.ai.dsh.clone();
    drop(config);

    // A route/overlay change invalidates the running harness: discard it so the
    // next turn boots with the freshly rendered overlay.
    state.ai.reconfigure(dsh_config).await;

    Ok(Json(serde_json::json!({"status": "ok"})))
}
