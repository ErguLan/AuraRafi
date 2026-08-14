//! State and action host for the engine settings surface.
//!
//! The surface emits typed RafUI actions. This host owns the mutable draft and
//! returns only navigation/save intents to the application boundary.

use std::collections::HashSet;

use eframe::{egui, egui_wgpu};
use raf_core::ai::{AgentMode, AiProvider};
use raf_core::config::{
    EngineSettings, Language, RenderExecutionPolicy, RenderQuality, ScriptLanguage, TargetPlatform,
    Theme, ViewportRenderMode, AGENT_MESSAGE_PAGE_SIZE_MAX, AGENT_MESSAGE_PAGE_SIZE_MIN,
};
use raf_core::i18n::t;
use raf_core::units::DisplayUnit;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiControlState, UiDispatchedAction,
};

use crate::panels::raf_ui_surface_bridge::RafUiSurfaceBridge;
use crate::settings_surface::{build_settings_surface_with_api_keys, provider_id, SettingsSection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSurfaceIntent {
    Save,
    Cancel,
}

pub struct SettingsSurfaceHost {
    pub section: SettingsSection,
    surface: RafUiSurfaceBridge,
    revealed_api_keys: HashSet<AiProvider>,
}

impl Default for SettingsSurfaceHost {
    fn default() -> Self {
        Self {
            section: SettingsSection::default(),
            surface: RafUiSurfaceBridge::new("raf_ui_engine_settings"),
            revealed_api_keys: HashSet::new(),
        }
    }
}

impl SettingsSurfaceHost {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        _language: Language,
        settings: &mut EngineSettings,
    ) -> Vec<SettingsSurfaceIntent> {
        let language = settings.language;
        let revealed = self.revealed_api_keys.iter().copied().collect::<Vec<_>>();
        let surface =
            build_settings_surface_with_api_keys(palette, settings, self.section, &revealed);
        let script_editor = settings.script_external_editor_cmd.clone();
        let actions = self.surface.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                seed_text(
                    controls,
                    "settings.script_external_editor",
                    &script_editor,
                    256,
                );
                seed_numeric_settings(controls, settings);
                seed_provider_settings(controls, settings);
            },
            |key| t(key, language),
        );
        self.apply_actions(actions, settings)
    }

    fn apply_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        settings: &mut EngineSettings,
    ) -> Vec<SettingsSurfaceIntent> {
        let mut intents = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetToggle { key, value } => {
                    self.commit_numeric_settings(settings);
                    apply_toggle(settings, &key, value);
                }
                UiAction::SetRange { key, value } => {
                    self.commit_numeric_settings(settings);
                    apply_range(settings, &key, value);
                    self.sync_numeric_text(settings, &key);
                }
                UiAction::SetText { key, value } => {
                    if !key.ends_with(".text") {
                        self.commit_numeric_settings(settings);
                    }
                    apply_text(settings, &key, value);
                }
                UiAction::Command { name } => {
                    self.commit_numeric_settings(settings);
                    if let Some(section) = SettingsSection::from_command(&name) {
                        self.section = section;
                    } else if name == "settings.save" {
                        intents.push(SettingsSurfaceIntent::Save);
                    } else if name == "settings.cancel" {
                        intents.push(SettingsSurfaceIntent::Cancel);
                    } else if let Some(id) = name.strip_prefix("settings.ai_provider.reveal.") {
                        if let Some(provider) = provider_from_id(id) {
                            if !self.revealed_api_keys.insert(provider) {
                                self.revealed_api_keys.remove(&provider);
                            }
                        }
                    } else if let Some(id) = name.strip_prefix("settings.ai_provider.default.") {
                        if let Some(provider) = provider_from_id(id) {
                            settings.default_ai_provider = provider;
                        }
                    } else if let Some(index) = name
                        .strip_prefix("settings.ai_model.default:")
                        .and_then(|value| value.parse::<usize>().ok())
                    {
                        if let Some(shortcut) = settings.agent_model_shortcuts.get(index) {
                            settings.default_agent_model = shortcut.label.clone();
                        }
                    } else if let Some(index) = name
                        .strip_prefix("settings.ai_model.remove:")
                        .and_then(|value| value.parse::<usize>().ok())
                    {
                        if index < settings.agent_model_shortcuts.len() {
                            let removed = settings.agent_model_shortcuts.remove(index);
                            if settings.default_agent_model == removed.label {
                                settings.default_agent_model.clear();
                            }
                        }
                    } else {
                        apply_command(settings, &name);
                    }
                }
                _ => {}
            }
        }
        intents
    }

    fn commit_numeric_settings(&mut self, settings: &mut EngineSettings) {
        for key in GLOBAL_NUMERIC_KEYS {
            let text_key = format!("{key}.text");
            let text = self
                .surface
                .with_control_state_read(|controls| controls.text(&text_key).to_string());
            let Some(text) = text else { continue };
            let Ok(value) = text.trim().parse::<f32>() else {
                continue;
            };
            apply_range(settings, key, value);
            self.sync_numeric_text(settings, key);
        }
    }

    fn sync_numeric_text(&mut self, settings: &EngineSettings, range_key: &str) {
        let Some(value) = numeric_text_value(settings, range_key) else {
            return;
        };
        let text_key = format!("{range_key}.text");
        self.surface.with_control_state(|controls| {
            controls.set_text(text_key.as_str(), value.clone(), 32);
        });
    }
}

