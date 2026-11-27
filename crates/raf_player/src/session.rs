//! Shared native host lifecycle for separate-window and in-place Play.
use crate::audio::RuntimeAudio;
use crate::loading::{failure_diagnostic, stage_diagnostic, PreparationLoading};
use crate::surface::player_surface;
use raf_core::{i18n::t, InputKey, InputOwner, InputRegionId, InputRouter, InputSnapshot};
use raf_render::api_graphic_basic::device::{BasicBackendType, BasicDevice};
use raf_render::api_graphic_basic::frame_scheduler::{
    DynamicResolutionController, FrameActivity, FramePacingBudget,
};
use raf_render::api_graphic_basic::ui_surface::loading::{
    build_loading_surface, build_runtime_loading_surface_with_options, insert_loading_brand,
    RuntimeLoadingView,
};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeUiInputBridge, NativeUiWindowHost, StudioUiPalette, UiAction,
};
use raf_render::api_graphic_basic::{
    CanvasTargetRect, EditorCanvasLayer, EditorCompositorMetrics, EditorUiLayer,
    NativeEditorCompositor,
};
use raf_render::{
    camera::{Camera, CameraMode},
    scene_renderer::{RenderOptions, SceneRenderer},
};
use raf_runtime::manifest::{RuntimeHostSettings, RuntimeManifest};
use raf_runtime::wire::RuntimeDiagnostic;
use raf_runtime::{RuntimeControl, RuntimePhase, RuntimeStatus, RuntimeWorld};
use raf_ui::{UiColorMode, UiDocument, UiEnvironment, UiRect};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, RecvTimeoutError},
    Arc,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// How long a deterministic caller blocks on preparation before trying again.
