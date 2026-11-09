//! Filesystem path helpers shared by both binaries.
//!
//! These used to be duplicated verbatim in `daemon/src/config.rs` and
//! `server/src/config.rs` (issue #20), with a third copy of `expand_tilde` in
//! `server/src/ai/service.rs`. The duplicates are gone: both crates call the
//! functions below.

use std::path::PathBuf;

/// The HyprTrace config directory.
///
/// Respects `$XDG_CONFIG_HOME` (falling back to `$HOME/.config`) through the
/// `directories` crate instead of hard-coding `$HOME/.config`, which used to
/// ignore a user's XDG override. This is where `config.toml` lives.
pub fn config_dir() -> anyhow::Result<PathBuf> {
    directories::ProjectDirs::from("", "", "hyprtrace")
        .map(|d| d.config_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory (is HOME set?)"))
}

/// Expand a leading `~` to the user's home directory.
///
/// Only a path that *starts* with `~` is expanded; the `~` replaced is always
/// the leading one (the previous hand-written copies used `replacen('~', …)`,
/// which searched the whole string). `~user/...` is not supported — it is
/// left untouched rather than silently mis-expanded.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        // Only `~` and `~/...` are home references; `~user/...` is left
        // untouched rather than turned into `/home/user2/...`.
        if rest.is_empty() || rest.starts_with('/') {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            return PathBuf::from(format!("{home}{rest}"));
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both functions read process-wide environment variables, so the two
    // assertions live in one test to keep them ordered and to avoid racing
    // other tests in this binary that might read `$HOME`.
    #[test]
    fn config_dir_respects_xdg_and_falls_back_to_home() {
        let xdg = std::env::temp_dir().join("hyprtrace-xdg-test");
        std::env::set_var("XDG_CONFIG_HOME", &xdg);
        assert_eq!(config_dir().unwrap(), xdg.join("hyprtrace"));

        std::env::remove_var("XDG_CONFIG_HOME");
        let home = std::env::var("HOME").unwrap();
        assert_eq!(config_dir().unwrap(), PathBuf::from(&home).join(".config/hyprtrace"));
    }

    #[test]
    fn expand_tilde_expands_only_the_leading_tilde() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~/x"), PathBuf::from(&home).join("x"));
        assert_eq!(
            expand_tilde("~/.local/share/hyprtrace/hyprtrace.db"),
            PathBuf::from(&home).join(".local/share/hyprtrace/hyprtrace.db")
        );
        // A tilde that is not leading is not a home reference.
        assert_eq!(expand_tilde("/tmp/a~b"), PathBuf::from("/tmp/a~b"));
        // `~user` is not supported and must be left alone, not half-expanded.
        assert_eq!(expand_tilde("~other/x"), PathBuf::from("~other/x"));
    }
}
