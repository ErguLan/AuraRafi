//! Manual local Play controller. Attached Agent/CLI/MCP commands never own this boundary.
use raf_core::runtime_config::RuntimeLaunchMode;
use raf_core::{
    config::EngineSettings,
    project::{Project, ProjectType},
    session::{ProjectSessionRegistry, SessionId},
    SceneGraph,
};
use raf_player::{PlayerLaunch, PlayerSession, PlayerSource};
use raf_render::api_graphic_basic::ui_surface::NativeUiWindowHost;
use raf_runtime::manifest::{RuntimeHostSettings, RuntimeManifest};
use raf_runtime::wire::{RuntimeDiagnostic, RuntimeDiagnosticSeverity, RuntimePacket, RuntimePeer};
use raf_runtime::{RuntimeControl, RuntimePhase, RuntimeStatus};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::console::LogLevel;
use crate::native_runtime_log::{
    preparation_timed_out, resolve_player_executable, ChildLogs, PlayerPreflight, LOG_TAIL_LIMIT,
    RUNTIME_READY_TIMEOUT,
};

const MAX_POLL_DIAGNOSTICS: usize = 64;

struct ChildRuntime {
    development_slot: u32,
    child: Child,
    listener: TcpListener,
    peer: Option<RuntimePeer>,
    authenticated: bool,
    token: Uuid,
    status: RuntimeStatus,
    temporary_directory: std::path::PathBuf,
    created: Instant,
    stopping: Option<Instant>,
    exit_code: Option<i32>,
    log_paths: ChildLogs,
    last_diagnostic: Option<RuntimeDiagnostic>,
    ready_deadline: Instant,
    timeout_reported: bool,
    connection_failed: bool,
}
impl Drop for ChildRuntime {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.log_paths.cleanup();
        // Only the owned snapshot is deleted. Never recurse into a user project.
        let _ = std::fs::remove_file(self.temporary_directory.join("snapshot.ron"));
        let _ = std::fs::remove_dir(&self.temporary_directory);
    }
}

/// A structured console line produced by the local runtime boundary.
///
/// `message` and `detail` may be locale keys (`console.*`, `runtime.*`) or
/// player-authored text. The workbench resolves keys when it appends the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConsoleEntry {
    pub level: LogLevel,
    pub code: String,
    pub source: String,
    pub message: String,
    pub detail: Option<String>,
    pub at_ms: u64,
    /// Bounded runtime log tail, already split into display lines.
    pub lines: Vec<String>,
}

impl RuntimeConsoleEntry {
    pub fn new(
        level: LogLevel,
        code: impl Into<String>,
        source: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            level,
            code: code.into(),
            source: source.into(),
            message: message.into(),
            detail,
            at_ms: 0,
            lines: Vec::new(),
        }
    }

    pub fn info(
        code: impl Into<String>,
        source: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self::new(LogLevel::Info, code, source, message, detail)
    }

    pub fn warning(
        code: impl Into<String>,
        source: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self::new(LogLevel::Warning, code, source, message, detail)
    }

    pub fn error(
        code: impl Into<String>,
        source: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self::new(LogLevel::Error, code, source, message, detail)
    }

    pub fn from_diagnostic(diagnostic: &RuntimeDiagnostic) -> Self {
        Self {
            level: LogLevel::from_runtime_severity(diagnostic.severity),
            code: diagnostic.code.clone(),
            source: if diagnostic.source.is_empty() {
                "runtime".to_string()
            } else {
                diagnostic.source.clone()
            },
            message: diagnostic.message.clone(),
            detail: diagnostic.detail.clone(),
            at_ms: diagnostic.at_ms,
            lines: Vec::new(),
        }
    }

    pub fn legacy(message: impl Into<String>) -> Self {
        Self::from_diagnostic(&RuntimeDiagnostic::info(
            "console.runtime",
            "runtime",
            message,
        ))
    }

    pub fn with_lines(mut self, lines: Vec<String>) -> Self {
        self.lines = lines;
        self
    }

    pub fn with_at_ms_if_zero(mut self, at_ms: u64) -> Self {
        if self.at_ms == 0 {
            self.at_ms = at_ms;
        }
        self
    }
}

