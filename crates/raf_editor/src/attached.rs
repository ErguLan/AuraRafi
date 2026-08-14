//! Local attached-agent bridge for the editor application.
//!
//! The listener owns no scene, UI, or renderer state. It only authenticates a
//! project-scoped client, queues command requests, and waits for the UI thread
//! to execute them through `AuraRafiApp`. This keeps all document mutation on
//! the same thread that owns editor history and session state.

use raf_core::ipc::{
    decode_frame, encode_frame, validate_hello, AttachWelcome, EndpointDescriptor, IpcFrame,
};
use raf_core::{CommandSource, EngineCommandRequest, EngineCommandResponse};
use raf_core::{Project, Revision};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;

const ACCEPT_POLL: Duration = Duration::from_millis(20);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub struct PendingAttachedCommand {
    pub project_id: Option<Uuid>,
    pub project_path: Option<PathBuf>,
    pub request: EngineCommandRequest,
    responder: Sender<EngineCommandResponse>,
}

struct AttachedState {
    descriptor: EndpointDescriptor,
    discovery_project: Option<PathBuf>,
}

pub struct AttachedCommandHost {
    state: Arc<Mutex<AttachedState>>,
    requests: Receiver<PendingAttachedCommand>,
    stop: Arc<AtomicBool>,
    listener_thread: Option<JoinHandle<()>>,
}

impl AttachedCommandHost {
    pub fn start() -> Self {
        match Self::try_start() {
            Ok(host) => host,
            Err(error) => {
                tracing::warn!(%error, "attached editor endpoint unavailable");
                Self::disabled()
            }
        }
    }

    fn try_start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("bind local attach endpoint: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("configure attach endpoint: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("read attach endpoint address: {error}"))?;
        let descriptor = EndpointDescriptor::new(address, Uuid::new_v4().to_string());
        let state = Arc::new(Mutex::new(AttachedState {
            descriptor,
            discovery_project: None,
        }));
        let (request_sender, requests) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let listener_thread = thread::Builder::new()
            .name("raf-editor-attach-listener".to_string())
            .spawn(move || listener_loop(listener, thread_state, request_sender, thread_stop))
            .map_err(|error| format!("start attach endpoint: {error}"))?;
        Ok(Self {
            state,
            requests,
            stop,
            listener_thread: Some(listener_thread),
        })
    }

    fn disabled() -> Self {
        let descriptor = EndpointDescriptor::new(
            "127.0.0.1:0".parse().expect("loopback address is valid"),
            String::new(),
        );
        let (_sender, requests) = mpsc::channel();
        Self {
            state: Arc::new(Mutex::new(AttachedState {
                descriptor,
                discovery_project: None,
            })),
            requests,
            stop: Arc::new(AtomicBool::new(true)),
            listener_thread: None,
        }
    }

    pub fn is_available(&self) -> bool {
        let state = self.state.lock().expect("attach state lock poisoned");
        state.descriptor.address != "127.0.0.1:0" && !state.descriptor.token.is_empty()
    }

    pub fn descriptor(&self) -> EndpointDescriptor {
        self.state
            .lock()
            .expect("attach state lock poisoned")
            .descriptor
            .clone()
    }

    pub fn update_project(
        &self,
        project: Option<&Project>,
        revision: Revision,
        session_id: Option<Uuid>,
        session_name: Option<String>,
        capabilities: Vec<String>,
    ) {
        let mut state = self.state.lock().expect("attach state lock poisoned");
        let previous_discovery = state.discovery_project.clone();
        state.descriptor.project_id = project.map(|project| project.id);
        state.descriptor.project_path = project.map(|project| project.path.clone());
        state.descriptor.session_id = session_id;
        state.descriptor.session_name = session_name;
        state.descriptor.revision = revision;
        state.descriptor.capabilities = capabilities;
        state.discovery_project = project.map(|project| project.path.clone());

        if previous_discovery != state.discovery_project {
            if let Some(previous) = previous_discovery {
                EndpointDescriptor::remove_for_project(&previous);
            }
        }
        if let Some(project_path) = state.discovery_project.as_deref() {
            if let Err(error) = state.descriptor.write_for_project(project_path) {
                tracing::warn!(%error, "could not publish editor attach descriptor");
            }
        }
    }

    pub fn update_revision(&self, revision: Revision) {
        let mut state = self.state.lock().expect("attach state lock poisoned");
        state.descriptor.revision = revision;
        if let Some(project_path) = state.discovery_project.as_deref() {
            if let Err(error) = state.descriptor.write_for_project(project_path) {
                tracing::debug!(%error, "could not refresh editor attach descriptor");
            }
        }
    }

    pub fn update_session(&self, session_id: Option<Uuid>, session_name: Option<String>) {
        let mut state = self.state.lock().expect("attach state lock poisoned");
        state.descriptor.session_id = session_id;
        state.descriptor.session_name = session_name;
        if let Some(project_path) = state.discovery_project.as_deref() {
            if let Err(error) = state.descriptor.write_for_project(project_path) {
                tracing::debug!(%error, "could not refresh editor session descriptor");
            }
        }
    }

    pub fn drain(&self) -> Vec<PendingAttachedCommand> {
        let mut pending = Vec::new();
        while let Ok(command) = self.requests.try_recv() {
            pending.push(command);
        }
        pending
    }

    pub fn respond(&self, command: PendingAttachedCommand, response: EngineCommandResponse) {
        let _ = command.responder.send(response);
    }
}