const GLOBAL_NUMERIC_KEYS: &[&str] = &[
    "settings.font_size",
    "settings.theme_experimental",
    "settings.ui_scale",
    "settings.fps_limit",
    "settings.grid_size",
    "settings.grid_load_distance",
    "settings.auto_save",
    "settings.move_sensitivity",
    "settings.rotate_sensitivity",
    "settings.scale_sensitivity",
    "settings.wasd_speed",
    "settings.gizmo_growth_scale",
    "settings.script_timeout_ms",
    "settings.agent_message_page_size",
    "settings.hierarchy_row_height",
    "settings.hierarchy_indent_width",
];

fn numeric_text_value(settings: &EngineSettings, key: &str) -> Option<String> {
    Some(match key {
        "settings.font_size" => format!("{:.1}", settings.font_size),
        "settings.theme_experimental" => format!("{:.0}", settings.theme_experimental),
        "settings.ui_scale" => format!("{:.2}", settings.ui_scale),
        "settings.fps_limit" => settings.fps_limit.to_string(),
        "settings.grid_size" => format!("{:.2}", settings.grid_size),
        "settings.grid_load_distance" => format!("{:.2}", settings.grid_load_distance),
        "settings.auto_save" => settings.auto_save_interval_seconds.to_string(),
        "settings.move_sensitivity" => format!("{:.2}", settings.move_gizmo_sensitivity),
        "settings.rotate_sensitivity" => format!("{:.2}", settings.rotate_gizmo_sensitivity),
        "settings.scale_sensitivity" => format!("{:.2}", settings.scale_gizmo_sensitivity),
        "settings.wasd_speed" => format!("{:.2}", settings.wasd_speed),
        "settings.gizmo_growth_scale" => format!("{:.0}", settings.gizmo_growth_scale),
        "settings.script_timeout_ms" => settings.script_timeout_ms.to_string(),
        "settings.agent_message_page_size" => settings.agent_message_page_size.to_string(),
        "settings.hierarchy_row_height" => format!("{:.0}", settings.hierarchy_row_height),
        "settings.hierarchy_indent_width" => format!("{:.0}", settings.hierarchy_indent_width),
        _ => return None,
    })
}

