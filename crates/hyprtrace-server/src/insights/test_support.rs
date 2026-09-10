//! Shared fixtures for the `insights` unit tests.

use super::session::InsightSession;

    pub(crate) fn session(class: &str, start: &str, end: &str, dur: i64, date: &str) -> InsightSession {
        InsightSession {
            class: class.to_string(),
            started_at: start.to_string(),
            ended_at: Some(end.to_string()),
            duration_ms: dur,
            date_local: date.to_string(),
        }
    }
