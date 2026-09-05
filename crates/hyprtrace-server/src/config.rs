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
    pub weekly_report_minute: u32
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
            weekly_report_minute: default_weekly_report_minute()
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

    pub fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(path, toml_str)?;
        Ok(())
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
