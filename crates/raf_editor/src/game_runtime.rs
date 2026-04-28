use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::Context;
use glam::Vec3;
use raf_core::project::ProjectSettings;
use raf_core::scene::{Aabb, ColliderType, RigidBodyType, SceneGraph, SceneNodeId, VariableValue};
use raf_nodes::graph::NodeGraph;
use rhai::{AST, Dynamic, Engine, FLOAT, ImmutableString, Map, Scope};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

use crate::panels::node_editor::NodeEditorDocument;

#[derive(Debug, Clone, Default)]
pub struct RuntimeInputState {
    pressed_keys: HashSet<String>,
}

impl RuntimeInputState {
    pub fn from_egui(ctx: &Context) -> Self {
        let mappings = [
            (egui::Key::W, "W"),
            (egui::Key::A, "A"),
            (egui::Key::S, "S"),
            (egui::Key::D, "D"),
            (egui::Key::Q, "Q"),
            (egui::Key::E, "E"),
            (egui::Key::Space, "SPACE"),
            (egui::Key::Enter, "ENTER"),
            (egui::Key::Escape, "ESCAPE"),
            (egui::Key::ArrowUp, "UP"),
            (egui::Key::ArrowDown, "DOWN"),
            (egui::Key::ArrowLeft, "LEFT"),
            (egui::Key::ArrowRight, "RIGHT"),
            (egui::Key::Tab, "TAB"),
        ];

        let mut pressed_keys = HashSet::new();
        ctx.input(|input| {
            for (key, label) in mappings {
                if input.key_down(key) {
                    pressed_keys.insert(label.to_string());
                }
            }
            if input.modifiers.shift {
                pressed_keys.insert("SHIFT".to_string());
            }
        });

        Self { pressed_keys }
    }

    fn key_down(&self, key: &str) -> bool {
        self.pressed_keys.contains(&key.trim().to_uppercase())
    }
}

#[derive(Debug, Default)]
pub struct RuntimeReport {
    pub logs: Vec<String>,
    pub errors: Vec<String>,
}

pub struct GameRuntimeState {
    pub scene: SceneGraph,
    graphs: Vec<NodeGraph>,
    scripts: Vec<RhaiScriptInstance>,
    trigger_pairs: HashSet<(usize, usize)>,
    audio: AudioRuntime,
    assets_root: Option<PathBuf>,
    enable_physics: bool,
}

impl GameRuntimeState {
    pub fn start(
        source_scene: &SceneGraph,
        node_document: &NodeEditorDocument,
        assets_root: Option<PathBuf>,
        settings: &ProjectSettings,
    ) -> (Self, RuntimeReport) {
        let mut report = RuntimeReport::default();
        let mut runtime = Self {
            scene: source_scene.clone(),
            graphs: node_document.graphs.clone(),
            scripts: Vec::new(),
            trigger_pairs: HashSet::new(),
            audio: AudioRuntime::new(settings.enable_audio, &mut report),
            assets_root,
            enable_physics: settings.enable_physics,
        };

        runtime.load_scripts(&mut report);
        runtime.execute_node_event("On Start", &mut report);
        runtime.execute_script_event(ScriptHook::OnStart, RuntimeInputState::default(), 0.0, None, &mut report);
        runtime
            .audio
            .play_autoplay_sources(&runtime.scene, runtime.assets_root.as_deref(), &mut report);
        (runtime, report)
    }

    pub fn update(&mut self, delta_time: f32, input: RuntimeInputState) -> RuntimeReport {
        let mut report = RuntimeReport::default();
        self.execute_node_event("On Update", &mut report);
        self.execute_script_event(ScriptHook::OnUpdate, input.clone(), delta_time, None, &mut report);

        if self.enable_physics {
            let entered_pairs = self.step_physics(delta_time);
            for (left, right) in entered_pairs {
                let left_path = self.scene.node_path(left);
                let right_path = self.scene.node_path(right);

                if let (Some(left_path), Some(right_path)) = (left_path, right_path) {
                    self.execute_script_event(
                        ScriptHook::OnTriggerEnter,
                        input.clone(),
                        delta_time,
                        Some((left_path.clone(), right_path.clone())),
                        &mut report,
                    );
                    self.execute_script_event(
                        ScriptHook::OnTriggerEnter,
                        input.clone(),
                        delta_time,
                        Some((right_path, left_path)),
                        &mut report,
                    );
                }
            }
        }

        self.audio.cleanup_finished();
        report
    }

