use crate::ai::{ChatMessage, StreamEvent, ToolCall};
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
use serde_json::json;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Serialize)]
pub struct AiModelsResponse {
    pub providers: std::collections::HashMap<String, Vec<String>>,
    pub default: String,
}

pub async fn ai_models(State(state): State<Arc<AppState>>) -> Json<AiModelsResponse> {
    // Snapshot under the lock, call outside it: provider HTTP requests can
    // take minutes and must not block other AI endpoints.
    let ai = {
        let ai = state.ai.lock().await;
        ai.snapshot()
    };
    let providers = ai.list_all_models().await;
    Json(AiModelsResponse {
        providers,
        default: ai.default_provider.clone(),
    })
}

/// GET /api/ai/tools — list available agent tools.
pub async fn ai_tools() -> Json<serde_json::Value> {
    let tools = crate::ai::tools::all_tools();
    Json(json!({
        "tools": tools
            .iter()
            .map(|t| json!({"name": t.name, "description": t.description}))
            .collect::<Vec<_>>()
    }))
}

#[derive(Deserialize)]
pub struct WeeklyReportRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// POST /api/ai/report/weekly — ask the AI to write an insightful weekly report
/// from the last 7 days of usage data. Returns Markdown.
pub async fn ai_weekly_report(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WeeklyReportRequest>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    // LOCAL dates: must match the local-date buckets in the summary tables (M3).
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let from = (chrono::Local::now() - chrono::Duration::days(6)).format("%Y-%m-%d").to_string();

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

    let messages = vec![
        crate::ai::ChatMessage::new("system", "You are a concise analytics reporter."),
        crate::ai::ChatMessage::new("user", prompt),
    ];

    let ai = {
        let ai = state.ai.lock().await;
        ai.snapshot()
    };
    let provider_name = req.provider.clone().unwrap_or_else(|| ai.default_provider.clone());
    let reply = match ai.chat(&provider_name, req.model.as_deref(), &messages).await {
        Ok(r) => r,
        Err(e) => {
            log::error!("AI weekly report failed: {}", e);
            return Err(Json(json!({"error": format!("AI error: {}", e)})));
        }
    };
    Ok(Json(json!({ "report": reply })))
}

pub async fn ai_conversations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AiMessage>>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.ai_conversations(100) {
        Ok(msgs) => Ok(Json(msgs)),
        Err(e) => {
            log::error!("Failed to get conversations: {}", e);
            Err(Json(json!({"error": "Internal server error"})))
        }
    }
}

pub async fn clear_conversations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    let db = state.db.lock().await;
    match db.clear_ai_conversations() {
        Ok(_) => Ok(Json(json!({"status": "ok"}))),
        Err(e) => {
            log::error!("Failed to clear conversations: {}", e);
            Err(Json(json!({"error": "Internal server error"})))
        }
    }
}

#[derive(Deserialize)]
pub struct AiChatRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
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

/// Resolve provider + validate. Returns (provider_name, system_prompt).
async fn resolve_provider(
    state: &Arc<AppState>,
    req: &AiChatRequest,
) -> Result<(String, String), String> {
    let ai = state.ai.lock().await;
    let provider_name = req
        .provider
        .clone()
        .unwrap_or_else(|| ai.default_provider.clone());
    if provider_name == "openai" && !ai.openai_configured {
        return Err("Please configure OpenAI API Key".to_string());
    }
    Ok((provider_name, ai.system_prompt.clone()))
}

