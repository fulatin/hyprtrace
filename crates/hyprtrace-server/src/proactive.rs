use std::sync::Arc;
use std::time::Duration;

/// Periodically analyzes recent usage data with the configured AI provider and
/// sends a desktop notification when the AI flags something notable (e.g.
/// excessive late-night use, a spike in gaming, missing a goal).
pub fn spawn_proactive_monitor(state: Arc<crate::routes::AppState>, interval_minutes: u64) {
    tokio::spawn(async move {
        let interval = Duration::from_secs((interval_minutes.max(15)) * 60);
        // Avoid repeating the same notification back-to-back.
        let mut last_notification: Option<String> = None;
        let mut tick: u64 = 0;

        loop {
            tick += 1;
            tokio::time::sleep(interval).await;

            if tick == 1 {
                // First tick after startup: give the daemon a chance to record data.
                tokio::time::sleep(Duration::from_secs(120)).await;
            }

            let insight = match build_insight(&state).await {
                Ok(i) => i,
                Err(e) => {
                    log::warn!("Proactive monitor: analysis failed: {}", e);
                    continue;
                }
            };

            if insight.trim().is_empty() {
                continue;
            }
            if last_notification.as_deref() == Some(insight.trim()) {
                continue;
            }
            last_notification = Some(insight.trim().to_string());

            log::info!("Proactive insight: {}", insight);
            // D-Bus notification on a blocking thread: the zbus call is
            // synchronous and must not stall this tokio worker.
            let body = insight.trim().to_string();
            tokio::task::spawn_blocking(move || {
                hyprtrace_common::notify::send("HyprTrace insight", &body);
            });
        }
    });
}

/// Gathers a compact snapshot of today's usage and asks the AI for a short,
/// actionable observation. Returns an empty string if there's nothing worth
/// flagging.
async fn build_insight(state: &Arc<crate::routes::AppState>) -> anyhow::Result<String> {
    // LOCAL today: must match the local-date buckets in the summary tables (M3).
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Snapshot data without holding the DB lock across the AI call.
    let (summary_json, goals_json, efficiency_json) = {
        let db = state.db.lock().await;
        let summary = db.today_summary(&today).ok();
        let goals = db.goal_progress().ok();
        let efficiency = db.efficiency_score(&today).ok();
        let s = summary
            .map(|s| serde_json::to_value(s))
            .transpose()
            .ok()
            .flatten();
        let g = goals
            .map(|g| serde_json::to_value(g))
            .transpose()
            .ok()
            .flatten();
        let e = efficiency
            .map(|e| serde_json::to_value(e))
            .transpose()
            .ok()
            .flatten();
        (s, g, e)
    };

    let snapshot = format!(
        "Today's usage snapshot: {:?}\nGoals: {:?}\nEfficiency: {:?}",
        summary_json, goals_json, efficiency_json
    );

    let prompt = format!(
        "You are a productivity watchdog for a window-usage tracker. \
         Given this snapshot of the user's day, decide whether there is ONE \
         notable, actionable observation worth nudging them about right now. \
         Examples: sustained late-night usage, unusually heavy gaming, a goal \
         at risk of being missed, or an efficiency score collapsing. \
         If nothing is notable, reply with exactly an empty string. \
         Otherwise reply with one short sentence (under 120 chars), no quotes, \
         no prefix.\n\n{}",
        snapshot
    );

    // The harness is the only AI runtime; run the watchdog as a dedicated
    // conversation so it never mixes with the user's chat history.
    let brief = format!(
        "System: You are a concise productivity assistant.\n\n{}",
        prompt
    );
    let session = format!("{}-proactive", state.ai.default_session().await);
    let reply = state.ai.complete(&session, &brief).await?;
    Ok(reply.trim().trim_matches('"').to_string())
}
