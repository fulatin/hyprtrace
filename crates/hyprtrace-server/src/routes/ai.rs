//! AI endpoints backed by the DeepSeek Harness runtime.
//!
//! Chat, non-streaming completions and the weekly report all delegate to
//! [`crate::ai::service::AiService`]; there is no in-process provider stack.
//! The chat stream keeps the historical NDJSON contract (one JSON object per
//! line) and adds two events: `reasoning` (thinking deltas) and `cancelled`.

use crate::ai::service::{AiEvent, AiStatus};
use crate::models::AiMessage;
use crate::routes::AppState;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Serialize)]
pub struct AiCurrent {
    pub provider: String,
    pub model: String,
}

#[derive(Serialize)]
pub struct AiModelsResponse {
    pub providers: HashMap<String, Vec<String>>,
    pub default: String,
    pub current: AiCurrent,
    pub status: AiStatus,
}

/// GET /api/ai/models[?refresh=1] — model inventory straight from the harness.
///
/// Without `refresh` this never boots the harness (a page load should not pay a
/// multi-second child start); it answers with the configured route. With
/// `refresh=1` the harness is started if needed, which is what the settings
/// page's "test connection" button uses.
pub async fn ai_models(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Json<AiModelsResponse> {
    let refresh = query
        .get("refresh")
        .is_some_and(|value| value == "1" || value == "true");
    let dsh = { state.config.lock().await.ai.dsh.clone() };
    let mut providers = HashMap::new();
    if dsh.enabled {
        providers.insert("dsh".to_string(), state.ai.models(refresh).await);
    }
    // Read health *after* a refresh may have booted the harness, otherwise the
    // embedded status snapshot would say "not ready" for a runtime we just
    // started on the caller's behalf.
    let status = state.ai.status().await;
    let current_model = if status.model.is_empty() {
        dsh.model.clone()
    } else {
        status.model.clone()
    };
    Json(AiModelsResponse {
        providers,
        default: "dsh".to_string(),
        current: AiCurrent {
            provider: dsh.provider.clone(),
            model: current_model,
        },
        status,
    })
}

/// GET /api/ai/status — harness health without starting it.
pub async fn ai_status(State(state): State<Arc<AppState>>) -> Json<AiStatus> {
    Json(state.ai.status().await)
}

/// GET /api/ai/tools — the tool set the harness actually offers the model.
pub async fn ai_tools(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({ "tools": state.ai.tools(None).await }))
}

#[derive(Deserialize)]
pub struct AiChatRequest {
    /// Optional model override; the harness is restarted with this route when
    /// it differs from the configured one.
    pub model: Option<String>,
    pub message: String,
    /// DSH session identity. Missing/empty falls back to the server's default
    /// conversation (older clients).
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Serialize)]
pub struct AiChatResponse {
    pub reply: String,
    pub model: String,
}

#[derive(Deserialize)]
pub struct AiCancelRequest {
    pub conversation_id: String,
}

fn session_of(req: &AiChatRequest, fallback: &str) -> String {
    req.conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .unwrap_or(fallback)
        .to_string()
}

fn disabled_error() -> String {
    "AI is disabled: set [ai.dsh] enabled = true in ~/.config/hyprtrace/config.toml".to_string()
}

async fn save_message(state: &Arc<AppState>, role: &str, content: &str) {
    let db = state.db.lock().await;
    if let Err(error) = db.save_ai_message(role, content, "dsh") {
        log::warn!("AI: failed to save {role} message: {error}");
    }
}

async fn finalize(state: &Arc<AppState>, msg_id: Option<i64>, full_text: &str) {
    if let Some(id) = msg_id {
        let db = state.db.lock().await;
        if let Err(error) = db.update_ai_message(id, full_text, true) {
            log::warn!("AI: finalize save failed: {error}");
        }
    }
}

