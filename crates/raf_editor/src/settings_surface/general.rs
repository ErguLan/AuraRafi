//! General editor settings: appearance, editor workspace, scripting and platform.

use raf_core::config::{EngineSettings, Language, ScriptLanguage, TargetPlatform, Theme};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiNode, UiSelectOption};

use super::controls::{
    card, group_label, range_row, range_row_disabled, range_row_disabled_with_description,
    range_row_with_description, row_with_reset, segment_row, select_row,
    select_row_with_description, text_row, toggle_row, toggle_row_with_description,
    toggle_row_with_key,
};

pub(super) fn appearance(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    let defaults = EngineSettings::default();
    let mut node = card(palette, "settings.appearance");
    node = node.with_child(group_label(palette, "settings.group.display"));
    node = node.with_child(row_with_reset(
        palette,
        select_row_with_description(
            palette,
            "settings.theme",
            "settings.theme",
            "settings.theme",
            vec![
                UiSelectOption::new("dark", "settings.value.dark"),
                UiSelectOption::new("light", "settings.value.light"),
                UiSelectOption::new("system", "settings.value.system"),
            ],
            match settings.theme {
                Theme::Dark => 0,
                Theme::Light => 1,
                Theme::System => 2,
            },
            open_select,
            "settings.theme_desc",
        ),
        settings.theme != defaults.theme,
        "settings.theme",
    ));
    node = node.with_child(row_with_reset(
        palette,
        range_row_with_description(
            palette,
            "settings.font_size",
            "settings.font_size",
            settings.font_size,
            10.0,
            24.0,
            1.0,
            format!("{:.0} px", settings.font_size),
            "settings.font_size_desc",
        ),
        settings.font_size != defaults.font_size,
        "settings.font_size",
    ));
    node = node.with_child(row_with_reset(
        palette,
        toggle_row_with_description(
            palette,
            "settings.auto-ui-scale",
            "settings.auto_ui_scale",
            settings.auto_ui_scale,
            "settings.auto_ui_scale_desc",
        ),
        settings.auto_ui_scale != defaults.auto_ui_scale,
        "settings.auto-ui-scale",
    ));
    node = node.with_child(row_with_reset(
        palette,
        range_row_disabled_with_description(
            palette,
            "settings.ui_scale",
            "settings.ui_scale",
            settings.ui_scale,
            0.5,
            3.0,
            0.1,
            format!("{:.1}x", settings.ui_scale),
            settings.auto_ui_scale,
            "settings.ui_scale_desc",
        ),
        settings.ui_scale != defaults.ui_scale,
        "settings.ui_scale",
    ));
    node = node.with_child(row_with_reset(
        palette,
        range_row_with_description(
            palette,
            "settings.theme-experimental",
            "settings.theme_experimental",
            settings.theme_experimental,
            0.0,
            100.0,
            1.0,
            format!("{:.0}%", settings.theme_experimental),
            "settings.theme_experimental_desc",
        ),
        settings.theme_experimental != defaults.theme_experimental,
        "settings.theme-experimental",
    ));

    node = node.with_child(group_label(palette, "settings.group.accessibility"));
    node = node.with_child(row_with_reset(
        palette,
        toggle_row_with_description(
            palette,
            "settings.native-accessibility",
            "settings.native_accessibility_enabled",
            settings.native_accessibility_enabled,
            "settings.native_accessibility_desc",
        ),
        settings.native_accessibility_enabled != defaults.native_accessibility_enabled,
        "settings.native-accessibility",
    ));
    node = node.with_child(row_with_reset(
        palette,
        toggle_row_with_description(
            palette,
            "settings.prefers-reduced-motion",
            "settings.prefers_reduced_motion",
            settings.prefers_reduced_motion,
            "settings.prefers_reduced_motion_desc",
        ),
        settings.prefers_reduced_motion != defaults.prefers_reduced_motion,
        "settings.prefers-reduced-motion",
    ));
    node = node.with_child(row_with_reset(
        palette,
        toggle_row_with_description(
            palette,
            "settings.high-contrast",
            "settings.high_contrast",
            settings.high_contrast,
            "settings.high_contrast_desc",
        ),
        settings.high_contrast != defaults.high_contrast,
        "settings.high-contrast",
    ));
    node = node.with_child(row_with_reset(
        palette,
        toggle_row_with_description(
            palette,
            "settings.reduce-transparency",
            "settings.reduce_transparency",
            settings.reduce_transparency,
            "settings.reduce_transparency_desc",
        ),
        settings.reduce_transparency != defaults.reduce_transparency,
        "settings.reduce-transparency",
    ));

    node = node.with_child(row_with_reset(
        palette,
        select_row_with_description(
            palette,
            "settings.language",
            "settings.language",
            "settings.language",
            vec![
                UiSelectOption::new("english", "settings.language.english"),
                UiSelectOption::new("spanish", "settings.language.spanish"),
            ],
            match settings.language {
                Language::English => 0,
                Language::Spanish => 1,
            },
            open_select,
            "settings.language_desc",
        ),
        settings.language != defaults.language,
        "settings.language",
    ));

    node.with_child(row_with_reset(
        palette,
        toggle_row_with_description(
            palette,
            "settings.simple-mode",
            "settings.simple_mode",
            settings.simple_mode,
            "settings.simple_mode_desc",
        ),
        settings.simple_mode != defaults.simple_mode,
        "settings.simple-mode",
    ))
}

