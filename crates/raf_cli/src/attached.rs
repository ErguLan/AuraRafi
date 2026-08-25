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

/// Loopback connect fails fast so stale descriptors do not stall the CLI.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// The editor queues attached commands onto its frame loop, so the Welcome
/// and command responses stay patient even while the editor is busy.
const IO_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AttachedClient {
    stream: TcpStream,
    welcome: AttachWelcome,
    project_path: PathBuf,
}

impl std::fmt::Debug for AttachedClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachedClient")
            .field("welcome", &self.welcome)
            .field("project_path", &self.project_path)
            .finish_non_exhaustive()
    }
}

impl AttachedClient {
    pub fn connect(project_path: &Path, client_name: &str) -> Result<Self, String> {
        let project_path = if project_path.is_file() {
            project_path.parent().unwrap_or(project_path).to_path_buf()
        } else {
            project_path.to_path_buf()
        };
        let descriptor = EndpointDescriptor::load_from_project(&project_path)?;
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
        Ok(Self {
            stream,
            welcome,
            project_path,
        })
    }

    pub fn resource(&self, uri: &str) -> Result<Value, String> {
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
            "raf://workspace" => Ok(json!({
                "path": self.project_path,
                "attached": true,
            })),
            _ => Err(format!("Unknown Rafi resource: {uri}")),
        }
    }
}

impl CommandEndpoint for AttachedClient {
    fn execute(&mut self, mut request: EngineCommandRequest) -> EngineCommandResponse {
        request.source = match request.source {
            CommandSource::Mcp => CommandSource::Mcp,
            _ => CommandSource::Cli,
        };
        let mut writer: &TcpStream = &self.stream;
        let mut reader: &TcpStream = &self.stream;
        if let Err(error) = write_frame(&mut writer, IpcFrame::Command(request.clone())) {
            return EngineCommandResponse::error(request.id, "Attached transport", error);
        }
        match read_frame(&mut reader) {
            Ok(IpcFrame::Response(response)) => response,
            Ok(IpcFrame::Error { message }) => {
                EngineCommandResponse::error(request.id, "Attached transport", message)
            }
            Ok(_) => EngineCommandResponse::error(
                request.id,
                "Attached transport",
                "The editor returned an unexpected frame.",
            ),
            Err(error) => EngineCommandResponse::error(request.id, "Attached transport", error),
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