/// POST /api/ai/chat — non-streaming completion.
pub async fn ai_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiChatRequest>,
) -> Result<Json<AiChatResponse>, Json<serde_json::Value>> {
    if req.message.trim().is_empty() {
        return Err(Json(json!({"error": "Message cannot be empty"})));
    }
    if !state.ai.enabled() {
        return Err(Json(json!({"error": disabled_error()})));
    }
    let session = session_of(&req, &state.ai.default_session().await);
    save_message(&state, "user", &req.message).await;
    match state.ai.complete(&session, &req.message).await {
        Ok(reply) => {
            save_message(&state, "assistant", &reply).await;
            Ok(Json(AiChatResponse {
                reply,
                model: "dsh".to_string(),
            }))
        }
        Err(error) => Err(Json(json!({"error": format!("AI error: {error}")}))),
    }
}

/// POST /api/ai/chat/agent — streamed agent turn (NDJSON).
pub async fn chat_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiChatRequest>,
) -> Result<Response<Body>, (StatusCode, Json<serde_json::Value>)> {
    if req.message.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Message cannot be empty"})),
        ));
    }
    if !state.ai.enabled() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": disabled_error()})),
        ));
    }
    let session = session_of(&req, &state.ai.default_session().await);
    save_message(&state, "user", &req.message).await;

    let (ndjson_tx, ndjson_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Infallible>>(64);
    tokio::spawn(run_ai_agent(
        state,
        session,
        req.message.clone(),
        req.model.clone(),
        ndjson_tx,
    ));
    let stream = ReceiverStream::new(ndjson_rx);
    Ok(Response::builder()
        .header("Content-Type", "application/x-ndjson")
        .header("Cache-Control", "no-cache")
        .body(Body::from_stream(stream))
        .unwrap())
}

/// POST /api/ai/cancel — cancel the active turn of one conversation.
pub async fn ai_cancel(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiCancelRequest>,
) -> Json<serde_json::Value> {
    let cancelled = state.ai.cancel(req.conversation_id.trim()).await;
    Json(json!({"status": if cancelled { "ok" } else { "not_found" }}))
}

async fn run_ai_agent(
    state: Arc<AppState>,
    session: String,
    message: String,
    model: Option<String>,
    ndjson_tx: tokio::sync::mpsc::Sender<Result<Vec<u8>, Infallible>>,
) {
    let http_alive = AtomicBool::new(true);
    macro_rules! send {
        ($v:expr) => {{
            if http_alive.load(Ordering::Relaxed) {
                let mut line = serde_json::to_string(&$v).unwrap_or_default();
                line.push('\n');
                if ndjson_tx.send(Ok(line.into_bytes())).await.is_err() {
                    http_alive.store(false, Ordering::Relaxed);
                    log::info!("AI: client disconnected; continuing server-side");
                }
            }
        }};
    }

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<AiEvent>(256);
    let service = state.ai.clone();
    let prompt_session = session.clone();
    let handle = tokio::spawn(async move {
        service
            .prompt(&prompt_session, &message, model.as_deref(), event_tx)
            .await
    });

    let mut full_text = String::new();
    let mut msg_id: Option<i64> = None;
    let mut failed = false;
    let mut cancelled = false;
    let mut client_gone = false;
    let mut last_save = std::time::Instant::now();

    loop {
        tokio::select! {
            // A dropped HTTP client is a cancellation request: forward it to
            // the harness so the model stops burning tokens.
            _ = ndjson_tx.closed(), if !client_gone => {
                client_gone = true;
                log::info!("AI: client disconnected; cancelling the turn");
                let _ = state.ai.cancel(&session).await;
            }
            event = event_rx.recv() => {
                let Some(event) = event else { break };
                if matches!(event, AiEvent::Error(_)) {
                    failed = true;
                }
                if matches!(event, AiEvent::Cancelled) {
                    cancelled = true;
                }
                if let AiEvent::Text(delta) = &event {
                    full_text.push_str(delta);
                    if msg_id.is_none() {
                        let db = state.db.lock().await;
                        msg_id = db.save_ai_message_streaming("assistant", "", "dsh").ok();
                    }
                }
                let wire = event.to_json();
                send!(wire);
                if last_save.elapsed().as_secs() >= 3 {
                    if let Some(id) = msg_id {
                        let db = state.db.lock().await;
                        if let Err(error) = db.update_ai_message(id, &full_text, false) {
                            log::warn!("AI: partial save failed: {error}");
                        }
                    }
                    last_save = std::time::Instant::now();
                }
            }
        }
    }

    match handle.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            failed = true;
            log::error!("AI: {error}");
            send!(json!({"type": "error", "message": format!("AI error: {error}")}));
        }
        Err(error) => {
            failed = true;
            log::error!("AI: task join error: {error}");
            send!(json!({"type": "error", "message": "Internal AI error"}));
        }
    }

    if !failed && !cancelled && !client_gone && msg_id.is_none() && full_text.trim().is_empty() {
        // Same history marker the old loop wrote when the model answered only
        // with tool calls. A failed, cancelled or client-aborted turn already
        // surfaced its outcome (and may have been wiped by "clear history").
        save_message(&state, "assistant", "(responded via tool calls)").await;
    }

    finalize(&state, msg_id, &full_text).await;
}

