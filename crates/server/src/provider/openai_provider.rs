use async_trait::async_trait;
use composer_protocol::ModelInfo;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::config_sections::ProviderConfig;
use crate::provider::{
    ModelEvent, ModelMessage, ModelRequest, ModelStream, PartialToolCalls, Provider, SseDecoder,
    ToolSpec, Usage,
};

/// An OpenAI-compatible `/chat/completions` endpoint (llama.cpp, vLLM, ...).
///
/// Reasoning is read from `delta.reasoning_content` (or `delta.reasoning`).
#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    /// Uses the configured model, or the first one the server lists.
    pub async fn connect(config: &ProviderConfig) -> Result<Self, String> {
        let mut provider = Self {
            client: reqwest::Client::new(),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
        };
        if provider.model.is_empty() {
            provider.model = provider.first_model().await?;
        }
        Ok(provider)
    }

    async fn first_model(&self) -> Result<String, String> {
        self.list_models()
            .await?
            .into_iter()
            .next()
            .map(|m| m.id)
            .ok_or_else(|| format!("no models listed at {}/models", self.base_url))
    }

    /// `GET /models`: ids, and OpenRouter's names and context lengths.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        let response = self
            .authorized(self.client.get(format!("{}/models", self.base_url)))
            .send()
            .await
            .map_err(|e| format!("listing models: {e}"))?;
        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("listing models: {e}"))?;
        Ok(models_of(&body))
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.api_key.is_empty() {
            request
        } else {
            request.bearer_auth(&self.api_key)
        }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn model(&self) -> &str {
        &self.model
    }

    async fn models(&self) -> Result<Vec<ModelInfo>, String> {
        self.list_models().await
    }

    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, String> {
        let body = request_body(&self.model, &request);
        let response = self
            .authorized(
                self.client
                    .post(format!("{}/chat/completions", self.base_url)),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "{status}: {}",
                text.chars().take(500).collect::<String>()
            ));
        }

        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            let mut bytes = response.bytes_stream();
            let mut sse = SseDecoder::new();
            let mut calls = PartialToolCalls::default();
            let mut usage = None;
            let mut finish_reason = None;
            'read: while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(e) => {
                        let _ = tx.send(Err(format!("stream interrupted: {e}"))).await;
                        return;
                    }
                };
                for data in sse.push(&chunk) {
                    if data == "[DONE]" {
                        break 'read;
                    }
                    let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
                        continue;
                    };
                    if let Some(u) = parse_usage(&chunk) {
                        usage = Some(u);
                    }
                    let choice = &chunk["choices"][0];
                    let delta = &choice["delta"];
                    let thinking = delta["reasoning_content"]
                        .as_str()
                        .or_else(|| delta["reasoning"].as_str());
                    for event in [
                        thinking.map(|t| ModelEvent::Thinking(t.to_string())),
                        delta["content"]
                            .as_str()
                            .map(|c| ModelEvent::Content(c.to_string())),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let empty = matches!(&event, ModelEvent::Thinking(t) | ModelEvent::Content(t) if t.is_empty());
                        if !empty && tx.send(Ok(event)).await.is_err() {
                            return; // receiver dropped: cancelled
                        }
                    }
                    if let Some(deltas) = delta["tool_calls"].as_array() {
                        calls.apply(deltas);
                    }
                    if let Some(reason) = choice["finish_reason"].as_str() {
                        finish_reason = Some(reason.to_string());
                    }
                }
            }
            let _ = tx
                .send(Ok(ModelEvent::Finished {
                    tool_calls: calls.finish(),
                    usage,
                    finish_reason,
                }))
                .await;
        });
        Ok(rx)
    }
}

fn models_of(body: &Value) -> Vec<ModelInfo> {
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(ModelInfo {
                id: m["id"].as_str()?.to_string(),
                name: m["name"].as_str().map(str::to_string),
                context: m["context_length"].as_u64().map(|c| c as u32),
            })
        })
        .collect()
}

fn request_body(model: &str, request: &ModelRequest) -> Value {
    let mut body = json!({
        "model": request.model.as_deref().unwrap_or(model),
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": request.messages.iter().map(message_json).collect::<Vec<_>>(),
    });
    if !request.tools.is_empty() {
        body["tools"] = request.tools.iter().map(tool_json).collect();
    }
    if let Some(effort) = &request.reasoning_effort {
        body["reasoning_effort"] = json!(effort);
    }
    body
}

