//! Client for an already-open AuraRafi editor.
//!
//! The client is deliberately transport-only. It authenticates against the
//! project-scoped descriptor and sends the same command frames used by the
//! editor's internal Agent and MCP adapter.

use raf_core::ipc::{
    decode_frame, encode_frame, AttachHello, AttachWelcome, EndpointDescriptor, IpcFrame,
};
use raf_core::{CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The editor queues attached commands onto its frame loop, so the Welcome
/// and command responses stay patient even while the editor is busy.
const IO_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AttachedClient {
    stream: TcpStream,
    welcome: AttachWelcome,
    project_path: PathBuf,
    client_name: String,
    last_context: Option<Value>,
}

impl std::fmt::Debug for AttachedClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachedClient")
            .field("welcome", &self.welcome)
            .field("project_path", &self.project_path)
            .field("has_context", &self.last_context.is_some())
            .finish_non_exhaustive()
    }
}

impl AttachedClient {
    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    pub fn connect(project_path: &Path, client_name: &str) -> Result<Self, String> {
        let project_path = if project_path.is_file() {
            project_path.parent().unwrap_or(project_path).to_path_buf()
        } else {
            project_path.to_path_buf()
        };
        let (stream, welcome) = Self::open_transport(&project_path, client_name)?;
        Ok(Self {
            stream,
            welcome,
            project_path,
            client_name: client_name.to_string(),
            last_context: None,
        })
    }

