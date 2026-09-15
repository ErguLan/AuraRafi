//! Settings intent adapters for the native workbench.
//!
//! These functions translate retained RafUI settings intents into the
//! persisted EngineSettings model. They are kept outside the workbench
//! lifecycle coordinator so editor composition does not own settings policy.

use raf_core::ai::AiProvider;
use raf_core::config::EngineSettings;

pub(crate) fn apply_settings_toggle(settings: &mut EngineSettings, key: &str, value: bool) -> bool {
    match key {
        "settings.simple_mode" => settings.simple_mode = value,
        "settings.prefers_reduced_motion" => settings.prefers_reduced_motion = value,
        "settings.high_contrast" => settings.high_contrast = value,
        "settings.reduce_transparency" => settings.reduce_transparency = value,
        "settings.show_fps_counter" => settings.show_fps_counter = value,
        "settings.auto_ui_scale" | "settings.auto-ui-scale" => settings.auto_ui_scale = value,
        "settings.vsync" => settings.vsync = value,
        "settings.multithreading" => settings.multithreading = value,
        "settings.grid_visible" | "settings.show_grid" => settings.grid_visible = value,
        "settings.snap_to_grid" => settings.snap_to_grid = value,
        "settings.inspector_live_transform_updates" => {
            settings.inspector_live_transform_updates = value
        }
        "settings.command_console_enabled" => settings.command_console_enabled = value,
        "settings.ai_persist_credentials" => settings.ai_persist_credentials = value,
        "settings.hierarchy_show_icons" => settings.hierarchy_show_icons = value,
        "settings.hierarchy_show_visibility" => settings.hierarchy_show_visibility = value,
        "settings.hierarchy_show_locked" => settings.hierarchy_show_locked = value,
        "settings.hierarchy_show_hidden" => settings.hierarchy_show_hidden = value,
        "settings.hierarchy_auto_reveal_selection" => {
            settings.hierarchy_auto_reveal_selection = value
        }
        "settings.hierarchy_expand_on_select" => settings.hierarchy_expand_on_select = value,
        "settings.hierarchy_animations" => settings.hierarchy_animations = value,
        "settings.electronics_show_status" => settings.electronics_show_status = value,
        "settings.show_viewport_labels" => settings.show_viewport_labels = value,
        "settings.solid_show_surface_edges" => settings.solid_show_surface_edges = value,
        "settings.solid_xray_mode" => settings.solid_xray_mode = value,
        "settings.solid_face_tonality" => settings.solid_face_tonality = value,
        "settings.invert_mouse_x" => settings.invert_mouse_x = value,
        "settings.invert_mouse_y" => settings.invert_mouse_y = value,
        "settings.uniform_scale_by_default" => settings.uniform_scale_by_default = value,
        "settings.multi_select_gizmo_enabled" => settings.multi_select_gizmo_enabled = value,
        "settings.invert_ws" => settings.invert_ws = value,
        "settings.focus_lock_enabled" => settings.focus_lock_enabled = value,
        "settings.responsive_layout" => settings.responsive_layout = value,
        "settings.headless" => settings.headless = value,
        "settings.script_runtime_enabled" => settings.script_runtime_enabled = value,
        "settings.script_hot_reload" => settings.script_hot_reload = value,
        "settings.agent_streaming_enabled" => settings.agent_streaming_enabled = value,
        "settings.agent_tool_call_limit_enabled" => settings.agent_tool_call_limit_enabled = value,
        "settings.theme.dark" if value => settings.theme = raf_core::config::Theme::Dark,
        "settings.theme.light" if value => settings.theme = raf_core::config::Theme::Light,
        "settings.theme.system" if value => settings.theme = raf_core::config::Theme::System,
        "settings.language.english" if value => {
            settings.language = raf_core::config::Language::English
        }
        "settings.language.spanish" if value => {
            settings.language = raf_core::config::Language::Spanish
        }
        "settings.quality.potato" if value => {
            settings.render_quality = raf_core::config::RenderQuality::Potato;
            settings.render_preset = raf_core::config::RenderPreset::Potato;
        }
        "settings.quality.low" if value => {
            settings.render_quality = raf_core::config::RenderQuality::Low;
            settings.render_preset = raf_core::config::RenderPreset::Low;
        }
        "settings.quality.medium" if value => {
            settings.render_quality = raf_core::config::RenderQuality::Medium;
            settings.render_preset = raf_core::config::RenderPreset::Medium;
        }
        "settings.quality.high" if value => {
            settings.render_quality = raf_core::config::RenderQuality::High;
            settings.render_preset = raf_core::config::RenderPreset::High;
        }
        "settings.render_execution_policy.auto" if value => {
            settings.render_execution_policy = raf_core::config::RenderExecutionPolicy::Auto
        }
        "settings.render_execution_policy.cpu_only" if value => {
            settings.render_execution_policy = raf_core::config::RenderExecutionPolicy::CpuOnly
        }
        "settings.render_execution_policy.gpu_preferred" if value => {
            settings.render_execution_policy = raf_core::config::RenderExecutionPolicy::GpuPreferred
        }
        "settings.units.metric" if value => {
            settings.display_unit = raf_core::units::DisplayUnit::Metric
        }
        "settings.units.imperial" if value => {
            settings.display_unit = raf_core::units::DisplayUnit::Imperial
        }
        "settings.units.game" if value => {
            settings.display_unit = raf_core::units::DisplayUnit::Game
        }
        "settings.viewport_render_mode.solid" if value => {
            settings.viewport_render_mode = raf_core::config::ViewportRenderMode::Solid
        }
        "settings.script_language.rhai" if value => {
            settings.default_script_language = raf_core::config::ScriptLanguage::Rhai
        }
        "settings.script_language.cpp" if value => {
            settings.default_script_language = raf_core::config::ScriptLanguage::Cpp
        }
        "settings.script_language.nodes" if value => {
            settings.default_script_language = raf_core::config::ScriptLanguage::Nodes
        }
        "settings.agent_mode.inspect" if value => {
            settings.agent_mode = raf_core::ai::AgentMode::Inspect
        }
        "settings.agent_mode.passive" | "settings.agent_mode.plan" if value => {
            settings.agent_mode = raf_core::ai::AgentMode::Plan
        }
        "settings.agent_mode.active" => {
            settings.agent_mode = if value {
                raf_core::ai::AgentMode::Active
            } else {
                raf_core::ai::AgentMode::Plan
            }
        }
        "settings.platform.desktop" if value => {
            settings.target_platform = raf_core::config::TargetPlatform::Desktop
        }
        "settings.platform.mobile" if value => {
            settings.target_platform = raf_core::config::TargetPlatform::Mobile
        }
        "settings.platform.web" if value => {
            settings.target_platform = raf_core::config::TargetPlatform::Web
        }
        "settings.platform.cloud" if value => {
            settings.target_platform = raf_core::config::TargetPlatform::Cloud
        }
        "settings.platform.console" if value => {
            settings.target_platform = raf_core::config::TargetPlatform::Console
        }
        "settings.fps_unlimited" => {
            settings.fps_limit = if value { 0 } else { 60 };
        }
        key if key.starts_with("settings.ai_provider.") && key.split('.').count() == 3 => {
            let Some(provider) = ai_provider_from_key(key) else {
                return false;
            };
            let Some(config) = settings
                .ai_providers
                .iter_mut()
                .find(|config| config.provider == provider)
            else {
                return false;
            };
            config.enabled = value;
        }
        _ => return false,
    }
    true
}

