//! Generic OpenAI-compatible chat client.
//!
//! Works with any provider that exposes `/chat/completions`:
//! OpenRouter, OpenAI, local LLMs, etc. Blocking, no async.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Default maximum number of tokens requested for one Agent response.
/// Providers may impose a lower effective limit.
pub const DEFAULT_MAX_TOKENS: u32 = raf_core::config::AGENT_MAX_RESPONSE_TOKENS_DEFAULT;

/// Configuration for an OpenAI-compatible endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// Base URL without the `/chat/completions` suffix.
    pub base_url: String,
    /// Model id sent in the request body.
    pub model: String,
    /// API key. Empty for local/no-auth endpoints.
    pub api_key: String,
    /// Maximum tokens per response.
    pub max_tokens: u32,
    /// Temperature.
    pub temperature: f32,
    /// Request Server-Sent Events and expose text deltas while the model is
    /// generating. Tool calls are still accumulated until the stream ends.
    #[serde(default = "default_streaming")]
    pub streaming: bool,
}

fn default_streaming() -> bool {
    true
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: "openrouter/auto".to_string(),
            api_key: String::new(),
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: 0.2,
            streaming: true,
        }
    }
}

/// A chat message in OpenAI format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// Function payload inside a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Tool definition in OpenAI format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAiFunction,
}

/// Function schema for a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

