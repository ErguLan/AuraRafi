//! Render quality and viewport presentation settings.

use raf_core::config::{EngineSettings, RenderExecutionPolicy, RenderQuality};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiNode, UiSelectOption};

use super::controls::{
    card, group_label, range_row, range_row_disabled, range_row_with_description, select_row,
    select_row_with_description, toggle_row, toggle_row_with_description,
};

pub(super) fn performance(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    card(palette, "settings.performance")
        .with_child(group_label(palette, "settings.group.editor_performance"))
        .with_child(select_row_with_description(
            palette,
            "settings.quality",
            "settings.quality",
            "settings.quality",
            vec![
                UiSelectOption::new("potato", "settings.value.potato"),
                UiSelectOption::new("low", "settings.value.low"),
                UiSelectOption::new("medium", "settings.value.medium"),
                UiSelectOption::new("high", "settings.value.high"),
            ],
            match settings.render_quality {
                RenderQuality::Potato => 0,
                RenderQuality::Low => 1,
                RenderQuality::Medium => 2,
                RenderQuality::High => 3,
            },
            open_select,
            "settings.quality_desc",
        ))
        .with_child(select_row_with_description(
            palette,
            "settings.render_execution_policy",
            "settings.render_execution_policy",
            "settings.render_execution_policy",
            vec![
                UiSelectOption::new("auto", "settings.render_execution_policy.auto"),
                UiSelectOption::new("cpu_only", "settings.render_execution_policy.cpu_only"),
                UiSelectOption::new(
                    "gpu_preferred",
                    "settings.render_execution_policy.gpu_preferred",
                ),
            ],
            match settings.render_execution_policy {
                RenderExecutionPolicy::Auto => 0,
                RenderExecutionPolicy::CpuOnly => 1,
                RenderExecutionPolicy::GpuPreferred => 2,
            },
            open_select,
            "settings.render_execution_policy_desc",
        ))
        .with_child(group_label(palette, "settings.group.frame_pacing"))
        .with_child(toggle_row_with_description(
            palette,
            "settings.vsync",
            "settings.vsync",
            settings.vsync,
            "settings.vsync_desc",
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.multithreading",
            "settings.multithreading",
            settings.multithreading,
            "settings.multithreading_desc",
        ))
        .with_child(toggle_row(
            palette,
            "settings.show-fps-counter",
            "settings.show_fps_counter",
            settings.show_fps_counter,
        ))
        .with_child(toggle_row(
            palette,
            "settings.fps-unlimited",
            "settings.fps_unlimited",
            settings.fps_limit == 0,
        ))
        .with_child(range_row_disabled(
            palette,
            "settings.fps_limit",
            "settings.fps_limit",
            settings.fps_limit.max(15) as f32,
            15.0,
            240.0,
            1.0,
            format!("{} fps", settings.fps_limit),
            settings.fps_limit == 0,
        ))
        .with_child(group_label(palette, "settings.group.particle_preview"))
        .with_child(toggle_row_with_description(
            palette,
            "settings.particle_preview.enabled",
            "settings.particle_preview.enabled",
            settings.particle_preview.enabled,
            "settings.particle_preview.enabled_desc",
        ))
        .with_child(toggle_row(
            palette,
            "settings.particle_preview.paused",
            "settings.particle_preview.paused",
            settings.particle_preview.paused,
        ))
        .with_child(toggle_row(
            palette,
            "settings.particle_preview.frozen",
            "settings.particle_preview.frozen",
            settings.particle_preview.frozen,
        ))
        .with_child(toggle_row(
            palette,
            "settings.particle_preview.stopped",
            "settings.particle_preview.stopped",
            settings.particle_preview.stopped,
        ))
        .with_child(range_row(
            palette,
            "settings.particle_preview.max_sprites",
            "settings.particle_preview.max_sprites",
            settings.particle_preview.max_sprites as f32,
            0.0,
            100_000.0,
            1_000.0,
            format!("{} sprites", settings.particle_preview.max_sprites),
        ))
        .with_child(range_row(
            palette,
            "settings.particle_preview.time_scale",
            "settings.particle_preview.time_scale",
            settings.particle_preview.time_scale,
            0.0,
            4.0,
            0.05,
            format!("{:.2}x", settings.particle_preview.time_scale),
        ))
        .with_child(range_row(
            palette,
            "settings.particle_preview.wind_x",
            "settings.particle_preview.wind_x",
            settings.particle_preview.wind[0],
            -20.0,
            20.0,
            0.1,
            format!("{:.2}", settings.particle_preview.wind[0]),
        ))
        .with_child(range_row(
            palette,
            "settings.particle_preview.wind_y",
            "settings.particle_preview.wind_y",
            settings.particle_preview.wind[1],
            -20.0,
            20.0,
            0.1,
            format!("{:.2}", settings.particle_preview.wind[1]),
        ))
        .with_child(range_row(
            palette,
            "settings.particle_preview.wind_z",
            "settings.particle_preview.wind_z",
            settings.particle_preview.wind[2],
            -20.0,
            20.0,
            0.1,
            format!("{:.2}", settings.particle_preview.wind[2]),
        ))
}

