mod ollama;
mod openai;
pub mod tools;

use crate::config::AiConfig;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use tools::ToolDef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// Assistant message that requested tool calls (provider-specific wire format).
    pub fn assistant_tool_calls(content: String, tool_calls: serde_json::Value) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            name: None,
        }
    }

    /// Tool result message (OpenAI: tool_call_id; Ollama: name).
    pub fn tool_result(provider: &str, call: &ToolCall, content: String) -> Self {
        Self {
            role: "tool".to_string(),
            content,
            tool_calls: None,
            tool_call_id: (provider == "openai").then(|| call.id.clone()),
            name: (provider != "openai").then(|| call.name.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug)]
pub enum StreamEvent {
    Text(String),
    ToolCall(ToolCall),
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn chat(&self, model: Option<&str>, messages: &[ChatMessage]) -> anyhow::Result<String>;
    async fn chat_stream(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()>;
    /// Streaming chat with optional tool support. Emits text chunks and
    /// completed tool calls as they are parsed from the provider stream.
    async fn chat_stream_events(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: Option<&[ToolDef]>,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()>;
    async fn list_models(&self) -> anyhow::Result<Vec<String>>;
    #[allow(dead_code)]
    fn name(&self) -> &str;
}

pub struct AiManager {
    providers: HashMap<String, Box<dyn AiProvider>>,
    pub default_provider: String,
    pub system_prompt: String,
    pub openai_configured: bool,
}

impl AiManager {
    pub fn from_config(config: &AiConfig) -> Self {
        let mut providers: HashMap<String, Box<dyn AiProvider>> = HashMap::new();

        providers.insert(
            "ollama".to_string(),
            Box::new(OllamaProvider::new(
                config.ollama.base_url.clone(),
                config.ollama.default_model.clone(),
            )),
        );

        let api_key = if config.openai.api_key.is_empty() {
            std::env::var("OPENAI_API_KEY").unwrap_or_default()
        } else {
            config.openai.api_key.clone()
        };

        let openai_configured = !api_key.is_empty();

        providers.insert(
            "openai".to_string(),
            Box::new(OpenAiProvider::new(
                api_key,
                config.openai.base_url.clone(),
                config.openai.default_model.clone(),
            )),
        );

        Self {
            providers,
            default_provider: config.default_provider.clone(),
            system_prompt: "You are a HyprTrace window usage analysis assistant. You have tools to query the LIVE Hyprland window manager state (active window, workspaces, monitors, devices, keybinds, version, etc.) and the user's historical usage data (daily summaries, app rankings, sessions, hourly breakdown, app trends). Use tools whenever the user asks about current system state or when you need concrete data — never guess. Analyze the data, provide efficiency suggestions, and identify potential time waste. Respond in the user's language.".to_string(),
            openai_configured,
        }
    }

    pub async fn chat(
        &self,
        provider_name: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
    ) -> anyhow::Result<String> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown AI provider: {}", provider_name))?;
        provider.chat(model, messages).await
    }

    pub async fn chat_stream(
        &self,
        provider_name: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown AI provider: {}", provider_name))?;
        provider.chat_stream(model, messages, tx).await
    }

    pub async fn chat_stream_events(
        &self,
        provider_name: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: Option<&[ToolDef]>,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown AI provider: {}", provider_name))?;
        provider
            .chat_stream_events(model, messages, tools, tx)
            .await
    }

    pub async fn list_all_models(&self) -> HashMap<String, Vec<String>> {
        let mut result = HashMap::new();
        for (name, provider) in &self.providers {
            // Bound each provider's model listing. An unreachable or slow
            // endpoint (e.g. a misconfigured base_url) must not hold the shared
            // AI lock for the full HTTP timeout (up to 300s), which would stall
            // every concurrent chat/agent request. A short timeout degrades to
            // an empty list rather than blocking the whole AI subsystem.
            let models = match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                provider.list_models(),
            )
            .await
            {
                Ok(Ok(models)) => models,
                _ => Vec::new(),
            };
            result.insert(name.clone(), models);
        }
        result
    }

    #[allow(dead_code)]
    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    #[allow(dead_code)]
    pub fn is_openai_configured(&self) -> bool {
        self.openai_configured
    }
}
