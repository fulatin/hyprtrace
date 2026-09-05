use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub ai: AiConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DaemonConfig {
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_seconds: u64,
    #[serde(default = "default_focused_threshold")]
    pub focused_threshold_seconds: u64,
    #[serde(default = "default_enable_input_monitor")]
    pub enable_input_monitor: bool,
    #[serde(default = "default_record_titles")]
    pub record_titles: bool,
    #[serde(default = "default_break_after_minutes")]
    pub break_after_minutes: u64,
    #[serde(default = "default_late_night_start")]
    pub late_night_start_hour: u32,
    #[serde(default = "default_late_night_end")]
    pub late_night_end_hour: u32,
    #[serde(default)]
    pub hyprlock_command: Option<String>,
}

fn default_db_path() -> String {
    "~/.local/share/hyprtrace/hyprtrace.db".to_string()
}

fn default_idle_timeout() -> u64 {
    300
}

fn default_focused_threshold() -> u64 {
    20 * 60
}

fn default_enable_input_monitor() -> bool {
    true
}

fn default_record_titles() -> bool {
    true
}

fn default_break_after_minutes() -> u64 {
    90
}

fn default_late_night_start() -> u32 {
    23
}

fn default_late_night_end() -> u32 {
    6
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            idle_timeout_seconds: default_idle_timeout(),
            focused_threshold_seconds: default_focused_threshold(),
            enable_input_monitor: default_enable_input_monitor(),
            record_titles: default_record_titles(),
            break_after_minutes: default_break_after_minutes(),
            late_night_start_hour: default_late_night_start(),
            late_night_end_hour: default_late_night_end(),
            hyprlock_command: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub retention_days: u32,
    #[serde(default)]
    pub weekly_report_enabled: bool,
    #[serde(default = "default_weekly_report_day")]
    pub weekly_report_day: u32,
    #[serde(default = "default_weekly_report_hour")]
    pub weekly_report_hour: u32,
    #[serde(default = "default_weekly_report_minute")]
    pub weekly_report_minute: u32,
    /// Optional static API token. When set (non-empty), every `/api/*`
    /// request except `/api/health` must present it via the `X-Auth-Token`
    /// or `Authorization: Bearer` header. Leave unset for plain localhost
    /// use; set it if you bind the server to a non-loopback address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    9420
}

fn default_weekly_report_day() -> u32 {
    1
}

fn default_weekly_report_hour() -> u32 {
    9
}

fn default_weekly_report_minute() -> u32 {
    0
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            retention_days: 0,
            weekly_report_enabled: false,
            weekly_report_day: default_weekly_report_day(),
            weekly_report_hour: default_weekly_report_hour(),
            weekly_report_minute: default_weekly_report_minute(),
            auth_token: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AiConfig {
    #[serde(default = "default_ai_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub openai: OpenAiConfig,
    #[serde(default = "default_proactive_interval")]
    pub proactive_interval_minutes: u64,
}

fn default_ai_provider() -> String {
    "ollama".to_string()
}

fn default_proactive_interval() -> u64 {
    120
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            default_provider: default_ai_provider(),
            ollama: OllamaConfig::default(),
            openai: OpenAiConfig::default(),
            proactive_interval_minutes: default_proactive_interval(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OllamaConfig {
    #[serde(default = "default_ollama_url")]
    pub base_url: String,
    #[serde(default = "default_ollama_model")]
    pub default_model: String,
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "qwen2.5:7b".to_string()
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_url(),
            default_model: default_ollama_model(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAiConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_openai_url")]
    pub base_url: String,
    #[serde(default = "default_openai_model")]
    pub default_model: String,
}

fn default_openai_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_openai_model() -> String {
    "gpt-4o-mini".to_string()
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: default_openai_url(),
            default_model: default_openai_model(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
            server: ServerConfig::default(),
            ai: AiConfig::default(),
        }
    }
}

impl Config {
    /// Load config; use defaults if file doesn't exist
    pub fn load() -> anyhow::Result<Self> {
        let config_dir = dirs_config_dir()?;
        let config_path = config_dir.join("config.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
            toml::from_str(&content).context("Failed to parse config file")
        } else {
            Ok(Config::default())
        }
    }

    pub fn config_path() -> anyhow::Result<PathBuf> {
        let config_dir = dirs_config_dir()?;
        Ok(config_dir.join("config.toml"))
    }

    pub fn db_path_expanded(&self) -> PathBuf {
        expand_tilde(&self.daemon.db_path)
    }
}

fn dirs_config_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    Ok(PathBuf::from(home).join(".config/hyprtrace"))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        PathBuf::from(path.replacen('~', &home, 1))
    } else {
        PathBuf::from(path)
    }
}

/// Load the raw TOML document from disk as a `toml::Value`.
///
/// Unlike `Config::load`, this keeps every key present in the file, including
/// fields the server does not model (e.g. daemon-only settings such as
/// `enable_input_monitor`). Returns an empty table if the file doesn't exist.
pub fn load_toml_document(path: &std::path::Path) -> anyhow::Result<toml::Value> {
    if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        toml::from_str(&content).with_context(|| format!("Failed to parse config file: {:?}", path))
    } else {
        Ok(toml::Value::Table(Default::default()))
    }
}