/// Preparation itself is a real worker; this is a channel wait, not a frame
/// pacing sleep.
const PREPARATION_WAIT_SLICE: Duration = Duration::from_millis(100);
/// Frames excluded from the rolling diagnostics window. The first frames of a
/// session include asset decode, pipeline creation, and the world's first
/// script tick; averaging them into steady-state numbers hides real progress.
const DIAGNOSTIC_WARMUP_FRAMES: u32 = 10;
#[derive(Clone)]
pub enum PlayerSource {
    Snapshot(Box<RuntimeManifest>),
    File(PathBuf),
    Session(Box<raf_runtime::session_loader::SessionSource>, Uuid),
}
enum Preparation {
    Stage(&'static str),
    Compile(raf_script::ScriptLoadProgress),
    Ready(
        Box<RuntimeWorld>,
        Option<UiDocument>,
        PathBuf,
        bool,
        Box<raf_runtime::session_loader::SessionSource>,
    ),
    Failed(Vec<String>),
}
#[derive(Clone, PartialEq)]
struct SurfaceKey {
    phase: RuntimePhase,
    size: [u32; 2],
    camera: bool,
    error: Option<String>,
    debug_interval: u64,
    debug_text: Option<String>,
}
/// Retained loading document changes only when one of these facts changes.
#[derive(Clone, PartialEq)]
struct LoadingSurfaceKey {
    status: String,
    detail: String,
    count: String,
    progress: u32,
    elapsed_secs: u64,
    history: Vec<String>,
    slow: bool,
    failed: bool,
    error: Option<String>,
    can_retry: bool,
}
/// Physical and logical frame geometry resolved once per presented frame.
struct FramePresentation {
    size: [u32; 2],
    scale: f32,
    logical: [u32; 2],
    environment: UiEnvironment,
}
pub struct PlayerSession {
    cursor: raf_render::api_graphic_basic::ui_surface::NativeCursorLease,
    context: Option<raf_runtime::session_loader::SessionSource>,
    transition_previous: Option<RuntimePhase>,
    cursor_suspended: bool,
    source: Option<PlayerSource>,
    receiver: Option<Receiver<Preparation>>,
    cancellation: Arc<AtomicBool>,
    pub settings: RuntimeHostSettings,
    instance: Uuid,
    /// Native API label the host presents with, reported by diagnostics.
    api_label: &'static str,
    world: Option<RuntimeWorld>,
    ui: DirectUiSurfaceHost,
    game_ui: crate::game_ui::GameUiPresenter,
    router: InputRouter,
    renderer: Option<SceneRenderer>,
    device: Option<BasicDevice>,
    audio: RuntimeAudio,
    root: Option<PathBuf>,
    status: RuntimeStatus,
    surface_key: Option<SurfaceKey>,
    cached_canvas: Option<(
        u64,
        [u32; 4],
        raf_core::runtime_config::RuntimeCameraPose,
        EditorCanvasLayer,
    )>,
    resolution: DynamicResolutionController,
    focused: bool,
    last_frame: Instant,
    last_render_ms: f64,
    last_diagnostics: Vec<String>,
    last_diagnostic_events: Vec<RuntimeDiagnostic>,
    startup_presented: bool,
    has_entered_game: bool,
    retry_source: Option<PlayerSource>,
    loading: PreparationLoading,
    loading_surface_key: Option<LoadingSurfaceKey>,
    diagnostic_frames: u32,
    diagnostic_step_ms: f64,
    diagnostic_render_ms: f64,
    diagnostic_world_ms: f64,
    diagnostic_ui_sync_ms: f64,
    diagnostic_scene_build_ms: f64,
    diagnostic_scene_rebuilds: u32,
    diagnostic_ui_rebuilds: u32,
    diagnostic_uploads: u64,
    diagnostic_warmup_left: u32,
    diagnostic_max_step_ms: f64,
    diagnostic_slow_frames: u32,
    last_compositor_metrics: EditorCompositorMetrics,
    last_scene_stats: raf_render::scene_renderer::FrameStats,
}

impl PlayerSession {
    fn scene_renderer_stats(&self) -> (u32, u32) {
        (
            self.last_scene_stats.total_entities,
            self.last_scene_stats.visible_entities,
        )
    }
}
impl PlayerSession {
    pub fn new(
        host: &NativeUiWindowHost,
        source: PlayerSource,
        settings: RuntimeHostSettings,
        instance: Uuid,
    ) -> Self {
        let palette = palette(settings.theme);
        let language = settings.language;
        let loading = PreparationLoading::new("runtime.loading_snapshot", language);
        let status = t(&loading.stage_key, language);
        let mut ui = host.graphics_context().create_ui_host(
            build_loading_surface(
                palette,
                0.0,
                language,
                &t("runtime.cancel_hint", language),
                &status,
                "",
            ),
            [8, 11, 15, 255],
        );
        insert_loading_brand(&mut ui);
        Self {
            cursor: host.cursor_lease(),
            context: None,
            transition_previous: None,
            cursor_suspended: false,
            source: Some(source),
            receiver: None,
            cancellation: Arc::new(AtomicBool::new(false)),
            audio: RuntimeAudio::new(false, Default::default()),
            settings,
            instance,
            api_label: host.api_label(),
            world: None,
            ui,
            game_ui: Default::default(),
            router: InputRouter::default(),
            renderer: None,
            device: None,
            root: None,
            status: RuntimeStatus {
                instance,
                phase: RuntimePhase::Preparing,
                fixed_ticks: 0,
                elapsed_seconds: 0.0,
                script_instances: 0,
                camera_ready: false,
                last_error: None,
            },
            surface_key: None,
            cached_canvas: None,
            resolution: DynamicResolutionController::default(),
            focused: true,
            last_frame: Instant::now(),
            last_render_ms: 0.0,
            last_diagnostics: Vec::new(),
            last_diagnostic_events: Vec::new(),
            startup_presented: false,
            has_entered_game: false,
            retry_source: None,
            loading,
            loading_surface_key: None,
            diagnostic_frames: 0,
            diagnostic_step_ms: 0.0,
            diagnostic_render_ms: 0.0,
            diagnostic_world_ms: 0.0,
            diagnostic_ui_sync_ms: 0.0,
            diagnostic_scene_build_ms: 0.0,
            diagnostic_scene_rebuilds: 0,
            diagnostic_ui_rebuilds: 0,
            diagnostic_uploads: 0,
            diagnostic_warmup_left: DIAGNOSTIC_WARMUP_FRAMES,
            diagnostic_max_step_ms: 0.0,
            diagnostic_slow_frames: 0,
            last_compositor_metrics: EditorCompositorMetrics::default(),
            last_scene_stats: raf_render::scene_renderer::FrameStats::default(),
        }
    }
    pub fn status(&self) -> &RuntimeStatus {
        &self.status
    }
    /// Active scene backend label for diagnostics and reports.
    ///
    /// A native handoff reports the API the compositor presents with; the
    /// recovery path reports its software backend.
    pub fn backend_name(&self) -> &'static str {
        match self.device.as_ref().map(BasicDevice::backend) {
            // Only the named CPU variant is matched; a native handoff is
            // reported with the API label the host actually presents with.
            Some(BasicBackendType::CpuSoftware) => BasicBackendType::CpuSoftware.label(),
            Some(_) => self.api_label,
            None => "uninitialized",
        }
    }
    /// CPU cost of the last composed presentation frame in milliseconds.
    pub fn last_render_millis(&self) -> f64 {
        self.last_render_ms
    }
    pub fn is_preparing(&self) -> bool {
        self.status.phase == RuntimePhase::Preparing
    }
    fn is_loading_failed(&self) -> bool {
        self.loading.failed || self.status.phase == RuntimePhase::Failed
    }
    pub fn should_close(&self) -> bool {
        self.status.phase == RuntimePhase::Stopped
    }
    pub fn entered_game(&mut self) -> bool {
        if self.world.is_some() && !self.has_entered_game {
            self.has_entered_game = true;
            return true;
        }
        false
    }
    pub fn set_focused(&mut self, focused: bool) {
        if !focused {
            self.cursor.release();
            self.cursor_suspended = true;
        }
        self.focused = focused;
        self.router.cancel_all();
        if let Some(world) = &mut self.world {
            world.set_focused(focused);
            self.status = world.status().clone();
        }
        self.audio
            .set_paused(self.status.phase == RuntimePhase::Paused);
        self.last_frame = Instant::now();
    }
    pub fn control(&mut self, control: RuntimeControl) {
        if control != RuntimeControl::Reload {
            self.cursor.release();
            self.cursor_suspended = true;
        }
        if control == RuntimeControl::Stop {
            let failed = self.loading.failed || self.status.phase == RuntimePhase::Failed;
            let already_stopped = self.status.phase == RuntimePhase::Stopped;
            self.receiver = None;
            self.source = None;
            self.retry_source = None;
            self.game_ui.clear();
            if let Some(world) = &mut self.world {
                world.stop();
                self.last_diagnostics.extend(world.drain_diagnostics());
            }
            self.cancellation.store(true, Ordering::Relaxed);
            self.status.phase = self
                .world
                .as_ref()
                .map_or(RuntimePhase::Stopped, |world| world.status().phase);
            // Cancel on the failure surface is a close request even when the
            // previous world is still shutting down its storage worker. A
            // session that already closed for the host must stay closed.
            if failed || already_stopped {
                self.status.phase = RuntimePhase::Stopped;
            }
            self.loading.failed = false;
            self.loading.slow = false;
            self.loading_surface_key = None;
            self.audio.stop();
            self.cached_canvas = None;
            return;
        }
        if let Some(world) = &mut self.world {
            world.control(control);
            self.status = world.status().clone();
        }
        self.audio
            .set_paused(self.status.phase == RuntimePhase::Paused);
        self.last_frame = Instant::now();
    }
    pub fn drain_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.last_diagnostics)
    }
    /// Structured preparation events for the editor console and runtime log.
    pub fn drain_diagnostic_events(&mut self) -> Vec<RuntimeDiagnostic> {
        std::mem::take(&mut self.last_diagnostic_events)
    }
    fn push_diagnostic_event(&mut self, diagnostic: RuntimeDiagnostic) {
        if self.last_diagnostic_events.len() < 256 {
            self.last_diagnostic_events.push(diagnostic);
        }
    }
    pub fn frame_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(if self.is_preparing() || self.is_loading_failed() {
            1.0 / 30.0
        } else if self
            .world
            .as_ref()
            .is_some_and(|w| w.has_active_simulation())
            || self.game_ui.has_active_motion()
        {
            1.0 / self.settings.runtime.fps_limit.clamp(15, 120) as f64
        } else {
            0.25
        })
    }
    fn start_worker(&mut self) {
        let Some(source) = self.source.take() else {
            return;
        };
        // Keep one source description for Retry. The worker owns the prepared
        // copy; this clone is dropped as soon as preparation succeeds.
        self.retry_source = Some(source.clone());
        let language = self.settings.language;
        let origin = self.loading.origin_key;
        self.loading.begin(origin, language);
        let stage_message = t(origin, language);
        self.push_diagnostic_event(stage_diagnostic(&stage_message, origin, 0));
        tracing::info!(
            stage = %origin,
            progress = 0.0_f64,
            path = %self.loading.detail,
            "runtime preparation started"
        );
        self.loading_surface_key = None;
        let (sender, receiver) = mpsc::sync_channel(32);
        self.receiver = Some(receiver);
        let cancellation = self.cancellation.clone();
        let instance = self.instance;
        let settings = self.settings.runtime.clone();
        // Detached workers own only the snapshot and a cancellation flag. Dropping
        // the receiver releases their bounded channel; they never touch editor state.
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Preparation, Vec<String>> {
                let manifest = match source {
                    PlayerSource::Snapshot(manifest) => *manifest,
                    PlayerSource::File(path) => RuntimeManifest::read(&path).map_err(|e| vec![e])?,
                    PlayerSource::Session(context,id) => context.load(id,instance).map_err(|e| vec![e])?,
                };
                if manifest.instance != instance { return Err(vec!["runtime snapshot instance mismatch".into()]); }
                let context = raf_runtime::session_loader::SessionSource::from_manifest(&manifest);
                let session_id = manifest.session_id;
                let storage = raf_runtime::storage::StorageConfig {
                    project: manifest.project_id, project_root: manifest.project_root.clone(),
                    custom_root: manifest.save_root_grant.clone(), development_slot: manifest.development_slot,
                    policy: manifest.project_settings.runtime.progress.clone(),
                };
                let root = manifest.project_root;
                let enable_audio = manifest.project_settings.enable_audio;
                let ui_path = manifest.ui_document_file;
                let mut world = RuntimeWorld::prepare(instance, manifest.scene, &root, manifest.project_settings,
                    settings, cancellation.clone(), |progress| { let _ = sender.try_send(Preparation::Compile(progress)); })?;
                world.configure_storage(storage);
                world.services_mut().current_session = session_id;
                world.services_mut().transition_status = "ready";
                if let Some(graph) = &manifest.node_graph {
                    let _ = sender.try_send(Preparation::Stage("runtime.compiling_nodes"));
                    world.attach_graph(graph)?;
                }
                let _ = sender.try_send(Preparation::Stage("runtime.loading_ui"));
                let document = load_ui_document(&root, ui_path)?;
                if let Some(document) = &document {
                    if document.space == raf_ui::UiDocumentSpace::Camera {
                        let binding = document.camera_binding.as_ref().ok_or_else(|| vec!["camera-space UI has no camera binding".into()])?;
                        let matched = world.scene.iter().any(|(id,node)| world.scene.is_valid_node(id) && node.game_camera.is_some() &&
                            (binding.camera_key == node.uuid.to_string() || binding.camera_key == node.name));
                        if !matched || binding.document_id != document.id {
                            return Err(vec!["camera-space UI must reference an authored camera and the same document ID".into()]);
                        }
                    }
                }
                if cancellation.load(Ordering::Relaxed) { return Err(vec!["Runtime preparation cancelled".into()]); }
                let _ = sender.try_send(Preparation::Stage("runtime.initializing"));
                Ok(Preparation::Ready(Box::new(world), document, root, enable_audio, Box::new(context)))
            })).unwrap_or_else(|_| Err(vec!["Runtime preparation panicked; launch was cancelled".into()]));
            let message = match result {
                Ok(message) => message,
                Err(errors) => Preparation::Failed(errors),
            };
            let _ = sender.send(message);
        });
    }
    fn poll_preparation(&mut self, host: &NativeUiWindowHost) {
        let messages: Vec<_> = self
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().take(32).collect())
            .unwrap_or_default();
        for message in messages {
            self.apply_preparation(message, host);
        }
    }
    /// Applies one preparation message. Event-driven hosts poll; deterministic
    /// headless callers block on the same worker and reuse this transition.
    fn apply_preparation(&mut self, message: Preparation, host: &NativeUiWindowHost) {
        match message {
            Preparation::Stage(key) => {
                let language = self.settings.language;
                let previous = self.loading.stage_key.clone();
                self.loading.stage(key, language);
                if previous != key {
                    let message = t(key, language);
                    let at_ms = self.loading.started.elapsed().as_millis() as u64;
                    tracing::info!(
                        stage = %key,
                        progress = self.loading.progress as f64,
                        path = %self.loading.detail,
                        "runtime preparation stage"
                    );
                    self.push_diagnostic_event(stage_diagnostic(&message, key, at_ms));
                }
                self.loading_surface_key = None;
            }
            Preparation::Compile(progress) => {
                let language = self.settings.language;
                let entering = self.loading.stage_key != "runtime.compiling";
                self.loading
                    .compile(&progress.path, progress.completed, progress.total, language);
                if entering {
                    let message = t("runtime.compiling", language);
                    let at_ms = self.loading.started.elapsed().as_millis() as u64;
                    tracing::info!(
                        stage = "runtime.compiling",
                        progress = self.loading.progress as f64,
                        path = %self.loading.detail,
                        "runtime preparation stage"
                    );
                    self.push_diagnostic_event(stage_diagnostic(
                        &message,
                        "runtime.compiling",
                        at_ms,
                    ));
                } else {
                    tracing::debug!(
                        stage = "runtime.compiling",
                        progress = self.loading.progress as f64,
                        path = %self.loading.detail,
                        "runtime preparation progress"
                    );
                }
                self.loading_surface_key = None;
            }
            Preparation::Ready(mut world, document, root, audio, context) => {
                if let Some(document) = &document {
                    if let Err(error) = world.game_ui_mut().mount_document(document) {
                        self.fail_preparation(vec![error]);
                        return;
                    }
                }
                if self.transition_previous.is_some() {
                    if let Some(previous) = &self.world {
                        world.services_mut().progress = previous.services().progress.clone();
                    }
                }
                // Bounded on_start runs against the candidate before committing it.
                // No disk requests are polled until the candidate becomes the live world.
                world.start();
                if self.transition_previous.is_some() && world.status().last_error.is_some() {
                    self.fail_preparation(vec![world
                        .status()
                        .last_error
                        .clone()
                        .unwrap_or_default()]);
                    return;
                }
                if let Some(previous) = &mut self.world {
                    world.adopt_storage_worker(previous);
                    previous.stop();
                    self.audio.stop();
                }
                self.context = Some(*context);
                self.transition_previous = None;
                world.activate_services();
                world.set_focused(self.focused);
                self.cached_canvas = None;
                self.surface_key = None;
                self.router.cancel_all();
                let mut renderer = SceneRenderer::new(1, 1);
                renderer.set_asset_root(Some(root.join("assets")));
                // Decode the visible city textures while the loading surface is
                // still on screen. The first frame then reuses the residency
                // cache instead of paying PNG decode on the transition.
                let quality = world.presentation_quality();
                renderer.prewarm_scene_textures(&world.scene, quality.texture_quality);
                self.renderer = Some(renderer);
                // A CpuOnly policy resolves to the CPU recovery rasterizer;
                // the window itself still presents through the AGB host.
                self.device = Some(BasicDevice::new(crate::graphics::device_config(
                    self.settings.render_execution_policy,
                    host.size(),
                )));
                self.game_ui.clear();
                self.status = world.status().clone();
                let audio_settings = world.audio_settings().clone();
                self.world = Some(*world);
                // A headless host never opens an audio output device.
                self.audio = RuntimeAudio::new(audio && !host.is_headless(), audio_settings);
                self.root = Some(root);
                self.receiver = None;
                self.retry_source = None;
                self.loading.finish(self.settings.language);
                self.loading_surface_key = None;
                self.last_frame = Instant::now();
            }
            Preparation::Failed(errors) => {
                self.fail_preparation(errors);
            }
        }
    }
    /// Blocks until the background preparation worker reports a terminal
    /// state. Deterministic hosts use this before their fixed frame loop; the
    /// native host reaches the same state by polling during loading frames.
    pub fn finish_preparation(&mut self, host: &NativeUiWindowHost) -> Result<(), String> {
        self.start_worker();
        while self.is_preparing() {
            self.poll_preparation(host);
            if !self.is_preparing() {
                break;
            }
            let message = {
                let Some(receiver) = self.receiver.as_ref() else {
                    return Err("runtime preparation ended without a world".to_string());
                };
                receiver.recv_timeout(PREPARATION_WAIT_SLICE)
            };
            match message {
                Ok(message) => self.apply_preparation(message, host),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("runtime preparation worker disconnected".to_string());
                }
            }
        }
        if self.status.phase == RuntimePhase::Failed {
            return Err(self
                .status
                .last_error
                .clone()
                .unwrap_or_else(|| "runtime preparation failed".to_string()));
        }
        Ok(())
    }
    fn fail_preparation(&mut self, errors: Vec<String>) {
        let message = errors
            .iter()
            .find(|error| !error.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "Runtime preparation failed".to_string());
        let surface_message = {
            let joined = errors
                .iter()
                .filter(|error| !error.trim().is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            if joined.is_empty() {
                message.clone()
            } else {
                joined
            }
        };
        let stage = self.loading.stage_key.clone();
        let path = if self.loading.detail == t("runtime.cancel_hint", self.settings.language) {
            String::new()
        } else {
            self.loading.detail.clone()
        };
        let at_ms = self.loading.started.elapsed().as_millis() as u64;
        self.loading.fail(surface_message);

        if let Some(phase) = self.transition_previous.take() {
            if let Some(world) = &mut self.world {
                world.transition_failed(message.clone());
                if phase == RuntimePhase::Running {
                    world.control(RuntimeControl::Resume);
                }
                world.set_focused(self.focused);
                self.status = world.status().clone();
            }
        } else {
            self.status.phase = RuntimePhase::Failed;
        }
        self.status.last_error = Some(message.clone());
        self.last_diagnostics.extend(
            errors
                .into_iter()
                .take(256usize.saturating_sub(self.last_diagnostics.len())),
        );
        tracing::error!(
            stage = %stage,
            progress = self.loading.progress as f64,
            path = %path,
            error = %message,
            "runtime preparation failed"
        );
        self.push_diagnostic_event(failure_diagnostic(&message, &stage, &path, at_ms));
        self.receiver = None;
        self.source = None;
        self.router.cancel_all();
        self.surface_key = None;
        self.loading_surface_key = None;
        self.last_frame = Instant::now();
    }
    fn begin_transition(&mut self, id: Uuid) {
        let Some(context) = self.context.clone() else {
            return;
        };
        let Some(world) = &mut self.world else {
            return;
        };
        if !matches!(
            world.status().phase,
            RuntimePhase::Running | RuntimePhase::Paused
        ) {
            return;
        }
        if world.services().progress.busy() {
            world.transition_failed("runtime.progress_busy_finish_before_transition".into());
            return;
        }
        self.transition_previous = Some(world.status().phase);
        world.control(RuntimeControl::Pause);
        world.services_mut().transition_status = "loading";
        self.audio.set_paused(true);
        self.cursor.release();
        let language = self.settings.language;
        self.source = Some(PlayerSource::Session(Box::new(context), id));
        self.status.phase = RuntimePhase::Preparing;
        self.status.last_error = None;
        self.loading.begin("runtime.loading_session", language);
        self.startup_presented = false;
        self.surface_key = None;
        self.loading_surface_key = None;
    }
    /// Advances one event-driven frame. Delta time comes from the host clock.
    pub fn draw(
        &mut self,
        host: &mut NativeUiWindowHost,
        compositor: &mut NativeEditorCompositor,
        input: &NativeUiInputBridge,
    ) -> Result<(), String> {
        self.poll_preparation(host);
        if self.is_loading_failed() {
            let view = self.frame_view(host, input.snapshot());
            if self.should_close() {
                return Ok(());
            }
            return self.present_failure(host, compositor, input, &view);
        }
        if self.is_preparing() {
            // The shared step owns preparation presentation and worker
            // start-up so deterministic hosts that call it directly keep
            // identical behavior.
            return self.step_with_snapshot(host, compositor, input, input.snapshot(), 0.0);
        }
        if self.should_close() {
            return Ok(());
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        self.step(host, compositor, input, dt)
    }

    /// Advances one simulation and presentation frame with an injected delta.
    /// Native hosts pass the elapsed frame time; deterministic callers pass an
    /// exact step so no wall clock can influence the simulated time.
    pub fn step(
        &mut self,
        host: &mut NativeUiWindowHost,
        compositor: &mut NativeEditorCompositor,
        input: &NativeUiInputBridge,
        dt: f64,
    ) -> Result<(), String> {
        self.step_with_snapshot(host, compositor, input, input.snapshot(), dt)
    }

    /// Deterministic frame entry point: the caller owns the input snapshot and
    /// the delta that reaches `RuntimeWorld::advance`. Preparation is polled
    /// here so a mid-session transition also progresses without an event loop.
    pub fn step_with_snapshot(
        &mut self,
        host: &mut NativeUiWindowHost,
        compositor: &mut NativeEditorCompositor,
        input: &NativeUiInputBridge,
        snapshot: &InputSnapshot,
        dt: f64,
    ) -> Result<(), String> {
        self.poll_preparation(host);
        if self.is_loading_failed() {
            let view = self.frame_view(host, snapshot);
            if self.should_close() {
                return Ok(());
            }
            return self.present_failure(host, compositor, input, &view);
        }
        if self.is_preparing() {
            let view = self.frame_view(host, snapshot);
            if self.should_close() {
                return Ok(());
            }
            return self.present_loading(host, compositor, input, &view);
        }
        let view = self.frame_view(host, snapshot);
        if self.should_close() {
            return Ok(());
        }
        let started = Instant::now();
        let result = self.step_inner(host, compositor, input, snapshot, dt, view);
        self.record_diagnostic_frame(started.elapsed(), compositor.metrics());
        result
    }

    fn record_diagnostic_frame(&mut self, elapsed: Duration, metrics: EditorCompositorMetrics) {
        self.last_compositor_metrics = metrics;
        if !self.settings.runtime.diagnostics {
            return;
        }
        if self.diagnostic_warmup_left > 0 {
            self.diagnostic_warmup_left -= 1;
            return;
        }
        let step_ms = elapsed.as_secs_f64() * 1000.0;
        self.diagnostic_frames = self.diagnostic_frames.saturating_add(1);
        self.diagnostic_step_ms += step_ms;
        self.diagnostic_max_step_ms = self.diagnostic_max_step_ms.max(step_ms);
        if step_ms > 50.0 {
            self.diagnostic_slow_frames = self.diagnostic_slow_frames.saturating_add(1);
        }
        self.diagnostic_render_ms += self.last_render_ms;
        self.diagnostic_scene_rebuilds += metrics.scene_packet_rebuilds;
        self.diagnostic_ui_rebuilds += metrics.ui_packet_rebuilds;
        self.diagnostic_uploads = self.diagnostic_uploads.saturating_add(metrics.upload_bytes);
        if self.diagnostic_frames < 120 {
            return;
        }
        let frames = f64::from(self.diagnostic_frames);
        let textures = self
            .renderer
            .as_ref()
            .map(SceneRenderer::texture_ingress_metrics)
            .unwrap_or_default();
        let scene_metrics = self.scene_renderer_stats();
        tracing::info!(
            frames = self.diagnostic_frames,
            total_entities = scene_metrics.0,
            visible_entities = scene_metrics.1,
            texture_misses = textures.misses,
            texture_hits = textures.hits,
            texture_evictions = textures.evictions,
            texture_entries = textures.entries,
            texture_bytes = textures.resident_bytes,
            step_ms = self.diagnostic_step_ms / frames,
            max_step_ms = self.diagnostic_max_step_ms,
            slow_frames = self.diagnostic_slow_frames,
            render_ms = self.diagnostic_render_ms / frames,
            world_ms = self.diagnostic_world_ms / frames,
            ui_sync_ms = self.diagnostic_ui_sync_ms / frames,
            scene_build_ms = self.diagnostic_scene_build_ms / frames,
            presented_fps = 1000.0 / (self.diagnostic_step_ms / frames).max(0.001),
            scene_rebuilds = self.diagnostic_scene_rebuilds,
            ui_rebuilds = self.diagnostic_ui_rebuilds,
            upload_bytes = self.diagnostic_uploads,
            draw_calls = metrics.draw_calls,
            "runtime frame diagnostics"
        );
        self.diagnostic_frames = 0;
        self.diagnostic_step_ms = 0.0;
        self.diagnostic_max_step_ms = 0.0;
        self.diagnostic_slow_frames = 0;
        self.diagnostic_render_ms = 0.0;
        self.diagnostic_world_ms = 0.0;
        self.diagnostic_ui_sync_ms = 0.0;
        self.diagnostic_scene_build_ms = 0.0;
        self.diagnostic_scene_rebuilds = 0;
        self.diagnostic_ui_rebuilds = 0;
        self.diagnostic_uploads = 0;
    }

    /// Resolves the physical target into logical layout, environment state, and
    /// shared cursor intent. Both the loading and game paths use this preamble.
    fn frame_view(
        &mut self,
        host: &NativeUiWindowHost,
        snapshot: &InputSnapshot,
    ) -> FramePresentation {
        let size = host.size();
        let scale = host.scale_factor();
        let logical = [
            (size[0] as f32 / scale).max(1.0) as u32,
            (size[1] as f32 / scale).max(1.0) as u32,
        ];
        let mut environment = UiEnvironment::new(logical[0], logical[1]);
        environment.scale_factor = scale;
        environment.high_contrast = self.settings.high_contrast;
        environment.prefers_reduced_motion = self.settings.reduced_motion;
        environment.color_mode = if self.settings.theme == raf_core::config::Theme::Light {
            UiColorMode::Light
        } else {
            UiColorMode::Dark
        };
        self.ui.set_environment(environment);
        if snapshot.key_pressed(InputKey::Escape) {
            if self.is_preparing() || self.is_loading_failed() || snapshot.modifiers.shift {
                self.control(RuntimeControl::Stop);
            } else {
                self.cursor.release();
                self.cursor_suspended = true;
                if let Some(world) = &mut self.world {
                    world.services_mut().cursor = Default::default();
                    world.services_mut().effective_cursor =
                        raf_script::services::CursorCapture::Free;
                }
            }
        }
        FramePresentation {
            size,
            scale,
            logical,
            environment,
        }
    }

    /// Presents preparation progress with stage, count, path and elapsed time.
    fn present_loading(
        &mut self,
        host: &mut NativeUiWindowHost,
        compositor: &mut NativeEditorCompositor,
        input: &NativeUiInputBridge,
        view: &FramePresentation,
    ) -> Result<(), String> {
        let language = self.settings.language;
        let palette = palette(self.settings.theme);
        let now = Instant::now();
        let elapsed_secs = self.loading.elapsed_secs(now);
        if self.loading.update_slow(now) {
            tracing::warn!(
                stage = %self.loading.stage_key,
                progress = self.loading.progress as f64,
                path = %self.loading.detail,
                "runtime preparation is slow"
            );
            self.loading_surface_key = None;
        }
        let status = t(&self.loading.stage_key, language);
        let key = LoadingSurfaceKey {
            status: status.clone(),
            detail: self.loading.detail.clone(),
            count: self.loading.count.clone(),
            progress: self.loading.progress.to_bits(),
            elapsed_secs,
            history: self.loading.history.clone(),
            slow: self.loading.slow,
            failed: false,
            error: None,
            can_retry: false,
        };
        if self.loading_surface_key.as_ref() != Some(&key) {
            self.ui
                .set_surface(build_runtime_loading_surface_with_options(
                    RuntimeLoadingView {
                        palette,
                        progress: self.loading.progress,
                        language,
                        detail: &self.loading.detail,
                        status: &status,
                        count: &self.loading.count,
                        elapsed_secs,
                        history: &self.loading.history,
                        slow: self.loading.slow,
                        failed: false,
                        error: None,
                        can_retry: false,
                    },
                ));
            self.loading_surface_key = Some(key);
        }
        let actions = self.ui.process_routed_input(
            view.logical,
            view.scale.max(1.0),
            |key| t(key, language),
            input,
            &mut self.router,
            InputOwner::RetainedUi(InputRegionId(0x6c6f6164)),
            UiRect::new(0.0, 0.0, view.logical[0] as f32, view.logical[1] as f32),
        );
        if actions.iter().any(|action| {
            matches!(&action.action, UiAction::Command { name } if name == "runtime.cancel")
        }) {
            self.control(RuntimeControl::Stop);
        }
        if self.should_close() || self.loading.failed {
            return Ok(());
        }
        let render_started = Instant::now();
        host.render_editor_frame(
            compositor,
            None,
            &mut self.ui,
            view.logical,
            view.scale.max(1.0),
            |key| t(key, language),
        )
        .map_err(|e| format!("runtime loading presentation: {e}"))?;
        self.last_render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
        if !self.startup_presented {
            self.startup_presented = true;
            self.start_worker();
        }
        Ok(())
    }

    /// Presents a terminal preparation failure until Retry or Cancel is chosen.
    fn present_failure(
        &mut self,
        host: &mut NativeUiWindowHost,
        compositor: &mut NativeEditorCompositor,
        input: &NativeUiInputBridge,
        view: &FramePresentation,
    ) -> Result<(), String> {
        let language = self.settings.language;
        let palette = palette(self.settings.theme);
        let elapsed_secs = self.loading.elapsed_secs(Instant::now());
        let status = t(&self.loading.stage_key, language);
        let cancel_hint = t("runtime.cancel_hint", language);
        let detail = if self.loading.detail == cancel_hint {
            String::new()
        } else {
            self.loading.detail.clone()
        };
        let error = self
            .loading
            .error
            .clone()
            .or_else(|| self.status.last_error.clone());
        let can_retry = self.retry_source.is_some();
        let key = LoadingSurfaceKey {
            status: status.clone(),
            detail: detail.clone(),
            count: self.loading.count.clone(),
            progress: self.loading.progress.to_bits(),
            elapsed_secs,
            history: self.loading.history.clone(),
            slow: false,
            failed: true,
            error: error.clone(),
            can_retry,
        };
        if self.loading_surface_key.as_ref() != Some(&key) {
            self.ui
                .set_surface(build_runtime_loading_surface_with_options(
                    RuntimeLoadingView {
                        palette,
                        progress: self.loading.progress,
                        language,
                        detail: &detail,
                        status: &status,
                        count: &self.loading.count,
                        elapsed_secs,
                        history: &self.loading.history,
                        slow: false,
                        failed: true,
                        error: error.as_deref(),
                        can_retry,
                    },
                ));
            self.loading_surface_key = Some(key);
        }
        self.last_render_ms = 0.0;
        let actions = self.ui.process_routed_input(
            view.logical,
            view.scale.max(1.0),
            |key| t(key, language),
            input,
            &mut self.router,
            InputOwner::RetainedUi(InputRegionId(0x6c6f6164)),
            UiRect::new(0.0, 0.0, view.logical[0] as f32, view.logical[1] as f32),
        );
        for action in actions {
            if let UiAction::Command { name } = action.action {
                match name.as_str() {
                    "runtime.retry" => self.retry_preparation(),
                    "runtime.cancel" => self.control(RuntimeControl::Stop),
                    _ => {}
                }
            }
        }
        if self.should_close() || self.status.phase == RuntimePhase::Preparing {
            return Ok(());
        }
        let render_started = Instant::now();
        host.render_editor_frame(
            compositor,
            None,
            &mut self.ui,
            view.logical,
            view.scale.max(1.0),
            |key| t(key, language),
        )
        .map_err(|e| format!("runtime failure presentation: {e}"))?;
        self.last_render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
        Ok(())
    }

    fn retry_preparation(&mut self) {
        let Some(source) = self.retry_source.take() else {
            return;
        };
        let language = self.settings.language;
        let origin = self.loading.origin_key;
        // A new attempt must not inherit the previous worker's cancellation
        // flag or preparation facts.
        self.cancellation = Arc::new(AtomicBool::new(false));
        self.source = Some(source);
        self.receiver = None;
        self.status = RuntimeStatus {
            instance: self.instance,
            phase: RuntimePhase::Preparing,
            fixed_ticks: 0,
            elapsed_seconds: 0.0,
            script_instances: 0,
            camera_ready: false,
            last_error: None,
        };
        self.loading.begin(origin, language);
        self.router.cancel_all();
        self.loading_surface_key = None;
        self.surface_key = None;
        self.cached_canvas = None;
        self.startup_presented = false;
        self.last_frame = Instant::now();
        tracing::info!(
            stage = %self.loading.stage_key,
            "runtime preparation retry requested"
        );
    }

    fn step_inner(
        &mut self,
        host: &mut NativeUiWindowHost,
        compositor: &mut NativeEditorCompositor,
        input: &NativeUiInputBridge,
        snapshot: &InputSnapshot,
        dt: f64,
        view: FramePresentation,
    ) -> Result<(), String> {
        self.last_render_ms = 0.0;
        let size = view.size;
        let scale = view.scale;
        let logical = view.logical;
        let headless = host.is_headless();
        let mut environment = view.environment;
        let language = self.settings.language;
        let palette = palette(self.settings.theme);
        let debug_world = self.world.as_ref().filter(|world| {
            let settings = world.project_settings();
            !headless
                && (settings.runtime.animation_debug_draw
                    || (settings.enable_physics && settings.runtime.physics_debug_draw))
        });
        let debug_interval = debug_world.map_or(0, |world| {
            let ticks = world.status().fixed_ticks;
            if self.status.phase == RuntimePhase::Running {
                ticks
                    / u64::from(
                        (world.project_settings().runtime.fixed_hz.clamp(15, 120) / 4).max(1),
                    )
            } else {
                ticks
            }
        });
        let debug_text = debug_world.map(|world| {
            self.surface_key
                .as_ref()
                .filter(|key| {
                    key.debug_interval == debug_interval && key.phase == self.status.phase
                })
                .and_then(|key| key.debug_text.clone())
                .unwrap_or_else(|| crate::physics_debug::stats(world))
        });
        let key = SurfaceKey {
            phase: self.status.phase,
            size: logical,
            camera: self.status.camera_ready,
            error: self.status.last_error.clone(),
            debug_interval,
            debug_text,
        };
        if self.surface_key.as_ref() != Some(&key) {
            self.ui.set_surface(player_surface(
                palette,
                Some(key.phase),
                logical,
                !key.camera,
                key.error.as_deref(),
                self.settings.runtime.diagnostics,
                key.debug_text.as_deref(),
            ));
            self.surface_key = Some(key);
        }
        let mut actions = self.ui.process_routed_input(
            logical,
            scale,
            |key| t(key, language),
            input,
            &mut self.router,
            InputOwner::RetainedUi(InputRegionId(0x72756e)),
            UiRect::new(0.0, 0.0, logical[0] as f32, logical[1] as f32),
        );
        let mut game_input = raf_runtime::input::RuntimeInputState::from_input(snapshot).snapshot;
        let mut ui_consumed_pointer =
            self.ui.has_interactive_hover() || self.ui.has_pointer_capture();
        let mut ui_consumed_keyboard = self.ui.captures_keyboard_input();
        let game_rect = UiRect::new(
            0.0,
            if headless { 0.0 } else { 34.0 },
            logical[0] as f32,
            (logical[1] as f32 - if headless { 0.0 } else { 34.0 }).max(1.0),
        );
        environment.viewport_size = [game_rect.width, game_rect.height];
        if let Some((world, root)) = self.world.as_mut().zip(self.root.as_ref()) {
            let ui_sync_started = Instant::now();
            self.game_ui
                .sync(host, world, root, self.settings.theme, environment);
            self.diagnostic_ui_sync_ms += ui_sync_started.elapsed().as_secs_f64() * 1000.0;
            let game_ui_input = self.game_ui.input(
                input,
                &mut self.router,
                world,
                game_rect,
                scale,
                language,
                ui_consumed_pointer,
            );
            ui_consumed_pointer |= game_ui_input.pointer;
            ui_consumed_keyboard |= game_ui_input.keyboard;
            for (instance, action) in game_ui_input.actions.into_iter().take(128) {
                let element = action.target_id;
                // Authored UI never executes commands against the editor kernel.
                let (name, value) = match action.action {
                    UiAction::Command { name } => (name, raf_script::ScriptValue::None),
                    UiAction::SetToggle { key, value } => {
                        (key, raf_script::ScriptValue::Bool(value))
                    }
                    UiAction::SetRange { key, value } => {
                        (key, raf_script::ScriptValue::Float(value))
                    }
                    UiAction::SetText { key, value } => {
                        (key, raf_script::ScriptValue::String(value))
                    }
                    UiAction::SetSelect { key, value, .. } => {
                        (key, raf_script::ScriptValue::String(value))
                    }
                    _ => continue,
                };
                let structured =
                    raf_script::ScriptValue::Record(std::collections::BTreeMap::from([
                        ("instance".into(), raf_script::ScriptValue::String(instance)),
                        ("element".into(), raf_script::ScriptValue::String(element)),
                        ("name".into(), raf_script::ScriptValue::String(name.clone())),
                        ("value".into(), value.clone()),
                    ]));
                if name.len() <= 128 && name != "ui.event" {
                    if let Err(error) = world.emit_ui_event(&name, value) {
                        if self.last_diagnostics.len() < 256 {
                            self.last_diagnostics.push(error);
                        }
                    }
                }
                if let Err(error) = world.emit_ui_event("ui.event", structured) {
                    if self.last_diagnostics.len() < 256 {
                        self.last_diagnostics.push(error);
                    }
                }
            }
        }
        if ui_consumed_pointer {
            game_input.suppress_pointer();
        }
        if ui_consumed_keyboard {
            game_input.suppress_keyboard();
        }
        if self.focused
            && !ui_consumed_pointer
            && !ui_consumed_keyboard
            && game_input.mouse_pressed.contains(&0)
        {
            self.cursor_suspended = false;
        }
        for action in actions.drain(..) {
            if let UiAction::Command { name } = action.action {
                match name.as_str() {
                    "runtime.pause" => self.control(RuntimeControl::Pause),
                    "runtime.resume" => self.control(RuntimeControl::Resume),
                    "runtime.step" => self.control(RuntimeControl::Step),
                    "runtime.stop" => self.control(RuntimeControl::Stop),
                    _ => {}
                }
            }
        }
        if let Some(world) = &mut self.world {
            let world_started = Instant::now();
            world.advance(dt, &game_input);
            self.diagnostic_world_ms += world_started.elapsed().as_secs_f64() * 1000.0;
            let intent = if self.focused
                && !self.cursor_suspended
                && world.status().phase == RuntimePhase::Running
                && !ui_consumed_pointer
                && !ui_consumed_keyboard
            {
                world.services().cursor
            } else {
                Default::default()
            };
            match self.cursor.apply(intent.capture, intent.visible) {
                Ok(mode) => world.services_mut().effective_cursor = mode,
                Err(error) => {
                    world.services_mut().effective_cursor =
                        raf_script::services::CursorCapture::Free;
                    if self.last_diagnostics.len() < 256 {
                        self.last_diagnostics.push(error);
                    }
                }
            }
            self.status = world.status().clone();
            let diagnostics = world.drain_diagnostics();
            self.last_diagnostics.extend(
                diagnostics
                    .into_iter()
                    .take(256usize.saturating_sub(self.last_diagnostics.len())),
            );
            if self.status.phase == RuntimePhase::Stopped {
                self.audio.stop();
                return Ok(());
            }
            if let Some(root) = &self.root {
                let listener = world
                    .presentation_camera_pose()
                    .map(|pose| pose.position)
                    .unwrap_or_default();
                self.audio.update_listener(&world.scene, listener);
                let commands = world.drain_audio();
                let audio = self.audio.consume(&world.scene, root, commands);
                self.audio
                    .set_paused(self.status.phase == RuntimePhase::Paused);
                self.last_diagnostics.extend(
                    audio
                        .into_iter()
                        .take(256usize.saturating_sub(self.last_diagnostics.len())),
                );
            }
        }
        let requested = self
            .world
            .as_mut()
            .and_then(|world| world.take_session_request());
        if let Some(id) = requested {
            self.begin_transition(id);
            if self.is_preparing() {
                return Ok(());
            }
        }
        if let Some((world, root)) = self.world.as_ref().zip(self.root.as_ref()) {
            self.game_ui
                .sync(host, world, root, self.settings.theme, environment);
        }
        let canvas = self.render_canvas(host, size, scale, dt as f32);
        let top = if headless { 0 } else { (34.0 * scale) as u32 };
        let mut layers = self.game_ui.overlay_layers(
            CanvasTargetRect {
                x: 0,
                y: top,
                width: size[0],
                height: size[1].saturating_sub(top).max(1),
            },
            [logical[0], (game_rect.height as u32).max(1)],
            scale,
        );
        // The Player toolbar is windowed-only chrome. A headless or streamed
        // capture must contain the game and its authored UI, never the local
        // runtime controls.
        if !headless {
            layers.push(EditorUiLayer {
                host: &mut self.ui,
                target_rect: CanvasTargetRect::full(size),
                logical_size: logical,
                raster_scale: scale.max(1.0),
            });
        }
        let render_started = Instant::now();
        host.render_editor_layers(compositor, canvas, &mut layers, |key| t(key, language))
            .map_err(|e| format!("runtime presentation: {e}"))?;
        self.last_render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
        self.router.reconcile_input(snapshot);
        Ok(())
    }
    fn render_canvas(
        &mut self,
        host: &NativeUiWindowHost,
        size: [u32; 2],
        scale: f32,
        dt: f32,
    ) -> Option<EditorCanvasLayer> {
        let world = self.world.as_ref()?;
        let pose = world.presentation_camera_pose()?;
        let quality = world.presentation_quality();
        let top = if host.is_headless() {
            0
        } else {
            (34.0 * scale) as u32
        };
        let target = CanvasTargetRect {
            x: 0,
            y: top,
            width: size[0],
            height: size[1].saturating_sub(top).max(1),
        };
        let mode = quality.viewport_resolution;
        let (min, max) = mode.resolution_range();
        self.resolution.set_resolution_limits(
            min,
            max,
            mode == raf_core::project::ViewportResolutionMode::Efficient,
        );
        let cap = if self.settings.render_execution_policy
            == raf_core::config::RenderExecutionPolicy::CpuOnly
        {
            [960.0_f32, 540.0_f32]
        } else {
            [1920.0_f32, 1080.0_f32]
        };
        let fit = (cap[0] / target.width.max(1) as f32)
            .min(cap[1] / target.height.max(1) as f32)
            .min(1.0);
        let scale = fit * self.resolution.scale();
        let source_size = [
            (target.width as f32 * scale).max(1.0) as u32,
            (target.height as f32 * scale).max(1.0) as u32,
        ];
        let cache_size = [size[0], size[1], source_size[0], source_size[1]];
        let revision = world
            .presentation_revision()
            .wrapping_mul(31)
            .wrapping_add(self.game_ui.revision());
        let particles_animating = world.project_settings().particle_runtime.enabled
            && world.scene.iter().any(|(_, node)| {
                node.particle_system.as_ref().is_some_and(|component| {
                    component.visible
                        && component.playback == raf_core::scene::ParticlePlayback::Playing
                })
            });
        if !particles_animating {
            if let Some((cached_revision, cached_size, cached_pose, canvas)) = &self.cached_canvas {
                if *cached_revision == revision
                    && *cached_size == cache_size
                    && *cached_pose == pose
                {
                    return Some(canvas.clone());
                }
            }
        }
        let camera = Camera {
            position: pose.position,
            target: pose.position + pose.forward,
            up: pose.up,
            mode: if pose.projection.orthographic {
                CameraMode::Orthographic
            } else {
                CameraMode::Perspective
            },
            fov: pose.projection.fov_degrees,
            near: pose.projection.near,
            far: pose.projection.far,
            ortho_scale: pose.projection.ortho_scale,
        };
        let options = RenderOptions {
            environment_override: Some(world.presentation_environment()),
            show_grid_3d: false,
            selection_outline: false,
            geometry_detail: quality.geometry_detail,
            texture_quality: quality.texture_quality,
            world_streaming_enabled: world.project_settings().world_streaming_enabled,
            world_stream_region_size: world.project_settings().world_stream_region_size,
            world_stream_load_radius: world.project_settings().world_stream_load_radius,
            particles: raf_render::particles::ParticleRenderOptions {
                enabled: world.project_settings().particle_runtime.enabled,
                paused: false,
                frozen: false,
                stopped: false,
                time_scale: world.project_settings().particle_runtime.time_scale,
                quality: world.project_settings().particle_runtime.quality,
                max_sprites: world.project_settings().particle_runtime.max_particles,
                wind: world.project_settings().particle_runtime.wind,
            },
            particle_dt: dt.clamp(1.0 / 240.0, 0.1),
            // World streaming decides which regions are resident. Camera
            // frustum culling must not change packet topology: rebuilding and
            // re-uploading the whole city while driving is far more expensive
            // than submitting the resident static instances.
            stable_scene_packet: true,
            // Play always retains the prepared scene topology. The project
            // toggle is a renderer diagnostic, not a reason to rebuild and
            // re-upload every mesh/texture when a script moves a wheel.
            scene_packet_reuse: true,
            ..RenderOptions::default()
        };
        let render_started = Instant::now();
        let build_started = Instant::now();
        let mut frame = self.renderer.as_mut()?.build_frame(
            &world.scene,
            &camera,
            source_size[0] as f32,
            source_size[1] as f32,
            &[],
            raf_render::scene_renderer::SCENE_BACKGROUND,
            raf_render::scene_renderer::SCENE_LIGHT_DIRECTION.normalize(),
            options,
            None,
        );
        self.diagnostic_scene_build_ms += build_started.elapsed().as_secs_f64() * 1000.0;
        self.last_scene_stats = frame.stats.clone();
        self.game_ui
            .world_commands(&mut frame, self.settings.language);
        crate::physics_debug::append(&mut frame.commands, world);
        let output = self.device.as_mut()?.execute_scene_frame(&frame);
        let canvas = EditorCanvasLayer {
            output: Arc::new(output),
            source_size,
            target_rect: target,
        };
        let budget = FramePacingBudget {
            foreground_fps: self.settings.runtime.fps_limit.clamp(15, 120) as u16,
            ..FramePacingBudget::eco()
        };
        self.resolution.update(
            budget,
            FrameActivity::Interactive,
            render_started.elapsed().as_secs_f32() * 1000.0,
            0.0,
        );
        self.cached_canvas = Some((revision, cache_size, pose, canvas.clone()));
        Some(canvas)
    }
}
impl Drop for PlayerSession {
    fn drop(&mut self) {
        self.control(RuntimeControl::Stop);
    }
}
pub fn palette(theme: raf_core::config::Theme) -> StudioUiPalette {
    if theme == raf_core::config::Theme::Light {
        StudioUiPalette::PaperLight
    } else {
        StudioUiPalette::IndustrialDark
    }
}
fn load_ui_document(
    root: &std::path::Path,
    path: Option<PathBuf>,
) -> Result<Option<UiDocument>, Vec<String>> {
    use std::io::Read;
    let Some(path) = path else {
        return Ok(None);
    };
    let path = root.join(path);
    if !path.exists() {
        return Ok(None);
    }
    let path = path.canonicalize().map_err(|e| vec![e.to_string()])?;
    let root = root.canonicalize().map_err(|e| vec![e.to_string()])?;
    if !path.starts_with(root) {
        return Err(vec!["game UI document escapes the project".into()]);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| vec![e.to_string()])?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| vec![e.to_string()])?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(vec!["game UI document exceeds 4 MiB".into()]);
    }
    let mut document: UiDocument =
        ron::de::from_bytes(&bytes).map_err(|e| vec![format!("game UI document: {e}")])?;
    document.migrate()?;
    Ok(Some(document))
}