/// Visible failure state for the workbench banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNotice {
    diagnostic: RuntimeDiagnostic,
}

impl RuntimeNotice {
    pub fn from_diagnostic(diagnostic: RuntimeDiagnostic) -> Self {
        Self { diagnostic }
    }

    pub fn diagnostic(&self) -> &RuntimeDiagnostic {
        &self.diagnostic
    }

    pub fn severity(&self) -> RuntimeDiagnosticSeverity {
        self.diagnostic.severity
    }

    pub fn message(&self) -> &str {
        &self.diagnostic.message
    }

    pub fn detail(&self) -> Option<&str> {
        self.diagnostic.detail.as_deref()
    }
}

/// Launch rejection with enough structure for both the console and the banner.
#[derive(Debug, Clone)]
pub struct RuntimeLaunchFailure {
    pub diagnostic: RuntimeDiagnostic,
}

impl RuntimeLaunchFailure {
    pub fn new(diagnostic: RuntimeDiagnostic) -> Self {
        Self { diagnostic }
    }

    pub fn diagnostic(&self) -> &RuntimeDiagnostic {
        &self.diagnostic
    }

    pub fn console_entry(&self) -> RuntimeConsoleEntry {
        RuntimeConsoleEntry::from_diagnostic(&self.diagnostic)
    }

    pub fn notice(&self) -> RuntimeNotice {
        RuntimeNotice::from_diagnostic(self.diagnostic.clone())
    }
}

impl From<RuntimeDiagnostic> for RuntimeLaunchFailure {
    fn from(diagnostic: RuntimeDiagnostic) -> Self {
        Self::new(diagnostic)
    }
}

impl From<String> for RuntimeLaunchFailure {
    fn from(error: String) -> Self {
        Self::new(crate::native_runtime_log::launch_failed(error))
    }
}

