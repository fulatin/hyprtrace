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
}

fn default_db_path() -> String {
    "~/.local/share/hyprtrace/hyprtrace.db".to_string()
}

fn default_idle_timeout() -> u64 {
    300
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            idle_timeout_seconds: default_idle_timeout(),
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