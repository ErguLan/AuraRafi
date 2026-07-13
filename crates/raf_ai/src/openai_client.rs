//! Generic OpenAI-compatible chat client.
//!
//! Works with any provider that exposes `/chat/completions`:
//! OpenRouter, OpenAI, local LLMs, etc. Blocking, no async.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: "openrouter/auto".to_string(),
            api_key: String::new(),
            max_tokens: 4096,
            temperature: 0.2,
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
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature,
        });

        if let Some(tool_list) = tools {
            if !tool_list.is_empty() {
                body["tools"] = serde_json::to_value(tool_list).map_err(|e| e.to_string())?;
                body["tool_choice"] = serde_json::json!("auto");
            }
        }

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