    fn load_scripts(&mut self, report: &mut RuntimeReport) {
        let Some(assets_root) = self.assets_root.clone() else {
            return;
        };

        for (id, node) in self.scene.iter() {
            let Some(entity_path) = self.scene.node_path(id) else {
                continue;
            };

            for relative_path in &node.scripts {
                let lower = relative_path.to_lowercase();
                if lower.ends_with(".rhai") {
                    let absolute_path = assets_root.join(relative_path);
                    match RhaiScriptInstance::load(id, entity_path.clone(), relative_path.clone(), &absolute_path) {
                        Ok(script) => self.scripts.push(script),
                        Err(error) => report.errors.push(error),
                    }
                } else if lower.ends_with(".lua")
                    || lower.ends_with(".rs")
                    || lower.ends_with(".cpp")
                    || lower.ends_with(".cc")
                    || lower.ends_with(".cxx")
                {
                    report.logs.push(format!(
                        "[runtime] Script '{}' attached to {} is detected, but editor play mode executes .rhai scripts directly in this toolchain.",
                        relative_path, entity_path
                    ));
                }
            }
        }
    }

    fn execute_node_event(&self, entry_name: &str, report: &mut RuntimeReport) {
        for graph in &self.graphs {
            for node in graph.nodes.iter().filter(|node| node.name == entry_name) {
                let output = raf_nodes::execute(graph, node.id);
                for log in output.logs {
                    report.logs.push(format!("[nodes:{}] {}", graph.name, log));
                }
                for error in output.errors {
                    report.errors.push(format!("[nodes:{}] {}", graph.name, error));
                }
            }
        }
    }

    fn execute_script_event(
        &mut self,
        hook: ScriptHook,
        input: RuntimeInputState,
        delta_time: f32,
        trigger_paths: Option<(String, String)>,
        report: &mut RuntimeReport,
    ) {
        if self.scripts.is_empty() {
            return;
        }

        let snapshot = Arc::new(RuntimeSceneSnapshot::from_scene(&self.scene));
        let mut pending_commands = Vec::new();

        for script in &self.scripts {
            let trigger_other = trigger_paths.as_ref().and_then(|(source, other)| {
                if &script.entity_path == source {
                    Some(other.clone())
                } else {
                    None
                }
            });

            match script.run(hook, snapshot.clone(), input.clone(), delta_time, trigger_other) {
                Ok(result) => {
                    pending_commands.extend(result.commands);
                    report.logs.extend(result.logs);
                    report.errors.extend(result.errors);
                }
                Err(error) => report.errors.push(error),
            }
        }

        self.apply_commands(pending_commands, report);
    }

    fn apply_commands(&mut self, commands: Vec<ScriptCommand>, report: &mut RuntimeReport) {
        for command in commands {
            match command {
                ScriptCommand::SetPosition { path, position } => {
                    if let Some(id) = self.scene.find_node_by_path(&path) {
                        if let Some(node) = self.scene.get_mut(id) {
                            node.position = position;
                        }
                    }
                }
                ScriptCommand::Translate { path, delta } => {
                    if let Some(id) = self.scene.find_node_by_path(&path) {
                        if let Some(node) = self.scene.get_mut(id) {
                            node.position += delta;
                        }
                    }
                }
                ScriptCommand::SetVariable { path, name, value } => {
                    if let Some(id) = self.scene.find_node_by_path(&path) {
                        if let Some(node) = self.scene.get_mut(id) {
                            node.set_variable(&name, value);
                        }
                    }
                }
                ScriptCommand::SetVelocity { path, velocity } => {
                    if let Some(id) = self.scene.find_node_by_path(&path) {
                        if let Some(node) = self.scene.get_mut(id) {
                            node.rigid_body.velocity = velocity;
                        }
                    }
                }
                ScriptCommand::PlayAudio { path } => {
                    self.audio.play_for_path(&self.scene, self.assets_root.as_deref(), &path, report);
                }
                ScriptCommand::StopAudio { path } => self.audio.stop_for_path(&path),
            }
        }
    }

