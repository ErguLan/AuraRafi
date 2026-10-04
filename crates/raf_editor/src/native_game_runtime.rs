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
use raf_runtime::wire::{RuntimePacket, RuntimePeer};
use raf_runtime::{RuntimeControl, RuntimePhase, RuntimeStatus};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;

struct ChildRuntime {
    child: Child,
    listener: TcpListener,
    peer: Option<RuntimePeer>,
    authenticated: bool,
    token: Uuid,
    status: RuntimeStatus,
    temporary_directory: PathBuf,
    created: Instant,
    stopping: Option<Instant>,
}
impl Drop for ChildRuntime {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // Only the owned snapshot is deleted. Never recurse into a user project.
        let _ = std::fs::remove_file(self.temporary_directory.join("snapshot.ron"));
        let _ = std::fs::remove_dir(&self.temporary_directory);
    }
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
    ) -> Result<Uuid, String> {
        if project.project_type != ProjectType::Game {
            return Err("local Play is available only in Game projects".into());
        }
        if self.count() >= settings.runtime.normalized().max_instances as usize {
            return Err("runtime instance limit reached".into());
        }
        if self.in_place.is_some() {
            return Err("an in-place runtime is already active".into());
        }
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
        let safe_settings = RuntimeHostSettings::from_engine(settings);
        let manifest = RuntimeManifest {
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
        if settings.runtime.launch_mode == RuntimeLaunchMode::SameWindow {
            self.in_place = Some(PlayerSession::new(
                host,
                PlayerSource::Snapshot(Box::new(manifest)),
                safe_settings,
                instance,
            ));
        } else {
            let directory = std::env::temp_dir().join(format!("raf-runtime-{instance}"));
            std::fs::create_dir(&directory)
                .map_err(|e| format!("runtime temporary directory: {e}"))?;
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
                let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
                command
                    .arg("--runtime")
                    .arg(serde_json::to_string(&launch).map_err(|e| e.to_string())?)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                let child = command
                    .spawn()
                    .map_err(|e| format!("start local runtime: {e}"))?;
                Ok::<_, String>(ChildRuntime {
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
                    created: Instant::now(),
                    stopping: None,
                })
            })();
            match result {
                Ok(child) => self.children.push(child),
                Err(error) => {
                    let _ = std::fs::remove_file(snapshot);
                    let _ = std::fs::remove_dir(directory);
                    return Err(error);
                }
            }
        }
        self.selected = Some(instance);
        self.finished = None;
        Ok(instance)
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
    /// Bounded polling; the native event loop schedules it while children exist.
    pub fn poll(&mut self) -> Vec<String> {
        let mut diagnostics = Vec::new();
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
                                    diagnostics
                                        .push("Rejected unauthenticated runtime connection".into());
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
                                RuntimePacket::Diagnostic(message) => {
                                    if diagnostics.len() < 64 {
                                        diagnostics.push(format!(
                                            "Runtime {}: {message}",
                                            child.status.instance
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(error) => {
                        child.stopping.get_or_insert_with(Instant::now);
                        if child.status.phase != RuntimePhase::Stopped
                            && diagnostics.len() < 64
                            && child.peer.is_some()
                        {
                            diagnostics.push(format!("Runtime connection: {error}"));
                        }
                        child.peer = None;
                    }
                }
            }
            if (!child.authenticated && child.created.elapsed() > Duration::from_secs(30))
                || child
                    .stopping
                    .is_some_and(|time| time.elapsed() > Duration::from_secs(2))
            {
                let _ = child.child.kill();
            }
        }
        let mut index = 0;
        while index < self.children.len() {
            let ended = match self.children[index].child.try_wait() {
                Ok(Some(status)) => Some(status.success()),
                Err(_) => Some(false),
                Ok(None) => None,
            };
            if let Some(success) = ended {
                let mut child = self.children.remove(index);
                if child.status.phase != RuntimePhase::Stopped {
                    child.status.phase = if success || child.stopping.is_some() {
                        RuntimePhase::Stopped
                    } else {
                        RuntimePhase::Failed
                    };
                }
                self.finished = Some(child.status.clone());
                if self.selected == Some(child.status.instance) {
                    self.selected = self.children.last().map(|c| c.status.instance);
                }
                diagnostics.push(format!(
                    "Runtime {} ended: {:?}",
                    child.status.instance, child.status.phase
                ));
            } else {
                index += 1;
            }
        }
        diagnostics
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
impl Drop for NativeRuntimeController {
    fn drop(&mut self) {
        self.stop_all();
    }
}
