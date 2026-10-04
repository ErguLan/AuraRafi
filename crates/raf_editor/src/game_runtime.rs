//! Editor-side cloned Game runtime harness.
//!
//! This is deliberately independent from the editor shell.  Winit supplies
//! the backend-neutral [`raf_core::InputSnapshot`], scripts run against a
//! cloned scene, and the authoring document is never mutated by Play-preview
//! code.

use std::path::PathBuf;

use raf_core::config::{ScriptExecutionMode, ScriptLanguage};
use raf_core::project::ProjectSettings;
use raf_core::{InputKey, InputSnapshot, SceneGraph};
use raf_script::{
    InputSnapshot as ScriptInputSnapshot, RhaiScriptRuntime, ScriptRuntimeOptions,
    ScriptRuntimeReport,
};

#[derive(Debug, Clone, Default)]
pub struct RuntimeInputState {
    pub snapshot: ScriptInputSnapshot,
}

impl RuntimeInputState {
    pub fn from_input(input: &InputSnapshot) -> Self {
        let mut snapshot = ScriptInputSnapshot::default();
        for (key, label) in SCRIPT_KEYS {
            if input.key_down(*key) {
                snapshot.keys_held.push((*label).to_string());
            }
            if input.key_pressed(*key) {
                snapshot.keys_pressed.push((*label).to_string());
            }
        }
        for (enabled, label) in [
            (input.modifiers.control, "ctrl"),
            (input.modifiers.shift, "shift"),
            (input.modifiers.alt, "alt"),
            (input.modifiers.command, "cmd"),
        ] {
            if enabled {
                snapshot.keys_held.push(label.to_string());
            }
        }
        for (button, label) in [
            (raf_core::PointerButton::Primary, 0),
            (raf_core::PointerButton::Secondary, 1),
            (raf_core::PointerButton::Middle, 2),
        ] {
            if input.button_down(button) {
                snapshot.mouse_held.push(label);
            }
        }
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

/// A preview runtime which owns only a cloned scene and its script state.
pub struct GameRuntimeState {
    pub scene: SceneGraph,
    scripts: Option<RhaiScriptRuntime>,
}

impl GameRuntimeState {
    pub fn start(
        source_scene: &SceneGraph,
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
            report.extend_script_report(load_report);
            if runtime.script_count() > 0 {
                report.extend_script_report(runtime.call_start(&mut scene));
                scripts = Some(runtime);
            }
        }
        (Self { scene, scripts }, report)
    }

    pub fn update(&mut self, delta_time: f32, input: RuntimeInputState) -> RuntimeReport {
        let mut report = RuntimeReport::default();
        if let Some(scripts) = self.scripts.as_mut() {
            report.extend_script_report(scripts.update(
                &mut self.scene,
                delta_time,
                &input.snapshot,
            ));
        }
        report
    }
}

fn scripts_enabled(settings: &ProjectSettings) -> bool {
    settings.enable_scripting && settings.script_execution_mode != ScriptExecutionMode::Disabled
}

const SCRIPT_KEYS: &[(InputKey, &str)] = &[
    (InputKey::A, "a"),
    (InputKey::B, "b"),
    (InputKey::C, "c"),
    (InputKey::D, "d"),
    (InputKey::E, "e"),
    (InputKey::F, "f"),
    (InputKey::G, "g"),
    (InputKey::H, "h"),
    (InputKey::I, "i"),
    (InputKey::J, "j"),
    (InputKey::K, "k"),
    (InputKey::L, "l"),
    (InputKey::M, "m"),
    (InputKey::N, "n"),
    (InputKey::O, "o"),
    (InputKey::P, "p"),
    (InputKey::Q, "q"),
    (InputKey::R, "r"),
    (InputKey::S, "s"),
    (InputKey::T, "t"),
    (InputKey::U, "u"),
    (InputKey::V, "v"),
    (InputKey::W, "w"),
    (InputKey::X, "x"),
    (InputKey::Y, "y"),
    (InputKey::Z, "z"),
    (InputKey::ArrowUp, "arrow_up"),
    (InputKey::ArrowDown, "arrow_down"),
    (InputKey::ArrowLeft, "arrow_left"),
    (InputKey::ArrowRight, "arrow_right"),
    (InputKey::Space, "space"),
    (InputKey::Enter, "enter"),
    (InputKey::Escape, "escape"),
    (InputKey::Tab, "tab"),
    (InputKey::Backspace, "backspace"),
    (InputKey::Delete, "delete"),
];