    fn step_physics(&mut self, delta_time: f32) -> Vec<(SceneNodeId, SceneNodeId)> {
        let dynamic_ids: Vec<_> = self
            .scene
            .iter()
            .filter(|(_, node)| {
                node.rigid_body.enabled && node.rigid_body.body_type == RigidBodyType::Dynamic
            })
            .map(|(id, _)| id)
            .collect();

        for id in dynamic_ids {
            let (previous_position, previous_velocity) = match self.scene.get(id) {
                Some(node) => (node.position, node.rigid_body.velocity),
                None => continue,
            };

            if let Some(node) = self.scene.get_mut(id) {
                if node.rigid_body.use_gravity {
                    node.rigid_body.velocity.y -= 9.81 * delta_time;
                }
                let damping = (1.0 - node.rigid_body.damping * delta_time).clamp(0.0, 1.0);
                node.rigid_body.velocity *= damping;
                node.position += node.rigid_body.velocity * delta_time;
            }

            let collided = self
                .overlapping_colliders(id)
                .into_iter()
                .any(|other_id| !self.is_trigger_pair(id, other_id));

            if collided {
                if let Some(node) = self.scene.get_mut(id) {
                    node.position = previous_position;
                    node.rigid_body.velocity = if previous_velocity.y < 0.0 {
                        Vec3::new(previous_velocity.x, 0.0, previous_velocity.z)
                    } else {
                        Vec3::ZERO
                    };
                }
            }
        }

        let colliders: Vec<_> = self
            .scene
            .iter()
            .filter_map(|(id, _)| self.world_aabb(id).map(|aabb| (id, aabb)))
            .collect();

        let mut current_pairs = HashSet::new();
        for left in 0..colliders.len() {
            for right in (left + 1)..colliders.len() {
                let (left_id, left_aabb) = colliders[left];
                let (right_id, right_aabb) = colliders[right];
                if left_aabb.intersects(&right_aabb) && self.is_trigger_pair(left_id, right_id) {
                    current_pairs.insert(ordered_pair(left_id.0, right_id.0));
                }
            }
        }

        let entered = current_pairs
            .difference(&self.trigger_pairs)
            .cloned()
            .map(|(left, right)| (SceneNodeId(left), SceneNodeId(right)))
            .collect::<Vec<_>>();

        self.trigger_pairs = current_pairs;
        entered
    }

    fn overlapping_colliders(&self, source_id: SceneNodeId) -> Vec<SceneNodeId> {
        let Some(source_aabb) = self.world_aabb(source_id) else {
            return Vec::new();
        };

        self.scene
            .iter()
            .filter_map(|(other_id, _)| {
                if other_id == source_id {
                    return None;
                }
                self.world_aabb(other_id)
                    .filter(|other_aabb| source_aabb.intersects(other_aabb))
                    .map(|_| other_id)
            })
            .collect()
    }

    fn world_aabb(&self, id: SceneNodeId) -> Option<Aabb> {
        let node = self.scene.get(id)?;
        if !node.visible || node.collider.collider_type == ColliderType::None {
            return None;
        }

        let world = self.scene.world_matrix(id);
        let center = world.transform_point3(node.collider.aabb.center() + node.collider.offset);
        let half_extents = node.collider.aabb.half_extents() * node.scale.abs();
        Some(Aabb {
            min: center - half_extents,
            max: center + half_extents,
        })
    }

    fn is_trigger_pair(&self, left: SceneNodeId, right: SceneNodeId) -> bool {
        self.scene
            .get(left)
            .map(|node| node.rigid_body.is_trigger)
            .unwrap_or(false)
            || self
                .scene
                .get(right)
                .map(|node| node.rigid_body.is_trigger)
                .unwrap_or(false)
    }
}

#[derive(Clone)]
struct EntitySnapshot {
    path: String,
    name: String,
    parent_name: String,
    parent_path: String,
    position: Vec3,
    variables: HashMap<String, VariableValue>,
}

