//! OpenClaw client - connects to the local OpenClaw gateway.
//!
//! OpenClaw runs on localhost:18789 by default. This client sends messages
//! via HTTP POST and receives responses. Lightweight, blocking, no async.

use serde::{Deserialize, Serialize};

/// Connection status to OpenClaw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Not connected, never tried.
    Disconnected,
    /// Successfully connected and got a response.
    Connected,
    /// Tried to connect but failed.
    Error,
    /// Currently sending a message.
    Sending,
}

/// OpenClaw gateway client configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawConfig {
    /// Gateway URL (default: http://localhost:18789).
    pub gateway_url: String,
    /// Auth token (optional, from openclaw dashboard).
    pub token: String,
    /// Whether to auto-connect on startup.
    pub auto_connect: bool,
}

impl Default for OpenClawConfig {
    fn default() -> Self {
        Self {
            gateway_url: "http://localhost:18789".to_string(),
            token: String::new(),
            auto_connect: false,
        }
    }
}

/// Response from OpenClaw gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawResponse {
    /// The assistant's text response.
    #[serde(default)]
    pub output: Vec<OpenClawOutput>,
    /// Error message if any.
    #[serde(default)]
    pub error: Option<String>,
}

/// A single output block from OpenClaw.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawOutput {
    #[serde(default, rename = "type")]
    pub output_type: String,
    #[serde(default)]
    pub content: Vec<OpenClawContent>,
}

/// Content block in an output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawContent {
    #[serde(default, rename = "type")]
    pub content_type: String,
    #[serde(default)]
    pub text: String,
}

/// The OpenClaw client.
pub struct OpenClawClient {
    pub config: OpenClawConfig,
    pub status: ConnectionStatus,
    /// Last error message for display.
    pub last_error: String,
    /// Reusable HTTP agent with timeout.
    agent: ureq::Agent,
}

impl Default for OpenClawClient {
    fn default() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(15))
            .build();
        Self {
            config: OpenClawConfig::default(),
            status: ConnectionStatus::Disconnected,
            last_error: String::new(),
            agent,
        }
    }
}

impl OpenClawClient {
    /// Create a client with custom config.
    pub fn with_config(config: OpenClawConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(15))
            .build();
        Self {
            config,
            status: ConnectionStatus::Disconnected,
            last_error: String::new(),
            agent,
        }
    }

    /// Test connection to the OpenClaw gateway.
    /// Returns true if the gateway is reachable.
    pub fn ping(&mut self) -> bool {
        let url = format!("{}/", self.config.gateway_url.trim_end_matches('/'));

        match self.agent.get(&url).call() {
            Ok(_resp) => {
                self.status = ConnectionStatus::Connected;
                self.last_error.clear();
                true
            }
            Err(e) => {
                self.status = ConnectionStatus::Error;
                self.last_error = format!("{}", e);
                false
            }
        }
    }

    /// Send a message to OpenClaw and get the response text.
    /// Uses the OpenResponses-compatible endpoint POST /v1/responses.
    pub fn send_message(&mut self, message: &str) -> Result<String, String> {
        self.status = ConnectionStatus::Sending;

        let url = format!(
            "{}/v1/responses",
            self.config.gateway_url.trim_end_matches('/')
        );

        let body = serde_json::json!({
            "model": "default",
            "input": message,
        });

        let mut req = self.agent.post(&url);

        // Add auth token if configured.
        if !self.config.token.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", self.config.token));
        }

        match req.send_json(body) {
            Ok(resp) => {
                match resp.into_string() {
                    Ok(text) => {
                        self.status = ConnectionStatus::Connected;
                        self.last_error.clear();
                        self.parse_response(&text)
                    }
                    Err(e) => {
                        self.status = ConnectionStatus::Error;
                        let msg = format!("Error reading response: {}", e);
                        self.last_error = msg.clone();
                        Err(msg)
                    }
                }
            }
            Err(e) => {
                self.status = ConnectionStatus::Error;
                let msg = format!("{}", e);
                self.last_error = msg.clone();
                Err(msg)
            }
        }
    }

    /// Parse the OpenResponses JSON or return raw text.
    fn parse_response(&self, text: &str) -> Result<String, String> {
        if let Ok(parsed) = serde_json::from_str::<OpenClawResponse>(text) {
            if let Some(err) = parsed.error {
                return Err(err);
            }
            // Extract text from output blocks.
            let mut result = String::new();
            for output in &parsed.output {
                for content in &output.content {
                    if content.content_type == "output_text"
                        || content.content_type == "text"
                        || content.content_type.is_empty()
                    {
                        if !result.is_empty() {
                            result.push('\n');
                        }
                        result.push_str(&content.text);
                    }
                }
            }
            if result.is_empty() {
                Ok(text.to_string())
            } else {
                Ok(result)
            }
        } else {
            // Not JSON - return raw text.
            Ok(text.to_string())
        }
    }

    /// Status display text (English).
    pub fn status_text(&self) -> &str {
        match self.status {
            ConnectionStatus::Disconnected => "Not connected",
            ConnectionStatus::Connected => "Connected",
            ConnectionStatus::Error => "Connection error",
            ConnectionStatus::Sending => "Sending...",
        }
    }

    /// Status display text (Spanish).
    pub fn status_text_es(&self) -> &str {
        match self.status {
            ConnectionStatus::Disconnected => "Sin conexion",
            ConnectionStatus::Connected => "Conectado",
            ConnectionStatus::Error => "Error de conexion",
            ConnectionStatus::Sending => "Enviando...",
        }
    }
}