/// POST /api/ai/report/weekly — generate a Markdown weekly report through the
/// harness. Returns `{report}`.
pub async fn ai_weekly_report(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    if !state.ai.enabled() {
        return Err(Json(json!({"error": disabled_error()})));
    }
    // LOCAL dates: must match the local-date buckets in the summary tables.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let from = (chrono::Local::now() - chrono::Duration::days(6))
        .format("%Y-%m-%d")
        .to_string();

    // Snapshot data without holding the lock across the AI call.
    let snapshot = {
        let db = state.db.lock().await;
        let mut parts = Vec::new();
        if let Ok(summary) = db.report(&from, &today) {
            parts.push(summary);
        }
        if let Ok(goals) = db.goal_progress() {
            let gs: Vec<String> = goals
                .iter()
                .map(|p| {
                    format!(
                        "{}: {}% ({}/{} target)",
                        p.goal.name,
                        p.pct.round(),
                        p.today_ms,
                        p.goal.daily_target_ms
                    )
                })
                .collect();
            if !gs.is_empty() {
                parts.push(format!("Today's goals progress:\n{}", gs.join("\n")));
            }
        }
        if let Ok(pred) = db.predict(14) {
            parts.push(format!(
                "Trend: daily avg {:.1}h, tomorrow projected {:.1}h",
                pred.daily_avg_ms as f64 / 3_600_000.0,
                pred.predicted_tomorrow_ms as f64 / 3_600_000.0
            ));
        }
        parts.join("\n\n")
    };

    let prompt = format!(
        "You are HyprTrace's analytics reporter. Below is raw usage data for the last week. \
         Write a well-structured Markdown weekly report in the user's language (Chinese if the \
         data suggests Chinese, otherwise English). Include: 1) a summary headline, 2) key \
         stats, 3) 2-3 specific insights about their habits, 4) 2-3 concrete suggestions. \
         Be honest and specific. Keep it under 500 words.\n\nDATA:\n{}",
        snapshot
    );

    let session = format!("{}-weekly", state.ai.default_session().await);
    match state.ai.complete(&session, &prompt).await {
        Ok(report) => Ok(Json(json!({ "report": report }))),
        Err(error) => {
            log::error!("AI weekly report failed: {error}");
            Err(Json(json!({"error": format!("AI error: {error}")})))
        }
    }
}

/// GET /api/ai/conversations — persisted chat history (display only; the
/// harness owns the actual conversation log).
pub async fn ai_conversations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AiMessage>>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.ai_conversations(100) {
        Ok(messages) => Ok(Json(messages)),
        Err(error) => {
            log::error!("Failed to get conversations: {error}");
            Err(Json(json!({"error": "Internal server error"})))
        }
    }
}

/// DELETE /api/ai/conversations — clear history and cancel every running turn.
pub async fn clear_conversations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    state.ai.cancel_all().await;
    let cleared = {
        let db = state.db.lock().await;
        db.clear_ai_conversations()
    };
    state.ai.reset_default_session().await;
    match cleared {
        Ok(_) => Ok(Json(json!({"status": "ok"}))),
        Err(error) => {
            log::error!("Failed to clear conversations: {error}");
            Err(Json(json!({"error": "Internal server error"})))
        }
    }
}