/// Build the full message list: system prompt + optional usage-data context
/// + recent history + the new user message.
async fn build_messages(
    state: &Arc<AppState>,
    system_prompt: String,
    req: &AiChatRequest,
) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage::new("system", system_prompt)];

    if req.include_data {
        // LOCAL dates: must match the local-date buckets in the summary tables (M3).
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let (from, to) = match req.date_range.as_deref() {
            Some("week") => {
                let d = chrono::Local::now() - chrono::Duration::days(7);
                (d.format("%Y-%m-%d").to_string(), today.clone())
            }
            Some("month") => {
                let d = chrono::Local::now() - chrono::Duration::days(30);
                (d.format("%Y-%m-%d").to_string(), today.clone())
            }
            _ => (today.clone(), today.clone()),
        };

        let mut context_parts = Vec::new();
        {
            let db = state.db.lock().await;
            if let Ok(summary) = db.today_summary(&from) {
                context_parts.push(format!(
                    "Date: {}, Total active: {}ms, Focused: {}ms, Apps: {}, Sessions: {}",
                    summary.date,
                    summary.total_active_ms,
                    summary.total_focused_ms,
                    summary.app_count,
                    summary.session_count
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
            messages.push(ChatMessage::new(
                "system",
                format!("User's usage data context:\n{}", context_parts.join("\n")),
            ));
        }
    }

    {
        let db = state.db.lock().await;
        if let Ok(history) = db.ai_conversations(10) {
            let mut last_role = String::new();
            for msg in history {
                if !msg.complete.unwrap_or(true) && msg.content.trim().is_empty() {
                    continue;
                }
                if msg.role == last_role {
                    continue;
                }
                messages.push(ChatMessage::new(&msg.role, msg.content));
                last_role = msg.role;
            }
        }
    }

    messages.push(ChatMessage::new("user", req.message.clone()));
    messages
}

pub async fn ai_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiChatRequest>,
) -> Result<Json<AiChatResponse>, Json<serde_json::Value>> {
    if req.message.trim().is_empty() {
        return Err(Json(json!({"error": "Message cannot be empty"})));
    }

    let (provider_name, system_prompt) = resolve_provider(&state, &req)
        .await
        .map_err(|e| Json(json!({"error": e})))?;

    let messages = build_messages(&state, system_prompt, &req).await;

    let ai = {
        let ai = state.ai.lock().await;
        ai.snapshot()
    };
    let reply = ai.chat(&provider_name, req.model.as_deref(), &messages).await;

    match reply {
        Ok(reply) => {
            {
                let db = state.db.lock().await;
                if let Err(e) = db.save_ai_message("user", &req.message, &provider_name) {
                    log::warn!("Failed to save user message: {}", e);
                }
                if let Err(e) = db.save_ai_message("assistant", &reply, &provider_name) {
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
                Err(Json(json!({"error": "Please configure OpenAI API Key"})))
            } else {
                Err(Json(json!({"error": "AI service unavailable"})))
            }
        }
    }
}

pub async fn chat_stream(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AiChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, Json<serde_json::Value>> {
    if req.message.trim().is_empty() {
        return Err(Json(json!({"error": "Message cannot be empty"})));
    }

    let (provider_name, system_prompt) = resolve_provider(&state, &req)
        .await
        .map_err(|e| Json(json!({"error": e})))?;

    let messages = build_messages(&state, system_prompt, &req).await;

    {
        let db = state.db.lock().await;
        if let Err(e) = db.save_ai_message("user", &req.message, &provider_name) {
            log::warn!("Failed to save user message: {}", e);
        }
    }

    let (ai_tx, mut ai_rx) = tokio::sync::mpsc::channel::<String>(64);
    let (sse_tx, sse_rx) = tokio::sync::mpsc::channel::<String>(64);

    let state_clone = state.clone();
    let pname = provider_name.clone();
    let model = req.model.clone();
    tokio::spawn(async move {
        let ai = {
            let ai = state_clone.ai.lock().await;
            ai.snapshot()
        };
        if let Err(e) = ai
            .chat_stream(&pname, model.as_deref(), &messages, ai_tx)
            .await
        {
            log::error!("AI chat stream error: {}", e);
        }
    });

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
            Json(json!({"error": "Message cannot be empty"})),
        ));
    }

    let (provider_name, system_prompt) = resolve_provider(&state, &req)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;

    let messages = build_messages(&state, system_prompt, &req).await;

    {
        let db = state.db.lock().await;
        if let Err(e) = db.save_ai_message("user", &req.message, &provider_name) {
            log::warn!("Failed to save user message: {}", e);
        }
    }

    let (ai_tx, mut ai_rx) = tokio::sync::mpsc::channel::<String>(64);

    let state_clone = state.clone();
    let pname = provider_name.clone();
    let model = req.model.clone();
    tokio::spawn(async move {
        let ai = {
            let ai = state_clone.ai.lock().await;
            ai.snapshot()
        };
        if let Err(e) = ai
            .chat_stream(&pname, model.as_deref(), &messages, ai_tx)
            .await
        {
            log::error!("AI chat stream error: {}", e);
        }
    });

    let (http_tx, http_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Infallible>>(64);

    let state_clone2 = state.clone();
    let model_name = provider_name.clone();
    tokio::spawn(async move {
        let mut full_content = String::new();
        let mut chunk_count = 0u64;
        let mut last_save = std::time::Instant::now();
        let mut http_alive = true;

        let db = state_clone2.db.lock().await;
        let msg_id = match db.save_ai_message_streaming("assistant", "", &model_name) {
            Ok(id) => id,
            Err(e) => {
                log::error!("Failed to init streaming message: {}", e);
                return;
            }
        };
        drop(db);

        while let Some(chunk) = ai_rx.recv().await {
            full_content.push_str(&chunk);
            chunk_count += 1;
            if http_alive && http_tx.send(Ok(chunk.into_bytes())).await.is_err() {
                http_alive = false;
                log::info!("Client disconnected mid-stream; continuing generation server-side");
            }
            if chunk_count % 5 == 0 || last_save.elapsed().as_secs() >= 3 {
                let db = state_clone2.db.lock().await;
                if let Err(e) = db.update_ai_message(msg_id, &full_content, false) {
                    log::warn!("Failed to save partial message: {}", e);
                }
                drop(db);
                last_save = std::time::Instant::now();
            }
        }

        if !full_content.is_empty() {
            let db = state_clone2.db.lock().await;
            if let Err(e) = db.update_ai_message(msg_id, &full_content, true) {
                log::warn!("Failed to finalize assistant message: {}", e);
            }
        }
    });

    let stream = ReceiverStream::new(http_rx);
    Ok(Response::builder()
        .header("Content-Type", "text/plain")
        .body(Body::from_stream(stream))
        .unwrap())
}

/// POST /api/ai/chat/agent — NDJSON event stream with tool calling.
///
/// Event lines:
///   {"type":"text","delta":"..."}
///   {"type":"tool_call","id":"...","name":"...","args":{...}}
///   {"type":"tool_result","id":"...","name":"...","ok":bool,"result":...}
///   {"type":"done"}
///   {"type":"error","message":"..."}
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

    let (provider_name, system_prompt) = resolve_provider(&state, &req)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e}))))?;

    let messages = build_messages(&state, system_prompt, &req).await;

    {
        let db = state.db.lock().await;
        if let Err(e) = db.save_ai_message("user", &req.message, &provider_name) {
            log::warn!("Failed to save user message: {}", e);
        }
    }

    let (ndjson_tx, ndjson_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Infallible>>(64);

    tokio::spawn(run_agent(
        state,
        provider_name,
        req.model.clone(),
        messages,
        ndjson_tx,
    ));

    let stream = ReceiverStream::new(ndjson_rx);
    Ok(Response::builder()
        .header("Content-Type", "application/x-ndjson")
        .header("Cache-Control", "no-cache")
        .body(Body::from_stream(stream))
        .unwrap())
}

