// GENERATED FILE — DO NOT EDIT.
//
// Rendered from the Rust models by `crates/hyprtrace-server/src/api_types.rs`.
// Regenerate with:
//     HYPRTRACE_WRITE_TYPES=1 cargo test -p hyprtrace-server api_types
// A test fails when this file and the models disagree, so editing it by hand
// only moves the failure to CI.


/**
 * Metadata for a single application, resolved from a `.desktop` file.
 */
export type AppMetadata = { desktop_id: string, display_name: string, icon: string, };

export type TodaySummary = { date: string, total_active_ms: number, total_idle_ms: number, total_focused_ms: number, app_count: number, session_count: number, top_apps: Array<AppRank>, };

export type AppRank = { class: string, total_ms: number, percentage: number, session_count: number, focused_ms: number, focused_session_count: number, category: string, };

export type CategoryRule = { id: number | null, pattern: string, category: string, priority: number, };

export type HourlyBucket = { hour: number, total_ms: number, session_count: number, focused_ms: number, };

export type Session = { id: number, class: string, title: string, workspace: string | null, started_at: string, ended_at: string | null, duration_ms: number | null, activity_state: string | null, focused_ms: number | null, };

/**
 * One full-text hit: the session, plus the matching title excerpt.
 */
export type SearchHit = { 
/**
 * Plain text with the match in context. No markup is embedded — the UI can
 * highlight the query itself, and nothing here needs escaping.
 */
snippet: string, id: number, class: string, title: string, workspace: string | null, started_at: string, ended_at: string | null, duration_ms: number | null, activity_state: string | null, focused_ms: number | null, };

export type DailyTrend = { date: string, total_ms: number, session_count: number, focused_ms: number, };

/**
 * Per-day activity totals, used for the yearly activity heatmap. Days with no
 * recorded sessions are zero-filled so the client can render a complete grid.
 */
export type DailyActivity = { date: string, total_ms: number, focused_ms: number, session_count: number, };

/**
 * Aggregate of time spent on a single window title (document/tab) over a range.
 */
export type TitleStat = { class: string, title: string, total_ms: number, session_count: number, last_used_at: string, };

export type AiMessage = { id: number, created_at: string, role: string, content: string, model: string, complete: boolean | null, };

export type AppResource = { class: string, avg_cpu_pct: number, peak_mem_kb: number, sample_count: number, };

export type ActivityEvent = { id: number, session_id: number | null, state: string, started_at: string, ended_at: string | null, duration_ms: number | null, };

export type DisruptionEvent = { id: number, kind: string, app: string | null, summary: string | null, occurred_at: string, };

export type EfficiencyScore = { date: string, score: number, focus_ratio: number, avg_session_secs: number, late_night_pct: number, disruption_count: number, total_active_ms: number, };

export type Goal = { id: number | null, name: string, target_type: string, target_key: string | null, daily_target_ms: number, enabled: boolean, };

export type GoalProgress = { goal: Goal, today_ms: number, pct: number, };

export type TrendPrediction = { today_ms: number, predicted_today_ms: number, predicted_tomorrow_ms: number, daily_avg_ms: number, slope: number, window_days: number, };

export type CurrentStatus = { current_app: string, current_session_min: number, today_ms: number, today_focused_ms: number, today_pct_goal: number, goal_name: string | null, efficiency_score: number | null, };

export type WorkspaceRecommendation = { app: string, workspace: string, time_pct: number, session_count: number, total_ms: number, confidence: string, };

export type Project = { id: number | null, name: string, color: string, sort_order: number, };

export type ProjectRule = { id: number | null, project_id: number, pattern: string, priority: number, };

export type ProjectStat = { project_id: number | null, name: string, color: string, total_ms: number, session_count: number, percentage: number, };
