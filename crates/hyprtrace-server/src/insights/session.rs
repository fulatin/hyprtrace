/// One `sessions` row reduced to the fields the analytics need.
#[derive(Debug, Clone)]
pub struct InsightSession {
    pub class: String,
    /// RFC3339 UTC start timestamp.
    pub started_at: String,
    /// RFC3339 UTC end timestamp; `None` for the still-running session.
    pub ended_at: Option<String>,
    pub duration_ms: i64,
    /// Local calendar date (`YYYY-MM-DD`).
    pub date_local: String,
}

impl InsightSession {
    /// Start instant in milliseconds since the Unix epoch, if parseable.
    pub(crate) fn start_ms(&self) -> Option<i64> {
        parse_rfc3339_ms(&self.started_at)
    }

    /// End instant; an open session is treated as ending when it started, which
    /// makes its gap 0 instead of a nonsense negative/huge value.
    pub(crate) fn end_ms(&self) -> Option<i64> {
        self.ended_at
            .as_deref()
            .and_then(parse_rfc3339_ms)
            .or_else(|| self.start_ms())
    }

    /// Positive dwell time used by dwell averages and the switch-cost model.
    pub(crate) fn dwell_ms(&self) -> i64 {
        self.duration_ms.max(0)
    }
}

fn parse_rfc3339_ms(ts: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp_millis())
}