async fn run_agent(
    state: Arc<AppState>,
    provider_name: String,
    model: Option<String>,
    mut messages: Vec<ChatMessage>,
    ndjson_tx: tokio::sync::mpsc::Sender<Result<Vec<u8>, Infallible>>,
) {
    let mut http_alive = true;
    macro_rules! send {
        ($v:expr) => {{
            if http_alive {
                let mut s = serde_json::to_string(&$v).unwrap_or_default();
                s.push('\n');
                if ndjson_tx.send(Ok(s.into_bytes())).await.is_err() {
                    http_alive = false;
                    log::info!("Agent: client disconnected; continuing server-side");
                }
            }
        }};
    }

    let model_name = provider_name.clone();
    let tools = crate::ai::tools::all_tools();
    let mut tools_enabled = true;
    let mut full_text = String::new();
    let mut msg_id: Option<i64> = None;
    let mut last_save = std::time::Instant::now();

    const MAX_ROUNDS: usize = 6;

    for round in 0..MAX_ROUNDS {
        let (ev_tx, mut ev_rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);
        let state_c = state.clone();
        let msgs = messages.clone();
        let pname_c = provider_name.clone();
        let model_c = model.clone();
        let tools_c: Option<Vec<crate::ai::ToolDef>> =
            if tools_enabled { Some(tools.clone()) } else { None };

        let handle = tokio::spawn(async move {
            let ai = {
                let ai = state_c.ai.lock().await;
                ai.snapshot()
            };
            ai.chat_stream_events(
                &pname_c,
                model_c.as_deref(),
                &msgs,
                tools_c.as_deref(),
                ev_tx,
            )
            .await
        });

        let mut round_text = String::new();
        let mut calls: Vec<ToolCall> = Vec::new();

        while let Some(ev) = ev_rx.recv().await {
            match ev {
                StreamEvent::Text(t) => {
                    round_text.push_str(&t);
                    full_text.push_str(&t);

                    if msg_id.is_none() {
                        let db = state.db.lock().await;
                        msg_id = db
                            .save_ai_message_streaming("assistant", "", &model_name)
                            .ok();
                        drop(db);
                    }
                    send!(json!({"type": "text", "delta": t}));

                    if last_save.elapsed().as_secs() >= 3 {
                        if let Some(id) = msg_id {
                            let db = state.db.lock().await;
                            if let Err(e) = db.update_ai_message(id, &full_text, false) {
                                log::warn!("Agent: partial save failed: {}", e);
                            }
                            drop(db);
                        }
                        last_save = std::time::Instant::now();
                    }
                }
                StreamEvent::ToolCall(tc) => calls.push(tc),
            }
        }

        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if round == 0 && tools_enabled {
                    log::warn!(
                        "Agent: tool-enabled request failed ({}), retrying without tools",
                        e
                    );
                    tools_enabled = false;
                    continue;
                }
                log::error!("Agent: provider stream error: {}", e);
                send!(json!({"type": "error", "message": format!("AI error: {}", e)}));
                finalize(&state, msg_id, &full_text, &model_name).await;
                return;
            }
            Err(e) => {
                log::error!("Agent: task join error: {}", e);
                send!(json!({"type": "error", "message": "Internal agent error"}));
                finalize(&state, msg_id, &full_text, &model_name).await;
                return;
            }
        }

        if calls.is_empty() {
            break;
        }

        // Append the assistant tool_calls message in provider wire format.
        let tcs: Vec<serde_json::Value> = calls
            .iter()
            .map(|tc| {
                if provider_name == "openai" {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments.to_string(),
                        }
                    })
                } else {
                    json!({
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments,
                        }
                    })
                }
            })
            .collect();
        messages.push(ChatMessage::assistant_tool_calls(
            round_text.clone(),
            json!(tcs),
        ));

        for tc in calls {
            log::info!("Agent: executing tool {} ({})", tc.name, tc.id);
            send!(json!({"type": "tool_call", "id": tc.id, "name": tc.name, "args": tc.arguments}));

            let result =
                crate::ai::tools::execute_tool(&tc.name, &tc.arguments, &state.db).await;

            match result {
                Ok(r) => {
                    send!(json!({"type": "tool_result", "id": tc.id, "name": tc.name, "ok": true, "result": r}));
                    messages.push(ChatMessage::tool_result(
                        &provider_name,
                        &tc,
                        r.to_string(),
                    ));
                }
                Err(e) => {
                    let es = e.to_string();
                    log::warn!("Agent: tool {} failed: {}", tc.name, es);
                    send!(json!({"type": "tool_result", "id": tc.id, "name": tc.name, "ok": false, "result": es}));
                    messages.push(ChatMessage::tool_result(
                        &provider_name,
                        &tc,
                        format!("Error: {}", es),
                    ));
                }
            }
        }
    }

    // Model produced only tool calls and no text — record a marker so the
    // conversation history isn't confusing.
    if msg_id.is_none() && full_text.trim().is_empty() {
        let db = state.db.lock().await;
        if let Err(e) = db.save_ai_message("assistant", "(responded via tool calls)", &model_name)
        {
            log::warn!("Agent: failed to save marker message: {}", e);
        }
    }

    finalize(&state, msg_id, &full_text, &model_name).await;
    let _ = ndjson_tx
        .send(Ok(b"{\"type\":\"done\"}\n".to_vec()))
        .await;
}

async fn finalize(
    state: &Arc<AppState>,
    msg_id: Option<i64>,
    full_text: &str,
    _model_name: &str,
) {
    if let Some(id) = msg_id {
        let db = state.db.lock().await;
        if let Err(e) = db.update_ai_message(id, full_text, true) {
            log::warn!("Agent: finalize save failed: {}", e);
        }
    }
}