/// Write a `toml::Value` document back to disk.
pub fn save_toml_document(path: &std::path::Path, doc: &toml::Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str = toml::to_string_pretty(doc).context("Failed to serialize config")?;
    std::fs::write(path, toml_str)?;
    Ok(())
}

/// Overwrite a single nested key (e.g. `["ai", "ollama", "default_model"]`)
/// inside a raw TOML document, creating intermediate tables as needed.
/// All other keys — including unknown ones — are left untouched.
pub fn set_toml_value(doc: &mut toml::Value, path: &[&str], value: toml::Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    if !doc.is_table() {
        *doc = toml::Value::Table(Default::default());
    }
    let mut current = doc.as_table_mut().expect("doc is a table");
    for key in parents {
        let needs_table = current.get(*key).map(|v| !v.is_table()).unwrap_or(true);
        if needs_table {
            current.insert(key.to_string(), toml::Value::Table(Default::default()));
        }
        current = current
            .get_mut(*key)
            .and_then(|v| v.as_table_mut())
            .expect("intermediate value is a table");
    }
    current.insert(last.to_string(), value);
}

/// Remove a single nested key (e.g. `["ai", "openai", "api_key"]`) from a raw
/// TOML document, leaving every other key — including unknown ones — untouched.
/// Returns whether the key was present.
///
/// This is what "unset" means for settings that are optional rather than
/// defaulted: an absent API key is not the same thing as an empty one, and
/// storing `api_key = ""` makes the provider look configured while every
/// request fails authentication.
pub fn remove_toml_value(doc: &mut toml::Value, path: &[&str]) -> bool {
    let Some((last, parents)) = path.split_last() else {
        return false;
    };
    let Some(doc_table) = doc.as_table_mut() else {
        return false;
    };
    if parents.is_empty() {
        return doc_table.remove(*last).is_some();
    }
    let mut current = doc_table;
    for key in parents {
        let Some(next) = current.get_mut(*key).and_then(|v| v.as_table_mut()) else {
            return false;
        };
        current = next;
    }
    match current.remove(*last) {
        Some(_) => true,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_toml_value_deletes_only_the_named_key() {
        let mut doc: toml::Value = toml::from_str(
            "[ai.openai]\napi_key = \"sk-123\"\nbase_url = \"https://api.openai.com/v1\"\n\n[daemon]\nrecord_titles = true\n",
        )
        .unwrap();

        assert!(remove_toml_value(&mut doc, &["ai", "openai", "api_key"]));
        let openai = doc.get("ai").and_then(|v| v.get("openai")).unwrap();
        assert!(
            openai.get("api_key").is_none(),
            "the key itself must be gone"
        );
        assert_eq!(
            openai.get("base_url").and_then(|v| v.as_str()),
            Some("https://api.openai.com/v1"),
            "siblings must survive"
        );
        assert_eq!(
            doc.get("daemon")
                .and_then(|v| v.get("record_titles"))
                .and_then(|v| v.as_bool()),
            Some(true),
            "unrelated sections must survive"
        );
        // Removing it twice is a no-op, not an error.
        assert!(!remove_toml_value(&mut doc, &["ai", "openai", "api_key"]));
    }

    #[test]
    fn remove_toml_value_tolerates_missing_paths() {
        let mut doc: toml::Value = toml::from_str("[ai]\ndefault_provider = \"ollama\"\n").unwrap();

        assert!(!remove_toml_value(&mut doc, &["ai", "openai", "api_key"]));
        assert_eq!(
            doc.get("ai")
                .and_then(|v| v.get("default_provider"))
                .and_then(|v| v.as_str()),
            Some("ollama")
        );
        // An intermediate value that is not a table must not be clobbered.
        assert!(!remove_toml_value(
            &mut doc,
            &["ai", "default_provider", "nested"]
        ));
        assert_eq!(
            doc.get("ai")
                .and_then(|v| v.get("default_provider"))
                .and_then(|v| v.as_str()),
            Some("ollama")
        );
        // A top-level key works too.
        assert!(remove_toml_value(&mut doc, &["ai"]));
        assert!(doc.get("ai").is_none());
    }
    #[test]
    fn set_toml_value_preserves_unknown_fields() {
        // Simulates a config file written by the daemon, which has fields the
        // server does not model (`enable_input_monitor`, `some_future_field`).
        let src = r#"
[daemon]
db_path = "~/.local/share/hyprtrace/hyprtrace.db"
enable_input_monitor = false
some_future_field = "keep-me"

[ai.ollama]
base_url = "http://localhost:11434"
default_model = "qwen2.5:7b"
"#;
        let mut doc: toml::Value = toml::from_str(src).unwrap();
        set_toml_value(
            &mut doc,
            &["ai", "ollama", "default_model"],
            toml::Value::String("llama3:8b".to_string()),
        );
        let out = toml::to_string_pretty(&doc).unwrap();

        // Unknown fields must survive the round-trip untouched.
        assert!(out.contains("enable_input_monitor = false"));
        assert!(out.contains("some_future_field = \"keep-me\""));
        // The requested key is updated.
        assert!(out.contains("llama3:8b"));

        // The document must still deserialize into the server's Config view.
        let reparsed: Config = toml::from_str(&out).unwrap();
        assert_eq!(reparsed.ai.ollama.default_model, "llama3:8b");
    }

    #[test]
    fn set_toml_value_creates_missing_sections() {
        let mut doc = toml::Value::Table(Default::default());
        set_toml_value(
            &mut doc,
            &["ai", "openai", "base_url"],
            toml::Value::String("https://example.com/v1".to_string()),
        );
        let out = toml::to_string(&doc).unwrap();
        assert!(out.contains("[ai.openai]"));
        assert!(out.contains("https://example.com/v1"));
    }

    #[test]
    fn disk_roundtrip_preserves_unknown_fields() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-cfg-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[daemon]\nenable_input_monitor = false\n\n[ai.openai]\ndefault_model = \"gpt-4o-mini\"\n",
        )
        .unwrap();

        let mut doc = load_toml_document(&path).unwrap();
        set_toml_value(
            &mut doc,
            &["ai", "openai", "default_model"],
            toml::Value::String("gpt-4o".to_string()),
        );
        save_toml_document(&path, &doc).unwrap();

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("enable_input_monitor = false"));
        assert!(on_disk.contains("gpt-4o"));
        assert!(!on_disk.contains("gpt-4o-mini"));

        let reparsed: Config = toml::from_str(&on_disk).unwrap();
        assert_eq!(reparsed.ai.openai.default_model, "gpt-4o");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_toml_document_missing_file_is_empty_table() {
        let path = std::env::temp_dir().join(format!(
            "hyprtrace-cfg-missing-{}.toml",
            uuid::Uuid::new_v4()
        ));
        let doc = load_toml_document(&path).unwrap();
        assert!(doc.as_table().map(|t| t.is_empty()).unwrap_or(false));
    }
}
