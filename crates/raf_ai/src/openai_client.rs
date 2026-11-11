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
    #[serde(default)]
    pub reasoning_effort: String,
    /// Use a user-authorized account plan through the Responses transport.
    #[serde(default)]
    pub chatgpt_plan: bool,
    /// Registration selected when the run started, so account switches do not redirect it.
    #[serde(default)]
    pub chatgpt_registration: Option<String>,
    /// Optional sampling override; None preserves the provider/model default.
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Request Server-Sent Events and expose text deltas while the model is
    /// generating. Tool calls are still accumulated until the stream ends.
    #[serde(default = "default_streaming")]
    pub streaming: bool,
}

fn default_streaming() -> bool {
    true
}

impl OpenAiConfig {
    pub fn reasoning_transport(&self) -> String {
        if !self.chatgpt_plan { return String::new(); }
        let digest = ring::digest::digest(&ring::digest::SHA256, self.chatgpt_registration.as_deref().unwrap_or("").as_bytes());
        use base64::Engine as _;
        format!("chatgpt_plan:{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_ref()))
    }
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: "openrouter/auto".to_string(),
            api_key: String::new(),
            max_tokens: DEFAULT_MAX_TOKENS,
            reasoning_effort: String::new(),
            chatgpt_plan: false,
            chatgpt_registration: None,
            temperature: None,
            streaming: true,
        }
    }
}

/// A chat message in OpenAI format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(default)]
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_details: Vec<Value>,
}

#[derive(Debug, Clone)]
pub enum StreamDelta {
    Text(String),
    Reasoning(String),
}

const MAX_REASONING_BYTES: usize = 8 * 1024 * 1024;

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
    #[serde(default)]
    pub finish_reason: Option<String>,
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
    session_id: String,
}

