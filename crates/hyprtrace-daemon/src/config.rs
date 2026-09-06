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
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    9420
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
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
}

fn default_ai_provider() -> String {
    "ollama".to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            default_provider: default_ai_provider(),
            ollama: OllamaConfig::default(),
            openai: OpenAiConfig::default(),
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
    /// Load config from ~/.config/hyprtrace/config.toml; create with defaults if missing
    pub fn load() -> anyhow::Result<Self> {
        let config_dir = dirs_config_dir()?;
        let config_path = config_dir.join("config.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
            toml::from_str(&content).context("Failed to parse config file")
        } else {
            let config = Config::default();
            config.save_to(&config_path)?;
            Ok(config)
        }
    }

    /// Load config in read-only mode; use defaults if file doesn't exist (no auto-create)
    pub fn load_readonly() -> anyhow::Result<Self> {
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

    fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }

    /// Expand ~ in db_path to $HOME
    pub fn db_path_expanded(&self) -> PathBuf {
        expand_tilde(&self.daemon.db_path)
    }

    /// Ensure the database directory exists
    pub fn ensure_db_dir(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.db_path_expanded().parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

fn dirs_config_dir() -> anyhow::Result<PathBuf> {
    // Respect XDG_CONFIG_HOME (falling back to $HOME/.config) via the
    // `directories` crate instead of hard-coding $HOME/.config, which ignores
    // a user's XDG override.
    directories::ProjectDirs::from("", "", "hyprtrace")
        .map(|d| d.config_dir().to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("cannot determine config directory (is HOME set?)"))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        PathBuf::from(path.replacen('~', &home, 1))
    } else {
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_respects_xdg_and_falls_back_to_home() {
        // A user who points XDG_CONFIG_HOME elsewhere must have the config
        // looked up there, not under the hard-coded $HOME/.config.
        let xdg = std::env::temp_dir().join("hyprtrace-xdg-test");
        std::env::set_var("XDG_CONFIG_HOME", &xdg);
        let dir = dirs_config_dir().unwrap();
        assert_eq!(dir, xdg.join("hyprtrace"));

        // Without XDG_CONFIG_HOME, use $HOME/.config/hyprtrace.
        std::env::remove_var("XDG_CONFIG_HOME");
        let home = std::env::var("HOME").unwrap();
        let dir = dirs_config_dir().unwrap();
        assert_eq!(dir, std::path::PathBuf::from(&home).join(".config/hyprtrace"));
    }
}
