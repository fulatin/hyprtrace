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

/// AI configuration. DeepSeek Harness is the one and only AI runtime: there is
/// no in-process provider stack any more, so this section only configures how
/// the harness child process is launched and how often the proactive monitor
/// runs.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AiConfig {
    #[serde(default = "default_proactive_interval")]
    pub proactive_interval_minutes: u64,
    #[serde(default)]
    pub dsh: DshConfig,
}

fn default_proactive_interval() -> u64 {
    120
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            proactive_interval_minutes: default_proactive_interval(),
            dsh: DshConfig::default(),
        }
    }
}

/// DeepSeek Harness runtime.
///
/// HyprTrace boots `dsh --profile <profile> --patch <overlay>` as its AI
/// runtime: chat, tool calls, model routing, history, cancellation and the
/// model/tool inventory all come from the harness. The overlay/persona/tools
/// live in `integrations/dsh/` in this repository; this struct only carries
/// the deployment-specific knobs. Every field has a serde default, so configs
/// written before this section existed still load.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DshConfig {
    /// Kill switch. When `false` every AI endpoint answers with a clear
    /// "AI is disabled" error instead of starting the harness.
    #[serde(default = "default_dsh_enabled")]
    pub enabled: bool,
    /// The harness launcher. Resolved through `$PATH` like any other command.
    #[serde(default = "default_dsh_command")]
    pub command: String,
    /// Shipped DSH profile to boot. `sdk` ships with the harness and speaks
    /// the newline-delimited JSON-RPC protocol this server drives.
    #[serde(default = "default_dsh_profile")]
    pub profile: String,
    /// Model route handed to `initialize`: the provider id the harness
    /// profile knows (e.g. `opencode-go`, `deepseek-official`).
    #[serde(default = "default_dsh_provider")]
    pub provider: String,
    /// Model name for that route (e.g. `deepseek-v4.1-flash`).
    #[serde(default = "default_dsh_model")]
    pub model: String,
    /// Credential reference name the pi-ai provider resolves per request
    /// (`apiKeyEnv` in the overlay). Empty omits the `apiKeyEnv` line.
    #[serde(default = "default_dsh_api_key_env")]
    pub api_key_env: String,
    /// Optional `baseURL` override for the pi-ai provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Optional pi-ai api kind (e.g. `openai-completions`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// Working directory recorded on the DSH session. Defaults to the home
    /// directory; the assistant has no filesystem tools, so it only anchors
    /// the session header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Absolute path to `hyprtrace-mcp.mjs`. Defaults to the copy shipped
    /// next to the binary / in the repository checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_script: Option<String>,
    /// Absolute path to the overlay template. Defaults to the copy shipped
    /// next to the binary / in the repository checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay_template: Option<String>,
    /// Absolute path to `hyprtrace-bridge.mjs` (the in-harness plugin that
    /// streams deltas, cancels turns and answers inventory commands).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_script: Option<String>,
    /// Tool-name prefix the harness must expose before a turn is allowed to
    /// run (e.g. `mcp__hyprtrace__`). This is the guard against a broken MCP
    /// bridge: the harness boots fine without its MCP tools and the model
    /// would otherwise answer from guesswork. Empty disables the check.
    #[serde(default = "default_dsh_tools_prefix")]
    pub tools_prefix: String,
    /// Extra arguments appended to the launcher command line (advanced use).
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Wall-clock budget for one chat turn before the child process is
    /// discarded (the SDK has no cancel method, so a stuck turn is aborted by
    /// dropping the runtime and starting a fresh one).
    #[serde(default = "default_dsh_timeout")]
    pub request_timeout_seconds: u64,
}

fn default_dsh_enabled() -> bool {
    true
}

fn default_dsh_command() -> String {
    "dsh".to_string()
}

fn default_dsh_profile() -> String {
    "sdk".to_string()
}

fn default_dsh_provider() -> String {
    "opencode-go".to_string()
}

fn default_dsh_model() -> String {
    "deepseek-v4.1-flash".to_string()
}

