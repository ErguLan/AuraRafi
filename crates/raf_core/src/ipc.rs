//! Local editor-attachment protocol shared by the CLI, MCP adapter and editor.
//!
//! The transport is intentionally small and provider-neutral. The first
//! implementation uses a loopback TCP stream because it is available on every
//! supported desktop target without adding a platform-specific dependency.
//! The descriptor and handshake keep that stream project-scoped; named pipes
//! and Unix sockets can be added behind the same frames later.

use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::command_protocol::{
    decode_line, encode_line, EngineCommandRequest, EngineCommandResponse,
    COMMAND_PROTOCOL_VERSION, MAX_COMMAND_FRAME_BYTES,
};
use crate::transaction::Revision;

pub const ATTACH_PROTOCOL_VERSION: u16 = 1;
pub const ATTACH_DIRECTORY: &str = ".aura_rafi";
pub const ATTACH_DESCRIPTOR_FILE: &str = "agent_endpoint.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointDescriptor {
    pub protocol: u16,
    pub transport: String,
    pub address: String,
    pub token: String,
    pub process_id: u32,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub session_name: Option<String>,
    #[serde(default)]
    pub revision: Revision,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl EndpointDescriptor {
    pub fn new(address: SocketAddr, token: impl Into<String>) -> Self {
        Self {
            protocol: ATTACH_PROTOCOL_VERSION,
            transport: "tcp_loopback".to_string(),
            address: address.to_string(),
            token: token.into(),
            process_id: std::process::id(),
            project_id: None,
            project_path: None,
            session_id: None,
            session_name: None,
            revision: 0,
            capabilities: Vec::new(),
        }
    }

    pub fn discovery_path(project_path: &Path) -> PathBuf {
        project_path
            .join(ATTACH_DIRECTORY)
            .join(ATTACH_DESCRIPTOR_FILE)
    }

    pub fn load_from_project(project_path: &Path) -> Result<Self, String> {
        let path = Self::discovery_path(project_path);
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("Unable to read {}: {error}", path.display()))?;
        let descriptor = serde_json::from_str(&raw)
            .map_err(|error| format!("Invalid endpoint descriptor {}: {error}", path.display()))?;
        Ok(descriptor)
    }

    pub fn write_for_project(&self, project_path: &Path) -> Result<PathBuf, String> {
        let path = Self::discovery_path(project_path);
        let Some(parent) = path.parent() else {
            return Err("Endpoint descriptor has no parent directory.".to_string());
        };
        fs::create_dir_all(parent)
            .map_err(|error| format!("Unable to create {}: {error}", parent.display()))?;
        let data = serde_json::to_string_pretty(self)
            .map_err(|error| format!("Endpoint descriptor encode: {error}"))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, data)
            .map_err(|error| format!("Unable to write {}: {error}", temporary.display()))?;
        // `rename` replaces atomically on Unix, but Windows rejects replacing
        // an existing file. The descriptor is disposable discovery metadata,
        // so remove only that exact previous descriptor before publishing.
        let _ = fs::remove_file(&path);
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(format!("Unable to publish {}: {error}", path.display()));
        }
        Ok(path)
    }

    pub fn remove_for_project(project_path: &Path) {
        let path = Self::discovery_path(project_path);
        let _ = fs::remove_file(path);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachHello {
    pub protocol: u16,
    pub client: String,
    pub token: String,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub project_path: Option<PathBuf>,
}

impl AttachHello {
    pub fn new(client: impl Into<String>, descriptor: &EndpointDescriptor) -> Self {
        Self {
            protocol: ATTACH_PROTOCOL_VERSION,
            client: client.into(),
            token: descriptor.token.clone(),
            project_id: descriptor.project_id,
            project_path: descriptor.project_path.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachWelcome {
    pub protocol: u16,
    pub accepted: bool,
    pub server: String,
    pub process_id: u32,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub project_path: Option<PathBuf>,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub session_name: Option<String>,
    #[serde(default)]
    pub revision: Revision,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl AttachWelcome {
    pub fn accepted(descriptor: &EndpointDescriptor) -> Self {
        Self {
            protocol: ATTACH_PROTOCOL_VERSION,
            accepted: true,
            server: "AuraRafi editor".to_string(),
            process_id: descriptor.process_id,
            project_id: descriptor.project_id,
            project_path: descriptor.project_path.clone(),
            session_id: descriptor.session_id,
            session_name: descriptor.session_name.clone(),
            revision: descriptor.revision,
            capabilities: descriptor.capabilities.clone(),
            error: None,
        }
    }

    pub fn rejected(descriptor: &EndpointDescriptor, error: impl Into<String>) -> Self {
        let mut welcome = Self::accepted(descriptor);
        welcome.accepted = false;
        welcome.error = Some(error.into());
        welcome
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum IpcFrame {
    Hello(AttachHello),
    Welcome(AttachWelcome),
    Command(EngineCommandRequest),
    Response(EngineCommandResponse),
    Error { message: String },
}

pub fn encode_frame(frame: &IpcFrame) -> Result<String, String> {
    encode_line(frame)
}

pub fn decode_frame(line: &str) -> Result<IpcFrame, String> {
    if line.len() > MAX_COMMAND_FRAME_BYTES {
        return Err("IPC frame exceeds the 1 MiB safety limit.".to_string());
    }
    decode_line(line)
}

pub fn validate_hello(hello: &AttachHello, descriptor: &EndpointDescriptor) -> Result<(), String> {
    if hello.protocol != ATTACH_PROTOCOL_VERSION {
        return Err(format!(
            "Unsupported attach protocol {} (expected {}).",
            hello.protocol, ATTACH_PROTOCOL_VERSION
        ));
    }
    if hello.token != descriptor.token {
        return Err("Attach token was rejected.".to_string());
    }
    if hello.project_id != descriptor.project_id {
        return Err("The requested project is not the editor's active project.".to_string());
    }
    if !same_path(
        hello.project_path.as_deref(),
        descriptor.project_path.as_deref(),
    ) {
        return Err("The requested project path is not the editor's active project.".to_string());
    }
    Ok(())
}

fn same_path(left: Option<&Path>, right: Option<&Path>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            if left == right {
                return true;
            }
            left.canonicalize().ok() == right.canonicalize().ok()
        }
        _ => false,
    }
}

pub fn command_protocol_is_current(request: &EngineCommandRequest) -> Result<(), String> {
    if request.protocol != COMMAND_PROTOCOL_VERSION {
        return Err(format!(
            "Unsupported command protocol {} (expected {}).",
            request.protocol, COMMAND_PROTOCOL_VERSION
        ));
    }
    request.validate()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_and_descriptor_round_trip_without_editor_types() {
        let descriptor =
            EndpointDescriptor::new("127.0.0.1:43123".parse().unwrap(), "secret-token");
        let hello = AttachHello::new("raf-test", &descriptor);
        let frame = IpcFrame::Hello(hello.clone());
        let encoded = encode_frame(&frame).unwrap();
        let decoded = decode_frame(&encoded).unwrap();
        assert_eq!(decoded, frame);
        validate_hello(&hello, &descriptor).unwrap();
    }

    #[test]
    fn hello_rejects_wrong_project_or_token() {
        let descriptor =
            EndpointDescriptor::new("127.0.0.1:43124".parse().unwrap(), "secret-token");
        let mut hello = AttachHello::new("raf-test", &descriptor);
        hello.token = "wrong".to_string();
        assert!(validate_hello(&hello, &descriptor).is_err());
    }
}