pub(crate) fn apply_settings_command(settings: &mut EngineSettings, key: &str) -> bool {
    match key {
        "settings.theme.dark"
        | "settings.theme.light"
        | "settings.theme.system"
        | "settings.language.english"
        | "settings.language.spanish"
        | "settings.units.metric"
        | "settings.units.imperial"
        | "settings.units.game" => apply_settings_toggle(settings, key, true),
        "settings.agent_mode.inspect" => {
            settings.agent_mode = raf_core::ai::AgentMode::Inspect;
            true
        }
        "settings.agent_mode.passive" | "settings.agent_mode.plan" => {
            settings.agent_mode = raf_core::ai::AgentMode::Plan;
            true
        }
        "settings.agent_mode.active" => {
            settings.agent_mode = raf_core::ai::AgentMode::Active;
            true
        }
        key if key.starts_with("settings.ai_provider.default.") => {
            let Some(provider) =
                ai_provider_from_id(key.trim_start_matches("settings.ai_provider.default."))
            else {
                return false;
            };
            settings.default_ai_provider = provider;
            true
        }
        key if key.starts_with("settings.ai_model.default:") => {
            let Some(index) = key
                .trim_start_matches("settings.ai_model.default:")
                .parse::<usize>()
                .ok()
            else {
                return false;
            };
            let Some(shortcut) = settings.agent_model_shortcuts.get(index) else {
                return false;
            };
            settings.default_agent_model = shortcut.label.clone();
            settings.default_ai_provider = shortcut.provider;
            true
        }
        key if key.starts_with("settings.ai_model.remove:") => {
            let Some(index) = key
                .trim_start_matches("settings.ai_model.remove:")
                .parse::<usize>()
                .ok()
            else {
                return false;
            };
            if index >= settings.agent_model_shortcuts.len() {
                return false;
            }
            let removed = settings.agent_model_shortcuts.remove(index);
            if settings.default_agent_model == removed.label {
                settings.default_agent_model.clear();
            }
            true
        }
        _ => return false,
    }
}