#[derive(Clone, Default)]
struct RuntimeSceneSnapshot {
    by_path: HashMap<String, EntitySnapshot>,
}

impl RuntimeSceneSnapshot {
    fn from_scene(scene: &SceneGraph) -> Self {
        let mut by_path = HashMap::new();
        for (id, node) in scene.iter() {
            let Some(path) = scene.node_path(id) else {
                continue;
            };
            let parent_name = node
                .parent
                .and_then(|parent_id| scene.get(parent_id))
                .map(|parent| parent.name.clone())
                .unwrap_or_default();
            let parent_path = node
                .parent
                .and_then(|parent_id| scene.node_path(parent_id))
                .unwrap_or_default();
            let variables = node
                .variables
                .iter()
                .map(|variable| (variable.name.clone(), variable.value.clone()))
                .collect();
            by_path.insert(
                path.clone(),
                EntitySnapshot {
                    path,
                    name: node.name.clone(),
                    parent_name,
                    parent_path,
                    position: node.position,
                    variables,
                },
            );
        }
        Self { by_path }
    }

    fn get(&self, path: &str) -> Option<&EntitySnapshot> {
        self.by_path.get(path)
    }
}

#[derive(Clone)]
struct ScriptCtx {
    path: String,
    name: String,
    parent_name: String,
    parent_path: String,
    position: Vec3,
    delta_time: f32,
    snapshot: Arc<RuntimeSceneSnapshot>,
    input: RuntimeInputState,
    commands: Arc<Mutex<Vec<ScriptCommand>>>,
}

impl ScriptCtx {
    fn name(&mut self) -> ImmutableString {
        self.name.clone().into()
    }

    fn path(&mut self) -> ImmutableString {
        self.path.clone().into()
    }

    fn parent_name(&mut self) -> ImmutableString {
        self.parent_name.clone().into()
    }

    fn parent_path(&mut self) -> ImmutableString {
        self.parent_path.clone().into()
    }

    fn delta_time(&mut self) -> f32 {
        self.delta_time
    }

    fn position(&mut self) -> Map {
        vec3_map(self.position)
    }

    fn key_down(&mut self, key: ImmutableString) -> bool {
        self.input.key_down(key.as_str())
    }

    fn exists(&mut self, path: ImmutableString) -> bool {
        self.snapshot.get(&normalize_path(path.as_str())).is_some()
    }

    fn find(&mut self, path: ImmutableString) -> ImmutableString {
        self.snapshot
            .get(&normalize_path(path.as_str()))
            .map(|entity| entity.path.clone().into())
            .unwrap_or_else(|| "".into())
    }

    fn get_var(&mut self, name: ImmutableString) -> Dynamic {
        self.snapshot
            .get(&self.path)
            .and_then(|entity| entity.variables.get(name.as_str()))
            .map(variable_to_dynamic)
            .unwrap_or(Dynamic::UNIT)
    }

    fn get_var_path(&mut self, path: ImmutableString, name: ImmutableString) -> Dynamic {
        self.snapshot
            .get(&normalize_path(path.as_str()))
            .and_then(|entity| entity.variables.get(name.as_str()))
            .map(variable_to_dynamic)
            .unwrap_or(Dynamic::UNIT)
    }

