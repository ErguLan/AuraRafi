//! State/action host for project-local settings.

use eframe::{egui, egui_wgpu};
use raf_core::config::{RenderPreset, ScriptExecutionMode, ScriptLanguage};
use raf_core::project::Project;
use raf_core::{config::Language, i18n::t};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiControlState, UiDispatchedAction,
};

use crate::panels::raf_ui_surface_bridge::RafUiSurfaceBridge;
use crate::project_settings_surface::build_project_settings_surface;

pub struct ProjectSettingsSurfaceHost {
    surface: RafUiSurfaceBridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSettingsSurfaceIntent {
    Changed,
    ResetPanels,
}

impl Default for ProjectSettingsSurfaceHost {
    fn default() -> Self {
        Self {
            surface: RafUiSurfaceBridge::new("raf_ui_project_settings"),
        }
    }
}

impl ProjectSettingsSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        project: &mut Project,
        global_console_commands_enabled: bool,
    ) -> Vec<ProjectSettingsSurfaceIntent> {
        let mut intents = Vec::new();
        if normalize_graphics_policy(project) {
            intents.push(ProjectSettingsSurfaceIntent::Changed);
        }
        let surface =
            build_project_settings_surface(palette, project, global_console_commands_enabled);
        let scene_name = project.settings.default_scene_name.clone();
        let actions = self.surface.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                seed_text(
                    controls,
                    "project-settings.default_scene_name",
                    &scene_name,
                    128,
                );
                seed_project_numeric_settings(controls, project);
            },
            |key| t(key, language),
        );
        intents.extend(self.apply_actions(actions, project, global_console_commands_enabled));
        if normalize_graphics_policy(project) {
            intents.push(ProjectSettingsSurfaceIntent::Changed);
        }
        intents
    }

    fn apply_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        project: &mut Project,
        global_console_commands_enabled: bool,
    ) -> Vec<ProjectSettingsSurfaceIntent> {
        let mut intents = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetToggle { key, value } => {
                    self.commit_numeric_settings(project);
                    if apply_toggle(project, &key, value, global_console_commands_enabled) {
                        intents.push(ProjectSettingsSurfaceIntent::Changed);
                    }
                }
                UiAction::SetRange { key, value } => {
                    self.commit_numeric_settings(project);
                    if apply_range(project, &key, value) {
                        intents.push(ProjectSettingsSurfaceIntent::Changed);
                    }
                    self.sync_numeric_text(project, &key);
                }
                UiAction::SetText { key, value } => {
                    if !key.ends_with(".text") {
                        self.commit_numeric_settings(project);
                    }
                    if key == "project-settings.default_scene_name"
                        && project.settings.default_scene_name != value
                    {
                        project.settings.default_scene_name = value;
                        intents.push(ProjectSettingsSurfaceIntent::Changed);
                    } else if let Some(range_key) = key.strip_suffix(".text") {
                        if let Ok(value) = value.trim().parse::<f32>() {
                            if apply_range(project, range_key, value) {
                                intents.push(ProjectSettingsSurfaceIntent::Changed);
                            }
                        }
                    }
                }
                UiAction::Command { name } => {
                    self.commit_numeric_settings(project);
                    if name == "project-settings.reset-panels" {
                        intents.push(ProjectSettingsSurfaceIntent::ResetPanels);
                    } else if apply_command(project, &name) {
                        intents.push(ProjectSettingsSurfaceIntent::Changed);
                    }
                }
                _ => {}
            }
        }
        intents
    }

    fn commit_numeric_settings(&mut self, project: &mut Project) {
        for key in PROJECT_NUMERIC_KEYS {
            let text_key = format!("{key}.text");
            let text = self
                .surface
                .with_control_state_read(|controls| controls.text(&text_key).to_string());
            let Some(text) = text else { continue };
            let Ok(value) = text.trim().parse::<f32>() else {
                continue;
            };
            apply_range(project, key, value);
            self.sync_numeric_text(project, key);
        }
    }

    fn sync_numeric_text(&mut self, project: &Project, range_key: &str) {
        let Some(value) = project_numeric_text_value(project, range_key) else {
            return;
        };
        let text_key = format!("{range_key}.text");
        self.surface.with_control_state(|controls| {
            controls.set_text(text_key.as_str(), value.clone(), 32);
        });
    }
}

const PROJECT_NUMERIC_KEYS: &[&str] = &[
    "project-settings.depth-resolution-scale",
    "project-settings.stream-region-size",
    "project-settings.stream-radius",
    "project-settings.stream-lod-bias",
];

fn project_numeric_text_value(project: &Project, key: &str) -> Option<String> {
    Some(match key {
        "project-settings.depth-resolution-scale" => {
            format!("{:.2}", project.settings.depth_resolution_scale)
        }
        "project-settings.stream-region-size" => {
            format!("{:.0}", project.settings.world_stream_region_size)
        }
        "project-settings.stream-radius" => project.settings.world_stream_load_radius.to_string(),
        "project-settings.stream-lod-bias" => project.settings.world_stream_lod_bias.to_string(),
        _ => return None,
    })
}

fn seed_text(controls: &mut UiControlState, key: &str, value: &str, max_length: usize) {
    if !controls.has_text(key) {
        controls.set_text(key, value, max_length);
    }
}

fn seed_project_numeric_settings(controls: &mut UiControlState, project: &Project) {
    let values = [
        (
            "project-settings.depth-resolution-scale.text",
            format!("{:.2}", project.settings.depth_resolution_scale),
        ),
        (
            "project-settings.stream-region-size.text",
            format!("{:.0}", project.settings.world_stream_region_size),
        ),
        (
            "project-settings.stream-radius.text",
            project.settings.world_stream_load_radius.to_string(),
        ),
        (
            "project-settings.stream-lod-bias.text",
            project.settings.world_stream_lod_bias.to_string(),
        ),
    ];
    for (key, value) in values {
        seed_text(controls, key, &value, 32);
    }
}

