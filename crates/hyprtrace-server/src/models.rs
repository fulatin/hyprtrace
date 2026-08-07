use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct TodaySummary {
    pub date: String,
    pub total_active_ms: i64,
    pub total_idle_ms: i64,
    pub total_focused_ms: i64,
    pub app_count: usize,
    pub session_count: i64,
    pub top_apps: Vec<AppRank>,
}

#[derive(Debug, Serialize, Clone)]
pub struct AppRank {
    pub class: String,
    pub total_ms: i64,
    pub percentage: f64,
    pub session_count: i64,
    pub focused_ms: i64,
    pub focused_session_count: i64,
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CategoryRule {
    pub id: Option<i64>,
    pub pattern: String,
    pub category: String,
    #[serde(default)]
    pub priority: i64,
}

#[derive(Debug, Serialize)]
pub struct HourlyBucket {
    pub hour: u8,
    pub total_ms: i64,
    pub session_count: i64,
    pub focused_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub id: i64,
    pub class: String,
    pub title: String,
    pub workspace: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub activity_state: Option<String>,
    pub focused_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DailyTrend {
    pub date: String,
    pub total_ms: i64,
    pub session_count: i64,
    pub focused_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct AiMessage {
    pub id: i64,
    pub created_at: String,
    pub role: String,
    pub content: String,
    pub model: String,
    pub complete: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: u32,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct AppResource {
    pub class: String,
    pub avg_cpu_pct: f64,
    pub peak_mem_kb: i64,
    pub sample_count: i64,
}

#[derive(Debug, Serialize)]
pub struct ActivityEvent {
    pub id: i64,
    pub session_id: Option<i64>,
    pub state: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DisruptionEvent {
    pub id: i64,
    pub kind: String,
    pub app: Option<String>,
    pub summary: Option<String>,
    pub occurred_at: String,
}

#[derive(Debug, Serialize)]
pub struct EfficiencyScore {
    pub date: String,
    pub score: i64,
    pub focus_ratio: f64,
    pub avg_session_secs: f64,
    pub late_night_pct: f64,
    pub disruption_count: i64,
    pub total_active_ms: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Goal {
    pub id: Option<i64>,
    pub name: String,
    pub target_type: String,
    pub target_key: Option<String>,
    pub daily_target_ms: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct GoalProgress {
    pub goal: Goal,
    pub today_ms: i64,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct TrendPrediction {
    pub today_ms: i64,
    pub predicted_today_ms: i64,
    pub predicted_tomorrow_ms: i64,
    pub daily_avg_ms: i64,
    pub slope: f64,
    pub window_days: i64,
}

#[derive(Debug, Serialize)]
pub struct CurrentStatus {
    pub current_app: String,
    pub current_session_min: i64,
    pub today_ms: i64,
    pub today_focused_ms: i64,
    pub today_pct_goal: f64,
    pub goal_name: Option<String>,
    pub efficiency_score: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceRecommendation {
    pub app: String,
    pub workspace: String,
    pub time_pct: f64,
    pub session_count: i64,
    pub total_ms: i64,
    pub confidence: String,
}
