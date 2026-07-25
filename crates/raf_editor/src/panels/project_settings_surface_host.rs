//! ApiGraphicBasic host for the retained per-project Settings surface.

use eframe::{egui, egui_wgpu};
use raf_core::config::{RenderPreset, ScriptExecutionMode, ScriptLanguage};
use raf_core::i18n::t;
use raf_core::project::Project;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiAction, UiDispatchedAction};

use crate::panels::raf_ui_surface_bridge::RafUiSurfaceBridge;
use crate::project_settings_surface::build_project_settings_surface;

pub struct ProjectSettingsSurfaceHost {
    bridge: RafUiSurfaceBridge,
}

impl Default for ProjectSettingsSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_project_settings_surface"),
        }
    }
}

impl ProjectSettingsSurfaceHost {
    /// Applies retained controls immediately to the active Project, preserving
    /// the legacy project.ron save transaction managed by `AuraRafiApp`.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        project: &mut Project,
        global_console_commands_enabled: &mut bool,
        language: raf_core::config::Language,
    ) -> bool {
        let mut changed = normalize_graphics_policy(project);
        let surface =
            build_project_settings_surface(palette, project, *global_console_commands_enabled);
        let default_scene_name = project.settings.default_scene_name.clone();
        let actions = self.bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                controls.set_text(
                    "project-settings.default-scene",
                    default_scene_name.clone(),
                    512,
                );
            },
            |key| t(key, language),
        );
        changed |= apply_actions(actions, project, global_console_commands_enabled);
        changed |= normalize_graphics_policy(project);
        changed
    }
}

fn apply_actions(
    actions: Vec<UiDispatchedAction>,
    project: &mut Project,
    global_console_commands_enabled: &mut bool,
) -> bool {
    let mut changed = false;
    for dispatched in actions {
        match dispatched.action {
            UiAction::SetToggle { key, value } => {
                changed |= apply_toggle(project, global_console_commands_enabled, &key, value)
            }
            UiAction::SetRange { key, value } => changed |= apply_range(project, &key, value),
            UiAction::SetText { key, value } => {
                if key == "project-settings.default-scene"
                    && project.settings.default_scene_name != value
                {
                    project.settings.default_scene_name = value;
                    changed = true;
                }
            }
            UiAction::Command { name } => changed |= apply_command(project, &name),
            _ => {}
        }
    }
    changed
}

fn apply_toggle(
    project: &mut Project,
    global_console_commands_enabled: &mut bool,
    key: &str,
    value: bool,
) -> bool {
    match key {
        "project-settings.show-hierarchy" => {
            set_bool(&mut project.settings.show_hierarchy_panel, value)
        }
        "project-settings.show-properties" => {
            set_bool(&mut project.settings.show_properties_panel, value)
        }
        "project-settings.enable-audio" => set_bool(&mut project.settings.enable_audio, value),
        "project-settings.enable-physics" => set_bool(&mut project.settings.enable_physics, value),
        "project-settings.pause-unfocused" => {
            set_bool(&mut project.settings.pause_when_unfocused, value)
        }
        "project-settings.enable-complements" => {
            set_bool(&mut project.settings.enable_complements, value)
        }
        "project-settings.enable-console" => {
            let project_changed = set_bool(&mut project.settings.enable_console_commands, value);
            let global_changed = set_bool(global_console_commands_enabled, value);
            project_changed || global_changed
        }
        "project-settings.enable-scripting" => {
            set_bool(&mut project.settings.enable_scripting, value)
        }
        "project-settings.script-language.rhai" => {
            set_script_language(project, ScriptLanguage::Rhai, value)
        }
        "project-settings.script-language.cpp" => {
            set_script_language(project, ScriptLanguage::Cpp, value)
        }
        "project-settings.script-language.nodes" => {
            set_script_language(project, ScriptLanguage::Nodes, value)
        }
        "project-settings.auto-attach-scripts" => {
            set_bool(&mut project.settings.auto_attach_scripts, value)
        }
        "project-settings.allow-gpu-features" => {
            set_bool(&mut project.settings.allow_gpu_features, value)
        }
        "project-settings.depth-accurate" => set_bool(&mut project.settings.depth_accurate, value),
        "project-settings.world-streaming" => {
            set_bool(&mut project.settings.world_streaming_enabled, value)
        }
        _ => false,
    }
}