pub(crate) fn apply_settings_select(settings: &mut EngineSettings, key: &str, value: &str) -> bool {
    match key {
        "settings.quality" => {
            settings.render_quality = match value {
                "potato" => {
                    settings.render_preset = raf_core::config::RenderPreset::Potato;
                    raf_core::config::RenderQuality::Potato
                }
                "low" => {
                    settings.render_preset = raf_core::config::RenderPreset::Low;
                    raf_core::config::RenderQuality::Low
                }
                "medium" => {
                    settings.render_preset = raf_core::config::RenderPreset::Medium;
                    raf_core::config::RenderQuality::Medium
                }
                "high" => {
                    settings.render_preset = raf_core::config::RenderPreset::High;
                    raf_core::config::RenderQuality::High
                }
                _ => return false,
            };
        }
        "settings.render_execution_policy" => {
            settings.render_execution_policy = match value {
                "auto" => raf_core::config::RenderExecutionPolicy::Auto,
                "cpu_only" => raf_core::config::RenderExecutionPolicy::CpuOnly,
                "gpu_preferred" => raf_core::config::RenderExecutionPolicy::GpuPreferred,
                _ => return false,
            };
        }
        "settings.viewport_render_mode" => {
            settings.viewport_render_mode = match value {
                "solid" => raf_core::config::ViewportRenderMode::Solid,
                _ => return false,
            };
        }
        "settings.default_script_language" => {
            settings.default_script_language = match value {
                "rhai" => raf_core::config::ScriptLanguage::Rhai,
                "cpp" => raf_core::config::ScriptLanguage::Cpp,
                _ => return false,
            };
        }
        "settings.target_platform" => {
            settings.target_platform = match value {
                "desktop" => raf_core::config::TargetPlatform::Desktop,
                "mobile" => raf_core::config::TargetPlatform::Mobile,
                "web" => raf_core::config::TargetPlatform::Web,
                "cloud" => raf_core::config::TargetPlatform::Cloud,
                "console" => raf_core::config::TargetPlatform::Console,
                _ => return false,
            };
        }
        "settings.default_ai_provider" => {
            let Some(provider) = ai_provider_from_id(value) else {
                return false;
            };
            settings.default_ai_provider = provider;
        }
        "settings.agent_mode" => {
            settings.agent_mode = match value {
                "inspect" => raf_core::ai::AgentMode::Inspect,
                "plan" => raf_core::ai::AgentMode::Plan,
                "active" => raf_core::ai::AgentMode::Active,
                _ => return false,
            };
        }
        _ => return false,
    }
    true
}