pub(super) fn viewport(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    card(palette, "settings.viewport")
        .with_child(group_label(palette, "settings.group.game"))
        .with_child(select_row(
            palette,
            "settings.viewport-render-mode",
            "settings.viewport_render_mode",
            "settings.viewport_render_mode",
            vec![UiSelectOption::new(
                "solid",
                "settings.viewport_render_mode.solid",
            )],
            0,
            open_select,
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.viewport-labels",
            "settings.show_viewport_labels",
            settings.show_viewport_labels,
            "settings.show_viewport_labels_desc",
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.surface-edges",
            "settings.solid_show_surface_edges",
            settings.solid_show_surface_edges,
            "settings.solid_show_surface_edges_desc",
        ))
        .with_child(toggle_row(
            palette,
            "settings.xray",
            "settings.solid_xray_mode",
            settings.solid_xray_mode,
        ))
        .with_child(toggle_row(
            palette,
            "settings.face-tonality",
            "settings.solid_face_tonality",
            settings.solid_face_tonality,
        ))
        .with_child(toggle_row(
            palette,
            "settings.invert-x",
            "settings.invert_mouse_x",
            settings.invert_mouse_x,
        ))
        .with_child(toggle_row(
            palette,
            "settings.invert-y",
            "settings.invert_mouse_y",
            settings.invert_mouse_y,
        ))
        .with_child(toggle_row(
            palette,
            "settings.focus-lock",
            "settings.focus_lock_enabled",
            settings.focus_lock_enabled,
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.invert-ws",
            "settings.invert_ws",
            settings.invert_ws,
            "settings.invert_ws_desc",
        ))
        .with_child(range_row(
            palette,
            "settings.wasd-speed",
            "settings.wasd_speed",
            settings.wasd_speed,
            0.05,
            5.0,
            0.1,
            format!("{:.2}x", settings.wasd_speed),
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.uniform-scale",
            "settings.uniform_scale_by_default",
            settings.uniform_scale_by_default,
            "settings.uniform_scale_by_default_desc",
        ))
        .with_child(toggle_row(
            palette,
            "settings.multi-select-gizmo",
            "settings.multi_select_gizmo_enabled",
            settings.multi_select_gizmo_enabled,
        ))
        .with_child(range_row_with_description(
            palette,
            "settings.gizmo-growth",
            "settings.gizmo_size",
            settings.gizmo_growth_scale,
            0.0,
            100.0,
            1.0,
            format!("{:.0}%", settings.gizmo_growth_scale),
            "settings.gizmo_size_desc",
        ))
        .with_child(range_row(
            palette,
            "settings.move-sensitivity",
            "settings.move_sensitivity",
            settings.move_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.1}", settings.move_gizmo_sensitivity),
        ))
        .with_child(range_row(
            palette,
            "settings.rotate-sensitivity",
            "settings.rotate_sensitivity",
            settings.rotate_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.1}", settings.rotate_gizmo_sensitivity),
        ))
        .with_child(range_row(
            palette,
            "settings.scale-sensitivity",
            "settings.scale_sensitivity",
            settings.scale_gizmo_sensitivity,
            0.25,
            4.0,
            0.05,
            format!("{:.1}", settings.scale_gizmo_sensitivity),
        ))
}