pub(super) fn editor(palette: StudioUiPalette, settings: &EngineSettings) -> UiNode {
    card(palette, "settings.editor")
        .with_child(group_label(palette, "settings.group.editor_view"))
        .with_child(toggle_row(
            palette,
            "settings.show-grid",
            "settings.show_grid",
            settings.grid_visible,
        ))
        .with_child(toggle_row(
            palette,
            "settings.snap-grid",
            "settings.snap_to_grid",
            settings.snap_to_grid,
        ))
        .with_child(toggle_row_with_description(
            palette,
            "settings.inspector-live-transform-updates",
            "settings.inspector_live_transform_updates",
            settings.inspector_live_transform_updates,
            "settings.inspector_live_transform_updates_desc",
        ))
        .with_child(range_row_disabled(
            palette,
            "settings.grid_size",
            "settings.grid_size",
            settings.grid_size,
            0.1,
            10.0,
            0.1,
            format!("{:.1} m", settings.grid_size),
            settings.simple_mode,
        ))
        .with_child(range_row_disabled(
            palette,
            "settings.grid_load_distance",
            "settings.grid_load_distance",
            settings.grid_load_distance,
            0.0,
            500.0,
            0.5,
            format!("{:.1} m", settings.grid_load_distance),
            settings.simple_mode,
        ))
        .with_child(group_label(palette, "settings.group.files"))
        .with_child(range_row_disabled(
            palette,
            "settings.auto-save",
            "settings.auto_save",
            settings.auto_save_interval_seconds as f32,
            30.0,
            600.0,
            1.0,
            format!("{} s", settings.auto_save_interval_seconds),
            settings.simple_mode,
        ))
        .with_child(segment_row(
            palette,
            "settings.units",
            "settings.units",
            &[
                (
                    "settings.metric",
                    "settings.units.metric",
                    settings.display_unit == raf_core::units::DisplayUnit::Metric,
                ),
                (
                    "settings.imperial",
                    "settings.units.imperial",
                    settings.display_unit == raf_core::units::DisplayUnit::Imperial,
                ),
                (
                    "settings.value.game_units",
                    "settings.units.game",
                    settings.display_unit == raf_core::units::DisplayUnit::Game,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "settings.command-console",
            "settings.command_console_enabled",
            settings.command_console_enabled,
        ))
        .with_child(group_label(palette, "settings.group.hierarchy"))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-icons",
            "settings.hierarchy_icons",
            "settings.hierarchy_show_icons",
            settings.hierarchy_show_icons,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-script-links",
            "settings.hierarchy_script_links",
            "settings.hierarchy_show_script_links",
            settings.hierarchy_show_script_links,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-script-indicator",
            "settings.hierarchy_script_indicator",
            "settings.hierarchy_show_script_indicators",
            settings.hierarchy_show_script_indicators,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-visibility",
            "settings.hierarchy_visibility",
            "settings.hierarchy_show_visibility",
            settings.hierarchy_show_visibility,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-locked",
            "settings.hierarchy_locked",
            "settings.hierarchy_show_locked",
            settings.hierarchy_show_locked,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-hidden",
            "settings.hierarchy_hidden",
            "settings.hierarchy_show_hidden",
            settings.hierarchy_show_hidden,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-auto-reveal",
            "settings.hierarchy_auto_reveal",
            "settings.hierarchy_auto_reveal_selection",
            settings.hierarchy_auto_reveal_selection,
        ))
        .with_child(toggle_row_with_key(
            palette,
            "settings.hierarchy-expand-selection",
            "settings.hierarchy_expand_selection",
            "settings.hierarchy_expand_on_select",
            settings.hierarchy_expand_on_select,
        ))
        .with_child(toggle_row(
            palette,
            "settings.hierarchy-animations",
            "settings.hierarchy_animations",
            settings.hierarchy_animations,
        ))
        .with_child(range_row(
            palette,
            "settings.hierarchy-row-height",
            "settings.hierarchy_row_height",
            settings.hierarchy_row_height,
            20.0,
            36.0,
            1.0,
            format!("{} px", settings.hierarchy_row_height.round()),
        ))
        .with_child(range_row(
            palette,
            "settings.hierarchy-indent-width",
            "settings.hierarchy_indent_width",
            settings.hierarchy_indent_width,
            8.0,
            28.0,
            1.0,
            format!("{} px", settings.hierarchy_indent_width.round()),
        ))
}

