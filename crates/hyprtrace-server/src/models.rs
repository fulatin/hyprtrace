use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TodaySummary {
    pub date: String,
    pub total_active_ms: i64,
    pub total_idle_ms: i64,
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
}

#[derive(Debug, Serialize)]
pub struct HourlyBucket {
    pub hour: u8,
    pub total_ms: i64,
    pub session_count: i64,
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
}

#[derive(Debug, Serialize)]
pub struct DailyTrend {
    pub date: String,
    pub total_ms: i64,
    pub session_count: i64,
}

#[derive(Debug, Serialize)]
pub struct AiMessage {
    pub id: i64,
    pub created_at: String,
    pub role: String,
    pub content: String,
    pub model: String,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: u32,
    pub page: u32,
    pub per_page: u32,
}