mod ollama;
mod openai;

use crate::config::AiConfig;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<String>;
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tx: tokio::sync::mpsc::Sender<String>,
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
            system_prompt: "You are a HyprTrace window usage analysis assistant. Analyze the user's application usage data, provide efficiency suggestions, and identify potential time waste. Respond in the user's language.".to_string(),
            openai_configured,
        }
    }

    pub async fn chat(&self, provider_name: &str, messages: &[ChatMessage]) -> anyhow::Result<String> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown AI provider: {}", provider_name))?;
        provider.chat(messages).await
    }

    pub async fn chat_stream(
        &self,
        provider_name: &str,
        messages: &[ChatMessage],
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown AI provider: {}", provider_name))?;
        provider.chat_stream(messages, tx).await
    }

    pub async fn list_all_models(&self) -> HashMap<String, Vec<String>> {
        let mut result = HashMap::new();
        for (name, provider) in &self.providers {
            let models = provider.list_models().await.unwrap_or_default();
            result.insert(name.clone(), models);
        }
        result
    }

    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    pub fn is_openai_configured(&self) -> bool {
        self.openai_configured
    }
}