fn apply_toggle(settings: &mut EngineSettings, key: &str, value: bool) {
    match key {
        "settings.simple_mode" => settings.simple_mode = value,
        "settings.auto_ui_scale" => settings.auto_ui_scale = value,
        "settings.vsync" => settings.vsync = value,
        "settings.multithreading" => settings.multithreading = value,
        "settings.show_fps_counter" => settings.show_fps_counter = value,
        "settings.fps_unlimited" => {
            settings.fps_limit = if value { 0 } else { 60 };
        }
        "settings.show_grid" => settings.grid_visible = value,
        "settings.snap_to_grid" => settings.snap_to_grid = value,
        "settings.command_console_enabled" => settings.command_console_enabled = value,
        "settings.agent_streaming_enabled" => settings.agent_streaming_enabled = value,
        "settings.hierarchy_show_icons" => settings.hierarchy_show_icons = value,
        "settings.hierarchy_show_visibility" => settings.hierarchy_show_visibility = value,
        "settings.hierarchy_show_locked" => settings.hierarchy_show_locked = value,
        "settings.hierarchy_show_hidden" => settings.hierarchy_show_hidden = value,
        "settings.hierarchy_auto_reveal_selection" => {
            settings.hierarchy_auto_reveal_selection = value
        }
        "settings.hierarchy_expand_on_select" => settings.hierarchy_expand_on_select = value,
        "settings.hierarchy_animations" => settings.hierarchy_animations = value,
        "settings.electronics_show_minimap" => settings.electronics_show_minimap = value,
        "settings.electronics_show_status" => settings.electronics_show_status = value,
        "settings.script_runtime_enabled" => settings.script_runtime_enabled = value,
        "settings.script_hot_reload" => settings.script_hot_reload = value,
        "settings.show_viewport_labels" => settings.show_viewport_labels = value,
        "settings.solid_show_surface_edges" => settings.solid_show_surface_edges = value,
        "settings.solid_xray_mode" => settings.solid_xray_mode = value,
        "settings.solid_face_tonality" => settings.solid_face_tonality = value,
        "settings.invert_mouse_x" => settings.invert_mouse_x = value,
        "settings.invert_mouse_y" => settings.invert_mouse_y = value,
        "settings.focus_lock_enabled" => settings.focus_lock_enabled = value,
        "settings.invert_ws" => settings.invert_ws = value,
        "settings.uniform_scale_by_default" => settings.uniform_scale_by_default = value,
        "settings.responsive_layout" => settings.responsive_layout = value,
        "settings.headless" => settings.headless = value,
        key if key.starts_with("settings.ai_provider.") => {
            if let Some(id) = key.strip_prefix("settings.ai_provider.") {
                if let Some(config) = settings
                    .ai_providers
                    .iter_mut()
                    .find(|config| provider_id(config.provider) == id)
                {
                    config.enabled = value;
                }
            }
        }
        _ => {}
    }
}

fn apply_range(settings: &mut EngineSettings, key: &str, value: f32) {
    match key {
        "settings.font_size" => settings.font_size = value.clamp(10.0, 24.0),
        "settings.theme_experimental" => settings.theme_experimental = value.clamp(0.0, 100.0),
        "settings.ui_scale" if !settings.auto_ui_scale => settings.ui_scale = value.clamp(0.5, 3.0),
        "settings.fps_limit" if settings.fps_limit != 0 => {
            settings.fps_limit = value.round().clamp(15.0, 240.0) as u32
        }
        "settings.grid_size" if !settings.simple_mode => {
            settings.grid_size = value.clamp(0.1, 10.0)
        }
        "settings.grid_load_distance" if !settings.simple_mode => {
            settings.grid_load_distance = value.clamp(0.0, 500.0)
        }
        "settings.auto_save" if !settings.simple_mode => {
            settings.auto_save_interval_seconds = value.round().clamp(30.0, 600.0) as u32
        }
        "settings.move_sensitivity" => settings.move_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.rotate_sensitivity" => settings.rotate_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.scale_sensitivity" => settings.scale_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.wasd_speed" => settings.wasd_speed = value.clamp(0.5, 5.0),
        "settings.gizmo_growth_scale" => settings.gizmo_growth_scale = value.clamp(0.0, 100.0),
        "settings.script_timeout_ms" => {
            settings.script_timeout_ms = value.round().clamp(10.0, 1000.0) as u32
        }
        "settings.agent_message_page_size" => {
            settings.agent_message_page_size = value.round().clamp(
                AGENT_MESSAGE_PAGE_SIZE_MIN as f32,
                AGENT_MESSAGE_PAGE_SIZE_MAX as f32,
            ) as u32
        }
        "settings.hierarchy_row_height" => settings.hierarchy_row_height = value.clamp(20.0, 36.0),
        "settings.hierarchy_indent_width" => {
            settings.hierarchy_indent_width = value.clamp(8.0, 28.0)
        }
        _ => {}
    }
}

