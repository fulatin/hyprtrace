use super::{AiProvider, ChatMessage, StreamEvent, ToolCall, ToolDef};
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
            .timeout(std::time::Duration::from_secs(300))
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

    async fn chat(&self, model: Option<&str>, messages: &[ChatMessage]) -> anyhow::Result<String> {
        if self.api_key.is_empty() {
            anyhow::bail!("OpenAI API key not configured");
        }

        let url = format!("{}/chat/completions", self.base_url);
        let body = serde_json::json!({
            "model": model.unwrap_or(&self.default_model),
            "messages": messages,
        });

        // Retry transient 429 / 5xx responses with exponential backoff; a
        // rate-limited or overloaded endpoint should not surface as a hard
        // failure on the first attempt.
        let mut attempt = 0u32;
        let resp = loop {
            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&body)
                .send()
                .await?;

            let status = resp.status();
            if !status.is_success() {
                let retryable = status.is_server_error() || status == 429;
                let text = resp.text().await.unwrap_or_default();
                if retryable && attempt < 3 {
                    attempt += 1;
                    let delay = std::time::Duration::from_millis(500 * (1 << attempt));
                    log::warn!(
                        "OpenAI API {}; retrying in {}ms (attempt {}/3)",
                        status,
                        delay.as_millis(),
                        attempt
                    );
                    tokio::time::sleep(delay).await;
                    continue;
                }
                anyhow::bail!("OpenAI API error {}: {}", status, text);
            }
            break resp;
        };

        let json: serde_json::Value = resp.json().await?;
        Ok(json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("(empty response)")
            .to_string())
    }

    async fn chat_stream(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> anyhow::Result<()> {
        let (ev_tx, mut ev_rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);
        let fwd = tokio::spawn(async move {
            while let Some(ev) = ev_rx.recv().await {
                if let StreamEvent::Text(t) = ev {
                    if tx.send(t).await.is_err() {
                        break;
                    }
                }
            }
        });
        self.chat_stream_events(model, messages, None, ev_tx)
            .await?;
        let _ = fwd.await;
        Ok(())
    }

    async fn chat_stream_events(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: Option<&[ToolDef]>,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        if self.api_key.is_empty() {
            anyhow::bail!("OpenAI API key not configured");
        }

        let url = format!("{}/chat/completions", self.base_url);
        let mut body = serde_json::json!({
            "model": model.unwrap_or(&self.default_model),
            "messages": messages,
            "stream": true,
        });
        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] =
                    serde_json::Value::Array(tools.iter().map(|t| t.to_api_json()).collect());
                body["tool_choice"] = serde_json::json!("auto");
            }
        }

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
        // index -> (id, name, arguments buffer)
        let mut pending: std::collections::BTreeMap<usize, (String, String, String)> =
            std::collections::BTreeMap::new();

        let mut result: anyhow::Result<()> = Ok(());
        let mut finish_reason = String::from("stop");

        'outer: while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    result = Err(e.into());
                    break;
                }
            };
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
                    let _ = tx.send(StreamEvent::Done(finish_reason.clone())).await;
                    break 'outer;
                }

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                    let delta = &json["choices"][0]["delta"];

                    // The finish_reason is reported on the last chunk before
                    // [DONE]; capture it so run_agent can detect truncation.
                    if let Some(r) = json["choices"][0]["finish_reason"].as_str() {
                        if !r.is_empty() {
                            finish_reason = r.to_string();
                        }
                    }

                    // Accumulate streamed tool_call fragments by index.
                    if let Some(calls) = delta["tool_calls"].as_array() {
                        for call in calls {
                            let idx = call["index"].as_u64().unwrap_or(0) as usize;
                            let entry = pending
                                .entry(idx)
                                .or_insert_with(|| (String::new(), String::new(), String::new()));
                            if let Some(id) = call["id"].as_str() {
                                if !id.is_empty() {
                                    entry.0 = id.to_string();
                                }
                            }
                            if let Some(name) = call["function"]["name"].as_str() {
                                if !name.is_empty() {
                                    entry.1 = name.to_string();
                                }
                            }
                            if let Some(args) = call["function"]["arguments"].as_str() {
                                entry.2.push_str(args);
                            }
                        }
                    }

                    if let Some(content) = delta["content"].as_str() {
                        if !content.is_empty() {
                            if tx
                                .send(StreamEvent::Text(content.to_string()))
                                .await
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        // Flush accumulated tool calls.
        for (idx, (id, name, args_buf)) in pending.into_iter() {
            if name.is_empty() {
                continue;
            }
            let arguments = if args_buf.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(&args_buf).unwrap_or_else(|_| {
                    log::warn!("Tool call {}: unparseable arguments: {}", name, args_buf);
                    serde_json::json!({})
                })
            };
            let _ = tx
                .send(StreamEvent::ToolCall(ToolCall {
                    id: if id.is_empty() {
                        format!("call_{}", idx)
                    } else {
                        id
                    },
                    name,
                    arguments,
                }))
                .await;
        }

        // If the stream ended without the [DONE] sentinel, still emit Done so
        // run_agent gets a finish_reason (e.g. "length") for truncation checks.
        let _ = tx.send(StreamEvent::Done(finish_reason)).await;

        result
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