    /// Opens one authenticated loopback transport. Connect and reconnect share
    /// this path so a recovered adapter is byte-identical to a fresh one.
    fn open_transport(
        project_path: &Path,
        client_name: &str,
    ) -> Result<(TcpStream, AttachWelcome), String> {
        let descriptor = EndpointDescriptor::load_from_project(project_path)?;
        if descriptor.transport != "tcp_loopback" {
            return Err(format!(
                "Unsupported attached transport '{}'. This build supports tcp_loopback.",
                descriptor.transport
            ));
        }
        let address: SocketAddr = descriptor
            .address
            .parse()
            .map_err(|error| format!("Invalid attached endpoint address: {error}"))?;
        // Plain blocking connect: loopback refuses dead ports instantly, and
        // connect_timeout's nonblocking dance has historically been flaky on
        // the windows-gnu toolchain.
        let stream = TcpStream::connect(address).map_err(|_| {
            format!(
                "No editor is answering at {address}. The descriptor is stale \
                 (crashed or killed editor); it will be replaced when the editor \
                 reopens this project. Run `raf editors` to list live editors."
            )
        })?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|error| format!("Configure attached read timeout: {error}"))?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|error| format!("Configure attached write timeout: {error}"))?;
        // Borrowed halves instead of try_clone keep this path byte-identical
        // to a plain connect/write/read loop, which is what the editor's
        // listener is validated against.
        let welcome = {
            let mut writer: &TcpStream = &stream;
            let mut reader: &TcpStream = &stream;
            let hello = AttachHello::new(client_name, &descriptor);
            write_frame(&mut writer, IpcFrame::Hello(hello))?;
            let frame = match read_frame(&mut reader) {
                Ok(frame) => frame,
                Err(error) => {
                    // Surface whatever the editor did send so transport bugs
                    // are diagnosable from the CLI alone.
                    let mut leftover = [0u8; 256];
                    let peeked = reader.read(&mut leftover).unwrap_or(0);
                    if peeked > 0 {
                        return Err(format!(
                            "{error}; pending bytes: {}",
                            String::from_utf8_lossy(&leftover[..peeked])
                        ));
                    }
                    return Err(error);
                }
            };
            let IpcFrame::Welcome(welcome) = frame else {
                return Err("Attached editor did not return a welcome frame.".to_string());
            };
            if !welcome.accepted {
                return Err(welcome
                    .error
                    .unwrap_or_else(|| "The editor rejected the attached client.".to_string()));
            }
            welcome
        };
        Ok((stream, welcome))
    }

    /// Re-reads the project descriptor and re-authenticates after the editor
    /// restarted. The project path is stable; the port and token are not.
    fn reconnect(&mut self) -> Result<(), String> {
        let (stream, welcome) = Self::open_transport(&self.project_path, &self.client_name)?;
        self.stream = stream;
        self.welcome = welcome;
        // The editor may have reopened a different revision or document, so a
        // cached context from the previous connection must not be served.
        self.last_context = None;
        Ok(())
    }

    /// Sends one command and reads its response. `Err` always means the
    /// transport failed; editor-sent protocol errors are returned as normal
    /// command responses.
    fn exchange(&mut self, request: &EngineCommandRequest) -> Result<EngineCommandResponse, String> {
        {
            let mut writer: &TcpStream = &self.stream;
            write_frame(&mut writer, IpcFrame::Command(request.clone()))?;
        }
        let frame = {
            let mut reader: &TcpStream = &self.stream;
            read_frame(&mut reader)?
        };
        match frame {
            IpcFrame::Response(response) => {
                self.refresh_from_response(&response);
                Ok(response)
            }
            IpcFrame::Error { message } => Ok(EngineCommandResponse::error(
                request.id,
                "Attached transport",
                message,
            )),
            _ => Ok(EngineCommandResponse::error(
                request.id,
                "Attached transport",
                "The editor returned an unexpected frame.",
            )),
        }
    }

    pub fn resource(&self, uri: &str) -> Result<Value, String> {
        if let Some(id) = uri.strip_prefix("raf://plan/") {
            return crate::cli::read_plan_resource(&self.project_path, id);
        }
        match uri {
            "raf://status" => Ok(json!({
                "attached": true,
                "process_id": self.welcome.process_id,
                "revision": self.welcome.revision,
                "project_id": self.welcome.project_id,
                "project_path": self.welcome.project_path,
                "session_id": self.welcome.session_id,
                "session_name": self.welcome.session_name,
                "runtime_enabled": false,
                "play_enabled": false,
            })),
            "raf://project" => Ok(json!({
                "id": self.welcome.project_id,
                "path": self.welcome.project_path,
            })),
            "raf://capabilities" => Ok(json!({
                "capabilities": self.welcome.capabilities,
            })),
            "raf://brief" => {
                let path = self
                    .project_path
                    .join(".aura_rafi/world/brief.json");
                let content = std::fs::read(&path).ok();
                let brief = content
                    .as_deref()
                    .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
                    .unwrap_or(Value::Null);
                Ok(json!({
                    "found": content.is_some(),
                    "path": ".aura_rafi/world/brief.json",
                    "brief": brief,
                }))
            }
            "raf://plans" => {
                let directory = self.project_path.join(".aura_rafi/world/plans");
                let mut plans = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&directory) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                            continue;
                        }
                        let Ok(bytes) = std::fs::read(&path) else {
                            continue;
                        };
                        let Ok(plan) = serde_json::from_slice::<Value>(&bytes) else {
                            continue;
                        };
                        plans.push(json!({
                            "id": plan.get("id").cloned().unwrap_or(Value::Null),
                            "status": plan.get("status").cloned().unwrap_or(Value::Null),
                            "mode": plan.get("mode").cloned().unwrap_or(Value::Null),
                            "groups": plan.get("groups").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                            "entities": plan.get("entities").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                            "updates": plan.get("updates").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                            "deletions": plan.get("deletions").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                        }));
                    }
                }
                plans.sort_by(|left, right| {
                    right
                        .get("id")
                        .and_then(Value::as_str)
                        .cmp(&left.get("id").and_then(Value::as_str))
                });
                plans.truncate(64);
                Ok(json!({"count": plans.len(), "plans": plans}))
            }
            "raf://digest" => {
                let digest = self
                    .last_context
                    .as_ref()
                    .and_then(|context| context.get("scene"))
                    .cloned()
                    .unwrap_or(Value::Null);
                Ok(json!({"digest": digest, "attached": true}))
            }
            "raf://workspace" => Ok(json!({
                "path": self.project_path,
                "attached": true,
            })),
            "raf://context" => Ok(self.last_context.clone().unwrap_or_else(|| {
                json!({
                    "attached": true,
                    "project": {
                        "id": self.welcome.project_id,
                        "path": self.welcome.project_path,
                    },
                    "revision": self.welcome.revision,
                    "context_loaded": false,
                })
            })),
            _ => Err(format!("Unknown Rafi resource: {uri}")),
        }
    }

    /// Keep MCP resources and follow-up CLI output aligned with the live
    /// editor after a command advances the document revision.
    fn refresh_from_response(&mut self, response: &EngineCommandResponse) {
        self.welcome.revision = response.revision;
        let Some(data) = response.data.as_object() else {
            return;
        };
        if let Some(capabilities) = data.get("capabilities").and_then(Value::as_array) {
            self.welcome.capabilities = capabilities
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        if let Some(project) = data.get("project").and_then(Value::as_object) {
            self.welcome.project_path = project
                .get("path")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .or_else(|| self.welcome.project_path.clone());
        }
        if let Some(session) = data.get("session").and_then(Value::as_object) {
            self.welcome.session_name = session
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if data.get("scene").is_some() && data.get("project").is_some() {
            self.last_context = Some(response.data.clone());
        }
    }
}

impl CommandEndpoint for AttachedClient {
    fn execute(&mut self, mut request: EngineCommandRequest) -> EngineCommandResponse {
        request.source = match request.source {
            CommandSource::Mcp => CommandSource::Mcp,
            _ => CommandSource::Cli,
        };
        match self.exchange(&request) {
            Ok(response) => response,
            Err(error) => {
                // The editor may have restarted between calls, which replaces
                // the descriptor port and token. Re-authenticate once so the
                // adapter survives editor restarts instead of staying dead.
                if let Err(reconnect) = self.reconnect() {
                    return EngineCommandResponse::error(
                        request.id,
                        "Attached transport",
                        format!("{error} Reconnect failed: {reconnect}"),
                    );
                }
                let read_only = !crate::cli::mcp_command_is_mutation(&request.name);
                if read_only {
                    // Reads have no side effects, so replaying one on the new
                    // transport is always safe.
                    return match self.exchange(&request) {
                        Ok(response) => response,
                        Err(retry) => EngineCommandResponse::error(
                            request.id,
                            "Attached transport",
                            format!(
                                "{error} The editor connection was re-established, but the read retry failed: {retry}"
                            ),
                        ),
                    };
                }
                EngineCommandResponse::error(
                    request.id,
                    "Attached transport",
                    format!(
                        "{error} The editor connection was re-established, but this mutation was \
                         not replayed because it may already have executed. Verify the scene before retrying."
                    ),
                )
            }
        }
    }
}

fn write_frame(writer: &mut impl Write, frame: IpcFrame) -> Result<(), String> {
    let encoded = encode_frame(&frame)?;
    writer
        .write_all(encoded.as_bytes())
        .and_then(|_| writer.flush())
        .map_err(|error| format!("Attached write: {error}"))
}

/// Reads one newline-delimited frame byte by byte. Attached payloads are
/// small, so the per-byte loop is negligible next to frame pacing and it
/// keeps the transport free of buffered-reader state.
fn read_frame(reader: &mut impl Read) -> Result<IpcFrame, String> {
    let mut line: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let bytes = reader
            .read(&mut byte)
            .map_err(|error| format!("Attached read: {error}"))?;
        if bytes == 0 {
            return Err("The editor closed the attached connection.".to_string());
        }
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
        if line.len() > raf_core::MAX_COMMAND_FRAME_BYTES {
            return Err("IPC frame exceeds the 1 MiB safety limit.".to_string());
        }
    }
    let text = String::from_utf8(line).map_err(|error| format!("Attached decode: {error}"))?;
    decode_frame(&text)
}