    fn set_var(&mut self, name: ImmutableString, value: Dynamic) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::SetVariable {
                path: self.path.clone(),
                name: name.to_string(),
                value: dynamic_to_variable(&value),
            });
        }
    }

    fn set_var_path(&mut self, path: ImmutableString, name: ImmutableString, value: Dynamic) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::SetVariable {
                path: normalize_path(path.as_str()),
                name: name.to_string(),
                value: dynamic_to_variable(&value),
            });
        }
    }

    fn translate(&mut self, x: f32, y: f32, z: f32) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::Translate {
                path: self.path.clone(),
                delta: Vec3::new(x, y, z),
            });
        }
    }

    fn translate_path(&mut self, path: ImmutableString, x: f32, y: f32, z: f32) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::Translate {
                path: normalize_path(path.as_str()),
                delta: Vec3::new(x, y, z),
            });
        }
    }

    fn set_position(&mut self, x: f32, y: f32, z: f32) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::SetPosition {
                path: self.path.clone(),
                position: Vec3::new(x, y, z),
            });
        }
    }

    fn set_position_path(&mut self, path: ImmutableString, x: f32, y: f32, z: f32) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::SetPosition {
                path: normalize_path(path.as_str()),
                position: Vec3::new(x, y, z),
            });
        }
    }

    fn set_velocity(&mut self, x: f32, y: f32, z: f32) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::SetVelocity {
                path: self.path.clone(),
                velocity: Vec3::new(x, y, z),
            });
        }
    }

    fn set_velocity_path(&mut self, path: ImmutableString, x: f32, y: f32, z: f32) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::SetVelocity {
                path: normalize_path(path.as_str()),
                velocity: Vec3::new(x, y, z),
            });
        }
    }

    fn play_audio(&mut self) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::PlayAudio {
                path: self.path.clone(),
            });
        }
    }

    fn play_audio_path(&mut self, path: ImmutableString) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::PlayAudio {
                path: normalize_path(path.as_str()),
            });
        }
    }

    fn stop_audio(&mut self) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(ScriptCommand::StopAudio {
                path: self.path.clone(),
            });
        }
    }
}

#[derive(Clone)]
enum ScriptCommand {
    SetPosition { path: String, position: Vec3 },
    Translate { path: String, delta: Vec3 },
    SetVariable { path: String, name: String, value: VariableValue },
    SetVelocity { path: String, velocity: Vec3 },
    PlayAudio { path: String },
    StopAudio { path: String },
}

#[derive(Debug, Clone, Copy)]
enum ScriptHook {
    OnStart,
    OnUpdate,
    OnTriggerEnter,
}

impl ScriptHook {
    fn function_name(self) -> &'static str {
        match self {
            Self::OnStart => "on_start",
            Self::OnUpdate => "on_update",
            Self::OnTriggerEnter => "on_trigger_enter",
        }
    }
}

struct ScriptRunResult {
    commands: Vec<ScriptCommand>,
    logs: Vec<String>,
    errors: Vec<String>,
}

struct RhaiScriptInstance {
    _entity_id: SceneNodeId,
    entity_path: String,
    relative_path: String,
    engine: Engine,
    ast: AST,
    logs: Arc<Mutex<Vec<String>>>,
    has_on_start: bool,
    has_on_update: bool,
    has_on_trigger_enter: bool,
}

impl RhaiScriptInstance {
    fn load(
        entity_id: SceneNodeId,
        entity_path: String,
        relative_path: String,
        absolute_path: &Path,
    ) -> Result<Self, String> {
        let source = std::fs::read_to_string(absolute_path).map_err(|error| {
            format!(
                "[runtime] Failed to read script '{}' for {}: {}",
                relative_path, entity_path, error
            )
        })?;
        let source_lower = source.to_lowercase();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let print_logs = logs.clone();
        let print_prefix = relative_path.clone();

        let mut engine = Engine::new();
        register_script_api(&mut engine);
        engine.on_print(move |text| {
            if let Ok(mut logs) = print_logs.lock() {
                logs.push(format!("[script:{}] {}", print_prefix, text));
            }
        });

        let ast = engine.compile(&source).map_err(|error| {
            format!(
                "[runtime] Failed to compile script '{}' for {}: {}",
                relative_path, entity_path, error
            )
        })?;

        Ok(Self {
            _entity_id: entity_id,
            entity_path,
            relative_path,
            engine,
            ast,
            logs,
            has_on_start: source_lower.contains("fn on_start"),
            has_on_update: source_lower.contains("fn on_update"),
            has_on_trigger_enter: source_lower.contains("fn on_trigger_enter"),
        })
    }