fn apply_text(settings: &mut EngineSettings, key: &str, value: String) {
    if key == "settings.script_external_editor" {
        settings.script_external_editor_cmd = value.chars().take(256).collect();
        return;
    }
    if let Some(rest) = key.strip_prefix("settings.ai_provider.") {
        let mut pieces = rest.splitn(2, '.');
        let Some(id) = pieces.next() else { return };
        let Some(field) = pieces.next() else { return };
        let Some(provider) = provider_from_id(id) else {
            return;
        };
        if let Some(config) = settings
            .ai_providers
            .iter_mut()
            .find(|config| config.provider == provider)
        {
            match field {
                "base_url" => config.base_url = value.chars().take(512).collect(),
                "model" => config.model = value.chars().take(256).collect(),
                "api_key" => config.api_key = value.chars().take(512).collect(),
                _ => {}
            }
        }
        return;
    }
    let Some(range_key) = key.strip_suffix(".text") else {
        return;
    };
    let Ok(value) = value.trim().parse::<f32>() else {
        return;
    };
    apply_range(settings, range_key, value);
}

fn seed_text(controls: &mut UiControlState, key: &str, value: &str, max_length: usize) {
    if !controls.has_text(key) {
        controls.set_text(key, value, max_length);
    }
}

fn seed_numeric_settings(controls: &mut UiControlState, settings: &EngineSettings) {
    let values = [
        (
            "settings.font_size.text",
            format!("{:.1}", settings.font_size),
        ),
        (
            "settings.theme_experimental.text",
            format!("{:.0}", settings.theme_experimental),
        ),
        (
            "settings.ui_scale.text",
            format!("{:.2}", settings.ui_scale),
        ),
        ("settings.fps_limit.text", settings.fps_limit.to_string()),
        (
            "settings.grid_size.text",
            format!("{:.2}", settings.grid_size),
        ),
        (
            "settings.grid_load_distance.text",
            format!("{:.2}", settings.grid_load_distance),
        ),
        (
            "settings.auto_save.text",
            settings.auto_save_interval_seconds.to_string(),
        ),
        (
            "settings.move_sensitivity.text",
            format!("{:.2}", settings.move_gizmo_sensitivity),
        ),
        (
            "settings.rotate_sensitivity.text",
            format!("{:.2}", settings.rotate_gizmo_sensitivity),
        ),
        (
            "settings.scale_sensitivity.text",
            format!("{:.2}", settings.scale_gizmo_sensitivity),
        ),
        (
            "settings.wasd_speed.text",
            format!("{:.2}", settings.wasd_speed),
        ),
        (
            "settings.gizmo_growth_scale.text",
            format!("{:.0}", settings.gizmo_growth_scale),
        ),
        (
            "settings.script_timeout_ms.text",
            settings.script_timeout_ms.to_string(),
        ),
        (
            "settings.agent_message_page_size.text",
            settings.agent_message_page_size.to_string(),
        ),
        (
            "settings.hierarchy_row_height.text",
            format!("{:.0}", settings.hierarchy_row_height),
        ),
        (
            "settings.hierarchy_indent_width.text",
            format!("{:.0}", settings.hierarchy_indent_width),
        ),
    ];
    for (key, value) in values {
        seed_numeric_text(controls, key, &value);
    }
}

fn seed_provider_settings(controls: &mut UiControlState, settings: &EngineSettings) {
    for config in &settings.ai_providers {
        let id = provider_id(config.provider);
        seed_text(
            controls,
            &format!("settings.ai_provider.{id}.base_url"),
            &config.base_url,
            512,
        );
        seed_text(
            controls,
            &format!("settings.ai_provider.{id}.model"),
            &config.model,
            256,
        );
        seed_text(
            controls,
            &format!("settings.ai_provider.{id}.api_key"),
            &config.api_key,
            512,
        );
    }
}

fn provider_from_id(id: &str) -> Option<AiProvider> {
    AiProvider::all()
        .iter()
        .copied()
        .find(|provider| provider_id(*provider) == id)
}

fn seed_numeric_text(controls: &mut UiControlState, key: &str, value: &str) {
    let current = controls.text(key);
    let stale_key_value =
        current == key || (current.starts_with("settings.") && current.ends_with(".text"));
    if !controls.has_text(key) || stale_key_value {
        controls.set_text(key, value, 32);
    }
}

