use std::path::PathBuf;

use egui::{Context, Key, PointerButton};
use raf_core::config::{ScriptExecutionMode, ScriptLanguage};
use raf_core::project::ProjectSettings;
use raf_core::SceneGraph;
use raf_script::{InputSnapshot, RhaiScriptRuntime, ScriptRuntimeOptions, ScriptRuntimeReport};

use crate::panels::node_editor::NodeEditorDocument;

#[derive(Debug, Clone, Default)]
pub struct RuntimeInputState {
    pub snapshot: InputSnapshot,
}

impl RuntimeInputState {
    pub fn from_egui(ctx: &Context) -> Self {
        let mut snapshot = InputSnapshot::default();

        ctx.input(|input| {
            for (key, label) in SCRIPT_KEYS {
                if input.key_down(*key) {
                    snapshot.keys_held.push((*label).to_string());
                }
                if input.key_pressed(*key) {
                    snapshot.keys_pressed.push((*label).to_string());
                }
            }

            push_modifier(&mut snapshot.keys_held, input.modifiers.ctrl, "ctrl");
            push_modifier(&mut snapshot.keys_held, input.modifiers.shift, "shift");
            push_modifier(&mut snapshot.keys_held, input.modifiers.alt, "alt");
            push_modifier(&mut snapshot.keys_held, input.modifiers.mac_cmd, "cmd");

            if input.pointer.button_down(PointerButton::Primary) {
                snapshot.mouse_held.push(0);
            }
            if input.pointer.button_down(PointerButton::Secondary) {
                snapshot.mouse_held.push(1);
            }
            if input.pointer.button_down(PointerButton::Middle) {
                snapshot.mouse_held.push(2);
            }
        });

        Self { snapshot }
    }
}

#[derive(Debug, Default)]
pub struct RuntimeReport {
    pub logs: Vec<String>,
    pub errors: Vec<String>,
}

impl RuntimeReport {
    fn extend_script_report(&mut self, report: ScriptRuntimeReport) {
        self.logs.extend(report.logs);
        self.errors.extend(report.errors);
    }
}

/// Editor-only cloned-scene harness prepared for a future runtime connection.
///
/// This type is not a shipping runtime and is intentionally kept behind the
/// editor's disabled build path while the runtime surface remains separate.
pub struct GameRuntimeState {
    pub scene: SceneGraph,
    scripts: Option<RhaiScriptRuntime>,
}

impl GameRuntimeState {
    pub fn start(
        source_scene: &SceneGraph,
        _node_document: &NodeEditorDocument,
        assets_root: Option<PathBuf>,
        settings: &ProjectSettings,
    ) -> (Self, RuntimeReport) {
        let mut scene = source_scene.clone();
        let mut report = RuntimeReport::default();
        let mut scripts = None;

        if scripts_enabled(settings) {
            let options = ScriptRuntimeOptions {
                allow_rhai: settings.allowed_script_languages.has(ScriptLanguage::Rhai),
                allow_wasm: settings.allowed_script_languages.has(ScriptLanguage::Cpp),
                allow_nodes: settings.allowed_script_languages.has(ScriptLanguage::Nodes),
                ..ScriptRuntimeOptions::default()
            };
            let (mut runtime, load_report) =
                RhaiScriptRuntime::load_from_scene(&scene, assets_root.as_deref(), options);
            let loaded_scripts = runtime.script_count();
            report.extend_script_report(load_report);

            if loaded_scripts > 0 {
                let start_report = runtime.call_start(&mut scene);
                report.extend_script_report(start_report);
                scripts = Some(runtime);
            }
        }

        (Self { scene, scripts }, report)
    }

    pub fn update(&mut self, delta_time: f32, input: RuntimeInputState) -> RuntimeReport {
        let mut report = RuntimeReport::default();

        if let Some(scripts) = self.scripts.as_mut() {
            let script_report = scripts.update(&mut self.scene, delta_time, &input.snapshot);
            report.extend_script_report(script_report);
        }

        report
    }
}

const SCRIPT_KEYS: &[(Key, &str)] = &[
    (Key::A, "a"),
    (Key::B, "b"),
    (Key::C, "c"),
    (Key::D, "d"),
    (Key::E, "e"),
    (Key::F, "f"),
    (Key::G, "g"),
    (Key::H, "h"),
    (Key::I, "i"),
    (Key::J, "j"),
    (Key::K, "k"),
    (Key::L, "l"),
    (Key::M, "m"),
    (Key::N, "n"),
    (Key::O, "o"),
    (Key::P, "p"),
    (Key::Q, "q"),
    (Key::R, "r"),
    (Key::S, "s"),
    (Key::T, "t"),
    (Key::U, "u"),
    (Key::V, "v"),
    (Key::W, "w"),
    (Key::X, "x"),
    (Key::Y, "y"),
    (Key::Z, "z"),
    (Key::ArrowUp, "arrow_up"),
    (Key::ArrowDown, "arrow_down"),
    (Key::ArrowLeft, "arrow_left"),
    (Key::ArrowRight, "arrow_right"),
    (Key::Space, "space"),
    (Key::Enter, "enter"),
    (Key::Escape, "escape"),
    (Key::Tab, "tab"),
    (Key::Backspace, "backspace"),
    (Key::Delete, "delete"),
];

fn scripts_enabled(settings: &ProjectSettings) -> bool {
    settings.enable_scripting && settings.script_execution_mode != ScriptExecutionMode::Disabled
}

fn push_modifier(target: &mut Vec<String>, enabled: bool, label: &str) {
    if enabled {
        target.push(label.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use raf_core::scene::Primitive;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn runtime_start_runs_attached_rhai_script_on_clone() {
        let project_root =
            std::env::temp_dir().join(format!("raf_editor_runtime_{}", Uuid::new_v4()));
        let script_dir = project_root.join("assets").join("scripts");
        fs::create_dir_all(&script_dir).unwrap();
        fs::write(
            script_dir.join("start.rhai"),
            r#"
fn on_start() {
    let body = get_node("Body");
    body.set_position(2.0, 0.0, 0.0);
}
"#,
        )
        .unwrap();

        let mut source_scene = SceneGraph::new();
        let body = source_scene.add_root_with_primitive("Body", Primitive::Cube);
        source_scene
            .get_mut(body)
            .unwrap()
            .scripts
            .push("scripts/start.rhai".to_string());

        let (runtime, report) = GameRuntimeState::start(
            &source_scene,
            &NodeEditorDocument::default(),
            Some(project_root.join("assets")),
            &ProjectSettings::default(),
        );

        assert!(report.errors.is_empty());
        assert_eq!(source_scene.get(body).unwrap().position, Vec3::ZERO);
        assert_eq!(
            runtime.scene.get(body).unwrap().position,
            Vec3::new(2.0, 0.0, 0.0)
        );

        let _ = fs::remove_dir_all(project_root);
    }
}
