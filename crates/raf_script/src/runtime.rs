//! Per-attachment Rhai lifecycle. This module has no window or editor dependency.
use crate::backends::{
    rhai_backend::{self, CompiledRhai},
    ExecutionResult,
};
use crate::host_api::{
    AudioCommand, AudioCommandQueue, InputSnapshot, ScriptContext, ScriptEventQueue, TimeInfo,
};
use raf_core::scene::{SceneGraph, SceneNodeId};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
const REPORT_LIMIT: usize = 256;

#[derive(Debug, Clone)]
pub struct ScriptRuntimeOptions {
    /// True for the runtime manifest's exact root; false retains legacy assets-root callers.
    pub root_is_project: bool,
    pub max_operations: u64,
    /// Aggregate wall-clock budget for one lifecycle pass, not per attachment.
    pub max_time_ms: u64,
    pub max_entities: usize,
    pub max_instances: usize,
    pub max_source_bytes: usize,
    pub allow_rhai: bool,
    pub allow_wasm: bool,
    pub allow_nodes: bool,
    pub cancellation: Arc<AtomicBool>,
}
impl Default for ScriptRuntimeOptions {
    fn default() -> Self {
        Self {
            root_is_project: false,
            max_operations: 100_000,
            max_time_ms: 10,
            max_entities: 10_000,
            max_instances: 1024,
            max_source_bytes: 256 * 1024,
            allow_rhai: true,
            allow_wasm: false,
            allow_nodes: false,
            cancellation: Arc::new(AtomicBool::new(false)),
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct ScriptRuntimeReport {
    pub logs: Vec<String>,
    pub errors: Vec<String>,
    pub loaded_scripts: usize,
    pub skipped_scripts: usize,
    pub compiled_files: usize,
}
impl ScriptRuntimeReport {
    fn push_execution(
        &mut self,
        path: &str,
        owner: SceneNodeId,
        hook: &str,
        result: ExecutionResult,
    ) -> bool {
        let success = result.success;
        let owner_label = if owner.0 == usize::MAX {
            "session graph".to_string()
        } else {
            format!("owner {}", owner.0)
        };
        for log in result.logs {
            if self.logs.len() < REPORT_LIMIT {
                self.logs
                    .push(format!("{path} [{owner_label} / {hook}]: {log}"));
            }
        }
        for error in result.errors {
            self.error(path, format!("{owner_label} / {hook}: {error}"));
        }
        success
    }
    fn error(&mut self, path: &str, reason: impl Into<String>) {
        if self.errors.len() < REPORT_LIMIT {
            self.errors.push(format!("{path}: {}", reason.into()));
        }
    }
    fn merge(&mut self, other: Self) {
        self.loaded_scripts += other.loaded_scripts;
        self.skipped_scripts += other.skipped_scripts;
        self.compiled_files += other.compiled_files;
        self.logs.extend(
            other
                .logs
                .into_iter()
                .take(REPORT_LIMIT.saturating_sub(self.logs.len())),
        );
        self.errors.extend(
            other
                .errors
                .into_iter()
                .take(REPORT_LIMIT.saturating_sub(self.errors.len())),
        );
    }
}
#[derive(Debug, Clone)]
pub struct ScriptLoadProgress {
    pub completed: usize,
    pub total: usize,
    pub path: String,
    pub reused_compilation: bool,
}
struct RhaiScriptBinding {
    global: bool,
    path: String,
    file: PathBuf,
    owner: SceneNodeId,
    identity: Uuid,
    compiled: Arc<CompiledRhai>,
    scope: rhai::Scope<'static>,
    started: bool,
    disabled: bool,
    destroyed: bool,
}
pub struct RhaiScriptRuntime {
    pub view: crate::view::RuntimeViewState,
    engine: rhai::Engine,
    bindings: Vec<RhaiScriptBinding>,
    audio: AudioCommandQueue,
    events: ScriptEventQueue,
    elapsed: f32,
    instance_id: u64,
    options: ScriptRuntimeOptions,
    started: bool,
    stopped: bool,
    timestamps: HashMap<PathBuf, Option<SystemTime>>,
    tick_deadline: Option<Instant>,
    project_root: Option<PathBuf>,
}
impl RhaiScriptRuntime {
    pub fn load_from_scene(
        scene: &SceneGraph,
        root: Option<&Path>,
        options: ScriptRuntimeOptions,
    ) -> (Self, ScriptRuntimeReport) {
        Self::load_with_progress(scene, root, options, |_| {})
    }
    pub fn load_with_progress(
        scene: &SceneGraph,
        root: Option<&Path>,
        options: ScriptRuntimeOptions,
        mut progress: impl FnMut(ScriptLoadProgress),
    ) -> (Self, ScriptRuntimeReport) {
        let root = root.map(|base| {
            if !options.root_is_project
                && base.file_name().and_then(|s| s.to_str()) == Some("assets")
            {
                base.parent().unwrap_or(base)
            } else {
                base
            }
        });
        let mut runtime = Self {
            view: crate::view::RuntimeViewState::default(),
            engine: rhai_backend::create_engine(options.max_operations),
            bindings: Vec::new(),
            audio: AudioCommandQueue::default(),
            elapsed: 0.0,
            events: ScriptEventQueue::default(),
            instance_id: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            options,
            started: false,
            stopped: false,
            timestamps: HashMap::new(),
            tick_deadline: None,
            project_root: root.map(Path::to_path_buf),
        };
        let mut report = ScriptRuntimeReport::default();
        let mut cache: HashMap<PathBuf, Result<Arc<CompiledRhai>, String>> = HashMap::new();
        let total = scene
            .iter()
            .filter(|(id, _)| scene.is_valid_node(*id))
            .map(|(_, n)| n.scripts.len())
            .sum();
        let mut completed = 0;
        let mut source_bytes = 0usize;
        'owners: for (owner, node) in scene.iter().filter(|(id, _)| scene.is_valid_node(*id)) {
            let mut attached = HashSet::new();
            for path in &node.scripts {
                if runtime.options.cancellation.load(Ordering::Relaxed) {
                    report.error(path, "compilation cancelled");
                    break 'owners;
                }
                completed += 1;
                if runtime.bindings.len() >= runtime.options.max_instances {
                    report.error(path, "script attachment limit exceeded");
                    break 'owners;
                }
                let extension = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if extension != "rhai" || !runtime.options.allow_rhai {
                    report.skipped_scripts += 1;
                    report.error(
                        path,
                        if extension == "rhai" {
                            "Rhai is disabled"
                        } else {
                            "only Rhai attachments are executable; session Nodes use the graph compiler, native attachments are unsupported"
                        },
                    );
                    progress(ScriptLoadProgress {
                        completed,
                        total,
                        path: path.clone(),
                        reused_compilation: false,
                    });
                    continue;
                }
                let file = match resolve_script_path(root, path) {
                    Ok(file) => file,
                    Err(error) => {
                        report.error(path, error);
                        continue;
                    }
                };
                if !attached.insert(file.clone()) {
                    continue;
                }
                let reused = cache.contains_key(&file);
                let compiled = cache.entry(file.clone()).or_insert_with(|| {
                    let source = read_source(&file, runtime.options.max_source_bytes)?;
                    source_bytes = source_bytes.saturating_add(source.len());
                    if source_bytes > 16 * 1024 * 1024 {
                        return Err("aggregate script source budget exceeded".into());
                    }
                    report.compiled_files += 1;
                    rhai_backend::compile_source(&runtime.engine, path, &source)
                        .map(Arc::new)
                        .map_err(|e| e.to_string())
                });
                match compiled {
                    Ok(compiled) => {
                        runtime.bindings.push(RhaiScriptBinding {
                            global: false,
                            path: path.clone(),
                            file: file.clone(),
                            owner,
                            identity: node.uuid,
                            compiled: compiled.clone(),
                            scope: rhai::Scope::new(),
                            started: false,
                            disabled: false,
                            destroyed: false,
                        });
                        runtime.timestamps.insert(file.clone(), modified(&file));
                        report.loaded_scripts += 1;
                    }
                    Err(error) => report.error(path, error.clone()),
                }
                progress(ScriptLoadProgress {
                    completed,
                    total,
                    path: path.clone(),
                    reused_compilation: reused,
                });
            }
        }
        (runtime, report)
    }
    pub fn script_count(&self) -> usize {
        self.bindings.len()
    }
    /// A session graph has no implicit entity owner; it shares the Rhai sandbox.
    pub fn attach_graph(&mut self, graph: &raf_nodes::NodeGraph) -> ScriptRuntimeReport {
        let mut report = ScriptRuntimeReport::default();
        if !self.options.allow_nodes || graph.nodes.is_empty() {
            return report;
        }
        let result = raf_nodes::runtime_compiler::to_rhai(graph).and_then(|source| {
            rhai_backend::compile_source(&self.engine, "session:nodes", &source)
                .map_err(|e| e.to_string())
        });
        match result {
            Ok(compiled) => {
                if self.bindings.len() >= self.options.max_instances {
                    report.error("session:nodes", "Script attachment limit exceeded");
                    return report;
                }
                self.bindings.push(RhaiScriptBinding {
                    global: true,
                    path: "session:nodes".into(),
                    file: PathBuf::new(),
                    owner: SceneNodeId(usize::MAX),
                    identity: Uuid::nil(),
                    compiled: Arc::new(compiled),
                    scope: rhai::Scope::new(),
                    started: false,
                    disabled: false,
                    destroyed: false,
                });
                report.loaded_scripts = 1;
                report.compiled_files = 1;
            }
            Err(error) => report.error("session:nodes", error),
        }
        report
    }
    pub fn active_script_count(&self) -> usize {
        self.bindings
            .iter()
            .filter(|b| b.started && !b.disabled && !b.destroyed)
            .count()
    }
    pub fn call_start(&mut self, scene: &mut SceneGraph) -> ScriptRuntimeReport {
        if self.started || self.stopped {
            return ScriptRuntimeReport::default();
        }
        self.started = true;
        self.run(scene, &InputSnapshot::default(), "on_start", 0.0)
    }
    pub fn update(
        &mut self,
        scene: &mut SceneGraph,
        dt: f32,
        input: &InputSnapshot,
    ) -> ScriptRuntimeReport {
        if !self.started || self.stopped {
            return ScriptRuntimeReport::default();
        }
        if !dt.is_finite() || dt < 0.0 {
            let mut report = ScriptRuntimeReport::default();
            report.error("runtime", "delta time must be finite and nonnegative");
            return report;
        }
        self.elapsed += dt;
        self.tick_deadline =
            Some(Instant::now() + Duration::from_millis(self.options.max_time_ms.clamp(1, 100)));
        self.run(scene, input, "on_update", dt)
    }
    pub fn fixed_update(
        &mut self,
        scene: &mut SceneGraph,
        dt: f32,
        input: &InputSnapshot,
    ) -> ScriptRuntimeReport {
        if !self.started || self.stopped || !dt.is_finite() || dt <= 0.0 {
            return ScriptRuntimeReport::default();
        }
        let mut report = self.run(scene, input, "on_fixed_update", dt);
        report.merge(self.dispatch_events(scene, input, dt));
        report
    }
    pub fn late_update(
        &mut self,
        scene: &mut SceneGraph,
        dt: f32,
        input: &InputSnapshot,
    ) -> ScriptRuntimeReport {
        if !self.started || self.stopped || !dt.is_finite() || dt <= 0.0 {
            return ScriptRuntimeReport::default();
        }
        let report = self.run(scene, input, "on_late_update", dt);
        self.tick_deadline = None;
        report
    }
    pub fn call_destroy(&mut self, scene: &mut SceneGraph) -> ScriptRuntimeReport {
        if self.stopped {
            return ScriptRuntimeReport::default();
        }
        // Cancellation must not prevent bounded cleanup hooks.
        let mut report = self.run(scene, &InputSnapshot::default(), "on_destroy", 0.0);
        self.stopped = true;
        self.events.events.clear();
        report.logs.push("Script runtime stopped".into());
        report
    }
    pub fn drain_audio(&mut self) -> Vec<AudioCommand> {
        self.audio.drain()
    }

    fn run(
        &mut self,
        scene: &mut SceneGraph,
        input: &InputSnapshot,
        hook: &str,
        dt: f32,
    ) -> ScriptRuntimeReport {
        let deadline = if hook == "on_start" || hook == "on_destroy" {
            Instant::now() + Duration::from_millis(self.options.max_time_ms.clamp(1, 100))
        } else {
            self.tick_deadline.unwrap_or_else(|| {
                Instant::now() + Duration::from_millis(self.options.max_time_ms.clamp(1, 100))
            })
        };
        let mut report = ScriptRuntimeReport::default();
        let cleanup = hook == "on_destroy";
        let indices: Vec<usize> = if cleanup {
            (0..self.bindings.len()).rev().collect()
        } else {
            (0..self.bindings.len()).collect()
        };
        for index in indices {
            let binding = &mut self.bindings[index];
            if binding.destroyed || (binding.disabled && !cleanup) {
                continue;
            }
            if !binding.global
                && (!scene.is_valid_node(binding.owner)
                    || scene.get(binding.owner).map(|n| n.uuid) != Some(binding.identity))
            {
                binding.disabled = true;
                binding.destroyed = true;
                continue;
            }
            if cleanup && !binding.started {
                binding.destroyed = true;
                continue;
            }
            let mut ctx = ScriptContext {
                view: &mut self.view,
                scene,
                input,
                audio: &mut self.audio,
                events: &mut self.events,
                time: TimeInfo {
                    elapsed: self.elapsed,
                    delta_time: dt,
                },
                instance_id: self.instance_id,
                owner: (!binding.global).then_some(binding.owner),
                entity_limit: self.options.max_entities,
                cancellation: if cleanup {
                    None
                } else {
                    Some(&self.options.cancellation)
                },
                deadline: Some(deadline),
            };
            if hook == "on_start" && !binding.started {
                let initialized = rhai_backend::initialize_scope(
                    &self.engine,
                    &binding.compiled,
                    &mut ctx,
                    &mut binding.scope,
                );
                if !report.push_execution(
                    &binding.path,
                    binding.owner,
                    "initialization",
                    initialized,
                ) {
                    binding.disabled = true;
                    continue;
                }
                binding.started = true;
            }
            let result = rhai_backend::call_hook(
                &self.engine,
                &binding.compiled,
                &mut ctx,
                &mut binding.scope,
                hook,
                dt,
            );
            if !report.push_execution(&binding.path, binding.owner, hook, result) {
                binding.disabled = true;
            }
            if cleanup {
                binding.destroyed = true;
            }
        }
        report
    }
    pub fn emit_event(
        &mut self,
        name: &str,
        value: crate::value::ScriptValue,
    ) -> Result<(), String> {
        if self.events.events.len() >= 256 || name.is_empty() || name.len() > 128 {
            return Err("runtime event queue limit exceeded".into());
        }
        if !matches!(
            &value,
            crate::value::ScriptValue::None
                | crate::value::ScriptValue::Bool(_)
                | crate::value::ScriptValue::Int(_)
        ) && !matches!(&value, crate::value::ScriptValue::Float(v) if v.is_finite())
            && !matches!(&value, crate::value::ScriptValue::String(v) if v.len() <= 4096)
        {
            return Err("runtime event payload requires a bounded scalar".into());
        }
        self.events.events.push(crate::host_api::ScriptEvent {
            name: name.into(),
            value,
            target: None,
        });
        Ok(())
    }
    fn dispatch_events(
        &mut self,
        scene: &mut SceneGraph,
        input: &InputSnapshot,
        dt: f32,
    ) -> ScriptRuntimeReport {
        let events = std::mem::take(&mut self.events.events);
        let mut report = ScriptRuntimeReport::default();
        let deadline = self.tick_deadline.unwrap_or_else(|| {
            Instant::now() + Duration::from_millis(self.options.max_time_ms.clamp(1, 100))
        });
        for event in events {
            for binding in &mut self.bindings {
                if !binding.compiled.has_on_event
                    || !binding.started
                    || binding.disabled
                    || binding.destroyed
                    || (!binding.global && !scene.is_valid_node(binding.owner))
                {
                    continue;
                }
                let mut ctx = ScriptContext {
                    view: &mut self.view,
                    scene,
                    input,
                    audio: &mut self.audio,
                    events: &mut self.events,
                    time: TimeInfo {
                        elapsed: self.elapsed,
                        delta_time: dt,
                    },
                    instance_id: self.instance_id,
                    owner: (!binding.global).then_some(binding.owner),
                    entity_limit: self.options.max_entities,
                    cancellation: Some(&self.options.cancellation),
                    deadline: Some(deadline),
                };
                if event
                    .target
                    .is_some_and(|target| target.resolve(&ctx).ok() != Some(binding.owner))
                {
                    continue;
                }
                let result = rhai_backend::call_event(
                    &self.engine,
                    &binding.compiled,
                    &mut ctx,
                    &mut binding.scope,
                    &event.name,
                    event.value.clone(),
                );
                if !report.push_execution(&binding.path, binding.owner, "on_event", result) {
                    binding.disabled = true;
                }
            }
        }
        report
    }

    /// Called explicitly at a safe frame boundary, at most once a second by the host.
    /// Failed compilations keep the previous AST and persistent state.
    pub fn reload_changed(&mut self) -> ScriptRuntimeReport {
        let mut report = ScriptRuntimeReport::default();
        if self.stopped {
            return report;
        }
        let files: Vec<PathBuf> = self.timestamps.keys().cloned().collect();
        for file in files {
            let stamp = modified(&file);
            if self.timestamps.get(&file) == Some(&stamp) {
                continue;
            }
            self.timestamps.insert(file.clone(), stamp);
            let compiled =
                resolve_script_path(self.project_root.as_deref(), &file.to_string_lossy())
                    .and_then(|canonical| read_source(&canonical, self.options.max_source_bytes))
                    .and_then(|source| {
                        rhai_backend::compile_source(&self.engine, &file.to_string_lossy(), &source)
                            .map_err(|e| e.to_string())
                    });
            match compiled {
                Ok(compiled) => {
                    let compiled = Arc::new(compiled);
                    for binding in &mut self.bindings {
                        if binding.file == file && binding.started && !binding.destroyed {
                            binding.compiled = compiled.clone();
                            binding.disabled = false;
                        }
                    }
                    report.compiled_files += 1;
                    if report.logs.len() < REPORT_LIMIT {
                        report.logs.push(format!("Reloaded {} (persistent scope preserved; top-level initialization not rerun)", file.display()));
                    }
                }
                Err(error) => report.error(
                    &file.to_string_lossy(),
                    format!("reload rejected; previous code kept: {error}"),
                ),
            }
        }
        report
    }
}
fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}
fn read_source(path: &Path, limit: usize) -> Result<String, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut source = String::new();
    file.take(limit.min(256 * 1024) as u64 + 1)
        .read_to_string(&mut source)
        .map_err(|e| e.to_string())?;
    if source.len() > limit.min(256 * 1024) {
        return Err("script source size limit exceeded".into());
    }
    Ok(source)
}
fn resolve_script_path(base: Option<&Path>, script_path: &str) -> Result<PathBuf, String> {
    let base = base.ok_or("project root is required")?;
    let root = base
        .canonicalize()
        .map_err(|e| format!("project root unavailable: {e}"))?;
    let path = Path::new(script_path);
    let candidates = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else {
        vec![root.join(path), root.join("assets").join(path)]
    };
    for candidate in candidates {
        if let Ok(canonical) = candidate.canonicalize() {
            if !canonical.starts_with(&root) {
                return Err("script path escapes the project root".into());
            }
            if canonical.is_file() {
                return Ok(canonical);
            }
        }
    }
    Err("script file not found inside project".into())
}
/// Static validation uses exactly the same confinement and compiler as Play.
pub fn validate_rhai_file(root: Option<&Path>, path: &str) -> Result<CompiledRhai, String> {
    let root = root.map(|base| {
        if base.file_name().and_then(|s| s.to_str()) == Some("assets") {
            base.parent().unwrap_or(base)
        } else {
            base
        }
    });
    let file = resolve_script_path(root, path)?;
    let source = read_source(&file, 256 * 1024)?;
    rhai_backend::compile_source(&rhai_backend::create_engine(100_000), path, &source)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::Primitive;
    struct ProjectFixture(PathBuf);
    #[test]
    fn exact_project_root_named_assets_cannot_escape_to_parent() {
        let fixture = ProjectFixture::new("fn on_update(dt) {}");
        let assets = fixture.0.join("assets");
        fs::create_dir(&assets).unwrap();
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("A", Primitive::Empty);
        scene
            .get_mut(id)
            .unwrap()
            .scripts
            .push("../scripts/test.rhai".into());
        let (runtime, report) = RhaiScriptRuntime::load_from_scene(
            &scene,
            Some(&assets),
            ScriptRuntimeOptions {
                root_is_project: true,
                ..Default::default()
            },
        );
        assert_eq!(runtime.script_count(), 0);
        assert!(report.errors.iter().any(|error| error.contains("escapes")));
    }
    impl ProjectFixture {
        fn new(source: &str) -> Self {
            let root = std::env::temp_dir().join(format!("raf-script-tests-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("scripts")).unwrap();
            fs::write(root.join("scripts/test.rhai"), source).unwrap();
            Self(root)
        }
        fn scene(&self) -> SceneGraph {
            let mut scene = SceneGraph::new();
            for name in ["A", "B"] {
                let id = scene.add_root_with_primitive(name, Primitive::Cube);
                scene
                    .get_mut(id)
                    .unwrap()
                    .scripts
                    .push("scripts/test.rhai".into());
            }
            scene
        }
    }
    impl Drop for ProjectFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn attachments_share_compilation_but_not_state_or_owner() {
        let fixture = ProjectFixture::new(
            r#"
let count = 0;
fn on_start() { let me = self_node(); me.set_property("started", true); }
fn on_update(dt) {
    count += 1;
    let me = self_node(); me.set_property("count", count); me.move_by(dt, 0.0, 0.0);
}
fn on_destroy() { let me = self_node(); me.set_property("destroyed", true); }
"#,
        );
        let source = fixture.scene();
        let mut scene = source.clone();
        let (mut runtime, loaded) = RhaiScriptRuntime::load_from_scene(
            &scene,
            Some(&fixture.0),
            ScriptRuntimeOptions::default(),
        );
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        assert_eq!(loaded.loaded_scripts, 2);
        assert_eq!(loaded.compiled_files, 1);
        assert!(runtime.call_start(&mut scene).errors.is_empty());
        assert!(runtime
            .update(&mut scene, 0.25, &InputSnapshot::default())
            .errors
            .is_empty());
        assert!(runtime
            .update(&mut scene, 0.25, &InputSnapshot::default())
            .errors
            .is_empty());
        for (id, node) in scene.iter() {
            assert_eq!(node.position.x, 0.5);
            assert_eq!(
                node.get_variable("count"),
                Some(&raf_core::scene::VariableValue::Number(2.0))
            );
            assert_eq!(source.get(id).unwrap().position.x, 0.0);
        }
        assert!(runtime.call_destroy(&mut scene).errors.is_empty());
        assert!(runtime.call_destroy(&mut scene).logs.is_empty());
        assert!(runtime
            .update(&mut scene, 1.0, &InputSnapshot::default())
            .logs
            .is_empty());
    }
    #[test]
    fn missing_reference_never_mutates_first_entity_and_disables_once() {
        let fixture = ProjectFixture::new(
            r#"fn on_update(dt) { let h = get_node("missing"); h.move_by(1.0, 0.0, 0.0); }"#,
        );
        let mut scene = fixture.scene();
        let (mut runtime, _) = RhaiScriptRuntime::load_from_scene(
            &scene,
            Some(&fixture.0),
            ScriptRuntimeOptions::default(),
        );
        runtime.call_start(&mut scene);
        assert_eq!(
            runtime
                .update(&mut scene, 0.1, &InputSnapshot::default())
                .errors
                .len(),
            2
        );
        assert!(runtime
            .update(&mut scene, 0.1, &InputSnapshot::default())
            .errors
            .is_empty());
        assert!(scene.iter().all(|(_, n)| n.position.x == 0.0));
    }
    #[test]
    fn infinite_script_is_bounded_and_cancel_prevents_loading() {
        let fixture = ProjectFixture::new("fn on_update(dt) { loop {} }");
        let mut scene = fixture.scene();
        let options = ScriptRuntimeOptions {
            max_operations: 1000,
            ..ScriptRuntimeOptions::default()
        };
        let (mut runtime, _) =
            RhaiScriptRuntime::load_from_scene(&scene, Some(&fixture.0), options.clone());
        runtime.call_start(&mut scene);
        assert!(!runtime
            .update(&mut scene, 0.1, &InputSnapshot::default())
            .errors
            .is_empty());
        options.cancellation.store(true, Ordering::Relaxed);
        let (cancelled, report) =
            RhaiScriptRuntime::load_from_scene(&scene, Some(&fixture.0), options);
        assert_eq!(cancelled.script_count(), 0);
        assert!(!report.errors.is_empty());
    }
    #[test]
    fn rejects_outside_project_paths_and_deleted_handles() {
        let fixture = ProjectFixture::new("fn on_update(dt) {}");
        assert!(resolve_script_path(Some(&fixture.0), "../Cargo.toml").is_err());
        let mut scene = fixture.scene();
        let mut audio = AudioCommandQueue::default();
        let mut events = ScriptEventQueue::default();
        let input = InputSnapshot::default();
        let id = scene.roots()[0];
        let mut ctx = ScriptContext {
            view: &mut crate::view::RuntimeViewState::default(),
            scene: &mut scene,
            input: &input,
            audio: &mut audio,
            events: &mut events,
            time: TimeInfo::default(),
            instance_id: 1,
            owner: Some(id),
            entity_limit: 100,
            cancellation: None,
            deadline: None,
        };
        let handle = ctx.self_node().unwrap();
        ctx.instance_id = 2;
        assert!(!handle.is_valid(&ctx));
        ctx.instance_id = 1;
        ctx.scene.remove_node(id);
        assert!(!handle.is_valid(&ctx));
    }
    #[test]
    fn hot_reload_preserves_scope_and_rejects_invalid_code_without_replacing_it() {
        let fixture = ProjectFixture::new("let count = 0; fn on_update(dt) { count += 1; let me = self_node(); me.set_property(\"count\", count); }");
        let mut scene = fixture.scene();
        let (mut runtime, _) = RhaiScriptRuntime::load_from_scene(
            &scene,
            Some(&fixture.0),
            ScriptRuntimeOptions {
                max_time_ms: 100,
                ..Default::default()
            },
        );
        assert!(runtime.call_start(&mut scene).errors.is_empty());
        assert!(runtime
            .update(&mut scene, 0.1, &InputSnapshot::default())
            .errors
            .is_empty());
        let file = fixture.0.join("scripts/test.rhai").canonicalize().unwrap();
        fs::write(&file, "fn on_update(dt) { count += 10; let me = self_node(); me.set_property(\"count\", count); }").unwrap();
        runtime.timestamps.insert(file.clone(), None);
        assert!(runtime.reload_changed().errors.is_empty());
        assert!(runtime
            .update(&mut scene, 0.1, &InputSnapshot::default())
            .errors
            .is_empty());
        fs::write(&file, "fn on_update(").unwrap();
        runtime.timestamps.insert(file, None);
        assert!(!runtime.reload_changed().errors.is_empty());
        assert!(runtime
            .update(&mut scene, 0.1, &InputSnapshot::default())
            .errors
            .is_empty());
        assert!(scene.iter().all(|(_, node)| node.get_variable("count")
            == Some(&raf_core::scene::VariableValue::Number(21.0))));
    }
}
