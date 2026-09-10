//! The one place that decides whether a window title may be persisted.
//!
//! `daemon.record_titles = false` is a privacy switch: `sessions.title` must
//! stay empty while the app class keeps being tracked. Two separate code paths
//! create sessions — `listener.rs` on a focus change and `idle_monitor.rs` when
//! activity resumes — and only the first one honoured the switch, so a session
//! resumed after idle silently stored the real window title. Both writers now
//! call [`recorded_title`], so the switch cannot be bypassed on one path again.

/// The title that may be written to `sessions.title`.
///
/// Returns an empty string when the user turned title recording off. This is an
/// allow/deny decision, not a content filter: when recording is disabled the
/// title is dropped regardless of what it contains.
pub fn recorded_title(title: &str, record_titles: bool) -> &str {
    if record_titles {
        title
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_stored_when_recording_is_enabled() {
        assert_eq!(recorded_title("main.rs — hyprtrace", true), "main.rs — hyprtrace");
    }

    #[test]
    fn titles_are_dropped_when_recording_is_disabled() {
        assert_eq!(recorded_title("main.rs — hyprtrace", false), "");
    }

    #[test]
    fn an_empty_title_stays_empty_either_way() {
        assert_eq!(recorded_title("", true), "");
        assert_eq!(recorded_title("", false), "");
    }
}