    fn run(
        &self,
        hook: ScriptHook,
        snapshot: Arc<RuntimeSceneSnapshot>,
        input: RuntimeInputState,
        delta_time: f32,
        trigger_other_path: Option<String>,
    ) -> Result<ScriptRunResult, String> {
        let should_run = match hook {
            ScriptHook::OnStart => self.has_on_start,
            ScriptHook::OnUpdate => self.has_on_update,
            ScriptHook::OnTriggerEnter => self.has_on_trigger_enter,
        };
        if !should_run {
            return Ok(ScriptRunResult {
                commands: Vec::new(),
                logs: Vec::new(),
                errors: Vec::new(),
            });
        }

        let Some(entity) = snapshot.get(&self.entity_path).cloned() else {
            return Ok(ScriptRunResult {
                commands: Vec::new(),
                logs: Vec::new(),
                errors: vec![format!(
                    "[runtime:{}] Entity {} disappeared from snapshot",
                    self.relative_path, self.entity_path
                )],
            });
        };

        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
        let commands = Arc::new(Mutex::new(Vec::new()));
        let context = ScriptCtx {
            path: entity.path,
            name: entity.name,
            parent_name: entity.parent_name,
            parent_path: entity.parent_path,
            position: entity.position,
            delta_time,
            snapshot,
            input,
            commands: commands.clone(),
        };

        let mut scope = Scope::new();
        let call_result = match hook {
            ScriptHook::OnTriggerEnter => self.engine.call_fn::<()>(
                &mut scope,
                &self.ast,
                hook.function_name(),
                (context, trigger_other_path.unwrap_or_default()),
            ),
            ScriptHook::OnStart | ScriptHook::OnUpdate => self
                .engine
                .call_fn::<()>(&mut scope, &self.ast, hook.function_name(), (context,)),
        };

        let mut errors = Vec::new();
        if let Err(error) = call_result {
            errors.push(format!(
                "[runtime:{}] {} failed on {}: {}",
                self.relative_path,
                hook.function_name(),
                self.entity_path,
                error
            ));
        }

        let logs = self.logs.lock().map(|logs| logs.clone()).unwrap_or_default();
        let commands = commands.lock().map(|commands| commands.clone()).unwrap_or_default();
        Ok(ScriptRunResult {
            commands,
            logs,
            errors,
        })
    }
}

fn register_script_api(engine: &mut Engine) {
    engine.register_type_with_name::<ScriptCtx>("RuntimeCtx");
    engine.register_get("name", ScriptCtx::name);
    engine.register_get("path", ScriptCtx::path);
    engine.register_get("parent_name", ScriptCtx::parent_name);
    engine.register_get("parent_path", ScriptCtx::parent_path);
    engine.register_get("delta_time", ScriptCtx::delta_time);
    engine.register_get("position", ScriptCtx::position);
    engine.register_fn("key_down", ScriptCtx::key_down);
    engine.register_fn("exists", ScriptCtx::exists);
    engine.register_fn("find", ScriptCtx::find);
    engine.register_fn("get_var", ScriptCtx::get_var);
    engine.register_fn("get_var_path", ScriptCtx::get_var_path);
    engine.register_fn("set_var", ScriptCtx::set_var);
    engine.register_fn("set_var_path", ScriptCtx::set_var_path);
    engine.register_fn("translate", ScriptCtx::translate);
    engine.register_fn("translate_path", ScriptCtx::translate_path);
    engine.register_fn("set_position", ScriptCtx::set_position);
    engine.register_fn("set_position_path", ScriptCtx::set_position_path);
    engine.register_fn("set_velocity", ScriptCtx::set_velocity);
    engine.register_fn("set_velocity_path", ScriptCtx::set_velocity_path);
    engine.register_fn("play_audio", ScriptCtx::play_audio);
    engine.register_fn("play_audio_path", ScriptCtx::play_audio_path);
    engine.register_fn("stop_audio", ScriptCtx::stop_audio);
}

struct AudioRuntime {
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sinks: HashMap<String, Sink>,
}

impl AudioRuntime {
    fn new(enabled: bool, report: &mut RuntimeReport) -> Self {
        if !enabled {
            return Self {
                _stream: None,
                handle: None,
                sinks: HashMap::new(),
            };
        }

        match OutputStream::try_default() {
            Ok((stream, handle)) => Self {
                _stream: Some(stream),
                handle: Some(handle),
                sinks: HashMap::new(),
            },
            Err(error) => {
                report.errors.push(format!("[runtime] Audio output unavailable: {}", error));
                Self {
                    _stream: None,
                    handle: None,
                    sinks: HashMap::new(),
                }
            }
        }
    }