fn default_dsh_api_key_env() -> String {
    "OPENCODE_GO_API_KEY".to_string()
}

fn default_dsh_timeout() -> u64 {
    300
}

fn default_dsh_tools_prefix() -> String {
    "mcp__hyprtrace__".to_string()
}

impl Default for DshConfig {
    fn default() -> Self {
        Self {
            enabled: default_dsh_enabled(),
            command: default_dsh_command(),
            profile: default_dsh_profile(),
            provider: default_dsh_provider(),
            model: default_dsh_model(),
            api_key_env: default_dsh_api_key_env(),
            base_url: None,
            api: None,
            cwd: None,
            tools_script: None,
            overlay_template: None,
            bridge_script: None,
            tools_prefix: default_dsh_tools_prefix(),
            extra_args: Vec::new(),
            request_timeout_seconds: default_dsh_timeout(),
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

/// Load the raw TOML document from disk as a `toml_edit::DocumentMut`, which
/// preserves comments, ordering and formatting so a later save does not mangle
/// the file. Unlike `Config::load`, this keeps every key present in the file,
/// including fields the server does not model (e.g. daemon-only settings such
/// as `enable_input_monitor`). Returns an empty document if the file doesn't
/// exist.
pub fn load_toml_document(path: &std::path::Path) -> anyhow::Result<toml_edit::DocumentMut> {
    if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        content
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("Failed to parse config file: {:?}", path))
    } else {
        Ok(toml_edit::DocumentMut::new())
    }
}

/// Write a `toml_edit::DocumentMut` back to disk, preserving its formatting and
/// comments. The write is atomic (a temp file in the same directory is renamed
/// into place) so a crash mid-write cannot leave a half-written config, and the
/// file is created with mode 0600 because it may contain an API key.
pub fn save_toml_document(
    path: &std::path::Path,
    doc: &toml_edit::DocumentMut,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_str = doc.to_string();

    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    tmp.write_all(toml_str.as_bytes())?;
    tmp.persist(path)?;
    Ok(())
}

/// Overwrite a single nested key (e.g. `["ai", "dsh", "model"]`)
/// inside a raw TOML document, creating intermediate tables as needed.
/// All other keys — including unknown ones — are left untouched, and comments
/// and formatting are preserved (unlike `toml::Value` re-serialization).
pub fn set_toml_value(doc: &mut toml_edit::DocumentMut, path: &[&str], value: toml_edit::Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut table = doc.as_table_mut();
    for key in parents {
        table = table
            .entry(*key)
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
            .expect("intermediate value is a table");
    }
    table.insert(*last, toml_edit::Item::Value(value));
}

/// Remove a single nested key (e.g. `["ai", "dsh", "api_key_env"]`) from a raw
/// TOML document, leaving every other key — including unknown ones — untouched.
/// Returns whether the key was present.
///
/// This is what "unset" means for settings that are optional rather than
/// defaulted: an absent API key is not the same thing as an empty one, and
/// storing `api_key = ""` makes the provider look configured while every
/// request fails authentication.
#[cfg(test)]
pub fn remove_toml_value(doc: &mut toml_edit::DocumentMut, path: &[&str]) -> bool {
    let Some((last, parents)) = path.split_last() else {
        return false;
    };
    let mut table = doc.as_table_mut();
    if parents.is_empty() {
        return table.remove(*last).is_some();
    }
    for key in parents {
        let Some(next) = table.get_mut(*key).and_then(|v| v.as_table_mut()) else {
            return false;
        };
        table = next;
    }
    table.remove(*last).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_toml_value_deletes_only_the_named_key() {
        let mut doc: toml_edit::DocumentMut =
            "[ai.openai]\napi_key = \"sk-123\"\nbase_url = \"https://api.openai.com/v1\"\n\n[daemon]\nrecord_titles = true\n"
                .parse()
                .unwrap();

        assert!(remove_toml_value(&mut doc, &["ai", "openai", "api_key"]));
        let openai = doc["ai"]["openai"].as_table().unwrap();
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
            doc["daemon"]["record_titles"].as_bool(),
            Some(true),
            "unrelated sections must survive"
        );
        // Removing it twice is a no-op, not an error.
        assert!(!remove_toml_value(&mut doc, &["ai", "openai", "api_key"]));
    }

