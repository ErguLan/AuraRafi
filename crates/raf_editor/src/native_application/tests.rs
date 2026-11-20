//! Unit tests for the native editor application boundary.

use raf_core::config::{RenderPreset, RenderQuality, ScriptLanguage};
use raf_core::project::{
    GeometryDetailMode, ProjectSettings, ProjectType, ViewportResolutionMode,
};
use raf_ui::UiResizeEdge;

use super::layout::should_queue_eager_hierarchy_refresh;
use super::project_settings::{
    apply_project_setting_command, apply_project_setting_range, apply_project_setting_toggle,
};
use super::shortcuts::{electronics_input_edge_pending, electronics_shortcut_command};
use super::window::native_resize_edge;

#[test]
fn gravity_presets_configure_character_values_and_preserve_basic_settings() {
    let mut settings = ProjectSettings::default();
    assert!(apply_project_setting_command(&mut settings, "project-settings.gravity-preset.arcade").changed);
    assert_eq!(settings.runtime.gravity, glam::Vec3::new(0.0, -20.0, 0.0));
    assert_eq!(settings.runtime.character_jump_height, 2.5);
    assert_eq!(settings.runtime.character_speed, 6.0);
    assert_eq!(settings.runtime.character_max_slope_degrees, 50.0);
    apply_project_setting_toggle(&mut settings, "project-settings.advanced-physics", false);
    assert!(!apply_project_setting_command(&mut settings, "project-settings.gravity-preset.moon").changed);
    assert_eq!(settings.runtime.gravity, glam::Vec3::new(0.0, -20.0, 0.0));
    assert!(apply_project_setting_range(&mut settings, "project-settings.character-speed", 5.0).changed);
    assert_eq!(settings.runtime.character_speed, 5.0);
}

#[test]
fn electronics_save_chord_dispatches_while_control_is_still_held() {
    let mut input = raf_core::InputSnapshot::default();
    input.modifiers.control = true;
    input.keys_pressed.insert(raf_core::InputKey::S);
    input.keys_down.insert(raf_core::InputKey::S);
    assert!(electronics_input_edge_pending(Some(ProjectType::Electronics), &input));
    assert_eq!(electronics_shortcut_command(&input, true, false), Some(crate::application_menu::command::PROJECT_SAVE));
    // Holding the keys must not dispatch another save on a UI-only frame.
    input.clear_transient();
    assert!(!electronics_input_edge_pending(Some(ProjectType::Electronics), &input));
    assert_eq!(electronics_shortcut_command(&input, true, false), None);
    input.modifiers.control = false;
    input.keys_down.remove(raf_core::InputKey::S);
    input.keys_released.insert(raf_core::InputKey::S);
    assert!(!electronics_input_edge_pending(Some(ProjectType::Electronics), &input));
}

#[test]
fn electronics_button_edges_dispatch_before_the_next_present_tick() {
    let mut input = raf_core::InputSnapshot::default();
    assert!(!electronics_input_edge_pending(Some(ProjectType::Electronics), &input));
    input.pointer_pressed_buttons.insert(raf_core::PointerButton::Primary);
    assert!(electronics_input_edge_pending(Some(ProjectType::Electronics), &input));
    assert!(!electronics_input_edge_pending(Some(ProjectType::Game), &input));
    input.clear_transient();
    input.pointer_released_buttons.insert(raf_core::PointerButton::Primary);
    assert!(electronics_input_edge_pending(Some(ProjectType::Electronics), &input));
    input.window_focused = false;
    assert!(!electronics_input_edge_pending(Some(ProjectType::Electronics), &input));
}

#[test]
fn native_resize_hit_test_prioritizes_corners() {
    assert_eq!(
        native_resize_edge([2.0, 2.0], [800.0, 600.0]),
        Some(UiResizeEdge::NorthWest)
    );
    assert_eq!(
        native_resize_edge([798.0, 598.0], [800.0, 600.0]),
        Some(UiResizeEdge::SouthEast)
    );
}

#[test]
fn native_resize_hit_test_leaves_content_clicks_alone() {
    assert_eq!(native_resize_edge([400.0, 300.0], [800.0, 600.0]), None);
    assert_eq!(native_resize_edge([-1.0, 300.0], [800.0, 600.0]), None);
}

#[test]
fn project_setting_controls_mutate_persisted_values_and_runtime_policy() {
    let mut settings = ProjectSettings::default();

    assert!(
        apply_project_setting_toggle(
            &mut settings,
            "project-settings.allow-gpu-features",
            true,
        )
        .changed
    );
    assert!(settings.allow_gpu_features);

    assert!(
        apply_project_setting_range(
            &mut settings,
            "project-settings.depth-resolution-scale",
            0.73,
        )
        .changed
    );
    assert_eq!(settings.depth_resolution_scale, 0.75);

    assert!(
        apply_project_setting_command(&mut settings, "project-settings.preset.medium",).changed
    );
    assert_eq!(settings.runtime_render_preset, RenderPreset::Medium);

    assert!(
        apply_project_setting_command(
            &mut settings,
            "project-settings.viewport-resolution.full",
        )
        .changed
    );
    assert_eq!(
        settings.viewport_resolution,
        ViewportResolutionMode::FullResolution
    );

    assert!(
        apply_project_setting_command(
            &mut settings,
            "project-settings.geometry-detail.detailed",
        )
        .changed
    );
    assert_eq!(settings.geometry_detail, GeometryDetailMode::Detailed);

    assert!(
        apply_project_setting_toggle(&mut settings, "project-settings.language.cpp", true,)
            .changed
    );
    assert!(settings.allowed_script_languages.has(ScriptLanguage::Cpp));

    settings.show_hierarchy_panel = false;
    settings.show_properties_panel = false;
    let reset = apply_project_setting_command(&mut settings, "project-settings.reset-panels");
    assert!(reset.changed);
    assert!(reset.layout_changed);
    assert!(settings.show_hierarchy_panel && settings.show_properties_panel);
}

#[test]
fn hierarchy_refresh_policy_is_lazy_only_for_potato() {
    assert!(!should_queue_eager_hierarchy_refresh(
        RenderQuality::Potato,
        true
    ));
    assert!(!should_queue_eager_hierarchy_refresh(
        RenderQuality::Low,
        false
    ));
    assert!(should_queue_eager_hierarchy_refresh(
        RenderQuality::Low,
        true
    ));
    assert!(should_queue_eager_hierarchy_refresh(
        RenderQuality::Medium,
        true
    ));
    assert!(should_queue_eager_hierarchy_refresh(
        RenderQuality::High,
        true
    ));
}
