//! Prepared script lifecycle harness.
//!
//! This module wires attached scene scripts to the existing Host API without
//! making the editor depend on backend internals. Rhai is the only executable
//! tier here; WASM and visual nodes are reported as unsupported prepared tiers
//! until their runners are wired. This is not the product game runtime by
//! itself.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use raf_core::scene::SceneGraph;

use crate::backends::rhai_backend::{self, CompiledRhai};
use crate::host_api::{AudioCommandQueue, InputSnapshot, ScriptContext, TimeInfo};

#[derive(Debug, Clone, Copy)]
pub struct ScriptRuntimeOptions {
    pub max_operations: u64,
    pub allow_rhai: bool,
    pub allow_wasm: bool,
    pub allow_nodes: bool,
}

impl Default for ScriptRuntimeOptions {
    fn default() -> Self {
        Self {
            max_operations: 100_000,
            allow_rhai: true,
            allow_wasm: false,
            allow_nodes: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptRuntimeReport {
    pub logs: Vec<String>,
    pub errors: Vec<String>,
    pub loaded_scripts: usize,
    pub skipped_scripts: usize,
}

impl ScriptRuntimeReport {
    fn push_execution(&mut self, path: &str, result: crate::backends::ExecutionResult) {
        self.logs.extend(result.logs);
        self.errors.extend(
            result
                .errors
                .into_iter()
                .map(|error| format!("{path}: {error}")),
        );
    }

    fn loaded(&mut self, path: &str) {
        self.loaded_scripts += 1;
        self.logs.push(format!("Loaded script: {path}"));
    }

    fn skipped(&mut self, path: &str, reason: impl Into<String>) {
        self.skipped_scripts += 1;
        self.logs
            .push(format!("Skipped script {path}: {}", reason.into()));
    }

    fn error(&mut self, path: &str, reason: impl Into<String>) {
        self.errors.push(format!("{path}: {}", reason.into()));
    }
}

struct RhaiScriptBinding {
    path: String,
    compiled: CompiledRhai,
}

pub struct RhaiScriptRuntime {
    engine: rhai::Engine,
    bindings: Vec<RhaiScriptBinding>,
    audio: AudioCommandQueue,
    elapsed: f32,
}

impl RhaiScriptRuntime {
    pub fn load_from_scene(
        scene: &SceneGraph,
        project_or_assets_root: Option<&Path>,
        options: ScriptRuntimeOptions,
    ) -> (Self, ScriptRuntimeReport) {
        let engine = rhai_backend::create_engine(options.max_operations);
        let mut runtime = Self {
            engine,
            bindings: Vec::new(),
            audio: AudioCommandQueue::default(),
            elapsed: 0.0,
        };
        let mut report = ScriptRuntimeReport::default();

        let mut unique_paths = HashSet::new();
        for (_, node) in scene.iter() {
            for script_path in &node.scripts {
                if unique_paths.insert(script_path.clone()) {
                    runtime.load_script(script_path, project_or_assets_root, options, &mut report);
                }
            }
        }

        (runtime, report)
    }

    pub fn script_count(&self) -> usize {
        self.bindings.len()
    }

    pub fn call_start(&mut self, scene: &mut SceneGraph) -> ScriptRuntimeReport {
        let input = InputSnapshot::default();
        let time = TimeInfo {
            elapsed: self.elapsed,
            delta_time: 0.0,
        };
        let mut report = ScriptRuntimeReport::default();

        for binding in &self.bindings {
            let mut ctx = ScriptContext {
                scene,
                input: &input,
                audio: &mut self.audio,
                time,
            };
            let result = rhai_backend::call_on_start(&self.engine, &binding.compiled, &mut ctx);
            report.push_execution(&binding.path, result);
        }

        report
    }

    pub fn update(
        &mut self,
        scene: &mut SceneGraph,
        delta_time: f32,
        input: &InputSnapshot,
    ) -> ScriptRuntimeReport {
        let dt = delta_time.max(0.0);
        self.elapsed += dt;
        let time = TimeInfo {
            elapsed: self.elapsed,
            delta_time: dt,
        };
        let mut report = ScriptRuntimeReport::default();

        for binding in &self.bindings {
            let mut ctx = ScriptContext {
                scene,
                input,
                audio: &mut self.audio,
                time,
            };
            let result =
                rhai_backend::call_on_update(&self.engine, &binding.compiled, &mut ctx, dt);
            report.push_execution(&binding.path, result);
        }

        report
    }

    fn load_script(
        &mut self,
        script_path: &str,
        project_or_assets_root: Option<&Path>,
        options: ScriptRuntimeOptions,
        report: &mut ScriptRuntimeReport,
    ) {
        let extension = Path::new(script_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match extension.as_str() {
            "rhai" if options.allow_rhai => {}
            "rhai" => {
                report.skipped(script_path, "Rhai is disabled by project settings");
                return;
            }
            "wasm" | "cpp" | "cc" | "cxx" if options.allow_wasm => {
                report.skipped(script_path, "WASM native module runtime is not wired yet");
                return;
            }
            "wasm" | "cpp" | "cc" | "cxx" => {
                report.skipped(script_path, "WASM native modules are disabled");
                return;
            }
            "nodes" | "graph" if options.allow_nodes => {
                report.skipped(script_path, "visual node runtime bridge is not wired yet");
                return;
            }
            "nodes" | "graph" => {
                report.skipped(script_path, "visual nodes are disabled");
                return;
            }
            _ => {
                report.skipped(script_path, "unsupported script extension");
                return;
            }
        }

        let Some(absolute_path) = resolve_script_path(project_or_assets_root, script_path) else {
            report.error(script_path, "script file not found");
            return;
        };

        let source = match fs::read_to_string(&absolute_path) {
            Ok(source) => source,
            Err(error) => {
                report.error(script_path, format!("failed to read script: {error}"));
                return;
            }
        };

        match rhai_backend::compile_source(&self.engine, script_path, &source) {
            Ok(compiled) => {
                self.bindings.push(RhaiScriptBinding {
                    path: script_path.to_string(),
                    compiled,
                });
                report.loaded(script_path);
            }
            Err(error) => report.error(script_path, error.to_string()),
        }
    }
}

fn resolve_script_path(base: Option<&Path>, script_path: &str) -> Option<PathBuf> {
    let path = Path::new(script_path);
    if path.is_absolute() && path.exists() {
        return Some(path.to_path_buf());
    }

    let base = base?;
    let mut candidates = vec![base.join(path)];

    if base.file_name().and_then(|name| name.to_str()) == Some("assets") {
        if let Some(project_root) = base.parent() {
            candidates.push(project_root.join(path));
        }
    } else {
        candidates.push(base.join("assets").join(path));
    }

    candidates.into_iter().find(|candidate| candidate.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use raf_core::scene::Primitive;
    use uuid::Uuid;

    #[test]
    fn runs_attached_rhai_start_and_update() {
        let project_root = temp_project_root();
        let script_dir = project_root.join("assets").join("scripts");
        fs::create_dir_all(&script_dir).unwrap();
        fs::write(
            script_dir.join("move.rhai"),
            r#"
fn on_start() {
    let body = get_node("Body");
    body.set_position(1.0, 2.0, 3.0);
}

fn on_update(dt) {
    let body = get_node("Body");
    body.move_by(dt, 0.0, 0.0);
}
"#,
        )
        .unwrap();

        let mut scene = SceneGraph::new();
        let body = scene.add_root_with_primitive("Body", Primitive::Cube);
        scene
            .get_mut(body)
            .unwrap()
            .scripts
            .push("scripts/move.rhai".to_string());

        let (mut runtime, load_report) = RhaiScriptRuntime::load_from_scene(
            &scene,
            Some(&project_root.join("assets")),
            ScriptRuntimeOptions::default(),
        );
        assert_eq!(runtime.script_count(), 1);
        assert!(load_report.errors.is_empty());

        let mut runtime_scene = scene.clone();
        let start_report = runtime.call_start(&mut runtime_scene);
        assert!(start_report.errors.is_empty(), "{:?}", start_report.errors);
        assert_eq!(
            runtime_scene.get(body).unwrap().position,
            Vec3::new(1.0, 2.0, 3.0)
        );

        let update_report = runtime.update(&mut runtime_scene, 0.5, &InputSnapshot::default());
        assert!(
            update_report.errors.is_empty(),
            "{:?}",
            update_report.errors
        );
        assert_eq!(
            runtime_scene.get(body).unwrap().position,
            Vec3::new(1.5, 2.0, 3.0)
        );

        let _ = fs::remove_dir_all(project_root);
    }

    #[test]
    fn reports_missing_attached_script() {
        let project_root = temp_project_root();
        let mut scene = SceneGraph::new();
        let body = scene.add_root_with_primitive("Body", Primitive::Cube);
        scene
            .get_mut(body)
            .unwrap()
            .scripts
            .push("scripts/missing.rhai".to_string());

        let (_runtime, report) = RhaiScriptRuntime::load_from_scene(
            &scene,
            Some(&project_root),
            ScriptRuntimeOptions::default(),
        );

        assert_eq!(report.loaded_scripts, 0);
        assert_eq!(report.errors.len(), 1);

        let _ = fs::remove_dir_all(project_root);
    }

    fn temp_project_root() -> PathBuf {
        std::env::temp_dir().join(format!("raf_script_runtime_{}", Uuid::new_v4()))
    }
}