pub(crate) fn apply_settings_range(settings: &mut EngineSettings, key: &str, value: f32) -> bool {
    let value = if value.is_finite() {
        value
    } else {
        return false;
    };
    match key {
        "settings.theme_experimental" => settings.theme_experimental = value.clamp(0.0, 100.0),
        "settings.font_size" => settings.font_size = value.clamp(10.0, 24.0),
        "settings.ui_scale" => settings.ui_scale = value.clamp(0.5, 3.0),
        "settings.fps_limit" => settings.fps_limit = value.round().clamp(15.0, 240.0) as u32,
        "settings.grid_size" => settings.grid_size = value.clamp(0.1, 10.0),
        "settings.grid_load_distance" => settings.grid_load_distance = value.clamp(0.0, 500.0),
        "settings.electronics_grid_step_mm" => {
            settings.electronics_grid_step_mm = value.clamp(5.0, 100.0)
        }
        "settings.electronics_grid_opacity" => {
            settings.electronics_grid_opacity = value.clamp(0.2, 1.0)
        }
        "settings.auto_save" => {
            settings.auto_save_interval_seconds = value.round().clamp(30.0, 600.0) as u32
        }
        "settings.hierarchy_row_height" => settings.hierarchy_row_height = value.clamp(20.0, 36.0),
        "settings.hierarchy_indent_width" => {
            settings.hierarchy_indent_width = value.clamp(8.0, 28.0)
        }
        "settings.wasd_speed" => settings.wasd_speed = value.clamp(0.05, 5.0),
        "settings.gizmo_growth_scale" => settings.gizmo_growth_scale = value.clamp(0.0, 100.0),
        "settings.move_sensitivity" => settings.move_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.rotate_sensitivity" => settings.rotate_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.scale_sensitivity" => settings.scale_gizmo_sensitivity = value.clamp(0.25, 4.0),
        "settings.script_timeout_ms" => {
            settings.script_timeout_ms = value.round().clamp(10.0, 1000.0) as u32
        }
        "settings.agent_message_page_size" => {
            settings.agent_message_page_size = value.round().clamp(
                raf_core::config::AGENT_MESSAGE_PAGE_SIZE_MIN as f32,
                raf_core::config::AGENT_MESSAGE_PAGE_SIZE_MAX as f32,
            ) as u32;
        }
        "settings.agent_max_response_tokens" => {
            settings.agent_max_response_tokens = value.round().clamp(
                raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MIN as f32,
                raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MAX as f32,
            ) as u32;
        }
        "settings.agent_max_tool_calls" => {
            settings.agent_max_tool_calls = value.round().clamp(
                raf_core::config::AGENT_MAX_TOOL_CALLS_MIN as f32,
                raf_core::config::AGENT_MAX_TOOL_CALLS_MAX as f32,
            ) as u32;
        }
        _ => return false,
    }
    true
}

pub(crate) fn is_settings_numeric_text_key(key: &str) -> bool {
    let Some(key) = key.strip_suffix(".text") else {
        return false;
    };
    matches!(
        key,
        "settings.theme_experimental"
            | "settings.font_size"
            | "settings.ui_scale"
            | "settings.fps_limit"
            | "settings.grid_size"
            | "settings.grid_load_distance"
            | "settings.electronics_grid_step_mm"
            | "settings.electronics_grid_opacity"
            | "settings.auto_save"
            | "settings.hierarchy_row_height"
            | "settings.hierarchy_indent_width"
            | "settings.wasd_speed"
            | "settings.gizmo_growth_scale"
            | "settings.move_sensitivity"
            | "settings.rotate_sensitivity"
            | "settings.scale_sensitivity"
            | "settings.script_timeout_ms"
            | "settings.agent_max_response_tokens"
            | "settings.agent_max_tool_calls"
    )
}

pub(crate) fn apply_settings_text(settings: &mut EngineSettings, key: &str, value: &str) -> bool {
    if key == "settings.script_external_editor" {
        settings.script_external_editor_cmd = value.to_string();
        return true;
    }
    let Some(rest) = key.strip_prefix("settings.ai_provider.") else {
        return false;
    };
    let Some((provider_id, field)) = rest.split_once('.') else {
        return false;
    };
    let Some(provider) = ai_provider_from_key(&format!("settings.ai_provider.{provider_id}"))
    else {
        return false;
    };
    let Some(config) = settings
        .ai_providers
        .iter_mut()
        .find(|config| config.provider == provider)
    else {
        return false;
    };
    match field {
        "base_url" => config.base_url = value.to_string(),
        "model" => config.model = value.to_string(),
        "api_key" => config.api_key = value.to_string(),
        _ => return false,
    }
    true
}

fn ai_provider_from_key(key: &str) -> Option<AiProvider> {
    ai_provider_from_id(key.split('.').nth(2)?)
}

