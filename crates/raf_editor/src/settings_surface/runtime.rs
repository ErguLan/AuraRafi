//! Runtime preferences for Play, window limits and diagnostics.

use raf_core::config::EngineSettings;
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiNode, UiSelectOption};

use super::controls::{card, range_row, select_row, toggle_row};

pub(super) fn runtime_preferences(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    use raf_core::runtime_config::{RuntimeErrorPolicy, RuntimeLaunchMode};
    let runtime = settings.runtime.normalized();
    let mut root = card(palette, "runtime.settings")
        .with_child(select_row(
            palette,
            "settings.runtime.launch",
            "settings.runtime.launch_mode",
            "settings.runtime.launch_mode",
            vec![
                UiSelectOption::new("separate", "settings.runtime.separate"),
                UiSelectOption::new("same", "settings.runtime.same"),
            ],
            usize::from(runtime.launch_mode == RuntimeLaunchMode::SameWindow),
            open_select,
        ))
        .with_child(select_row(
            palette,
            "settings.runtime.error",
            "settings.runtime.error_policy",
            "settings.runtime.error_policy",
            vec![
                UiSelectOption::new("pause", "runtime.pause"),
                UiSelectOption::new("disable", "settings.runtime.disable_script"),
                UiSelectOption::new("stop", "runtime.stop"),
            ],
            match runtime.error_policy {
                RuntimeErrorPolicy::Pause => 0,
                RuntimeErrorPolicy::DisableScript => 1,
                RuntimeErrorPolicy::Stop => 2,
            },
            open_select,
        ));
    for (field, value, min, max, step) in [
        ("max_instances", runtime.max_instances as f32, 1.0, 4.0, 1.0),
        ("width", runtime.window_size[0] as f32, 640.0, 1920.0, 16.0),
        ("height", runtime.window_size[1] as f32, 360.0, 1080.0, 9.0),
        (
            "script_budget_ms",
            runtime.script_budget_ms as f32,
            1.0,
            100.0,
            1.0,
        ),
        (
            "script_operation_limit",
            runtime.script_operation_limit as f32,
            1000.0,
            1_000_000.0,
            1000.0,
        ),
    ] {
        let key = format!("settings.runtime.{field}");
        root = root.with_child(range_row(
            palette,
            key.clone(),
            key,
            value,
            min,
            max,
            step,
            format!("{value:.0}"),
        ));
    }
    root.with_child(toggle_row(
        palette,
        "settings.runtime.hot-reload",
        "settings.runtime.hot_reload",
        runtime.hot_reload,
    ))
    .with_child(toggle_row(
        palette,
        "settings.runtime.diagnostics",
        "settings.runtime.diagnostics",
        runtime.diagnostics,
    ))
}