fn apply_toggle(
    project: &mut Project,
    key: &str,
    value: bool,
    global_console_commands_enabled: bool,
) -> bool {
    if key == "project-settings.enable-console" && !global_console_commands_enabled {
        return false;
    }
    if key != "project-settings.enable-scripting"
        && (key.starts_with("project-settings.language.")
            || key == "project-settings.auto-attach-scripts")
        && !project.settings.enable_scripting
    {
        return false;
    }
    let setting = match key {
        "project-settings.show-hierarchy" => &mut project.settings.show_hierarchy_panel,
        "project-settings.show-properties" => &mut project.settings.show_properties_panel,
        "project-settings.enable-audio" => &mut project.settings.enable_audio,
        "project-settings.enable-physics" => &mut project.settings.enable_physics,
        "project-settings.pause-unfocused" => &mut project.settings.pause_when_unfocused,
        "project-settings.enable-complements" => &mut project.settings.enable_complements,
        "project-settings.enable-console" => &mut project.settings.enable_console_commands,
        "project-settings.enable-scripting" => &mut project.settings.enable_scripting,
        "project-settings.auto-attach-scripts" => &mut project.settings.auto_attach_scripts,
        "project-settings.allow-gpu-features" => &mut project.settings.allow_gpu_features,
        "project-settings.depth-accurate" => &mut project.settings.depth_accurate,
        "project-settings.world-streaming" => &mut project.settings.world_streaming_enabled,
        key if key.starts_with("project-settings.language.") => {
            let language = match key.rsplit('.').next() {
                Some("rhai") => ScriptLanguage::Rhai,
                Some("cpp") => ScriptLanguage::Cpp,
                Some("nodes") => ScriptLanguage::Nodes,
                _ => return false,
            };
            let before = project.settings.allowed_script_languages.has(language);
            project
                .settings
                .allowed_script_languages
                .set(language, value);
            return before != value;
        }
        _ => return false,
    };
    if *setting == value {
        false
    } else {
        *setting = value;
        true
    }
}

fn apply_range(project: &mut Project, key: &str, value: f32) -> bool {
    match key {
        "project-settings.depth-resolution-scale" if project.settings.depth_accurate => set_f32(
            &mut project.settings.depth_resolution_scale,
            value.clamp(0.35, 1.0),
        ),
        "project-settings.stream-region-size" if project.settings.world_streaming_enabled => {
            set_f32(
                &mut project.settings.world_stream_region_size,
                value.clamp(32.0, 512.0),
            )
        }
        "project-settings.stream-radius" if project.settings.world_streaming_enabled => set_u32(
            &mut project.settings.world_stream_load_radius,
            value.round().clamp(1.0, 8.0) as u32,
        ),
        "project-settings.stream-lod-bias" if project.settings.world_streaming_enabled => set_i8(
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
        "project-settings.script-mode.disabled" if project.settings.enable_scripting => set_value(
            &mut project.settings.script_execution_mode,
            ScriptExecutionMode::Disabled,
        ),
        "project-settings.script-mode.editor" if project.settings.enable_scripting => set_value(
            &mut project.settings.script_execution_mode,
            ScriptExecutionMode::EditorOnly,
        ),
        "project-settings.script-mode.runtime" if project.settings.enable_scripting => set_value(
            &mut project.settings.script_execution_mode,
            ScriptExecutionMode::Runtime,
        ),
        "project-settings.preset.potato" => set_value(
            &mut project.settings.runtime_render_preset,
            RenderPreset::Potato,
        ),
        "project-settings.preset.low" => set_value(
            &mut project.settings.runtime_render_preset,
            RenderPreset::Low,
        ),
        "project-settings.preset.medium" if project.settings.allow_gpu_features => set_value(
            &mut project.settings.runtime_render_preset,
            RenderPreset::Medium,
        ),
        "project-settings.preset.high" if project.settings.allow_gpu_features => set_value(
            &mut project.settings.runtime_render_preset,
            RenderPreset::High,
        ),
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

fn set_bool(target: &mut bool, value: bool) -> bool {
    if *target == value {
        false
    } else {
        *target = value;
        true
    }
}

fn set_f32(target: &mut f32, value: f32) -> bool {
    if (*target - value).abs() <= f32::EPSILON {
        false
    } else {
        *target = value;
        true
    }
}

fn set_u32(target: &mut u32, value: u32) -> bool {
    if *target == value {
        false
    } else {
        *target = value;
        true
    }
}

fn set_i8(target: &mut i8, value: i8) -> bool {
    if *target == value {
        false
    } else {
        *target = value;
        true
    }
}

fn set_value<T: PartialEq>(target: &mut T, value: T) -> bool {
    if *target == value {
        false
    } else {
        *target = value;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::project::{ProjectSettings, ProjectType};
    use std::path::PathBuf;
    use uuid::Uuid;

    fn project() -> Project {
        let now = chrono::Utc::now();
        Project {
            id: Uuid::new_v4(),
            name: "Demo".to_string(),
            project_type: ProjectType::Game,
            path: PathBuf::from("."),
            created_at: now,
            modified_at: now,
            engine_version: "0.9.0".to_string(),
            settings: ProjectSettings::default(),
        }
    }

    #[test]
    fn project_settings_host_applies_and_bounds_streaming_values() {
        let mut value = project();
        value.settings.world_streaming_enabled = true;
        assert!(apply_range(
            &mut value,
            "project-settings.stream-region-size",
            1000.0
        ));
        assert_eq!(value.settings.world_stream_region_size, 512.0);
    }
}