pub(crate) fn ai_provider_from_id(id: &str) -> Option<AiProvider> {
    match id {
        "puerto" => Some(AiProvider::Puerto),
        "openrouter" => Some(AiProvider::OpenRouter),
        "openai" => Some(AiProvider::OpenAI),
        "genai" => Some(AiProvider::GenAI),
        "claude" => Some(AiProvider::Claude),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::config::{Language, Theme};
    use raf_core::units::DisplayUnit;

    #[test]
    fn segmented_settings_commands_update_the_draft() {
        let mut settings = EngineSettings::default();

        assert!(apply_settings_command(
            &mut settings,
            "settings.theme.light"
        ));
        assert_eq!(settings.theme, Theme::Light);
        assert!(apply_settings_command(
            &mut settings,
            "settings.language.spanish"
        ));
        assert_eq!(settings.language, Language::Spanish);
        assert!(apply_settings_command(
            &mut settings,
            "settings.units.imperial"
        ));
        assert_eq!(settings.display_unit, DisplayUnit::Imperial);
        assert!(apply_settings_command(&mut settings, "settings.units.game"));
        assert_eq!(settings.display_unit, DisplayUnit::Game);
        assert!(!apply_settings_command(
            &mut settings,
            "settings.units.unknown"
        ));
    }

    #[test]
    fn reduce_transparency_toggle_updates_the_settings_draft() {
        let mut settings = EngineSettings::default();

        assert!(apply_settings_toggle(
            &mut settings,
            "settings.reduce_transparency",
            true
        ));
        assert!(settings.reduce_transparency);
    }

    #[test]
    fn numeric_settings_are_finite_and_clamped_at_the_adapter_boundary() {
        let mut settings = EngineSettings::default();

        assert!(apply_settings_range(
            &mut settings,
            "settings.font_size",
            999.0
        ));
        assert_eq!(settings.font_size, 24.0);
        assert!(apply_settings_range(
            &mut settings,
            "settings.agent_max_response_tokens",
            1.0
        ));
        assert_eq!(
            settings.agent_max_response_tokens,
            raf_core::config::AGENT_MAX_RESPONSE_TOKENS_MIN
        );
        assert!(apply_settings_toggle(
            &mut settings,
            "settings.agent_tool_call_limit_enabled",
            true
        ));
        assert!(settings.agent_tool_call_limit_enabled);
        assert!(apply_settings_range(
            &mut settings,
            "settings.agent_max_tool_calls",
            999_999.0
        ));
        assert_eq!(
            settings.agent_max_tool_calls,
            raf_core::config::AGENT_MAX_TOOL_CALLS_MAX
        );
        assert!(is_settings_numeric_text_key(
            "settings.agent_max_tool_calls.text"
        ));
        assert!(!apply_settings_range(
            &mut settings,
            "settings.ui_scale",
            f32::NAN
        ));
        assert!(is_settings_numeric_text_key("settings.grid_size.text"));
        assert!(!is_settings_numeric_text_key("settings.grid_size"));
        assert!(!is_settings_numeric_text_key(
            "settings.agent_message_page_size.text"
        ));
    }

    #[test]
    fn provider_text_and_select_adapters_keep_the_provider_identity() {
        let mut settings = EngineSettings::default();
        let provider = settings
            .ai_providers
            .first()
            .map(|config| config.provider)
            .expect("default settings include an AI provider");
        let id = match provider {
            AiProvider::Puerto => "puerto",
            AiProvider::OpenRouter => "openrouter",
            AiProvider::OpenAI => "openai",
            AiProvider::GenAI => "genai",
            AiProvider::Claude => "claude",
        };

        assert!(apply_settings_text(
            &mut settings,
            &format!("settings.ai_provider.{id}.model"),
            "test-model"
        ));
        assert_eq!(
            settings.ai_providers.first().expect("provider").model,
            "test-model"
        );
        assert!(apply_settings_select(
            &mut settings,
            "settings.default_ai_provider",
            id
        ));
        assert_eq!(settings.default_ai_provider, provider);
    }

    #[test]
    fn removed_viewport_render_modes_are_not_selectable() {
        let mut settings = EngineSettings::default();

        assert!(!apply_settings_select(
            &mut settings,
            "settings.viewport_render_mode",
            "wireframe"
        ));
        assert!(!apply_settings_select(
            &mut settings,
            "settings.viewport_render_mode",
            "preview"
        ));
        assert!(!apply_settings_toggle(
            &mut settings,
            "settings.viewport_render_mode.wireframe",
            true
        ));
        assert!(!apply_settings_toggle(
            &mut settings,
            "settings.viewport_render_mode.preview",
            true
        ));
        assert_eq!(
            settings.viewport_render_mode,
            raf_core::config::ViewportRenderMode::Solid
        );
    }
}
