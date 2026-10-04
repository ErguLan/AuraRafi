use crate::physics::PhysicsWorld;
use glam::Vec3;
use raf_core::runtime_config::{RuntimeCameraPose, RuntimeErrorPolicy, RuntimePreferences};
use raf_core::{
    config::{ScriptExecutionMode, ScriptLanguage},
    project::ProjectSettings,
    SceneGraph,
};
use raf_script::{
    AudioCommand, InputSnapshot, RhaiScriptRuntime, ScriptLoadProgress, ScriptRuntimeOptions,
    ScriptRuntimeReport,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimePhase {
    Preparing,
    Running,
    Paused,
    Stopping,
    Stopped,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeControl {
    Pause,
    Resume,
    Step,
    Stop,
    Reload,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub instance: Uuid,
    pub phase: RuntimePhase,
    pub fixed_ticks: u64,
    pub elapsed_seconds: f64,
    pub script_instances: usize,
    pub camera_ready: bool,
    pub last_error: Option<String>,
}
/// Owns an isolated scene and all its simulation state. It never saves authoring data.
pub struct RuntimeWorld {
    previous_camera: Option<(Option<Uuid>, RuntimeCameraPose)>,
    pub scene: SceneGraph,
    status: RuntimeStatus,
    scripts: Option<RhaiScriptRuntime>,
    settings: ProjectSettings,
    preferences: RuntimePreferences,
    accumulator: f64,
    pending_pressed: Vec<String>,
    physics: PhysicsWorld,
    diagnostics: VecDeque<String>,
    audio: Vec<AudioCommand>,
    last_reload: Instant,
    focus_paused: bool,
}
impl RuntimeWorld {
    pub fn prepare(
        instance: Uuid,
        scene: SceneGraph,
        root: &std::path::Path,
        settings: ProjectSettings,
        preferences: RuntimePreferences,
        cancellation: Arc<AtomicBool>,
        progress: impl FnMut(ScriptLoadProgress),
    ) -> Result<Self, Vec<String>> {
        let preferences = preferences.normalized();
        crate::validation::validate_scene(&scene).map_err(|e| vec![e])?;
        if cancellation.load(Ordering::Relaxed) {
            return Err(vec!["Runtime preparation cancelled".into()]);
        }
        let entity_limit = settings.runtime.max_entities.clamp(100, 100_000);
        if scene.len() > entity_limit {
            return Err(vec!["Scene exceeds the configured entity limit".into()]);
        }
        let (scripts, report) = if settings.enable_scripting
            && settings.script_execution_mode != ScriptExecutionMode::Disabled
        {
            let options = ScriptRuntimeOptions {
                root_is_project: true,
                allow_rhai: settings.allowed_script_languages.has(ScriptLanguage::Rhai),
                allow_nodes: settings.allowed_script_languages.has(ScriptLanguage::Nodes),
                max_time_ms: preferences.script_budget_ms as u64,
                max_operations: preferences.script_operation_limit,
                max_entities: entity_limit,
                cancellation: cancellation.clone(),
                ..ScriptRuntimeOptions::default()
            };
            let (scripts, report) =
                RhaiScriptRuntime::load_with_progress(&scene, Some(root), options, progress);
            (Some(scripts), report)
        } else {
            (None, ScriptRuntimeReport::default())
        };
        if !report.errors.is_empty() {
            return Err(report.errors);
        }
        if cancellation.load(Ordering::Relaxed) {
            return Err(vec!["Runtime preparation cancelled".into()]);
        }
        let physics = if settings.enable_physics {
            PhysicsWorld::prepare(&scene)?
        } else {
            PhysicsWorld::default()
        };
        let mut world = Self {
            previous_camera: None,
            status: RuntimeStatus {
                instance,
                phase: RuntimePhase::Preparing,
                fixed_ticks: 0,
                elapsed_seconds: 0.0,
                script_instances: scripts.as_ref().map_or(0, |s| s.script_count()),
                camera_ready: false,
                last_error: None,
            },
            scene,
            scripts,
            settings,
            preferences,
            accumulator: 0.0,
            pending_pressed: Vec::new(),
            physics,
            diagnostics: VecDeque::new(),
            audio: Vec::new(),
            last_reload: Instant::now(),
            focus_paused: false,
        };
        world.record(report);
        if let Some(scripts) = &mut world.scripts {
            scripts.view.active_camera = world.settings.runtime.active_camera;
        }
        world.status.camera_ready = world.camera_pose().is_some();
        Ok(world)
    }
    pub fn status(&self) -> &RuntimeStatus {
        &self.status
    }
    pub fn project_settings(&self) -> &ProjectSettings {
        &self.settings
    }
    pub fn attach_graph(&mut self, graph: &raf_nodes::NodeGraph) -> Result<(), Vec<String>> {
        if let Some(scripts) = &mut self.scripts {
            let report = scripts.attach_graph(graph);
            if !report.errors.is_empty() {
                return Err(report.errors);
            }
            self.record(report);
        }
        Ok(())
    }
    pub fn active_camera_uuid(&self) -> Option<Uuid> {
        self.scripts
            .as_ref()
            .map(|s| s.view.active_camera)
            .unwrap_or(self.settings.runtime.active_camera)
    }
    pub fn presentation_camera_pose(&self) -> Option<RuntimeCameraPose> {
        let pose = self.camera_pose()?;
        if self.status.phase != RuntimePhase::Running {
            return Some(pose);
        }
        let Some((uuid, previous)) = &self.previous_camera else {
            return Some(pose);
        };
        if *uuid != self.active_camera_uuid() || previous.projection != pose.projection {
            return Some(pose);
        }
        let alpha = (self.accumulator / self.fixed_step()).clamp(0.0, 1.0) as f32;
        let orientation = |p: &RuntimeCameraPose| {
            let right = p.forward.cross(p.up).normalize_or_zero();
            glam::Quat::from_mat3(&glam::Mat3::from_cols(
                right,
                right.cross(p.forward).normalize_or_zero(),
                -p.forward,
            ))
        };
        let rotation = orientation(previous).slerp(orientation(&pose), alpha);
        Some(RuntimeCameraPose {
            position: previous.position.lerp(pose.position, alpha),
            forward: rotation * -Vec3::Z,
            up: rotation * Vec3::Y,
            projection: pose.projection,
        })
    }
    /// Starting occurs only after the loading frame has been presented.
    pub fn start(&mut self) {
        if self.status.phase != RuntimePhase::Preparing {
            return;
        }
        self.status.phase = RuntimePhase::Running;
        if let Some(scripts) = &mut self.scripts {
            let report = scripts.call_start(&mut self.scene);
            self.record(report);
        }
        self.status.camera_ready = self.camera_pose().is_some();
        if self.settings.enable_audio {
            let names: Vec<String> = self
                .scene
                .iter()
                .filter(|(id, n)| {
                    self.scene.is_valid_node(*id)
                        && n.audio_source.enabled
                        && n.audio_source.autoplay
                        && !n.audio_source.clip.is_empty()
                })
                .map(|(_, n)| n.name.clone())
                .take(16)
                .collect();
            self.audio
                .extend(names.into_iter().map(|name| AudioCommand::Play { name }));
        }
    }
    pub fn control(&mut self, control: RuntimeControl) {
        self.pending_pressed.clear();
        self.previous_camera = None;
        match control {
            RuntimeControl::Pause if self.status.phase == RuntimePhase::Running => {
                self.status.phase = RuntimePhase::Paused;
                self.focus_paused = false;
                self.accumulator = 0.0;
            }
            RuntimeControl::Resume if self.status.phase == RuntimePhase::Paused => {
                self.status.phase = RuntimePhase::Running;
                self.focus_paused = false;
                self.accumulator = 0.0;
            }
            RuntimeControl::Step if self.status.phase == RuntimePhase::Paused => {
                self.tick(&InputSnapshot::default())
            }
            RuntimeControl::Stop => self.stop(),
            RuntimeControl::Reload
                if matches!(
                    self.status.phase,
                    RuntimePhase::Running | RuntimePhase::Paused
                ) =>
            {
                self.reload()
            }
            _ => {}
        }
    }
    pub fn set_focused(&mut self, focused: bool) {
        if !focused {
            self.pending_pressed.clear();
        }
        if !self.settings.pause_when_unfocused {
            return;
        }
        if !focused && self.status.phase == RuntimePhase::Running {
            self.status.phase = RuntimePhase::Paused;
            self.focus_paused = true;
            self.accumulator = 0.0;
        } else if focused && self.focus_paused && self.status.phase == RuntimePhase::Paused {
            self.status.phase = RuntimePhase::Running;
            self.focus_paused = false;
            self.accumulator = 0.0;
        }
    }
    /// Fixed simulation is independent of display pacing. Catch-up is bounded.
    pub fn advance(&mut self, real_dt: f64, input: &InputSnapshot) -> bool {
        if self.status.phase != RuntimePhase::Running || !real_dt.is_finite() || real_dt < 0.0 {
            return false;
        }
        if self.preferences.hot_reload && self.last_reload.elapsed() >= Duration::from_secs(1) {
            self.reload();
            self.last_reload = Instant::now();
            if self.status.phase != RuntimePhase::Running {
                return false;
            }
        }
        let step = self.fixed_step();
        for key in input.keys_pressed.iter().take(128) {
            if self.pending_pressed.len() < 128 && !self.pending_pressed.contains(key) {
                self.pending_pressed.push(key.clone());
            }
        }
        self.accumulator += real_dt.min(0.25);
        let mut ticks = 0;
        while self.accumulator + f64::EPSILON >= step
            && ticks < 4
            && self.status.phase == RuntimePhase::Running
        {
            let mut snapshot = input.clone();
            // Edge-triggered input is consumed once, not for every catch-up tick.
            snapshot.keys_pressed = std::mem::take(&mut self.pending_pressed);
            self.tick(&snapshot);
            self.accumulator -= step;
            ticks += 1;
        }
        if ticks == 4 && self.accumulator >= step {
            self.accumulator %= step;
        }
        ticks > 0
    }
    pub fn drain_diagnostics(&mut self) -> Vec<String> {
        self.diagnostics.drain(..).collect()
    }
    pub fn drain_audio(&mut self) -> Vec<AudioCommand> {
        std::mem::take(&mut self.audio)
    }
    pub fn emit_ui_event(
        &mut self,
        name: &str,
        value: raf_script::ScriptValue,
    ) -> Result<(), String> {
        if let Some(scripts) = &mut self.scripts {
            scripts.emit_event(name, value)
        } else {
            Err("this runtime has no executable scripts".into())
        }
    }
    pub fn has_active_simulation(&self) -> bool {
        self.status.phase == RuntimePhase::Running
            && (self
                .scripts
                .as_ref()
                .is_some_and(|s| s.active_script_count() > 0)
                || self.physics.has_dynamic_bodies())
    }
    fn fixed_step(&self) -> f64 {
        1.0 / self.settings.runtime.fixed_hz.clamp(15, 120) as f64
    }
    fn tick(&mut self, input: &InputSnapshot) {
        if !matches!(
            self.status.phase,
            RuntimePhase::Running | RuntimePhase::Paused
        ) {
            return;
        }
        let dt = self.fixed_step() as f32;
        self.previous_camera = self
            .camera_pose()
            .map(|pose| (self.active_camera_uuid(), pose));
        let mut input = input.clone();
        for action in self.settings.runtime.input_actions.iter().take(128) {
            if action.name.is_empty() || action.name.len() > 128 {
                continue;
            }
            let held = action
                .keys
                .iter()
                .take(16)
                .any(|key| input.is_key_held(key));
            let pressed = action
                .keys
                .iter()
                .take(16)
                .any(|key| input.was_key_pressed(key));
            if held {
                input
                    .keys_held
                    .push(format!("action:{}", action.name.to_ascii_lowercase()));
            }
            if pressed {
                input
                    .keys_pressed
                    .push(format!("action:{}", action.name.to_ascii_lowercase()));
            }
        }
        if let Some(scripts) = &mut self.scripts {
            let report = scripts.update(&mut self.scene, dt, &input);
            if !self.record(report) {
                return;
            }
        }
        if matches!(
            self.status.phase,
            RuntimePhase::Stopped | RuntimePhase::Failed
        ) {
            return;
        }
        if let Some(scripts) = &mut self.scripts {
            let report = scripts.fixed_update(&mut self.scene, dt, &input);
            if !self.record(report) {
                return;
            }
        }
        if matches!(
            self.status.phase,
            RuntimePhase::Stopped | RuntimePhase::Failed
        ) {
            return;
        }
        if self.settings.enable_physics {
            if let Err(error) = self.physics.step(&mut self.scene, dt) {
                if !self.record(ScriptRuntimeReport {
                    errors: vec![error],
                    ..ScriptRuntimeReport::default()
                }) {
                    return;
                }
            }
        }
        if let Some(scripts) = &mut self.scripts {
            let report = scripts.late_update(&mut self.scene, dt, &input);
            if !self.record(report) {
                return;
            }
        }
        self.status.fixed_ticks = self.status.fixed_ticks.saturating_add(1);
        self.status.elapsed_seconds = self.status.fixed_ticks as f64 * self.fixed_step();
        self.status.camera_ready = self.camera_pose().is_some();
    }
    fn record(&mut self, report: ScriptRuntimeReport) -> bool {
        let failed = !report.errors.is_empty();
        if let Some(error) = report.errors.last() {
            self.status.last_error = Some(error.clone());
        }
        for message in report
            .logs
            .into_iter()
            .chain(report.errors.into_iter().map(|e| format!("ERROR: {e}")))
        {
            if self.diagnostics.len() == 256 {
                self.diagnostics.pop_front();
            }
            self.diagnostics.push_back(message);
        }
        if let Some(scripts) = &mut self.scripts {
            let audio = scripts.drain_audio();
            if self.settings.enable_audio {
                self.audio.extend(
                    audio
                        .into_iter()
                        .take(256usize.saturating_sub(self.audio.len())),
                );
            }
            self.status.script_instances = scripts.active_script_count();
        }
        if failed && self.status.phase != RuntimePhase::Stopping {
            match self.preferences.error_policy {
                RuntimeErrorPolicy::Pause => {
                    self.status.phase = RuntimePhase::Paused;
                    self.focus_paused = false;
                }
                RuntimeErrorPolicy::DisableScript => {}
                RuntimeErrorPolicy::Stop => self.stop(),
            }
        }
        !failed || self.preferences.error_policy == RuntimeErrorPolicy::DisableScript
    }
    fn reload(&mut self) {
        if let Some(scripts) = &mut self.scripts {
            let report = scripts.reload_changed();
            self.record(report);
        }
    }
    pub fn stop(&mut self) {
        if matches!(
            self.status.phase,
            RuntimePhase::Stopped | RuntimePhase::Stopping
        ) {
            return;
        }
        self.status.phase = RuntimePhase::Stopping;
        if let Some(scripts) = &mut self.scripts {
            let report = scripts.call_destroy(&mut self.scene);
            self.record(report);
        }
        self.audio.clear();
        self.pending_pressed.clear();
        self.status.phase = RuntimePhase::Stopped;
        self.status.script_instances = 0;
        self.accumulator = 0.0;
    }
    pub fn camera_pose(&self) -> Option<RuntimeCameraPose> {
        let uuid = self.active_camera_uuid()?;
        let mut matches = self
            .scene
            .iter()
            .filter(|(id, n)| self.scene.is_valid_node(*id) && n.uuid == uuid);
        let (id, node) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        let projection = node.game_camera.as_ref()?.clone();
        if !projection.fov_degrees.is_finite()
            || !(1.0..179.0).contains(&projection.fov_degrees)
            || !projection.near.is_finite()
            || !projection.far.is_finite()
            || projection.near <= 0.0
            || projection.far <= projection.near
            || projection.far > 100_000.0
            || !projection.ortho_scale.is_finite()
            || projection.ortho_scale <= 0.0
        {
            return None;
        }
        let matrix = self.scene.world_matrix(id);
        let position = matrix.transform_point3(Vec3::ZERO);
        let forward = matrix.transform_vector3(-Vec3::Z).normalize_or_zero();
        let up = matrix.transform_vector3(Vec3::Y).normalize_or_zero();
        if !position.is_finite()
            || !forward.is_finite()
            || !up.is_finite()
            || forward.length_squared() < 0.5
            || up.length_squared() < 0.5
            || forward.cross(up).length_squared() < 0.001
        {
            return None;
        }
        Some(RuntimeCameraPose {
            position,
            forward,
            up,
            projection,
        })
    }
}
impl Drop for RuntimeWorld {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::{Primitive, VariableValue};
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new(source: &str) -> Self {
            let root = std::env::temp_dir().join(format!("raf-runtime-test-{}", Uuid::new_v4()));
            std::fs::create_dir_all(root.join("scripts")).unwrap();
            std::fs::write(root.join("scripts/main.rhai"), source).unwrap();
            Self(root)
        }
        fn world(&self, scene: SceneGraph) -> RuntimeWorld {
            let settings = ProjectSettings {
                enable_physics: false,
                enable_audio: false,
                ..Default::default()
            };
            RuntimeWorld::prepare(
                Uuid::new_v4(),
                scene,
                &self.0,
                settings,
                RuntimePreferences {
                    hot_reload: false,
                    script_budget_ms: 100,
                    ..Default::default()
                },
                Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .unwrap_or_else(|errors| panic!("{errors:?}"))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn empty_world_does_not_invent_camera_or_character_and_step_is_one_tick() {
        let fixture = Fixture::new("");
        let mut world = fixture.world(SceneGraph::new());
        world.start();
        assert!(world.camera_pose().is_none());
        assert!(world.scene.is_empty());
        world.control(RuntimeControl::Pause);
        assert!(!world.advance(1.0, &InputSnapshot::default()));
        world.control(RuntimeControl::Step);
        assert_eq!(world.status().fixed_ticks, 1);
        assert_eq!(world.status().phase, RuntimePhase::Paused);
        world.control(RuntimeControl::Resume);
        world.advance(1.0, &InputSnapshot::default());
        assert_eq!(world.status().fixed_ticks, 5);
        world.stop();
        world.stop();
        assert_eq!(world.status().phase, RuntimePhase::Stopped);
    }
    #[test]
    fn input_edges_survive_display_frames_and_authoring_is_isolated() {
        let fixture = Fixture::new(
            r#"
let presses = 0;
fn on_update(dt) {
    if was_key_just_pressed("space") { presses += 1; }
    let me = self_node(); me.set_property("presses", presses);
}
fn on_destroy() { let me = self_node(); me.set_property("cleaned", true); }
"#,
        );
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("Controller", Primitive::Empty);
        scene
            .get_mut(id)
            .unwrap()
            .scripts
            .push("scripts/main.rhai".into());
        let mut world = fixture.world(scene.clone());
        world.start();
        let input = InputSnapshot {
            keys_pressed: vec!["space".into()],
            ..Default::default()
        };
        assert!(!world.advance(0.001, &input));
        assert!(world.advance(0.02, &InputSnapshot::default()));
        assert_eq!(
            world.scene.get(id).unwrap().get_variable("presses"),
            Some(&VariableValue::Number(1.0))
        );
        world.advance(0.08, &InputSnapshot::default());
        assert_eq!(
            world.scene.get(id).unwrap().get_variable("presses"),
            Some(&VariableValue::Number(1.0))
        );
        assert!(scene.get(id).unwrap().get_variable("presses").is_none());
        world.stop();
        assert_eq!(
            world.scene.get(id).unwrap().get_variable("cleaned"),
            Some(&VariableValue::Bool(true))
        );
    }
    #[test]
    fn error_pause_aborts_remaining_hooks_and_camera_reference_is_explicit() {
        let fixture = Fixture::new(
            r#"
fn on_update(dt) { throw "stop tick"; }
fn on_fixed_update(dt) { let me = self_node(); me.set_property("should_not_run", true); }
"#,
        );
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("Camera", Primitive::Empty);
        let node = scene.get_mut(id).unwrap();
        node.game_camera = Some(Default::default());
        node.scripts.push("scripts/main.rhai".into());
        let uuid = node.uuid;
        let mut world = fixture.world(scene);
        assert!(world.camera_pose().is_none());
        world.settings.runtime.active_camera = Some(uuid);
        world.scripts.as_mut().unwrap().view.active_camera = Some(uuid);
        assert!(world.camera_pose().is_some());
        world.start();
        world.advance(0.02, &InputSnapshot::default());
        assert_eq!(world.status().phase, RuntimePhase::Paused);
        assert!(world
            .scene
            .get(id)
            .unwrap()
            .get_variable("should_not_run")
            .is_none());
        world.scene.remove_node(id);
        assert!(world.camera_pose().is_none());
    }
    #[test]
    fn scripted_camera_switch_follow_and_stop_leave_authoring_untouched() {
        let fixture = Fixture::new(include_str!(
            "../../../examples/runtime/camera_controller.rhai"
        ));
        let mut scene = SceneGraph::new();
        let root = scene.add_root("World");
        let camera = scene.add_child(root, "Camera");
        let other = scene.add_child(root, "Overview");
        scene.get_mut(other).unwrap().game_camera = Some(Default::default());
        let target = scene.add_child_with_primitive(root, "Player", Primitive::Cube);
        let controller = scene.add_root("Controller");
        scene
            .get_mut(controller)
            .unwrap()
            .scripts
            .push("scripts/main.rhai".into());
        let original = scene.clone();
        let mut world = fixture.world(scene);
        let mut independent = fixture.world(original.clone());
        world.start();
        assert!(
            world.status().camera_ready,
            "{:?}",
            world.drain_diagnostics()
        );
        assert!(independent.camera_pose().is_none());
        world.advance(
            0.02,
            &InputSnapshot {
                keys_held: vec!["w".into()],
                ..Default::default()
            },
        );
        assert!(world.scene.get(target).unwrap().position.z < 0.0);
        assert!(world.scene.get(camera).unwrap().position.z > 0.0);
        assert!(world.presentation_camera_pose().is_some());
        world.advance(
            0.02,
            &InputSnapshot {
                keys_pressed: vec!["c".into()],
                ..Default::default()
            },
        );
        assert_eq!(
            world.active_camera_uuid(),
            Some(world.scene.get(other).unwrap().uuid)
        );
        world.scene.remove_node(target);
        world.advance(0.02, &InputSnapshot::default());
        assert_eq!(world.status().phase, RuntimePhase::Running);
        world.stop();
        independent.stop();
        assert!(original.get(camera).unwrap().game_camera.is_none());
        assert_eq!(original.get(target).unwrap().position, Vec3::ZERO);
    }
    #[test]
    fn camera_late_hook_observes_physics_and_free_camera_rotates() {
        let fixture=Fixture::new("fn on_start(){let c=entity(\"Camera\"); c.add_camera(); activate_camera(c);} fn on_late_update(dt){let c=get_active_camera(); let t=entity(\"Body\"); c.follow(t,[0.0,2.0,8.0],[0.0,0.0,0.0],0.0,dt);}");
        let mut scene = SceneGraph::new();
        let body = scene.add_root("Body");
        let camera = scene.add_root("Camera");
        scene
            .get_mut(camera)
            .unwrap()
            .scripts
            .push("scripts/main.rhai".into());
        let b = &mut scene.get_mut(body).unwrap().rigid_body;
        b.enabled = true;
        b.body_type = raf_core::scene::RigidBodyType::Dynamic;
        b.use_gravity = false;
        b.damping = 0.0;
        b.velocity = Vec3::X * 6.0;
        let mut world = fixture.world(scene);
        world.settings.enable_physics = true;
        world.physics = PhysicsWorld::prepare(&world.scene).unwrap();
        world.start();
        world.advance(0.02, &InputSnapshot::default());
        assert_eq!(
            world.status().phase,
            RuntimePhase::Running,
            "{:?}",
            world.status().last_error
        );
        assert!((world.scene.get(camera).unwrap().position.x - 0.1).abs() < 1e-5);
        let fixture = Fixture::new(include_str!("../../../examples/runtime/free_camera.rhai"));
        let mut scene = SceneGraph::new();
        let camera = scene.add_root_with_primitive("Camera", Primitive::Cube);
        scene
            .get_mut(camera)
            .unwrap()
            .scripts
            .push("scripts/main.rhai".into());
        let mut world = fixture.world(scene);
        world.start();
        world.advance(
            0.02,
            &InputSnapshot {
                keys_held: vec!["left".into(), "w".into()],
                ..Default::default()
            },
        );
        assert!(world.scene.get(camera).unwrap().rotation.y > 0.0);
        assert!(world.scene.get(camera).unwrap().position.z < 0.0);
        assert_eq!(
            world.status().phase,
            RuntimePhase::Running,
            "{:?}",
            world.status().last_error
        );
    }
    fn graph_node(
        graph: &mut raf_nodes::NodeGraph,
        slug: &str,
        props: &[(&str, &str)],
    ) -> raf_nodes::NodeId {
        let mut node = raf_nodes::catalog::create(slug).unwrap();
        for (key, value) in props {
            node.properties
                .iter_mut()
                .find(|p| p.key == *key)
                .unwrap()
                .value = (*value).into();
        }
        graph.add_node(node)
    }
    fn graph_link(
        graph: &mut raf_nodes::NodeGraph,
        a: raf_nodes::NodeId,
        out: &str,
        b: raf_nodes::NodeId,
        input: &str,
    ) {
        let output = graph
            .node(a)
            .unwrap()
            .pins
            .iter()
            .find(|p| p.name == out)
            .unwrap()
            .id;
        let input = graph
            .node(b)
            .unwrap()
            .pins
            .iter()
            .find(|p| p.name == input)
            .unwrap()
            .id;
        graph.try_connect(a, output, b, input).unwrap();
    }
    #[test]
    fn nodes_snapshot_uses_shared_camera_hierarchy_api_and_safe_missing_follow() {
        let fixture = Fixture::new("");
        let mut scene = SceneGraph::new();
        let root = scene.add_root("World");
        let camera = scene.add_root_with_primitive("Camera", Primitive::Cube);
        let mut graph = raf_nodes::NodeGraph::new("Camera");
        let start = graph_node(&mut graph, "on-start", &[]);
        let get = graph_node(&mut graph, "get-entity", &[("reference", "Camera")]);
        let add = graph_node(&mut graph, "add-camera", &[]);
        let lens = graph_node(
            &mut graph,
            "set-camera-fov",
            &[("entity", "Camera"), ("fov_degrees", "75")],
        );
        let activate = graph_node(&mut graph, "activate-camera", &[("entity", "Camera")]);
        let parent = graph_node(
            &mut graph,
            "set-parent",
            &[("entity", "Camera"), ("target", "World")],
        );
        let late = graph_node(&mut graph, "on-late-update", &[]);
        let follow = graph_node(
            &mut graph,
            "follow-camera",
            &[("entity", "Camera"), ("target", "Missing")],
        );
        graph_link(&mut graph, start, "Out", add, "In");
        graph_link(&mut graph, get, "Result", add, "Entity");
        graph_link(&mut graph, add, "Out", lens, "In");
        graph_link(&mut graph, lens, "Out", activate, "In");
        graph_link(&mut graph, activate, "Out", parent, "In");
        graph_link(&mut graph, late, "Out", follow, "In");
        let encoded = ron::to_string(&graph).unwrap();
        let restored = ron::from_str(&encoded).unwrap();
        let mut world = fixture.world(scene.clone());
        world.attach_graph(&restored).unwrap();
        world.start();
        world.advance(0.02, &InputSnapshot::default());
        assert_eq!(
            world.status().phase,
            RuntimePhase::Running,
            "{:?}",
            world.status().last_error
        );
        assert_eq!(world.scene.get(camera).unwrap().parent, Some(root));
        assert_eq!(world.camera_pose().unwrap().projection.fov_degrees, 75.0);
        assert!(scene.get(camera).unwrap().game_camera.is_none());
        assert_eq!(world.status().script_instances, 1);
    }
    #[test]
    fn nodes_loops_delay_and_invalid_hardware_have_real_bounded_behavior() {
        let fixture = Fixture::new("");
        let mut graph = raf_nodes::NodeGraph::new("Flow");
        let start = graph_node(&mut graph, "on-start", &[]);
        let loop_node = graph_node(&mut graph, "for-loop", &[("end", "3")]);
        let print = graph_node(&mut graph, "print", &[("message", "body")]);
        let delay = graph_node(&mut graph, "delay", &[("seconds", "0.03")]);
        let after = graph_node(&mut graph, "print", &[("message", "after")]);
        graph_link(&mut graph, start, "Out", loop_node, "In");
        graph_link(&mut graph, loop_node, "Loop Body", print, "In");
        graph_link(&mut graph, loop_node, "Completed", delay, "In");
        graph_link(&mut graph, delay, "Out", after, "In");
        let mut world = fixture.world(SceneGraph::new());
        world.attach_graph(&graph).unwrap();
        world.start();
        let logs = world.drain_diagnostics();
        assert_eq!(
            logs.iter().filter(|s| s.ends_with(": body")).count(),
            3,
            "{logs:?}"
        );
        world.advance(0.04, &InputSnapshot::default());
        let logs = world.drain_diagnostics();
        assert_eq!(
            logs.iter().filter(|s| s.ends_with(": after")).count(),
            1,
            "{logs:?}"
        );
        assert_eq!(world.status().phase, RuntimePhase::Running, "{logs:?}");
        let mut unsupported = raf_nodes::NodeGraph::new("Hardware");
        graph_node(&mut unsupported, "serial-read", &[]);
        assert!(raf_nodes::runtime_compiler::to_rhai(&unsupported).is_err());
        let mut bounded = raf_nodes::NodeGraph::new("Unbounded");
        let start = graph_node(&mut bounded, "on-start", &[]);
        let while_node = graph_node(&mut bounded, "while-loop", &[("condition", "true")]);
        graph_link(&mut bounded, start, "Out", while_node, "In");
        let mut world = fixture.world(SceneGraph::new());
        world.attach_graph(&bounded).unwrap();
        world.start();
        assert_eq!(world.status().phase, RuntimePhase::Paused);
        assert!(world.status().last_error.is_some());
    }
}
