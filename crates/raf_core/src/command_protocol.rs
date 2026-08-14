//! Transport-neutral command protocol for RafUI, console, CLI and agents.
//!
//! The protocol deliberately contains no window, egui, WGPU or runtime/play
//! types. A client can talk to an already-open editor or to a headless engine
//! process through the same newline-delimited JSON messages.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::command::{Command, CommandId};
use crate::transaction::{
    ArtifactRef, ExecutionBudget, Revision, TransactionId, UndoToken, VerificationSummary,
};

pub const COMMAND_PROTOCOL_VERSION: u16 = 1;
pub const MAX_COMMAND_FRAME_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    RafUi,
    Console,
    Cli,
    Mcp,
    Plugin,
    Ipc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCommandRequest {
    pub protocol: u16,
    pub id: CommandId,
    pub name: String,
    #[serde(default)]
    pub params: Value,
    pub source: CommandSource,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<Revision>,
    #[serde(default)]
    pub transaction_id: Option<TransactionId>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub budget: Option<ExecutionBudget>,
}

impl EngineCommandRequest {
    pub fn new(name: impl Into<String>, params: Value, source: CommandSource) -> Self {
        Self {
            protocol: COMMAND_PROTOCOL_VERSION,
            id: CommandId::new(),
            name: name.into(),
            params,
            source,
            session: None,
            confirm: false,
            dry_run: false,
            expected_revision: None,
            transaction_id: None,
            idempotency_key: None,
            budget: None,
        }
    }

    pub fn as_command(
        &self,
        category: impl Into<String>,
        description: impl Into<String>,
    ) -> Command {
        Command {
            id: self.id,
            name: self.name.clone(),
            category: category.into(),
            description: description.into(),
            params: self.params.clone(),
            executed: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.protocol != COMMAND_PROTOCOL_VERSION {
            return Err(format!(
                "Unsupported command protocol {} (expected {}).",
                self.protocol, COMMAND_PROTOCOL_VERSION
            ));
        }
        if self.name.trim().is_empty() {
            return Err("Command name cannot be empty.".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineCommandResponse {
    pub protocol: u16,
    pub id: CommandId,
    pub ok: bool,
    #[serde(default)]
    pub changed: bool,
    pub title: String,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub diff: Option<Value>,
    #[serde(default)]
    pub undo_available: bool,
    #[serde(default)]
    pub revision: Revision,
    #[serde(default)]
    pub transaction_id: Option<TransactionId>,
    #[serde(default)]
    pub undo_token: Option<UndoToken>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactRef>,
    #[serde(default)]
    pub metrics: Value,
    #[serde(default)]
    pub verification: Option<VerificationSummary>,
}

impl EngineCommandResponse {
    pub fn error(id: CommandId, title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            protocol: COMMAND_PROTOCOL_VERSION,
            id,
            ok: false,
            changed: false,
            title: title.into(),
            lines: vec![message.into()],
            data: Value::Null,
            warnings: Vec::new(),
            diff: None,
            undo_available: false,
            revision: 0,
            transaction_id: None,
            undo_token: None,
            artifacts: Vec::new(),
            metrics: Value::Null,
            verification: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcEndpoint {
    Stdio,
    Tcp(SocketAddr),
    UnixSocket(PathBuf),
    WindowsNamedPipe(String),
}

/// Encodes one bounded JSON frame. The transport itself can be stdin/stdout,
/// a local socket or a named pipe; no platform-specific code is required here.
pub fn encode_line<T: Serialize>(value: &T) -> Result<String, String> {
    let line = serde_json::to_string(value).map_err(|error| format!("IPC encode: {error}"))?;
    if line.len() > MAX_COMMAND_FRAME_BYTES {
        return Err("IPC frame exceeds the 1 MiB safety limit.".to_string());
    }
    Ok(format!("{line}\n"))
}

pub fn decode_line<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T, String> {
    if line.len() > MAX_COMMAND_FRAME_BYTES {
        return Err("IPC frame exceeds the 1 MiB safety limit.".to_string());
    }
    serde_json::from_str(line.trim()).map_err(|error| format!("IPC decode: {error}"))
}

pub trait CommandEndpoint {
    fn execute(&mut self, request: EngineCommandRequest) -> EngineCommandResponse;
}

/// Runs a bounded newline-delimited command loop over stdin/stdout or any
/// equivalent reader/writer. This is the cross-platform IPC seam for a future
/// standalone CLI and MCP host; it does not require an active UI window.
pub fn serve_lines<R: BufRead, W: Write, E: CommandEndpoint>(
    reader: R,
    mut writer: W,
    endpoint: &mut E,
) -> Result<usize, String> {
    let mut processed = 0;
    for line in reader.lines() {
        let line = line.map_err(|error| format!("IPC read: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match decode_line::<EngineCommandRequest>(&line) {
            Ok(request) => match request.validate() {
                Ok(()) => endpoint.execute(request),
                Err(error) => {
                    EngineCommandResponse::error(request.id, "Invalid command request", error)
                }
            },
            Err(error) => {
                EngineCommandResponse::error(CommandId::new(), "Invalid command frame", error)
            }
        };
        let encoded = encode_line(&response)?;
        writer
            .write_all(encoded.as_bytes())
            .map_err(|error| format!("IPC write: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("IPC flush: {error}"))?;
        processed += 1;
    }
    Ok(processed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_as_bounded_json() {
        let mut request = EngineCommandRequest::new(
            "game.add",
            serde_json::json!({"primitive": "cube"}),
            CommandSource::Cli,
        );
        request.dry_run = true;
        request.expected_revision = Some(4);
        request.budget = Some(ExecutionBudget {
            max_tool_calls: Some(4),
            max_milliseconds: Some(500),
            profile: Some("potato".to_string()),
        });
        let encoded = encode_line(&request).unwrap();
        let decoded: EngineCommandRequest = decode_line(&encoded).unwrap();
        assert_eq!(decoded.name, "game.add");
        assert_eq!(decoded.source, CommandSource::Cli);
        assert!(decoded.dry_run);
        assert_eq!(decoded.expected_revision, Some(4));
        assert_eq!(
            decoded.budget.as_ref().unwrap().profile.as_deref(),
            Some("potato")
        );
    }

    #[test]
    fn oversized_frames_are_rejected_before_json_work() {
        let line = "x".repeat(MAX_COMMAND_FRAME_BYTES + 1);
        assert!(decode_line::<Value>(&line).is_err());
    }

    #[test]
    fn stdio_loop_is_ui_independent_and_returns_one_response_per_request() {
        struct Echo;
        impl CommandEndpoint for Echo {
            fn execute(&mut self, request: EngineCommandRequest) -> EngineCommandResponse {
                EngineCommandResponse {
                    protocol: COMMAND_PROTOCOL_VERSION,
                    id: request.id,
                    ok: true,
                    changed: false,
                    title: request.name,
                    lines: Vec::new(),
                    data: Value::Null,
                    warnings: Vec::new(),
                    diff: None,
                    undo_available: false,
                    revision: 0,
                    transaction_id: None,
                    undo_token: None,
                    artifacts: Vec::new(),
                    metrics: Value::Null,
                    verification: None,
                }
            }
        }
        let request = EngineCommandRequest::new("help", Value::Null, CommandSource::Cli);
        let input = encode_line(&request).unwrap();
        let mut output = Vec::new();
        let count = serve_lines(input.as_bytes(), &mut output, &mut Echo).unwrap();
        assert_eq!(count, 1);
        let response: EngineCommandResponse =
            decode_line(std::str::from_utf8(&output).unwrap()).unwrap();
        assert!(response.ok);
    }
}
