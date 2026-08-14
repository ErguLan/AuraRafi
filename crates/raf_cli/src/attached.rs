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
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AttachedClient {
    writer: BufWriter<TcpStream>,
    reader: BufReader<TcpStream>,
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
        let address = descriptor
            .address
            .parse()
            .map_err(|error| format!("Invalid attached endpoint address: {error}"))?;
        let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|error| {
            format!("Unable to connect to the open editor at {address}: {error}")
        })?;
        stream
            .set_read_timeout(Some(CONNECT_TIMEOUT))
            .map_err(|error| format!("Configure attached read timeout: {error}"))?;
        stream
            .set_write_timeout(Some(CONNECT_TIMEOUT))
            .map_err(|error| format!("Configure attached write timeout: {error}"))?;
        let reader_stream = stream
            .try_clone()
            .map_err(|error| format!("Clone attached stream: {error}"))?;
        let mut writer = BufWriter::new(stream);
        let mut reader = BufReader::new(reader_stream);
        let hello = AttachHello::new(client_name, &descriptor);
        write_frame(&mut writer, IpcFrame::Hello(hello))?;
        let welcome = read_frame(&mut reader)?;
        let IpcFrame::Welcome(welcome) = welcome else {
            return Err("Attached editor did not return a welcome frame.".to_string());
        };
        if !welcome.accepted {
            return Err(welcome
                .error
                .unwrap_or_else(|| "The editor rejected the attached client.".to_string()));
        }
        Ok(Self {
            writer,
            reader,
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
        if let Err(error) = write_frame(&mut self.writer, IpcFrame::Command(request.clone())) {
            return EngineCommandResponse::error(request.id, "Attached transport", error);
        }
        match read_frame(&mut self.reader) {
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

fn read_frame(reader: &mut impl BufRead) -> Result<IpcFrame, String> {
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .map_err(|error| format!("Attached read: {error}"))?;
    if bytes == 0 {
        return Err("The editor closed the attached connection.".to_string());
    }
    decode_frame(&line)
}
