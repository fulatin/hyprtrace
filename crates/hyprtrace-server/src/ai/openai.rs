use super::{AiProvider, ChatMessage};
use async_trait::async_trait;

pub struct OpenAiProvider {
    api_key: String,
    pub base_url: String,
    pub default_model: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: String, base_url: String, default_model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_default();
        Self {
            api_key,
            base_url,
            default_model,
            client,
        }
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat(&self, messages: &[ChatMessage]) -> anyhow::Result<String> {
        if self.api_key.is_empty() {
            anyhow::bail!("OpenAI API key not configured");
        }

        let url = format!("{}/chat/completions", self.base_url);
        let body = serde_json::json!({
            "model": self.default_model,
            "messages": messages,
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, text);
        }

        let json: serde_json::Value = resp.json().await?;
        Ok(json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("(empty response)")
            .to_string())
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        if self.api_key.is_empty() {
            anyhow::bail!("OpenAI API key not configured");
        }

        let url = format!("{}/chat/completions", self.base_url);
        let body = serde_json::json!({
            "model": self.default_model,
            "messages": messages,
            "stream": true,
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, text);
        }

        let mut stream = resp.bytes_stream();
        use futures::StreamExt;
        let mut buf: Vec<u8> = Vec::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buf.extend_from_slice(&chunk);

            while let Some(line_end) = buf.iter().position(|&b| b == b'\n') {
                let line_bytes = buf[..line_end].to_vec();
                buf = buf[line_end + 1..].to_vec();
                let line = std::str::from_utf8(&line_bytes)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }

                let data = line[6..].trim().to_string();
                if data == "[DONE]" {
                    return Ok(());
                }

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                    if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                        if !content.is_empty() {
                            if tx.send(content.to_string()).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        if self.api_key.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/models", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let json: serde_json::Value = resp.json().await?;
        let models = json["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["id"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(models)
    }
}