fn message_json(message: &ModelMessage) -> Value {
    match message {
        ModelMessage::System { content } => json!({ "role": "system", "content": content }),
        ModelMessage::User { content, images } if images.is_empty() => {
            json!({ "role": "user", "content": content })
        }
        ModelMessage::User { content, images } => {
            let mut parts = vec![json!({ "type": "text", "text": content })];
            parts.extend(
                images
                    .iter()
                    .map(|url| json!({ "type": "image_url", "image_url": { "url": url } })),
            );
            json!({ "role": "user", "content": parts })
        }
        ModelMessage::Assistant {
            content,
            thinking,
            tool_calls,
        } => {
            let mut m = json!({ "role": "assistant", "content": content });
            if let Some(thinking) = thinking {
                m["reasoning_content"] = json!(thinking);
            }
            if !tool_calls.is_empty() {
                m["tool_calls"] = tool_calls
                    .iter()
                    .map(|c| json!({ "id": c.id, "type": "function", "function": { "name": c.name, "arguments": c.arguments } }))
                    .collect();
            }
            m
        }
        ModelMessage::Tool {
            call_id,
            content,
            images,
        } if images.is_empty() => {
            json!({ "role": "tool", "tool_call_id": call_id, "content": content })
        }
        // A screenshot from an MCP tool: image parts in the tool message (`tool_images`).
        ModelMessage::Tool {
            call_id,
            content,
            images,
        } => {
            let mut parts = vec![json!({ "type": "text", "text": content })];
            parts.extend(
                images
                    .iter()
                    .map(|url| json!({ "type": "image_url", "image_url": { "url": url } })),
            );
            json!({ "role": "tool", "tool_call_id": call_id, "content": parts })
        }
    }
}

fn tool_json(tool: &ToolSpec) -> Value {
    json!({ "type": "function", "function": { "name": tool.name, "description": tool.description, "parameters": tool.parameters } })
}

fn parse_usage(chunk: &Value) -> Option<Usage> {
    let usage = chunk.get("usage")?;
    Some(Usage {
        prompt_tokens: usage.get("prompt_tokens")?.as_u64()? as u32,
        completion_tokens: usage
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_from_llama_server_and_openrouter() {
        let llama =
            json!({ "object": "list", "data": [{ "id": "qwen3-coder", "object": "model" }] });
        assert_eq!(
            models_of(&llama),
            [ModelInfo {
                id: "qwen3-coder".into(),
                name: None,
                context: None
            }]
        );
        let openrouter = json!({ "data": [{
            "id": "anthropic/claude-sonnet-4.5",
            "name": "Anthropic: Claude Sonnet 4.5",
            "context_length": 1000000,
            "pricing": { "prompt": "0.000003" }
        }, { "name": "no id" }] });
        assert_eq!(
            models_of(&openrouter),
            [ModelInfo {
                id: "anthropic/claude-sonnet-4.5".into(),
                name: Some("Anthropic: Claude Sonnet 4.5".into()),
                context: Some(1_000_000),
            }]
        );
    }

    #[test]
    fn reasoning_effort_is_sent_when_set() {
        let request = ModelRequest {
            messages: vec![],
            tools: vec![],
            model: None,
            reasoning_effort: Some("medium".into()),
        };
        assert_eq!(request_body("m", &request)["reasoning_effort"], "medium");
    }

    #[test]
    fn a_tool_result_with_images_has_image_parts() {
        let plain = ModelMessage::Tool {
            call_id: "c".into(),
            content: "ok".into(),
            images: vec![],
        };
        assert_eq!(message_json(&plain)["content"], "ok");
        let shot = ModelMessage::Tool {
            call_id: "c".into(),
            content: "the screen".into(),
            images: vec!["data:image/png;base64,AAAA".into()],
        };
        assert_eq!(
            message_json(&shot),
            json!({ "role": "tool", "tool_call_id": "c", "content": [
                { "type": "text", "text": "the screen" },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,AAAA" } },
            ] })
        );
    }

    #[test]
    fn a_request_goes_to_its_own_model() {
        let request = |model: Option<&str>| ModelRequest {
            messages: vec![],
            tools: vec![],
            model: model.map(str::to_string),
            reasoning_effort: None,
        };
        assert_eq!(request_body("default", &request(None))["model"], "default");
        assert!(
            request_body("default", &request(None))
                .get("reasoning_effort")
                .is_none()
        );
        assert_eq!(
            request_body("default", &request(Some("other")))["model"],
            "other"
        );
    }
}