impl From<&str> for RuntimeLaunchFailure {
    fn from(error: &str) -> Self {
        Self::new(crate::native_runtime_log::launch_failed(error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLaunch {
    pub instance: Uuid,
    pub player_version: String,
}

#[derive(Debug, Default, Clone)]
pub struct RuntimePollReport {
    pub entries: Vec<RuntimeConsoleEntry>,
    pub notice: Option<RuntimeNotice>,
    /// A previously failed local Play is healthy again (in-place `Running`).
    pub clear_notice: bool,
}

#[derive(Default)]
pub struct NativeRuntimeController {
    children: Vec<ChildRuntime>,
    pub in_place: Option<PlayerSession>,
    selected: Option<Uuid>,
    finished: Option<RuntimeStatus>,
}
impl NativeRuntimeController {
    pub fn has_instances(&self) -> bool {
        !self.children.is_empty() || self.in_place.is_some()
    }
    pub fn count(&self) -> usize {
        self.children.len() + usize::from(self.in_place.is_some())
    }
    pub fn selected_status(&self) -> Option<&RuntimeStatus> {
        if let Some(session) = &self.in_place {
            return Some(session.status());
        }
        self.children
            .iter()
            .find(|child| Some(child.status.instance) == self.selected)
            .map(|child| &child.status)
            .or(self.finished.as_ref())
    }
    pub fn toolbar_state(
        &self,
        settings: &EngineSettings,
    ) -> raf_player::surface::RuntimeToolbarState {
        raf_player::surface::RuntimeToolbarState {
            phase: if self.has_instances() {
                self.selected_status().map(|status| status.phase)
            } else {
                None
            },
            can_launch: self.in_place.is_none()
                && self.count() < settings.runtime.normalized().max_instances as usize,
            instances: self.count(),
        }
    }
    pub fn launch(
        &mut self,
        project: &Project,
        scene: &SceneGraph,
        node_graph: Option<&raf_nodes::NodeGraph>,
        settings: &EngineSettings,
        host: &NativeUiWindowHost,
    ) -> Result<RuntimeLaunch, RuntimeLaunchFailure> {
        if project.project_type != ProjectType::Game {
            return Err("local Play is available only in Game projects".into());
        }
        if self.count() >= settings.runtime.normalized().max_instances as usize {
            return Err("runtime instance limit reached".into());
        }
        if self.in_place.is_some() {
            return Err("an in-place runtime is already active".into());
        }
        // Preflight happens before project documents are read or a temporary
        // directory exists, so a stale player can never flash a window.
        let player_preflight = if settings.runtime.launch_mode == RuntimeLaunchMode::SameWindow {
            None
        } else {
            let executable = std::env::current_exe().map_err(|error| {
                RuntimeLaunchFailure::from(format!("resolve editor executable: {error}"))
            })?;
            let player = resolve_player_executable(&executable);
            Some(PlayerPreflight::probe(&player, env!("CARGO_PKG_VERSION"))?)
        };
        let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
        let session = if let Some(id) = project.settings.runtime.startup_session {
            registry
                .sessions
                .iter()
                .find(|s| s.id == SessionId(id))
                .ok_or("configured runtime startup session no longer exists")?
        } else {
            registry.active().ok_or("project has no active session")?
        };
        let scene = if registry.active_session == session.id {
            scene.clone()
        } else {
            // Strict read: a corrupt session must never turn into an empty successful runtime.
            let path = project.path.join(&session.scene_file);
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            if !path.starts_with(project.path.canonicalize().map_err(|e| e.to_string())?) {
                return Err("runtime session escapes the project".into());
            }
            use std::io::Read;
            let mut raw = Vec::new();
            std::fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .take(raf_runtime::manifest::MANIFEST_LIMIT as u64 + 1)
                .read_to_end(&mut raw)
                .map_err(|e| e.to_string())?;
            if raw.len() > raf_runtime::manifest::MANIFEST_LIMIT {
                return Err("runtime session scene is too large".into());
            }
            ron::de::from_bytes(&raw).map_err(|e| format!("runtime session scene: {e}"))?
        };
        let instance = Uuid::new_v4();
        let development_slot = (0..4)
            .find(|slot| {
                !self
                    .children
                    .iter()
                    .any(|child| child.development_slot == *slot)
            })
            .ok_or("runtime development slot limit reached")?;
        let node_graph = if registry.active_session == session.id {
            node_graph.cloned()
        } else {
            let path = project.path.join(&session.nodes_file);
            if path.exists() {
                use std::io::Read;
                let path = path.canonicalize().map_err(|e| e.to_string())?;
                if !path.starts_with(project.path.canonicalize().map_err(|e| e.to_string())?) {
                    return Err("runtime graph escapes project".into());
                }
                let mut bytes = Vec::new();
                std::fs::File::open(path)
                    .map_err(|e| e.to_string())?
                    .take(1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > 1024 * 1024 {
                    return Err("Runtime graph exceeds 1 MiB".into());
                }
                Some(ron::de::from_bytes(&bytes).map_err(|e| format!("runtime graph: {e}"))?)
            } else {
                None
            }
        };
        let mut safe_settings = RuntimeHostSettings::from_engine(settings);
        // Play presentation pacing belongs to the game. Window size, instances,
        // VSync and launch mode remain host preferences in the editor.
        safe_settings.runtime.fps_limit = project.settings.runtime.fps_limit.clamp(15, 120);
        safe_settings.runtime.script_budget_ms = project.settings.runtime.script_budget_ms.clamp(1, 100);
        safe_settings.runtime.script_operation_limit = project
            .settings
            .runtime
            .script_operation_limit
            .clamp(1_000, 1_000_000);
        let manifest = RuntimeManifest {
            project_id: project.id,
            session_id: Some(session.id.0),
            development_slot,
            save_root_grant: settings.runtime_save_roots.get(&project.id).cloned(),
            node_graph,
            protocol: 1,
            instance,
            project_root: project.path.clone(),
            project_name: project.name.clone(),
            ui_document_file: Some(session.ui_document_file.clone()),
            scene,
            project_settings: project.settings.clone(),
            host_settings: safe_settings.clone(),
        };
        let player_version = if settings.runtime.launch_mode == RuntimeLaunchMode::SameWindow {
            self.in_place = Some(PlayerSession::new(
                host,
                PlayerSource::Snapshot(Box::new(manifest)),
                safe_settings,
                instance,
            ));
            env!("CARGO_PKG_VERSION").to_string()
        } else {
            let preflight = player_preflight
                .ok_or_else(|| RuntimeLaunchFailure::from("runtime preflight did not run"))?;
            let player_path = preflight.executable.path.clone();
            let directory = std::env::temp_dir().join(format!("raf-runtime-{instance}"));
            std::fs::create_dir(&directory)
                .map_err(|e| format!("runtime temporary directory: {e}"))?;
            let logs = match ChildLogs::create(&directory) {
                Ok(logs) => logs,
                Err(error) => {
                    let _ = std::fs::remove_dir(&directory);
                    return Err(format!("runtime log files: {error}").into());
                }
            };
            let snapshot = directory.join("snapshot.ron");
            let result = (|| {
                manifest.write(&snapshot)?;
                let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
                listener.set_nonblocking(true).map_err(|e| e.to_string())?;
                let token = Uuid::new_v4();
                let launch = PlayerLaunch {
                    manifest: snapshot.clone(),
                    settings: safe_settings,
                    project_name: project.name.clone(),
                    instance,
                    control_address: Some(listener.local_addr().map_err(|e| e.to_string())?),
                    control_token: token,
                };
                let (stdout, stderr) = logs
                    .open_stdio()
                    .map_err(|e| format!("runtime log files: {e}"))?;
                let mut command = Command::new(&player_path);
                command
                    .arg("--runtime")
                    .arg(serde_json::to_string(&launch).map_err(|e| e.to_string())?)
                    .stdin(Stdio::null())
                    .stdout(stdout)
                    .stderr(stderr);
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                let child = command
                    .spawn()
                    .map_err(|e| format!("start local runtime: {e}"))?;
                let created = Instant::now();
                Ok::<_, String>(ChildRuntime {
                    development_slot,
                    child,
                    listener,
                    peer: None,
                    authenticated: false,
                    token,
                    status: RuntimeStatus {
                        instance,
                        phase: RuntimePhase::Preparing,
                        fixed_ticks: 0,
                        elapsed_seconds: 0.0,
                        script_instances: 0,
                        camera_ready: false,
                        last_error: None,
                    },
                    temporary_directory: directory.clone(),
                    created,
                    stopping: None,
                    exit_code: None,
                    log_paths: logs.clone(),
                    last_diagnostic: None,
                    ready_deadline: created + RUNTIME_READY_TIMEOUT,
                    timeout_reported: false,
                    connection_failed: false,
                })
            })();
            match result {
                Ok(child) => self.children.push(child),
                Err(error) => {
                    logs.cleanup();
                    let _ = std::fs::remove_file(snapshot);
                    let _ = std::fs::remove_dir(directory);
                    return Err(error.into());
                }
            }
            preflight.reported_version
        };
        self.selected = Some(instance);
        self.finished = None;
        Ok(RuntimeLaunch {
            instance,
            player_version,
        })
    }
    pub fn control(&mut self, control: RuntimeControl) -> Result<(), String> {
        if let Some(session) = &mut self.in_place {
            session.control(control);
            return Ok(());
        }
        let child = self
            .children
            .iter_mut()
            .find(|child| Some(child.status.instance) == self.selected)
            .ok_or("no active runtime instance")?;
        if control == RuntimeControl::Stop {
            child.stopping = Some(Instant::now());
            child.status.phase = RuntimePhase::Stopping;
        }
        if child.authenticated {
            child
                .peer
                .as_mut()
                .ok_or("runtime connection unavailable")?
                .send(RuntimePacket::Control(control))?;
        } else if control != RuntimeControl::Stop {
            return Err("runtime is still connecting".into());
        }
        Ok(())
    }
    pub fn select_next(&mut self) {
        if self.children.len() < 2 {
            return;
        }
        let index = self
            .children
            .iter()
            .position(|child| Some(child.status.instance) == self.selected)
            .unwrap_or(0);
        self.selected = Some(
            self.children[(index + 1) % self.children.len()]
                .status
                .instance,
        );
    }
    /// Bounded polling for child runtimes and same-window Play diagnostics.
    /// The native event loop schedules it while any runtime instance exists.
    pub fn poll(&mut self) -> RuntimePollReport {
        let mut report = RuntimePollReport::default();
        // Same-window Play runs in this process. Its structured preparation
        // events are the only path to the editor console and failure banner.
        if let Some(session) = &mut self.in_place {
            let status = session.status().clone();
            let elapsed_ms = (status.elapsed_seconds.max(0.0) * 1000.0).round() as u64;
            let mut structured_messages = Vec::new();
            for diagnostic in session
                .drain_diagnostic_events()
                .into_iter()
                .take(MAX_POLL_DIAGNOSTICS)
            {
                structured_messages.push(diagnostic.message.clone());
                let entry = RuntimeConsoleEntry::from_diagnostic(&diagnostic)
                    .with_at_ms_if_zero(elapsed_ms);
                if diagnostic.severity == RuntimeDiagnosticSeverity::Error {
                    report.notice = Some(RuntimeNotice::from_diagnostic(diagnostic));
                }
                push_diagnostic(&mut report.entries, entry);
            }
            // Legacy lines still carry world/script text. Keep them, but do not
            // duplicate the structured failure that already carried code and
            // severity.
            for message in session
                .drain_diagnostics()
                .into_iter()
                .take(MAX_POLL_DIAGNOSTICS)
            {
                if structured_messages
                    .iter()
                    .any(|structured| structured.as_str() == message.as_str())
                {
                    continue;
                }
                push_diagnostic(
                    &mut report.entries,
                    RuntimeConsoleEntry::legacy(message).with_at_ms_if_zero(elapsed_ms),
                );
            }
            if (self.selected.is_none() || self.selected == Some(status.instance))
                && status.phase == RuntimePhase::Running
                && status.last_error.is_none()
                && report.notice.is_none()
            {
                report.clear_notice = true;
            }
        }
        for child in &mut self.children {
            if child.peer.is_none() {
                match child.listener.accept() {
                    Ok((stream, address)) if address.ip().is_loopback() => {
                        child.peer = RuntimePeer::new(stream).ok();
                    }
                    _ => {}
                }
            }
            if let Some(peer) = &mut child.peer {
                match peer.poll() {
                    Ok(packets) => {
                        for packet in packets {
                            if !child.authenticated {
                                if matches!(packet, RuntimePacket::Hello { token, instance } if token == child.token && instance == child.status.instance)
                                {
                                    child.authenticated = true;
                                    if child.stopping.is_some() {
                                        let _ =
                                            peer.send(RuntimePacket::Control(RuntimeControl::Stop));
                                    }
                                } else {
                                    push_diagnostic(
                                        &mut report.entries,
                                        RuntimeConsoleEntry::warning(
                                            "console.runtime",
                                            "editor",
                                            "Rejected unauthenticated runtime connection",
                                            None,
                                        ),
                                    );
                                    child.stopping = Some(Instant::now());
                                }
                                continue;
                            }
                            match packet {
                                RuntimePacket::Status(status)
                                    if status.instance == child.status.instance =>
                                {
                                    child.status = status
                                }
                                RuntimePacket::DiagnosticEvent(diagnostic) => {
                                    child.last_diagnostic = Some(diagnostic.clone());
                                    push_diagnostic(
                                        &mut report.entries,
                                        RuntimeConsoleEntry::from_diagnostic(&diagnostic)
                                            .with_at_ms_if_zero(
                                                child.created.elapsed().as_millis() as u64,
                                            ),
                                    );
                                }
                                RuntimePacket::Diagnostic(message) => {
                                    let diagnostic = RuntimeDiagnostic::info(
                                        "console.runtime",
                                        "runtime",
                                        message,
                                    );
                                    child.last_diagnostic = Some(diagnostic.clone());
                                    push_diagnostic(
                                        &mut report.entries,
                                        RuntimeConsoleEntry::from_diagnostic(&diagnostic)
                                            .with_at_ms_if_zero(
                                                child.created.elapsed().as_millis() as u64,
                                            ),
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(error) => {
                        child.stopping.get_or_insert_with(Instant::now);
                        child.connection_failed = true;
                        if child.status.phase != RuntimePhase::Stopped {
                            push_diagnostic(
                                &mut report.entries,
                                RuntimeConsoleEntry::warning(
                                    "console.runtime",
                                    "editor",
                                    format!("Runtime connection: {error}"),
                                    None,
                                ),
                            );
                        }
                        child.peer = None;
                    }
                }
            }
            if !child.timeout_reported
                && child.stopping.is_none()
                && preparation_timed_out(
                    child.authenticated,
                    child.status.phase,
                    Instant::now(),
                    child.ready_deadline,
                )
            {
                let timed_out_phase = child.status.phase;
                child.timeout_reported = true;
                child.stopping.get_or_insert_with(Instant::now);
                child.status.phase = RuntimePhase::Failed;
                let _ = child.child.kill();
                let _ = child.child.wait();
                let lines = child.log_paths.collect_tail(LOG_TAIL_LIMIT).console_lines();
                let mut detail = format!(
                    "authenticated={} phase={:?}",
                    child.authenticated, timed_out_phase
                );
                if let Some(last) = &child.last_diagnostic {
                    detail.push_str(&format!("; last diagnostic: {}", last.code));
                }
                let diagnostic = RuntimeDiagnostic::error(
                    "console.runtime_timeout",
                    "editor",
                    "console.runtime_timeout",
                )
                .detail(detail);
                report.entries.push(
                    RuntimeConsoleEntry::from_diagnostic(&diagnostic)
                        .with_lines(lines)
                        .with_at_ms_if_zero(child.created.elapsed().as_millis() as u64),
                );
                report.notice = Some(RuntimeNotice::from_diagnostic(diagnostic));
            }
            if child
                .stopping
                .is_some_and(|time| time.elapsed() > Duration::from_secs(2))
            {
                let _ = child.child.kill();
            }
        }
        let mut index = 0;
        while index < self.children.len() {
            let ended = self.children[index].child.try_wait();
            let status = match ended {
                Ok(Some(status)) => Some(status),
                Ok(None) => {
                    index += 1;
                    continue;
                }
                Err(_) => None,
            };
            let mut child = self.children.remove(index);
            let success = status.as_ref().is_some_and(|status| status.success());
            child.exit_code = status.as_ref().and_then(|status| status.code());
            let intentional_stop = child.stopping.is_some() && !child.connection_failed;
            if !child.timeout_reported
                && !matches!(
                    child.status.phase,
                    RuntimePhase::Stopped | RuntimePhase::Failed
                )
            {
                child.status.phase = if success || intentional_stop {
                    RuntimePhase::Stopped
                } else {
                    RuntimePhase::Failed
                };
            }
            if !child.timeout_reported {
                let failed = child.status.phase == RuntimePhase::Failed;
                let tail_lines = child.log_paths.collect_tail(LOG_TAIL_LIMIT).console_lines();
                let severity = if !failed {
                    RuntimeDiagnosticSeverity::Info
                } else {
                    RuntimeDiagnosticSeverity::Error
                };
                let exit_code = child
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unavailable".to_string());
                let diagnostic = RuntimeDiagnostic::new(
                    "console.runtime_exited",
                    severity,
                    "runtime",
                    "console.runtime_exited",
                )
                .detail(format!("exit code: {exit_code}"));
                let mut lines = Vec::new();
                if let Some(last) = &child.last_diagnostic {
                    lines.push(format!("last diagnostic: {}", last.code));
                }
                lines.extend(tail_lines);
                report.entries.push(
                    RuntimeConsoleEntry::from_diagnostic(&diagnostic)
                        .with_lines(lines)
                        .with_at_ms_if_zero(child.created.elapsed().as_millis() as u64),
                );
                if failed {
                    report.notice = Some(RuntimeNotice::from_diagnostic(diagnostic));
                }
            }
            self.finished = Some(child.status.clone());
            if self.selected == Some(child.status.instance) {
                self.selected = self.children.last().map(|c| c.status.instance);
            }
        }
        report
    }
    pub fn finish_in_place(&mut self) {
        if let Some(mut session) = self.in_place.take() {
            session.control(RuntimeControl::Stop);
            self.finished = Some(session.status().clone());
        }
        self.selected = None;
    }
    pub fn stop_all(&mut self) {
        self.finish_in_place();
        for child in &mut self.children {
            if let Some(peer) = &mut child.peer {
                let _ = peer.send(RuntimePacket::Control(RuntimeControl::Stop));
                let _ = peer.poll();
            }
        }
        self.children.clear();
    }
}

fn push_diagnostic(entries: &mut Vec<RuntimeConsoleEntry>, entry: RuntimeConsoleEntry) {
    // Keep the flood guard for stage noise but never let it hide an error.
    let limit = if entry.level == LogLevel::Info {
        MAX_POLL_DIAGNOSTICS
    } else {
        MAX_POLL_DIAGNOSTICS * 2
    };
    if entries.len() < limit {
        entries.push(entry);
    }
}

impl Drop for NativeRuntimeController {
    fn drop(&mut self) {
        self.stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_event_keeps_code_severity_source_and_detail() {
        let diagnostic = RuntimeDiagnostic::error(
            "console.runtime_timeout",
            "runtime",
            "console.runtime_timeout",
        )
        .detail("after 20s")
        .at_ms(123);
        let entry = RuntimeConsoleEntry::from_diagnostic(&diagnostic);

        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.code, "console.runtime_timeout");
        assert_eq!(entry.source, "runtime");
        assert_eq!(entry.message, "console.runtime_timeout");
        assert_eq!(entry.detail.as_deref(), Some("after 20s"));
        assert_eq!(entry.at_ms, 123);
    }

    #[test]
    fn legacy_diagnostic_routes_as_info_with_a_timestamp_slot() {
        let entry = RuntimeConsoleEntry::legacy("legacy runtime message").with_at_ms_if_zero(250);

        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.code, "console.runtime");
        assert_eq!(entry.source, "runtime");
        assert_eq!(entry.message, "legacy runtime message");
        assert_eq!(entry.at_ms, 250);
    }

    #[test]
    fn diagnostic_flood_stays_bounded_but_keeps_errors() {
        let mut entries = Vec::new();
        for index in 0..MAX_POLL_DIAGNOSTICS {
            push_diagnostic(
                &mut entries,
                RuntimeConsoleEntry::info(
                    "console.runtime",
                    "runtime",
                    format!("stage {index}"),
                    None,
                ),
            );
        }
        push_diagnostic(
            &mut entries,
            RuntimeConsoleEntry::error(
                "console.runtime_timeout",
                "editor",
                "console.runtime_timeout",
                None,
            ),
        );

        assert!(entries.len() <= MAX_POLL_DIAGNOSTICS * 2);
        assert!(entries.iter().any(|entry| entry.level == LogLevel::Error));
    }

    #[test]
    fn launch_failure_carries_the_same_diagnostic_to_console_and_banner() {
        let failure =
            RuntimeLaunchFailure::from("player executable not found: C:/build/aurarafi-player.exe");
        let entry = failure.console_entry();
        let notice = failure.notice();

        assert_eq!(entry.code, "console.runtime_launch_failed");
        assert_eq!(notice.message(), "console.runtime_launch_failed");
        assert_eq!(
            notice.detail(),
            Some("player executable not found: C:/build/aurarafi-player.exe")
        );
    }
}