    fn play_autoplay_sources(
        &mut self,
        scene: &SceneGraph,
        assets_root: Option<&Path>,
        report: &mut RuntimeReport,
    ) {
        let paths: Vec<_> = scene
            .iter()
            .filter_map(|(id, node)| {
                if node.audio_source.enabled && node.audio_source.autoplay {
                    scene.node_path(id)
                } else {
                    None
                }
            })
            .collect();

        for path in paths {
            self.play_for_path(scene, assets_root, &path, report);
        }
    }

    fn play_for_path(
        &mut self,
        scene: &SceneGraph,
        assets_root: Option<&Path>,
        path: &str,
        report: &mut RuntimeReport,
    ) {
        let Some(handle) = &self.handle else {
            return;
        };
        let Some(assets_root) = assets_root else {
            return;
        };
        let Some(node_id) = scene.find_node_by_path(path) else {
            return;
        };
        let Some(node) = scene.get(node_id) else {
            return;
        };
        if !node.audio_source.enabled || node.audio_source.clip.trim().is_empty() {
            return;
        }

        let clip_path = assets_root.join(&node.audio_source.clip);
        let file = match std::fs::File::open(&clip_path) {
            Ok(file) => file,
            Err(error) => {
                report.errors.push(format!(
                    "[runtime] Failed to open audio clip '{}' for {}: {}",
                    node.audio_source.clip, path, error
                ));
                return;
            }
        };

        let sink = match Sink::try_new(handle) {
            Ok(sink) => sink,
            Err(error) => {
                report.errors.push(format!("[runtime] Failed to create audio sink: {}", error));
                return;
            }
        };
        sink.set_volume(node.audio_source.volume.max(0.0));

        match Decoder::new(BufReader::new(file)) {
            Ok(decoder) => {
                if node.audio_source.looping {
                    sink.append(decoder.repeat_infinite());
                } else {
                    sink.append(decoder);
                }
                sink.play();
                self.sinks.insert(path.to_string(), sink);
            }
            Err(error) => {
                report.errors.push(format!(
                    "[runtime] Failed to decode audio clip '{}' for {}: {}",
                    clip_path.display(), path, error
                ));
            }
        }
    }

    fn stop_for_path(&mut self, path: &str) {
        if let Some(sink) = self.sinks.remove(path) {
            sink.stop();
        }
    }

    fn cleanup_finished(&mut self) {
        self.sinks.retain(|_, sink| !sink.empty());
    }
}

fn vec3_map(value: Vec3) -> Map {
    let mut map = Map::new();
    map.insert("x".into(), Dynamic::from(value.x as FLOAT));
    map.insert("y".into(), Dynamic::from(value.y as FLOAT));
    map.insert("z".into(), Dynamic::from(value.z as FLOAT));
    map
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", trimmed)
    }
}

fn ordered_pair(left: usize, right: usize) -> (usize, usize) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn variable_to_dynamic(value: &VariableValue) -> Dynamic {
    match value {
        VariableValue::Bool(value) => Dynamic::from(*value),
        VariableValue::Number(value) => Dynamic::from(*value as FLOAT),
        VariableValue::Text(value) => Dynamic::from(value.clone()),
    }
}

fn dynamic_to_variable(value: &Dynamic) -> VariableValue {
    if value.is::<bool>() {
        VariableValue::Bool(value.clone_cast::<bool>())
    } else if value.is::<i64>() {
        VariableValue::Number(value.clone_cast::<i64>() as f32)
    } else if value.is::<FLOAT>() {
        VariableValue::Number(value.clone_cast::<FLOAT>() as f32)
    } else if value.is::<ImmutableString>() {
        VariableValue::Text(value.clone_cast::<ImmutableString>().to_string())
    } else if value.is::<String>() {
        VariableValue::Text(value.clone_cast::<String>())
    } else {
        VariableValue::Text(value.to_string())
    }
}