fn apply_command(settings: &mut EngineSettings, command: &str) {
    match command {
        "settings.theme.dark" => settings.theme = Theme::Dark,
        "settings.theme.light" => settings.theme = Theme::Light,
        "settings.theme.system" => settings.theme = Theme::System,
        "settings.language.english" => settings.language = Language::English,
        "settings.language.spanish" => settings.language = Language::Spanish,
        "settings.quality.potato" => settings.render_quality = RenderQuality::Potato,
        "settings.quality.low" => settings.render_quality = RenderQuality::Low,
        "settings.quality.medium" => settings.render_quality = RenderQuality::Medium,
        "settings.quality.high" => settings.render_quality = RenderQuality::High,
        "settings.policy.auto" => settings.render_execution_policy = RenderExecutionPolicy::Auto,
        "settings.policy.cpu_only" => {
            settings.render_execution_policy = RenderExecutionPolicy::CpuOnly
        }
        "settings.policy.gpu_preferred" => {
            settings.render_execution_policy = RenderExecutionPolicy::GpuPreferred
        }
        "settings.units.metric" => settings.display_unit = DisplayUnit::Metric,
        "settings.units.imperial" => settings.display_unit = DisplayUnit::Imperial,
        "settings.viewport_render_mode.solid" => {
            settings.viewport_render_mode = ViewportRenderMode::Solid
        }
        "settings.viewport_render_mode.wireframe" => {
            settings.viewport_render_mode = ViewportRenderMode::Wireframe
        }
        "settings.viewport_render_mode.preview" => {
            settings.viewport_render_mode = ViewportRenderMode::Preview
        }
        "settings.script_language.rhai" => settings.default_script_language = ScriptLanguage::Rhai,
        "settings.script_language.cpp" => settings.default_script_language = ScriptLanguage::Cpp,
        "settings.script_language.nodes" => {
            settings.default_script_language = ScriptLanguage::Nodes
        }
        "settings.agent_mode.passive" => settings.agent_mode = AgentMode::Passive,
        "settings.agent_mode.active" => settings.agent_mode = AgentMode::Active,
        "settings.platform.desktop" => settings.target_platform = TargetPlatform::Desktop,
        "settings.platform.mobile" => settings.target_platform = TargetPlatform::Mobile,
        "settings.platform.web" => settings.target_platform = TargetPlatform::Web,
        "settings.platform.cloud" => settings.target_platform = TargetPlatform::Cloud,
        "settings.platform.console" => settings.target_platform = TargetPlatform::Console,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::ai::AiProvider;

    #[test]
    fn settings_host_applies_numeric_bounds_before_persistence() {
        let mut settings = EngineSettings::default();
        apply_range(&mut settings, "settings.font_size", 100.0);
        apply_range(&mut settings, "settings.fps_limit", 1.0);
        apply_range(&mut settings, "settings.agent_message_page_size", 2.0);
        assert_eq!(settings.font_size, 24.0);
        assert_eq!(settings.fps_limit, 15);
        assert_eq!(settings.agent_message_page_size, 4);

        apply_range(&mut settings, "settings.agent_message_page_size", 100.0);
        assert_eq!(settings.agent_message_page_size, 32);
    }

    #[test]
    fn settings_host_keeps_provider_changes_keyed_by_stable_ids() {
        let mut settings = EngineSettings::default();
        apply_toggle(&mut settings, "settings.ai_provider.openai", true);
        assert!(settings
            .ai_providers
            .iter()
            .find(|config| config.provider == AiProvider::OpenAI)
            .is_some_and(|config| config.enabled));
    }

    #[test]
    fn settings_host_applies_agent_streaming_toggle() {
        let mut settings = EngineSettings::default();
        assert!(settings.agent_streaming_enabled);

        apply_toggle(&mut settings, "settings.agent_streaming_enabled", false);

        assert!(!settings.agent_streaming_enabled);
    }

    #[test]
    fn settings_host_applies_hierarchy_display_bounds() {
        let mut settings = EngineSettings::default();
        apply_toggle(&mut settings, "settings.hierarchy_show_icons", false);
        apply_range(&mut settings, "settings.hierarchy_row_height", 100.0);
        apply_range(&mut settings, "settings.hierarchy_indent_width", 1.0);

        assert!(!settings.hierarchy_show_icons);
        assert_eq!(settings.hierarchy_row_height, 36.0);
        assert_eq!(settings.hierarchy_indent_width, 8.0);
    }

    #[test]
    fn settings_host_repairs_stale_numeric_translation_keys() {
        let mut controls = UiControlState::default();
        controls.set_text("settings.grid_size.text", "settings.grid_size.text", 32);

        seed_numeric_text(&mut controls, "settings.grid_size.text", "1.25");

        assert_eq!(controls.text("settings.grid_size.text"), "1.25");
    }
}
