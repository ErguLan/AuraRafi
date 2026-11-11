//! Shared native host lifecycle for separate-window and in-place Play.
use crate::audio::RuntimeAudio;
use crate::surface::player_surface;
use raf_core::{i18n::t, InputKey, InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::device::BasicDevice;
use raf_render::api_graphic_basic::frame_scheduler::{
    DynamicResolutionController, FrameActivity, FramePacingBudget,
};
use raf_render::api_graphic_basic::ui_surface::loading::{
    build_loading_surface, insert_loading_brand,
};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeUiInputBridge, NativeUiWindowHost, StudioUiPalette, UiAction,
};
use raf_render::api_graphic_basic::{
    CanvasTargetRect, EditorCanvasLayer, EditorUiLayer, NativeEditorCompositor,
};
use raf_render::{
    camera::{Camera, CameraMode},
    scene_renderer::{RenderOptions, SceneRenderer},
};
use raf_runtime::manifest::{RuntimeHostSettings, RuntimeManifest};
use raf_runtime::{RuntimeControl, RuntimePhase, RuntimeStatus, RuntimeWorld};
use raf_ui::{UiColorMode, UiDocument, UiEnvironment, UiRect};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc,
};
use std::time::Instant;
use uuid::Uuid;
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
    loading_key: String,
    cached_canvas: Option<(
        u64,
        [u32; 4],
        raf_core::runtime_config::RuntimeCameraPose,
        EditorCanvasLayer,
    )>,
    resolution: DynamicResolutionController,
    focused: bool,
    last_frame: Instant,
    last_diagnostics: Vec<String>,
    startup_presented: bool,
    has_entered_game: bool,
    loading_progress: f32,
    loading_count: String,
    loading_detail: String,
    loading_surface_key: Option<(String, String, String, u32)>,
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
        let status = t("runtime.loading_snapshot", language);
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
            audio: RuntimeAudio::new(false),
            settings,
            instance,
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
            loading_key: "runtime.loading_snapshot".into(),
            cached_canvas: None,
            resolution: DynamicResolutionController::default(),
            focused: true,
            last_frame: Instant::now(),
            last_diagnostics: Vec::new(),
            startup_presented: false,
            has_entered_game: false,
            loading_progress: 0.0,
            loading_count: String::new(),
            loading_detail: t("runtime.cancel_hint", language),
            loading_surface_key: None,
        }
    }
    pub fn status(&self) -> &RuntimeStatus {
        &self.status
    }
    pub fn is_preparing(&self) -> bool {
        self.status.phase == RuntimePhase::Preparing
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
            self.receiver = None;
            self.source = None;
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
    pub fn frame_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(if self.is_preparing() {
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
            match message {
                Preparation::Stage(key) => {
                    self.loading_key = key.into();
                    self.loading_progress = 0.0;
                    self.loading_count.clear();
                    self.loading_detail = t("runtime.cancel_hint", self.settings.language);
                }
                Preparation::Compile(progress) => {
                    self.loading_key = "runtime.compiling".into();
                    self.loading_progress = if progress.total > 0 {
                        progress.completed as f32 / progress.total as f32
                    } else {
                        0.0
                    };
                    self.loading_count = format!("{}/{}", progress.completed, progress.total);
                    self.loading_detail = format!(
                        "{}\n{}",
                        progress.path,
                        t("runtime.cancel_hint", self.settings.language)
                    );
                }
                Preparation::Ready(mut world, document, root, audio, context) => {
                    if let Some(document) = &document {
                        if let Err(error) = world.game_ui_mut().mount_document(document) {
                            self.fail_preparation(vec![error]);
                            continue;
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
                        continue;
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
                    self.renderer = Some(renderer);
                    let mut config = host.basic_device_config();
                    if self.settings.render_execution_policy
                        == raf_core::config::RenderExecutionPolicy::CpuOnly
                    {
                        config.force_cpu = true;
                        config.allow_gpu = false;
                    }
                    self.device = Some(BasicDevice::new(config));
                    self.game_ui.clear();
                    self.status = world.status().clone();
                    self.world = Some(*world);
                    self.audio = RuntimeAudio::new(audio);
                    self.root = Some(root);
                    self.receiver = None;
                    self.last_frame = Instant::now();
                }
                Preparation::Failed(errors) => {
                    self.fail_preparation(errors);
                }
            }
        }
    }
    fn fail_preparation(&mut self, errors: Vec<String>) {
        if let Some(phase) = self.transition_previous.take() {
            if let Some(world) = &mut self.world {
                world.transition_failed(
                    errors
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "runtime.session_failed".into()),
                );
                if phase == RuntimePhase::Running {
                    world.control(RuntimeControl::Resume);
                }
                world.set_focused(self.focused);
                self.status = world.status().clone();
            }
        } else {
            self.status.phase = RuntimePhase::Failed;
            self.status.last_error = errors.first().cloned();
        }
        self.last_diagnostics.extend(
            errors
                .into_iter()
                .take(256usize.saturating_sub(self.last_diagnostics.len())),
        );
        self.receiver = None;
        self.surface_key = None;
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
        self.source = Some(PlayerSource::Session(Box::new(context), id));
        self.status.phase = RuntimePhase::Preparing;
        self.loading_key = "runtime.loading_session".into();
        self.loading_progress = 0.0;
        self.loading_count.clear();
        self.loading_detail = t("runtime.cancel_hint", self.settings.language);
        self.startup_presented = false;
        self.loading_surface_key = None;
    }
    pub fn draw(
        &mut self,
        host: &mut NativeUiWindowHost,
        compositor: &mut NativeEditorCompositor,
        input: &NativeUiInputBridge,
    ) -> Result<(), String> {
        self.poll_preparation(host);
        let size = host.size();
        let scale = host.window().scale_factor() as f32;
        let logical = [
            (size[0] as f32 / scale).max(1.0) as u32,
            (size[1] as f32 / scale).max(1.0) as u32,
        ];
        let language = self.settings.language;
        let palette = palette(self.settings.theme);
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
        if input.snapshot().key_pressed(InputKey::Escape) {
            if self.is_preparing() || input.snapshot().modifiers.shift {
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
        if self.should_close() {
            return Ok(());
        }
        if self.is_preparing() {
            let loading_key = (
                self.loading_key.clone(),
                self.loading_detail.clone(),
                self.loading_count.clone(),
                self.loading_progress.to_bits(),
            );
            if self.loading_surface_key.as_ref() != Some(&loading_key) {
                self.ui.set_surface(build_loading_surface(
                    palette,
                    self.loading_progress,
                    language,
                    &self.loading_detail,
                    &t(&self.loading_key, language),
                    &self.loading_count,
                ));
                self.loading_surface_key = Some(loading_key);
            }
            host.render_editor_frame(
                compositor,
                None,
                &mut self.ui,
                logical,
                scale.max(1.0),
                |key| t(key, language),
            )
            .map_err(|e| format!("runtime loading presentation: {e}"))?;
            if !self.startup_presented {
                self.startup_presented = true;
                self.start_worker();
            }
            return Ok(());
        }
        let key = SurfaceKey {
            phase: self.status.phase,
            size: logical,
            camera: self.status.camera_ready,
            error: self.status.last_error.clone(),
        };
        if self.surface_key.as_ref() != Some(&key) {
            self.ui.set_surface(player_surface(
                palette,
                Some(key.phase),
                logical,
                !key.camera,
                key.error.as_deref(),
                self.settings.runtime.diagnostics,
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
        let mut game_input =
            raf_runtime::input::RuntimeInputState::from_input(input.snapshot()).snapshot;
        let mut ui_consumed_pointer =
            self.ui.has_interactive_hover() || self.ui.has_pointer_capture();
        let mut ui_consumed_keyboard = self.ui.captures_keyboard_input();
        let game_rect=UiRect::new(0.0,34.0,logical[0] as f32,(logical[1] as f32-34.0).max(1.0));
        environment.viewport_size=[game_rect.width,game_rect.height];
        if let Some((world,root))=self.world.as_mut().zip(self.root.as_ref()) {
            self.game_ui.sync(host,world,root,self.settings.theme,environment);
            let game_ui_input=self.game_ui.input(input,&mut self.router,world,game_rect,scale,language,ui_consumed_pointer);
            ui_consumed_pointer |= game_ui_input.pointer;
            ui_consumed_keyboard |= game_ui_input.keyboard;
                for (instance,action) in game_ui_input.actions.into_iter().take(128) {
                    let element=action.target_id;
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
                    let structured=raf_script::ScriptValue::Record(std::collections::BTreeMap::from([
                        ("instance".into(),raf_script::ScriptValue::String(instance)),
                        ("element".into(),raf_script::ScriptValue::String(element)),
                        ("name".into(),raf_script::ScriptValue::String(name.clone())),
                        ("value".into(),value.clone()),
                    ]));
                    if name.len()<=128 && name!="ui.event" {
                        if let Err(error)=world.emit_ui_event(&name,value){if self.last_diagnostics.len()<256{self.last_diagnostics.push(error);}}
                    }
                    if let Err(error)=world.emit_ui_event("ui.event",structured){if self.last_diagnostics.len()<256{self.last_diagnostics.push(error);}}
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
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        if let Some(world) = &mut self.world {
            world.advance(dt, &game_input);
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
        if let Some((world,root))=self.world.as_ref().zip(self.root.as_ref()) {
            self.game_ui.sync(host,world,root,self.settings.theme,environment);
        }
        let canvas = self.render_canvas(host,size,scale);
        let top=(34.0*scale) as u32;
        let mut layers=self.game_ui.overlay_layers(CanvasTargetRect{x:0,y:top,width:size[0],height:size[1].saturating_sub(top).max(1)},[logical[0],(game_rect.height as u32).max(1)],scale);
        layers.push(EditorUiLayer {
            host: &mut self.ui,
            target_rect: CanvasTargetRect::full(size),
            logical_size: logical,
            raster_scale: scale.max(1.0),
        });
        host.render_editor_layers(compositor, canvas, &mut layers, |key| t(key, language))
            .map_err(|e| format!("runtime presentation: {e}"))?;
        self.router.reconcile_input(input.snapshot());
        Ok(())
    }
    fn render_canvas(&mut self, host:&NativeUiWindowHost,size: [u32; 2], scale: f32) -> Option<EditorCanvasLayer> {
        let world = self.world.as_ref()?;
        let pose = world.presentation_camera_pose()?;
        let top = (34.0 * scale) as u32;
        let target = CanvasTargetRect {
            x: 0,
            y: top,
            width: size[0],
            height: size[1].saturating_sub(top).max(1),
        };
        let mode = world.project_settings().viewport_resolution;
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
        let revision = world.presentation_revision().wrapping_mul(31).wrapping_add(self.game_ui.revision());
        if let Some((cached_revision, cached_size, cached_pose, canvas)) = &self.cached_canvas {
            if *cached_revision == revision && *cached_size == cache_size && *cached_pose == pose {
                return Some(canvas.clone());
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
            geometry_detail: world_render_options(world).0,
            texture_quality: world_render_options(world).1,
            world_streaming_enabled: world.project_settings().world_streaming_enabled,
            world_stream_region_size: world.project_settings().world_stream_region_size,
            world_stream_load_radius: world.project_settings().world_stream_load_radius,
            ..RenderOptions::default()
        };
        let render_started = Instant::now();
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
        let cpu=self.device.as_ref()?.backend()==raf_render::api_graphic_basic::device::BasicBackendType::CpuSoftware;
        if let Err(error)=self.game_ui.world_commands(host,&mut frame,cpu,self.settings.language){
            if self.last_diagnostics.len()<256 {self.last_diagnostics.push(error);}
        }
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
fn world_render_options(
    world: &RuntimeWorld,
) -> (
    raf_core::project::GeometryDetailMode,
    raf_core::project::TextureQualityMode,
) {
    (
        world.project_settings().geometry_detail,
        world.project_settings().texture_quality,
    )
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
