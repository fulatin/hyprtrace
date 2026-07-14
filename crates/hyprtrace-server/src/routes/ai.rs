use crate::ai::ChatMessage;
use crate::models::AiMessage;
use crate::routes::AppState;
use axum::body::Body;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::Response;
use axum::Json;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

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

pub async fn ai_conversations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AiMessage>>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.ai_conversations(100) {
        Ok(msgs) => Ok(Json(msgs)),
        Err(e) => {
            log::error!("Failed to get conversations: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
}

pub async fn clear_conversations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.clear_ai_conversations() {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => {
            log::error!("Failed to clear conversations: {}", e);
            Err(Json(serde_json::json!({"error": "Internal server error"})))
        }
    }
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

            Ok(Json(AiChatResponse {
                reply,
                model: provider_name,
            }))
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

pub async fn chat_stream(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, Json<serde_json::Value>> {
    if req.message.trim().is_empty() {
        return Err(Json(serde_json::json!({"error": "Message cannot be empty"})));
    }

    let provider_name = {
        let ai = state.ai.lock().await;
        req.provider
            .clone()
            .unwrap_or_else(|| ai.default_provider.clone())
    };

    {
        let ai = state.ai.lock().await;
        if provider_name == "openai" && !ai.openai_configured {
            return Err(Json(serde_json::json!({"error": "Please configure OpenAI API Key"})));
        }
    }

    let mut messages = Vec::new();

    messages.push(ChatMessage {
        role: "system".to_string(),
        content: {
            let ai = state.ai.lock().await;
            ai.system_prompt.clone()
        },
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

    // Save user message immediately
    {
        let db = state.db.lock().await;
        if let Err(e) = db.save_ai_message("user", &req.message, &provider_name) {
            log::warn!("Failed to save user message: {}", e);
        }
    }

    // Two channels: ai_tx receives raw chunks from provider, sse_tx sends to HTTP response
    let (ai_tx, mut ai_rx) = tokio::sync::mpsc::channel::<String>(64);
    let (sse_tx, sse_rx) = tokio::sync::mpsc::channel::<String>(64);

    // Spawn AI streaming
    let state_clone = state.clone();
    let pname = provider_name.clone();
    let msgs = messages.clone();
    tokio::spawn(async move {
        let ai = state_clone.ai.lock().await;
        if let Err(e) = ai.chat_stream(&pname, &msgs, ai_tx).await {
            log::error!("AI chat stream error: {}", e);
        }
    });

    // Spawn forwarding + accumulation + save
    let state_clone2 = state.clone();
    let model_name = provider_name.clone();
    tokio::spawn(async move {
        let mut full_content = String::new();
        while let Some(chunk) = ai_rx.recv().await {
            full_content.push_str(&chunk);
            if sse_tx.send(chunk).await.is_err() {
                return;
            }
        }
        if !full_content.is_empty() {
            let db = state_clone2.db.lock().await;
            if let Err(e) = db.save_ai_message("assistant", &full_content, &model_name) {
                log::warn!("Failed to save assistant message: {}", e);
            }
        }
    });

    use futures::StreamExt;
    let stream = ReceiverStream::new(sse_rx).map(|chunk| Ok(Event::default().data(chunk)));
    Ok(Sse::new(stream))
}

pub async fn chat_stream_text(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiChatRequest>,
) -> Result<Response<Body>, (StatusCode, Json<serde_json::Value>)> {
    if req.message.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Message cannot be empty"})),
        ));
    }

    let provider_name = {
        let ai = state.ai.lock().await;
        req.provider
            .clone()
            .unwrap_or_else(|| ai.default_provider.clone())
    };

    {
        let ai = state.ai.lock().await;
        if provider_name == "openai" && !ai.openai_configured {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Please configure OpenAI API Key"})),
            ));
        }
    }

    let mut messages = Vec::new();

    messages.push(ChatMessage {
        role: "system".to_string(),
        content: {
            let ai = state.ai.lock().await;
            ai.system_prompt.clone()
        },
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

    {
        let db = state.db.lock().await;
        if let Err(e) = db.save_ai_message("user", &req.message, &provider_name) {
            log::warn!("Failed to save user message: {}", e);
        }
    }

    let (ai_tx, mut ai_rx) = tokio::sync::mpsc::channel::<String>(64);

    let state_clone = state.clone();
    let pname = provider_name.clone();
    let msgs = messages.clone();
    tokio::spawn(async move {
        let ai = state_clone.ai.lock().await;
        if let Err(e) = ai.chat_stream(&pname, &msgs, ai_tx).await {
            log::error!("AI chat stream error: {}", e);
        }
    });

    let (http_tx, http_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Infallible>>(64);

    let state_clone2 = state.clone();
    let model_name = provider_name.clone();
    tokio::spawn(async move {
        let mut full_content = String::new();
        while let Some(chunk) = ai_rx.recv().await {
            full_content.push_str(&chunk);
            if http_tx.send(Ok(chunk.into_bytes())).await.is_err() {
                return;
            }
        }
        if !full_content.is_empty() {
            let db = state_clone2.db.lock().await;
            if let Err(e) = db.save_ai_message("assistant", &full_content, &model_name) {
                log::warn!("Failed to save assistant message: {}", e);
            }
        }
    });

    let stream = ReceiverStream::new(http_rx);
    Ok(Response::builder()
        .header("Content-Type", "text/plain")
        .body(Body::from_stream(stream))
        .unwrap())
}
