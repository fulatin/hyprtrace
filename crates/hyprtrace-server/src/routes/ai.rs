use crate::ai::ChatMessage;
use crate::routes::AppState;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub struct AiModelsResponse {
    pub providers: std::collections::HashMap<String, Vec<String>>,
    pub default: String,
}

pub async fn ai_models(
    State(state): State<Arc<AppState>>,
) -> Json<AiModelsResponse> {
    let ai = state.ai.lock().await;
    let providers = ai.list_all_models().await;
    Json(AiModelsResponse {
        providers,
        default: ai.default_provider.clone(),
    })
}

#[derive(Deserialize)]
pub struct AiChatRequest {
    pub provider: Option<String>,
    pub message: String,
    #[serde(default)]
    pub include_data: bool,
    pub date_range: Option<String>,
}

#[derive(Serialize)]
pub struct AiChatResponse {
    pub reply: String,
    pub model: String,
}

pub async fn ai_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiChatRequest>,
) -> Result<Json<AiChatResponse>, Json<serde_json::Value>> {
    if req.message.trim().is_empty() {
        return Err(Json(serde_json::json!({"error": "Message cannot be empty"})));
    }

    let provider_name;
    let system_prompt;
    let openai_configured;
    {
        let ai = state.ai.lock().await;
        provider_name = req
            .provider
            .unwrap_or_else(|| ai.default_provider.clone());
        system_prompt = ai.system_prompt.clone();
        openai_configured = ai.openai_configured;
    }

    if provider_name == "openai" && !openai_configured {
        return Err(Json(serde_json::json!({"error": "Please configure OpenAI API Key"})));
    }

    let mut messages = Vec::new();

    messages.push(ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
    });

    if req.include_data {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let (from, to) = match req.date_range.as_deref() {
            Some("week") => {
                let d = chrono::Utc::now() - chrono::Duration::days(7);
                (d.format("%Y-%m-%d").to_string(), today.clone())
            }
            Some("month") => {
                let d = chrono::Utc::now() - chrono::Duration::days(30);
                (d.format("%Y-%m-%d").to_string(), today.clone())
            }
            _ => (today.clone(), today.clone()),
        };

        let mut context_parts = Vec::new();

        {
            let db = state.db.lock().await;
            if let Ok(summary) = db.today_summary(&from) {
                context_parts.push(format!(
                    "Date: {}, Total active: {}ms, Apps: {}, Sessions: {}",
                    summary.date, summary.total_active_ms, summary.app_count, summary.session_count
                ));
            }

            if let Ok(apps) = db.app_ranking(&from, &to, 10) {
                let app_str: Vec<String> = apps
                    .iter()
                    .map(|a| format!("{}: {}ms ({:.1}%)", a.class, a.total_ms, a.percentage))
                    .collect();
                context_parts.push(format!("Top apps: {}", app_str.join(", ")));
            }
        }

        if !context_parts.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: format!("User's usage data context:\n{}", context_parts.join("\n")),
            });
        }
    }

    {
        let db = state.db.lock().await;
        if let Ok(history) = db.ai_conversations(10) {
            for msg in history {
                messages.push(ChatMessage {
                    role: msg.role,
                    content: msg.content,
                });
            }
        }
    }

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: req.message.clone(),
    });

    let reply = {
        let ai = state.ai.lock().await;
        ai.chat(&provider_name, &messages).await
    };

    match reply {
        Ok(reply) => {
            let model = if provider_name == "ollama" {
                "ollama".to_string()
            } else {
                "openai".to_string()
            };

            {
                let db = state.db.lock().await;
                if let Err(e) = db.save_ai_message("user", &req.message, &model) {
                    log::warn!("Failed to save user message: {}", e);
                }
                if let Err(e) = db.save_ai_message("assistant", &reply, &model) {
                    log::warn!("Failed to save assistant message: {}", e);
                }
            }

            Ok(Json(AiChatResponse { reply, model }))
        }
        Err(e) => {
            log::error!("AI chat error: {}", e);
            if e.to_string().contains("API key") {
                Err(Json(serde_json::json!({"error": "Please configure OpenAI API Key"})))
            } else {
                Err(Json(serde_json::json!({"error": "AI service unavailable"})))
            }
        }
    }
}