impl OpenAiClient {
    pub fn new(config: OpenAiConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(600))
            .build();
        Self { config, agent, session_id: uuid::Uuid::new_v4().to_string() }
    }

    /// Keep the service session tied to the saved conversation across restarts.
    pub fn set_session_id(&mut self, id: &str) {
        if uuid::Uuid::parse_str(id).is_ok() { self.session_id = id.to_owned(); }
    }

    /// Send a chat request with optional tools.
    pub fn chat(
        &self,
        messages: &[OpenAiMessage],
        tools: Option<&[OpenAiTool]>,
    ) -> Result<OpenAiMessage, String> {
        if self.config.chatgpt_plan {
            return self.chat_stream_with_reasoning(messages, tools, |_| {});
        }
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
        if self.config.base_url.starts_with("https://opencode.ai/inference/") {
            req = req.set("User-Agent", "AuraRafi-Agent/1.0").set("x-opencode-session", &self.session_id);
        }
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
        self.chat_stream_with_reasoning(messages, tools, |delta| {
            if let StreamDelta::Text(text) = delta {
                on_text_delta(&text);
            }
        })
    }

    pub fn chat_stream_with_reasoning<F>(
        &self,
        messages: &[OpenAiMessage],
        tools: Option<&[OpenAiTool]>,
        mut on_delta: F,
    ) -> Result<OpenAiMessage, String>
    where
        F: FnMut(StreamDelta),
    {
        if self.config.chatgpt_plan {
            return crate::chatgpt_responses::chat(&self.config, messages, tools, &mut on_delta);
        }
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
        if self.config.base_url.starts_with("https://opencode.ai/inference/") {
            req = req.set("User-Agent", "AuraRafi-Agent/1.0").set("x-opencode-session", &self.session_id);
        }
        if !self.config.api_key.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", self.config.api_key));
        }

        match req.send_json(body) {
            Ok(resp) => Self::parse_stream_response(resp, &mut on_delta),
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
            "stream": stream,
        });

        if let Some(temperature) = self.config.temperature {
            if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
                return Err("Temperature override must be between 0 and 2.".into());
            }
            body["temperature"] = serde_json::json!(temperature);
        }

        if self.config.max_tokens > 0 {
            let key = if self.config.base_url.starts_with("https://api.openai.com/") {
                "max_completion_tokens"
            } else {
                "max_tokens"
            };
            body[key] = self.config.max_tokens.into();
        }
        if !self.config.reasoning_effort.is_empty() {
            if !matches!(
                self.config.reasoning_effort.as_str(),
                "none" | "low" | "medium" | "high" | "max"
            ) {
                return Err("Unsupported reasoning override.".into());
            }
            // Reasoning models may reject temperature. Never silently retry a
            // paid request with different reasoning or a different model.
            body.as_object_mut().unwrap().remove("temperature");
            if self.config.base_url.starts_with("https://openrouter.ai/") {
                body["reasoning"] = serde_json::json!({"effort": self.config.reasoning_effort});
            } else {
                body["reasoning_effort"] = self.config.reasoning_effort.clone().into();
            }
        }

        if let Some(tool_list) = tools {
            if !tool_list.is_empty() {
                body["tools"] = serde_json::to_value(tool_list).map_err(|e| e.to_string())?;
                // Some thinking endpoints reject tool_choice even when it is auto.
                if !self.is_deepseek() {
                    body["tool_choice"] = serde_json::json!("auto");
                }
            }
        }
        Ok(body)
    }

    pub fn is_deepseek(&self) -> bool {
        let base = self.config.base_url.trim_end_matches('/');
        base == "https://api.deepseek.com" || base.starts_with("https://api.deepseek.com/")
    }

    fn parse_stream_response<F>(
        response: ureq::Response,
        on_delta: &mut F,
    ) -> Result<OpenAiMessage, String>
    where
        F: FnMut(StreamDelta),
    {
        let reader = BufReader::new(response.into_reader());
        let mut content = String::new();
        let mut tool_calls = BTreeMap::<usize, StreamToolCallAccumulator>::new();
        let mut reasoning = StreamReasoning::default();
        let mut saw_choice = false;
        let mut completed = false;

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
                completed = true;
                break;
            }

            let (has_choice, terminal) =
                Self::consume_stream_payload(data, &mut content, &mut tool_calls, &mut reasoning, on_delta)?;
            saw_choice |= has_choice;
            completed |= terminal;
        }

        if !completed {
            return Err("The provider stream ended before completion. Partial output is not a completed task; inspect saved state before continuing.".into());
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
            reasoning_content: (!reasoning.text.is_empty()).then_some(reasoning.text),
            reasoning: None,
            reasoning_details: reasoning.details,
        })
    }

    fn consume_stream_payload<F>(
        data: &str,
        content: &mut String,
        tool_calls: &mut BTreeMap<usize, StreamToolCallAccumulator>,
        reasoning: &mut StreamReasoning,
        on_delta: &mut F,
    ) -> Result<(bool, bool), String>
    where
        F: FnMut(StreamDelta),
    {
        let chunk: OpenAiStreamResponse = serde_json::from_str(data)
            .map_err(|error| format!("Invalid streaming JSON response: {error}"))?;
        if let Some(error) = chunk.error {
            return Err(error.message);
        }
        let saw_choice = !chunk.choices.is_empty();
        let mut completed = false;
        for choice in chunk.choices {
            check_finish_reason(choice.finish_reason.as_deref())?;
            completed |= matches!(
                choice.finish_reason.as_deref(),
                Some("stop" | "tool_calls" | "function_call")
            );
            if let Some(delta) = choice.delta.content {
                content.push_str(&delta);
                on_delta(StreamDelta::Text(delta));
            }
            let trace = choice.delta.reasoning_content.or(choice.delta.reasoning);
            if let Some(delta) = trace.as_deref() {
                if reasoning.text.len().saturating_add(delta.len()) > MAX_REASONING_BYTES {
                    return Err("Provider reasoning exceeds the safe continuation budget.".into());
                }
                reasoning.text.push_str(delta);
                on_delta(StreamDelta::Reasoning(delta.to_owned()));
            }
            for detail in choice.delta.reasoning_details {
                let bytes = serde_json::to_vec(&detail).map_err(|e| e.to_string())?.len();
                reasoning.detail_bytes = reasoning.detail_bytes.saturating_add(bytes);
                if reasoning.detail_bytes > MAX_REASONING_BYTES {
                    return Err("Provider reasoning details exceed the safe continuation budget.".into());
                }
                if trace.is_none() {
                    if let Some(text) = detail.get("text").or_else(|| detail.get("summary")).and_then(Value::as_str) {
                        if reasoning.text.len().saturating_add(text.len()) > MAX_REASONING_BYTES {
                            return Err("Provider reasoning exceeds the safe continuation budget.".into());
                        }
                        reasoning.text.push_str(text);
                        on_delta(StreamDelta::Reasoning(text.to_owned()));
                    }
                }
                reasoning.details.push(detail);
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
        Ok((saw_choice, completed))
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
            .ok_or_else(|| "Response contained no choices".to_string())
            .and_then(|choice| {
                check_finish_reason(choice.finish_reason.as_deref())?;
                Ok(choice.message)
            })
    }
}

fn check_finish_reason(reason: Option<&str>) -> Result<(), String> {
    match reason {
        Some("length") => Err("Provider output limit reached. This response is incomplete; inspect saved state before continuing. No incomplete tool call was executed.".into()),
        Some("content_filter") => Err("The provider stopped this response through its content filter. This is not a completed task.".into()),
        _ => Ok(()),
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
    finish_reason: Option<String>,
    #[serde(default)]
    delta: OpenAiStreamDelta,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_details: Vec<Value>,
    #[serde(default)]
    tool_calls: Vec<OpenAiStreamToolCall>,
}

#[derive(Default)]
struct StreamReasoning {
    text: String,
    details: Vec<Value>,
    detail_bytes: usize,
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
    fn harness_automatic_output_omits_manual_limits_and_sampling() {
        let config = OpenAiConfig {
            max_tokens: 0,
            ..OpenAiConfig::default()
        };
        let body = OpenAiClient::new(config)
            .request_body(&[], None, false)
            .unwrap();
        for key in [
            "max_tokens",
            "max_completion_tokens",
            "reasoning",
            "reasoning_effort",
            "temperature",
        ] {
            assert!(body.get(key).is_none(), "unexpected override: {key}");
        }
    }

    #[test]
    fn harness_reasoning_and_output_use_the_endpoint_dialect() {
        let mut config = OpenAiConfig {
            max_tokens: 8192,
            reasoning_effort: "high".into(),
            ..OpenAiConfig::default()
        };
        let router = OpenAiClient::new(config.clone())
            .request_body(&[], None, true)
            .unwrap();
        assert_eq!(router["reasoning"]["effort"], "high");
        assert_eq!(router["max_tokens"], 8192);
        config.base_url = "https://api.openai.com/v1".into();
        let openai = OpenAiClient::new(config.clone())
            .request_body(&[], None, false)
            .unwrap();
        assert_eq!(openai["reasoning_effort"], "high");
        assert_eq!(openai["max_completion_tokens"], 8192);
        assert!(openai.get("max_tokens").is_none());
        config.reasoning_effort = "invented".into();
        assert!(OpenAiClient::new(config)
            .request_body(&[], None, false)
            .is_err());
    }

    #[test]
    fn harness_truncated_responses_cannot_become_completed_tool_calls() {
        for reason in ["length", "content_filter"] {
            let response = serde_json::json!({"choices":[{"finish_reason":reason,"message":{"role":"assistant","content":"partial"}}]});
            assert!(OpenAiClient::parse_response(&response.to_string()).is_err());
            let delta = serde_json::json!({"choices":[{"finish_reason":reason,"delta":{}}]});
            assert!(OpenAiClient::consume_stream_payload(
                &delta.to_string(),
                &mut String::new(),
                &mut BTreeMap::new(),
                &mut StreamReasoning::default(),
                &mut |_| {}
            )
            .is_err());
        }
    }

    #[test]
    fn streaming_payloads_accumulate_text_and_tool_arguments() {
        let payloads = [
            r#"{"choices":[{"delta":{"content":"Voy a crear"}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"game_add","arguments":"{\"name\":\"Cube\""}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"}"}}]}}]}"#,
        ];
        let mut content = String::new();
        let mut tool_calls = BTreeMap::new();
        let mut reasoning = StreamReasoning::default();
        let mut deltas = Vec::new();
        for payload in payloads {
            assert!(
                OpenAiClient::consume_stream_payload(
                    payload,
                    &mut content,
                    &mut tool_calls,
                    &mut reasoning,
                    &mut |delta| if let StreamDelta::Text(text) = delta { deltas.push(text) },
                )
                .unwrap()
                .0
            );
        }

        assert_eq!(content, "Voy a crear");
        assert_eq!(deltas, vec!["Voy a crear"]);
        let call = tool_calls.get(&0).expect("streamed tool call");
        assert_eq!(call.id.as_deref(), Some("call_1"));
        assert_eq!(call.name, "game_add");
        assert_eq!(call.arguments, r#"{"name":"Cube"}"#);
    }

    #[test]
    fn reasoning_stream_preserves_provider_details_and_visible_text() {
        let mut reasoning = StreamReasoning::default();
        let mut visible = String::new();
        let payload = r#"{"choices":[{"delta":{"reasoning":"Consider ","reasoning_details":[{"type":"reasoning.encrypted","data":"opaque"}]}},{"delta":{"reasoning":"carefully"},"finish_reason":"stop"}]}"#;
        OpenAiClient::consume_stream_payload(payload, &mut String::new(), &mut BTreeMap::new(), &mut reasoning,
            &mut |delta| if let StreamDelta::Reasoning(text) = delta { visible.push_str(&text) }).unwrap();
        assert_eq!(visible, "Consider carefully");
        assert_eq!(reasoning.text, visible);
        assert_eq!(reasoning.details[0]["data"], "opaque");
    }

    #[test]
    fn reasoning_origin_separates_chatgpt_accounts_from_api_keys() {
        let mut config = OpenAiConfig { chatgpt_plan: true, chatgpt_registration: Some("first".into()), ..OpenAiConfig::default() };
        let first = config.reasoning_transport();
        config.chatgpt_registration = Some("second".into());
        assert_ne!(first, config.reasoning_transport());
        config.chatgpt_plan = false;
        assert!(config.reasoning_transport().is_empty());
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
                    reasoning_content: None,
                    reasoning: None,
                    reasoning_details: Vec::new(),
                }],
                None,
                false,
            )
            .expect("request body");

        assert_eq!(body["max_tokens"], serde_json::json!(8_192));
    }
}