fn apply_range(project: &mut Project, key: &str, value: f32) -> bool {
    match key {
        "project-settings.depth-resolution-scale" => set_f32(
            &mut project.settings.depth_resolution_scale,
            value.clamp(0.35, 1.0),
        ),
        "project-settings.stream-region-size" => set_f32(
            &mut project.settings.world_stream_region_size,
            value.clamp(32.0, 512.0),
        ),
        "project-settings.stream-radius" => set_u32(
            &mut project.settings.world_stream_load_radius,
            value.round().clamp(1.0, 8.0) as u32,
        ),
        "project-settings.stream-lod-bias" => set_i8(
            &mut project.settings.world_stream_lod_bias,
            value.round().clamp(0.0, 4.0) as i8,
        ),
        _ => false,
    }
}

fn apply_command(project: &mut Project, command: &str) -> bool {
    match command {
        "project-settings.save.standard" => set_bool(&mut project.settings.linear_save, false),
        "project-settings.save.linear" => set_bool(&mut project.settings.linear_save, true),
        "project-settings.script-mode.disabled" => {
            set_script_execution_mode(project, ScriptExecutionMode::Disabled)
        }
        "project-settings.script-mode.editor" => {
            set_script_execution_mode(project, ScriptExecutionMode::EditorOnly)
        }
        "project-settings.script-mode.runtime" => {
            set_script_execution_mode(project, ScriptExecutionMode::Runtime)
        }
        "project-settings.preset.potato" => set_render_preset(project, RenderPreset::Potato),
        "project-settings.preset.low" => set_render_preset(project, RenderPreset::Low),
        "project-settings.preset.medium" if project.settings.allow_gpu_features => {
            set_render_preset(project, RenderPreset::Medium)
        }
        "project-settings.preset.high" if project.settings.allow_gpu_features => {
            set_render_preset(project, RenderPreset::High)
        }
        _ => false,
    }
}

fn normalize_graphics_policy(project: &mut Project) -> bool {
    if !project.settings.allow_gpu_features
        && matches!(
            project.settings.runtime_render_preset,
            RenderPreset::Medium | RenderPreset::High
        )
    {
        project.settings.runtime_render_preset = RenderPreset::Low;
        true
    } else {
        false
    }
}

fn set_script_language(project: &mut Project, language: ScriptLanguage, enabled: bool) -> bool {
    let previous = project.settings.allowed_script_languages.has(language);
    if previous != enabled {
        project
            .settings
            .allowed_script_languages
            .set(language, enabled);
        true
    } else {
        false
    }
}

fn set_script_execution_mode(project: &mut Project, value: ScriptExecutionMode) -> bool {
    if project.settings.script_execution_mode != value {
        project.settings.script_execution_mode = value;
        true
    } else {
        false
    }
}

fn set_render_preset(project: &mut Project, value: RenderPreset) -> bool {
    if project.settings.runtime_render_preset != value {
        project.settings.runtime_render_preset = value;
        true
    } else {
        false
    }
}

fn set_bool(slot: &mut bool, value: bool) -> bool {
    if *slot != value {
        *slot = value;
        true
    } else {
        false
    }
}

fn set_f32(slot: &mut f32, value: f32) -> bool {
    if (*slot - value).abs() > f32::EPSILON {
        *slot = value;
        true
    } else {
        false
    }
}

fn set_u32(slot: &mut u32, value: u32) -> bool {
    if *slot != value {
        *slot = value;
        true
    } else {
        false
    }
}

fn set_i8(slot: &mut i8, value: i8) -> bool {
    if *slot != value {
        *slot = value;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::project::{ProjectSettings, ProjectType};

    fn project() -> Project {
        Project {
            id: uuid::Uuid::nil(),
            name: "project-settings-host".to_string(),
            project_type: ProjectType::Game,
            path: std::path::PathBuf::from("project-settings-host"),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            engine_version: "0.9.0".to_string(),
            settings: ProjectSettings::default(),
        }
    }

    #[test]
    fn gpu_gate_normalizes_advanced_project_presets() {
        let mut project = project();
        project.settings.runtime_render_preset = RenderPreset::High;
        assert!(normalize_graphics_policy(&mut project));
        assert_eq!(project.settings.runtime_render_preset, RenderPreset::Low);
    }

    #[test]
    fn console_toggle_stays_linked_to_the_global_switch() {
        let mut project = project();
        let mut global = false;
        assert!(apply_toggle(
            &mut project,
            &mut global,
            "project-settings.enable-console",
            true,
        ));
        assert!(project.settings.enable_console_commands);
        assert!(global);
    }
}