    #[test]
    fn remove_toml_value_tolerates_missing_paths() {
        let mut doc: toml_edit::DocumentMut =
            "[ai]\ndefault_provider = \"ollama\"\n".parse().unwrap();

        assert!(!remove_toml_value(&mut doc, &["ai", "openai", "api_key"]));
        assert_eq!(
            doc["ai"]["default_provider"].as_str(),
            Some("ollama")
        );
        // An intermediate value that is not a table must not be clobbered.
        assert!(!remove_toml_value(
            &mut doc,
            &["ai", "default_provider", "nested"]
        ));
        assert_eq!(doc["ai"]["default_provider"].as_str(), Some("ollama"));
        // A top-level key works too.
        assert!(remove_toml_value(&mut doc, &["ai"]));
        assert!(doc.as_table().get("ai").is_none());
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

[ai.dsh]
model = "deepseek-v4.1-flash"
"#;
        let mut doc: toml_edit::DocumentMut = src.parse().unwrap();
        set_toml_value(
            &mut doc,
            &["ai", "dsh", "model"],
            toml_edit::Value::from("deepseek-v4-pro"),
        );
        let out = doc.to_string();

        // Unknown fields must survive the round-trip untouched.
        assert!(out.contains("enable_input_monitor = false"));
        assert!(out.contains("some_future_field = \"keep-me\""));
        // The requested key is updated.
        assert!(out.contains("deepseek-v4-pro"));

        // The document must still deserialize into the server's Config view.
        let reparsed: Config = toml::from_str(&out).unwrap();
        assert_eq!(reparsed.ai.dsh.model, "deepseek-v4-pro");
    }

    #[test]
    fn set_toml_value_creates_missing_sections() {
        let mut doc = toml_edit::DocumentMut::new();
        set_toml_value(
            &mut doc,
            &["ai", "openai", "base_url"],
            toml_edit::Value::from("https://example.com/v1"),
        );
        let out = doc.to_string();
        assert!(out.contains("[ai.openai]"));
        assert!(out.contains("https://example.com/v1"));
    }

    #[test]
    fn set_toml_value_preserves_comments() {
        // A comment attached to a key must survive an unrelated edit.
        let src = "# 采样间隔\n[daemon]\nidle_timeout_seconds = 300  # 注释\n";
        let mut doc: toml_edit::DocumentMut = src.parse().unwrap();
        set_toml_value(
            &mut doc,
            &["daemon", "record_titles"],
            toml_edit::Value::from(false),
        );
        let out = doc.to_string();
        assert!(out.contains("idle_timeout_seconds = 300  # 注释"), "comment lost: {out}");
        assert!(out.contains("record_titles = false"));
    }

    #[test]
    fn disk_roundtrip_preserves_unknown_fields() {
        let dir = std::env::temp_dir().join(format!("hyprtrace-cfg-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[daemon]\nenable_input_monitor = false\n\n[ai.dsh]\nmodel = \"deepseek-v4.1-flash\"\n",
        )
        .unwrap();

        let mut doc = load_toml_document(&path).unwrap();
        set_toml_value(
            &mut doc,
            &["ai", "dsh", "model"],
            toml_edit::Value::from("deepseek-v4-pro"),
        );
        save_toml_document(&path, &doc).unwrap();

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("enable_input_monitor = false"));
        assert!(on_disk.contains("deepseek-v4-pro"));
        assert!(!on_disk.contains("deepseek-v4.1-flash"));

        let reparsed: Config = toml::from_str(&on_disk).unwrap();
        assert_eq!(reparsed.ai.dsh.model, "deepseek-v4-pro");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_toml_document_missing_file_is_empty() {
        let path = std::env::temp_dir().join(format!(
            "hyprtrace-cfg-missing-{}.toml",
            uuid::Uuid::new_v4()
        ));
        let doc = load_toml_document(&path).unwrap();
        assert!(doc.as_table().is_empty());
    }
}