pub(super) fn scripting(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    card(palette, "settings.scripting")
        .with_child(select_row(
            palette,
            "settings.script-language",
            "settings.default_script_language",
            "settings.default_script_language",
            vec![
                UiSelectOption::new("rhai", "settings.script_language.rhai"),
                UiSelectOption::new("cpp", "settings.script_language.cpp"),
            ],
            match settings.default_script_language {
                ScriptLanguage::Cpp => 1,
                ScriptLanguage::Rhai | ScriptLanguage::Nodes => 0,
            },
            open_select,
        ))
        .with_child(text_row(
            palette,
            "settings.script-editor",
            "settings.script_external_editor",
        ))
}

pub(super) fn platform(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    open_select: Option<&str>,
) -> UiNode {
    card(palette, "settings.target_platform")
        .with_child(group_label(palette, "settings.group.platform"))
        .with_child(select_row_with_description(
            palette,
            "settings.platform",
            "settings.target_platform",
            "settings.target_platform",
            vec![
                UiSelectOption::new("desktop", "settings.value.desktop"),
                UiSelectOption::new("mobile", "settings.value.mobile"),
                UiSelectOption::new("web", "settings.value.web"),
                UiSelectOption::new("cloud", "settings.value.cloud"),
                UiSelectOption::new("console", "settings.value.console"),
            ],
            match settings.target_platform {
                TargetPlatform::Desktop => 0,
                TargetPlatform::Mobile => 1,
                TargetPlatform::Web => 2,
                TargetPlatform::Cloud => 3,
                TargetPlatform::Console => 4,
            },
            open_select,
            "settings.target_platform_desc",
        ))
        .with_child(toggle_row(
            palette,
            "settings.responsive",
            "settings.responsive_layout",
            settings.responsive_layout,
        ))
        .with_child(toggle_row(
            palette,
            "settings.headless",
            "settings.headless",
            settings.headless,
        ))
}