/// Response body from `/chat/completions`.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiResponse {
    #[serde(default)]
    pub choices: Vec<OpenAiChoice>,
    #[serde(default)]
    pub error: Option<OpenAiError>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChoice {
    pub message: OpenAiMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiError {
    pub message: String,
}

/// Lightweight OpenAI-compatible client.
#[derive(Clone)]
pub struct OpenAiClient {
    pub config: OpenAiConfig,
    agent: ureq::Agent,
}

impl OpenAiClient {
    pub fn new(config: OpenAiConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .build();
        Self { config, agent }
    }

    /// Send a chat request with optional tools.
    pub fn chat(
        &self,
        messages: &[OpenAiMessage],
        tools: Option<&[OpenAiTool]>,
    ) -> Result<OpenAiMessage, String> {
        if self.config.streaming {
            return self.chat_stream(messages, tools, |_| {});
        }

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let body = self.request_body(messages, tools, false)?;

        let mut req = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json");
        if !self.config.api_key.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", self.config.api_key));
        }

        match req.send_json(body) {
            Ok(resp) => match resp.into_string() {
                Ok(text) => Self::parse_response(&text),
                Err(e) => Err(format!("Failed to read response body: {}", e)),
            },
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                Err(format!("HTTP {}: {}", code, detail))
            }
            Err(e) => Err(format!("Request failed: {}", e)),
        }
    }

    /// Send a streaming chat request and invoke `on_text_delta` as text
    /// arrives. Tool-call fragments are reconstructed into one normal
    /// `OpenAiMessage` before this method returns, so the runtime can keep its
    /// existing approval and execution flow unchanged.
    pub fn chat_stream<F>(
        &self,
        messages: &[OpenAiMessage],
        tools: Option<&[OpenAiTool]>,
        mut on_text_delta: F,
    ) -> Result<OpenAiMessage, String>
    where
        F: FnMut(&str),
    {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = self.request_body(messages, tools, true)?;
        let mut req = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("Accept", "text/event-stream");
        if !self.config.api_key.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", self.config.api_key));
        }

        match req.send_json(body) {
            Ok(resp) => Self::parse_stream_response(resp, &mut on_text_delta),
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                Err(format!("HTTP {}: {}", code, detail))
            }
            Err(e) => Err(format!("Request failed: {}", e)),
        }
    }

    fn request_body(
        &self,
        messages: &[OpenAiMessage],
        tools: Option<&[OpenAiTool]>,
        stream: bool,
    ) -> Result<Value, String> {
        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature,
            "stream": stream,
        });

        if let Some(tool_list) = tools {
            if !tool_list.is_empty() {
                body["tools"] = serde_json::to_value(tool_list).map_err(|e| e.to_string())?;
                body["tool_choice"] = serde_json::json!("auto");
            }
        }
        Ok(body)
    }

    fn parse_stream_response<F>(
        response: ureq::Response,
        on_text_delta: &mut F,
    ) -> Result<OpenAiMessage, String>
    where
        F: FnMut(&str),
    {
        let reader = BufReader::new(response.into_reader());
        let mut content = String::new();
        let mut tool_calls = BTreeMap::<usize, StreamToolCallAccumulator>::new();
        let mut saw_choice = false;

        for line in reader.lines() {
            let line =
                line.map_err(|error| format!("Failed to read streaming response: {error}"))?;
            let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                continue;
            };
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                break;
            }

            saw_choice |=
                Self::consume_stream_payload(data, &mut content, &mut tool_calls, on_text_delta)?;
        }

        if !saw_choice {
            return Err("Streaming response contained no choices".to_string());
        }

        let tool_calls = tool_calls
            .into_iter()
            .map(|(index, call)| ToolCall {
                id: call.id.unwrap_or_else(|| format!("stream-call-{index}")),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: call.name,
                    arguments: call.arguments,
                },
            })
            .collect::<Vec<_>>();

        Ok(OpenAiMessage {
            role: "assistant".to_string(),
            content: serde_json::json!(content),
            tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            tool_call_id: None,
        })
    }

    fn consume_stream_payload<F>(
        data: &str,
        content: &mut String,
        tool_calls: &mut BTreeMap<usize, StreamToolCallAccumulator>,
        on_text_delta: &mut F,
    ) -> Result<bool, String>
    where
        F: FnMut(&str),
    {
        let chunk: OpenAiStreamResponse = serde_json::from_str(data)
            .map_err(|error| format!("Invalid streaming JSON response: {error}"))?;
        if let Some(error) = chunk.error {
            return Err(error.message);
        }
        let saw_choice = !chunk.choices.is_empty();
        for choice in chunk.choices {
            if let Some(delta) = choice.delta.content {
                content.push_str(&delta);
                on_text_delta(&delta);
            }
            for call in choice.delta.tool_calls {
                let entry = tool_calls.entry(call.index).or_default();
                if let Some(id) = call.id {
                    entry.id = Some(id);
                }
                if let Some(function) = call.function {
                    if let Some(name) = function.name {
                        entry.name.push_str(&name);
                    }
                    if let Some(arguments) = function.arguments {
                        entry.arguments.push_str(&arguments);
                    }
                }
            }
        }
        Ok(saw_choice)
    }

    fn parse_response(text: &str) -> Result<OpenAiMessage, String> {
        let parsed: OpenAiResponse =
            serde_json::from_str(text).map_err(|e| format!("Invalid JSON response: {}", e))?;

        if let Some(error) = parsed.error {
            return Err(error.message);
        }

        parsed
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .ok_or_else(|| "Response contained no choices".to_string())
    }
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiStreamResponse {
    #[serde(default)]
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    error: Option<OpenAiError>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiStreamChoice {
    #[serde(default)]
    delta: OpenAiStreamDelta,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiStreamToolCall>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiStreamToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiStreamFunction>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct StreamToolCallAccumulator {
    id: Option<String>,
    name: String,
    arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_payloads_accumulate_text_and_tool_arguments() {
        let payloads = [
            r#"{"choices":[{"delta":{"content":"Voy a crear"}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"game_add","arguments":"{\"name\":\"Cube\""}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"}"}}]}}]}"#,
        ];
        let mut content = String::new();
        let mut tool_calls = BTreeMap::new();
        let mut deltas = Vec::new();
        for payload in payloads {
            assert!(OpenAiClient::consume_stream_payload(
                payload,
                &mut content,
                &mut tool_calls,
                &mut |delta| deltas.push(delta.to_string()),
            )
            .unwrap());
        }

        assert_eq!(content, "Voy a crear");
        assert_eq!(deltas, vec!["Voy a crear"]);
        let call = tool_calls.get(&0).expect("streamed tool call");
        assert_eq!(call.id.as_deref(), Some("call_1"));
        assert_eq!(call.name, "game_add");
        assert_eq!(call.arguments, r#"{"name":"Cube"}"#);
    }

    #[test]
    fn request_body_uses_the_configured_response_limit() {
        let mut config = OpenAiConfig::default();
        config.max_tokens = 8_192;
        let client = OpenAiClient::new(config);

        let body = client
            .request_body(
                &[OpenAiMessage {
                    role: "user".to_string(),
                    content: serde_json::json!("hello"),
                    tool_calls: None,
                    tool_call_id: None,
                }],
                None,
                false,
            )
            .expect("request body");

        assert_eq!(body["max_tokens"], serde_json::json!(8_192));
    }
}