impl Drop for AttachedCommandHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.listener_thread.take() {
            let _ = thread.join();
        }
        if let Ok(state) = self.state.lock() {
            if let Some(project_path) = state.discovery_project.as_deref() {
                EndpointDescriptor::remove_for_project(project_path);
            }
        }
    }
}

fn listener_loop(
    listener: TcpListener,
    state: Arc<Mutex<AttachedState>>,
    request_sender: Sender<PendingAttachedCommand>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _address)) => {
                let connection_state = Arc::clone(&state);
                let connection_sender = request_sender.clone();
                let connection_stop = Arc::clone(&stop);
                let _ = thread::Builder::new()
                    .name("raf-editor-attach-client".to_string())
                    .spawn(move || {
                        if let Err(error) = handle_connection(
                            stream,
                            connection_state,
                            connection_sender,
                            connection_stop,
                        ) {
                            tracing::debug!(%error, "attached client disconnected");
                        }
                    });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL);
            }
            Err(error) => {
                tracing::warn!(%error, "attached listener stopped");
                break;
            }
        }
    }
}

fn handle_connection(
    stream: TcpStream,
    state: Arc<Mutex<AttachedState>>,
    request_sender: Sender<PendingAttachedCommand>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    // Accepted sockets inherit the listener's nonblocking flag on Windows.
    // The connection worker uses bounded blocking reads with a timeout, so
    // switch only this per-client stream back to blocking mode.
    stream
        .set_nonblocking(false)
        .map_err(|error| format!("configure attached blocking client: {error}"))?;
    stream
        .set_read_timeout(Some(COMMAND_TIMEOUT))
        .map_err(|error| format!("configure attached client: {error}"))?;
    let reader_stream = stream
        .try_clone()
        .map_err(|error| format!("clone attached client: {error}"))?;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = BufWriter::new(stream);
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .map_err(|error| format!("read attach hello: {error}"))?;
    if bytes == 0 {
        return Ok(());
    }
    let descriptor = state
        .lock()
        .map_err(|_| "attach state lock poisoned".to_string())?
        .descriptor
        .clone();
    let hello = match decode_frame(&line) {
        Ok(IpcFrame::Hello(hello)) => hello,
        Ok(_) => {
            write_frame(
                &mut writer,
                IpcFrame::Welcome(AttachWelcome::rejected(
                    &descriptor,
                    "First frame must be an attach hello.",
                )),
            )?;
            return Ok(());
        }
        Err(error) => {
            write_frame(
                &mut writer,
                IpcFrame::Welcome(AttachWelcome::rejected(&descriptor, error)),
            )?;
            return Ok(());
        }
    };
    if let Err(error) = validate_hello(&hello, &descriptor) {
        write_frame(
            &mut writer,
            IpcFrame::Welcome(AttachWelcome::rejected(&descriptor, error)),
        )?;
        return Ok(());
    }
    write_frame(
        &mut writer,
        IpcFrame::Welcome(AttachWelcome::accepted(&descriptor)),
    )?;

    line.clear();
    while !stop.load(Ordering::Acquire) {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| format!("read attached command: {error}"))?;
        if bytes == 0 {
            break;
        }
        let request = match decode_frame(&line) {
            Ok(IpcFrame::Command(mut request)) => {
                request.source = CommandSource::Ipc;
                request
            }
            Ok(_) => {
                write_frame(
                    &mut writer,
                    IpcFrame::Error {
                        message: "Expected a command frame.".to_string(),
                    },
                )?;
                continue;
            }
            Err(error) => {
                write_frame(&mut writer, IpcFrame::Error { message: error })?;
                continue;
            }
        };
        let id = request.id;
        if let Err(error) = request.validate() {
            write_frame(
                &mut writer,
                IpcFrame::Response(EngineCommandResponse::error(
                    id,
                    "Invalid attached command",
                    error,
                )),
            )?;
            continue;
        }
        let (response_sender, response_receiver) = mpsc::channel();
        request_sender
            .send(PendingAttachedCommand {
                project_id: hello.project_id,
                project_path: hello.project_path.clone(),
                request,
                responder: response_sender,
            })
            .map_err(|_| "editor command queue is unavailable".to_string())?;
        let command_started = Instant::now();
        let response = loop {
            match response_receiver.recv_timeout(Duration::from_millis(200)) {
                Ok(response) => break response,
                Err(mpsc::RecvTimeoutError::Timeout)
                    if !stop.load(Ordering::Acquire)
                        && command_started.elapsed() < COMMAND_TIMEOUT =>
                {
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    break EngineCommandResponse::error(
                        id,
                        if stop.load(Ordering::Acquire) {
                            "Attached command interrupted"
                        } else {
                            "Attached command timeout"
                        },
                        if stop.load(Ordering::Acquire) {
                            "The editor attach endpoint is shutting down."
                        } else {
                            "The editor did not respond before the command budget expired."
                        },
                    )
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break EngineCommandResponse::error(
                        id,
                        "Attached command unavailable",
                        "The editor command queue is unavailable.",
                    )
                }
            }
        };
        write_frame(&mut writer, IpcFrame::Response(response))?;
    }
    Ok(())
}

fn write_frame(writer: &mut impl Write, frame: IpcFrame) -> Result<(), String> {
    let encoded = encode_frame(&frame)?;
    writer
        .write_all(encoded.as_bytes())
        .and_then(|_| writer.flush())
        .map_err(|error| format!("write attached frame: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::{AttachHello, CommandSource, EngineCommandRequest, ProjectType};
    use serde_json::json;
    use std::io::{BufRead, BufReader, BufWriter};
    use std::net::TcpStream;
    use std::time::Instant;

    #[test]
    fn disabled_host_is_safe_for_startup_failure() {
        let host = AttachedCommandHost::disabled();
        assert!(!host.is_available());
        assert!(host.drain().is_empty());
    }

    #[test]
    fn loopback_handshake_queues_a_project_scoped_command() {
        let root = std::env::temp_dir().join(format!("raf-attached-test-{}", uuid::Uuid::new_v4()));
        let project = Project::create("Harness", ProjectType::Game, &root).unwrap();
        let host = AttachedCommandHost::try_start().unwrap();
        host.update_project(Some(&project), 7, None, None, vec!["game.add".to_string()]);
        let descriptor = host.descriptor();
        let stream =
            TcpStream::connect(descriptor.address.parse::<std::net::SocketAddr>().unwrap())
                .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let reader_stream = stream.try_clone().unwrap();
        let mut writer = BufWriter::new(stream);
        let mut reader = BufReader::new(reader_stream);
        write_frame(
            &mut writer,
            IpcFrame::Hello(AttachHello::new("raf-test", &descriptor)),
        )
        .unwrap();
        assert!(
            matches!(read_test_frame(&mut reader).unwrap(), IpcFrame::Welcome(welcome) if welcome.accepted)
        );

        let request =
            EngineCommandRequest::new("game.describe_scene", json!({}), CommandSource::Ipc);
        let request_id = request.id;
        write_frame(&mut writer, IpcFrame::Command(request)).unwrap();
        let started = Instant::now();
        let pending = loop {
            if let Some(pending) = host.drain().into_iter().next() {
                break pending;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(pending.project_id, Some(project.id));
        assert_eq!(pending.request.source, CommandSource::Ipc);
        host.respond(
            pending,
            EngineCommandResponse::error(request_id, "ok", "test"),
        );
        assert!(
            matches!(read_test_frame(&mut reader).unwrap(), IpcFrame::Response(response) if response.id == request_id)
        );
        drop(writer);
        drop(reader);
        drop(host);
        let _ = std::fs::remove_dir_all(root);
    }

    fn read_test_frame(reader: &mut impl BufRead) -> Result<IpcFrame, String> {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        decode_frame(&line)
    }
}
