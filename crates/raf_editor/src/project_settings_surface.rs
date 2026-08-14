//! RafUI presentation for project-local settings.
//!
//! Project settings are deliberately separate from engine settings. They are
//! serialized with the active project and never modify global preferences.

use raf_core::config::{RenderPreset, ScriptExecutionMode, ScriptLanguage};
use raf_core::project::Project;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiRange, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface, UiTextInput,
    UiTextStyle, UiToggle,
};

const CONTROL_WIDTH: f32 = 232.0;
const ACCENT: [u8; 4] = [232, 133, 28, 255];

pub fn build_project_settings_surface(
    palette: StudioUiPalette,
    project: &Project,
    global_console_commands_enabled: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("project-settings.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("project-settings.header", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    compact: UiCompactMode::Wrap,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(18.0, 0.0),
                    responsive: vec![raf_ui::UiResponsiveRule {
                        max_width: 520.0,
                        flow: Some(UiFlow::Column),
                        basis: Some([0.0, 82.0]),
                        padding: Some(UiSpacing::xy(12.0, 8.0)),
                        gap: Some(2.0),
                        compact: Some(UiCompactMode::Stack),
                        grid_columns: None,
                    }],
                    ..UiLayout::fixed(0.0, 46.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("project-settings.title", UiNodeKind::Label)
                        .with_text_key("app.project_settings")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(
                    UiNode::new("project-settings.title-separator", UiNodeKind::Label)
                        .with_text_value("|")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(8.0, 22.0)),
                )
                .with_child(
                    UiNode::new("project-settings.project-name", UiNodeKind::Label)
                        .with_text_value(project.name.clone())
                        .with_text_style(UiTextStyle::body(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(
                    UiNode::new("project-settings.header-spacer", UiNodeKind::Panel).with_layout(
                        UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        },
                    ),
                ),
        )
        .with_child(
            UiNode::scroll_view("project-settings.scroll", UiScrollAxis::Vertical)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    gap: 14.0,
                    padding: UiSpacing::xy(22.0, 18.0),
                    overflow: UiOverflow::ScrollY,
                    responsive: vec![raf_ui::UiResponsiveRule {
                        max_width: 520.0,
                        flow: Some(UiFlow::Column),
                        basis: Some([0.0, 0.0]),
                        padding: Some(UiSpacing::xy(12.0, 12.0)),
                        gap: Some(10.0),
                        compact: Some(UiCompactMode::Stack),
                        grid_columns: None,
                    }],
                    ..UiLayout::fill(UiFlow::Column)
                })
                .with_child(overview(palette, project))
                .with_child(layout(palette, project))
                .with_child(runtime(palette, project, global_console_commands_enabled))
                .with_child(saving(palette, project))
                .with_child(scripting(palette, project))
                .with_child(graphics(palette, project)),
        );

    let mut surface = UiSurface::new("editor.project-settings", palette, root);
    surface.style_sheet = project_settings_style_sheet(palette);
    surface
}

fn overview(palette: StudioUiPalette, project: &Project) -> UiNode {
    section(palette, "project-settings.overview", "app.project_overview")
        .with_child(info_row(palette, "app.project_name", project.name.clone()))
        .with_child(info_row(
            palette,
            "app.project_type",
            project.project_type.display_name().to_string(),
        ))
        .with_child(info_row(
            palette,
            "app.project_version",
            project.engine_version.clone(),
        ))
        .with_child(info_row(
            palette,
            "app.project_path",
            project.path.display().to_string(),
        ))
}

fn layout(palette: StudioUiPalette, project: &Project) -> UiNode {
    section(palette, "project-settings.layout", "app.project_layout")
        .with_child(toggle_row(
            palette,
            "project-settings.show-hierarchy",
            "app.show_hierarchy_panel",
            "project-settings.show-hierarchy",
            project.settings.show_hierarchy_panel,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.show-properties",
            "app.show_properties_panel",
            "project-settings.show-properties",
            project.settings.show_properties_panel,
        ))
        .with_child(
            UiNode::new("project-settings.reset-panels", UiNodeKind::Button)
                .with_class("project-settings-reset-panels")
                .with_layout(UiLayout::fit_content())
                .with_text_key("app.reset_panels")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "project-settings.reset-panels",
                )),
        )
}

fn runtime(
    palette: StudioUiPalette,
    project: &Project,
    global_console_commands_enabled: bool,
) -> UiNode {
    section(palette, "project-settings.runtime", "app.project_runtime")
        .with_child(toggle_row(
            palette,
            "project-settings.enable-audio",
            "app.enable_audio",
            "project-settings.enable-audio",
            project.settings.enable_audio,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.enable-physics",
            "app.enable_physics",
            "project-settings.enable-physics",
            project.settings.enable_physics,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.pause-unfocused",
            "app.pause_when_unfocused",
            "project-settings.pause-unfocused",
            project.settings.pause_when_unfocused,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.enable-complements",
            "app.enable_complements",
            "project-settings.enable-complements",
            project.settings.enable_complements,
        ))
        .with_child(toggle_row_disabled(
            palette,
            "project-settings.enable-console",
            "app.enable_console_commands",
            "project-settings.enable-console",
            project.settings.enable_console_commands && global_console_commands_enabled,
            !global_console_commands_enabled,
        ))
        .with_child(text_row(
            palette,
            "project-settings.default-scene",
            "app.default_scene_name",
            &project.settings.default_scene_name,
        ))
}

fn saving(palette: StudioUiPalette, project: &Project) -> UiNode {
    section(palette, "project-settings.saving", "app.project_saving").with_child(segment_row(
        palette,
        "project-settings.save-mode",
        "app.project_save_mode",
        &[
            (
                "app.project_save_mode_standard",
                "project-settings.save.standard",
                !project.settings.linear_save,
            ),
            (
                "app.project_save_mode_linear",
                "project-settings.save.linear",
                project.settings.linear_save,
            ),
        ],
    ))
}

fn scripting(palette: StudioUiPalette, project: &Project) -> UiNode {
    let mut node = section(
        palette,
        "project-settings.scripting",
        "app.project_scripting",
    )
    .with_child(toggle_row(
        palette,
        "project-settings.enable-scripting",
        "app.enable_scripting",
        "project-settings.enable-scripting",
        project.settings.enable_scripting,
    ));
    let scripting_enabled = project.settings.enable_scripting;
    for language in ScriptLanguage::all() {
        let id = script_language_id(language);
        node = node.with_child(toggle_row_disabled(
            palette,
            format!("project-settings.language.{id}"),
            script_language_key(language),
            format!("project-settings.language.{id}"),
            project.settings.allowed_script_languages.has(language),
            !scripting_enabled,
        ));
    }
    node = node.with_child(segment_row_disabled(
        palette,
        "project-settings.script-mode",
        "app.script_execution_mode",
        &[
            (
                "app.project_script_mode_disabled",
                "project-settings.script-mode.disabled",
                project.settings.script_execution_mode == ScriptExecutionMode::Disabled,
            ),
            (
                "app.project_script_mode_editor",
                "project-settings.script-mode.editor",
                project.settings.script_execution_mode == ScriptExecutionMode::EditorOnly,
            ),
            (
                "app.project_script_mode_runtime",
                "project-settings.script-mode.runtime",
                project.settings.script_execution_mode == ScriptExecutionMode::Runtime,
            ),
        ],
        !scripting_enabled,
    ));
    node.with_child(toggle_row_disabled(
        palette,
        "project-settings.auto-attach-scripts",
        "app.auto_attach_scripts",
        "project-settings.auto-attach-scripts",
        project.settings.auto_attach_scripts,
        !scripting_enabled,
    ))
}

fn graphics(palette: StudioUiPalette, project: &Project) -> UiNode {
    section(palette, "project-settings.graphics", "app.graphics_policy")
        .with_child(toggle_row(
            palette,
            "project-settings.allow-gpu-features",
            "app.allow_gpu_features",
            "project-settings.allow-gpu-features",
            project.settings.allow_gpu_features,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.depth-accurate",
            "app.depth_accurate",
            "project-settings.depth-accurate",
            project.settings.depth_accurate,
        ))
        .with_child(range_row_disabled(
            palette,
            "project-settings.depth-resolution-scale",
            "app.depth_resolution_scale",
            "project-settings.depth-resolution-scale",
            project.settings.depth_resolution_scale,
            0.35,
            1.0,
            0.05,
            format!("{:.2}x", project.settings.depth_resolution_scale),
            !project.settings.depth_accurate,
        ))
        .with_child(segment_row_with_disabled_values(
            palette,
            "project-settings.render-preset",
            "app.render_preset",
            &[
                (
                    "app.project_render_preset_potato",
                    "project-settings.preset.potato",
                    project.settings.runtime_render_preset == RenderPreset::Potato,
                ),
                (
                    "app.project_render_preset_low",
                    "project-settings.preset.low",
                    project.settings.runtime_render_preset == RenderPreset::Low,
                ),
                (
                    "app.project_render_preset_medium",
                    "project-settings.preset.medium",
                    project.settings.runtime_render_preset == RenderPreset::Medium,
                ),
                (
                    "app.project_render_preset_high",
                    "project-settings.preset.high",
                    project.settings.runtime_render_preset == RenderPreset::High,
                ),
            ],
            false,
            if project.settings.allow_gpu_features {
                &[]
            } else {
                &[
                    "project-settings.preset.medium",
                    "project-settings.preset.high",
                ]
            },
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.world-streaming",
            "app.world_streaming",
            "project-settings.world-streaming",
            project.settings.world_streaming_enabled,
        ))
        .with_child(range_row_disabled(
            palette,
            "project-settings.stream-region-size",
            "app.world_stream_region_size",
            "project-settings.stream-region-size",
            project.settings.world_stream_region_size,
            32.0,
            512.0,
            16.0,
            format!("{:.0} m", project.settings.world_stream_region_size),
            !project.settings.world_streaming_enabled,
        ))
        .with_child(range_row_disabled(
            palette,
            "project-settings.stream-radius",
            "app.world_stream_radius",
            "project-settings.stream-radius",
            project.settings.world_stream_load_radius as f32,
            1.0,
            8.0,
            1.0,
            project.settings.world_stream_load_radius.to_string(),
            !project.settings.world_streaming_enabled,
        ))
        .with_child(range_row_disabled(
            palette,
            "project-settings.stream-lod-bias",
            "app.world_stream_lod_bias",
            "project-settings.stream-lod-bias",
            project.settings.world_stream_lod_bias as f32,
            0.0,
            4.0,
            1.0,
            project.settings.world_stream_lod_bias.to_string(),
            !project.settings.world_streaming_enabled,
        ))
}

fn section(palette: StudioUiPalette, id: &str, title_key: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("project-settings-section")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::same(14.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new(format!("{id}.title"), UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 24.0)),
        )
}

fn info_row(palette: StudioUiPalette, label_key: &str, value: String) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("project-settings.info.{label_key}"),
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        compact: UiCompactMode::Stack,
        align_items: UiAlign::Center,
        gap: 16.0,
        responsive: vec![raf_ui::UiResponsiveRule {
            max_width: 520.0,
            flow: Some(UiFlow::Column),
            basis: Some([0.0, 48.0]),
            padding: Some(UiSpacing::xy(4.0, 4.0)),
            gap: Some(2.0),
            compact: Some(UiCompactMode::Stack),
            grid_columns: None,
        }],
        ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(format!("{label_key}.label"), UiNodeKind::Label)
            .with_text_key(label_key)
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fixed(150.0, 22.0)),
    )
    .with_child(
        UiNode::new(format!("{label_key}.value"), UiNodeKind::Label)
            .with_text_value(value)
            .with_text_style(UiTextStyle::body(tokens.text))
            .with_layout(UiLayout::fit_content()),
    )
}

fn toggle_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    value: bool,
) -> UiNode {
    toggle_row_disabled(palette, id, label_key, value_key, value, false)
}

fn toggle_row_disabled(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    value: bool,
    disabled: bool,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let row = row(
        palette,
        id.clone(),
        label_key,
        UiNode::toggle(format!("{id}.control"), UiToggle::new(value_key, value))
            .with_class("project-settings-toggle")
            .with_layout(UiLayout::fixed(48.0, 28.0))
            .disabled(disabled),
    );
    if disabled {
        row.with_class("project-settings-row-disabled")
    } else {
        row
    }
}

fn range_row_disabled(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    _display: String,
    disabled: bool,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let value_key = value_key.into();
    let numeric_key = format!("{value_key}.text");
    let control = UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 360.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 60.0]),
                padding: None,
                gap: Some(4.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::fixed(CONTROL_WIDTH + 78.0, 30.0)
        })
        .with_child(
            UiNode::range(
                format!("{id}.slider"),
                UiRange::new(value_key.clone(), value, min, max, step),
            )
            .with_class("project-settings-range")
            .with_layout(UiLayout {
                grow: 1.0,
                responsive: vec![raf_ui::UiResponsiveRule {
                    max_width: 360.0,
                    flow: Some(UiFlow::None),
                    basis: Some([0.0, 28.0]),
                    padding: None,
                    gap: None,
                    compact: None,
                    grid_columns: None,
                }],
                ..UiLayout::fixed(CONTROL_WIDTH - 84.0, 28.0)
            })
            .disabled(disabled),
        )
        .with_child(
            UiNode::text_input(
                format!("{id}.value"),
                UiTextInput {
                    value_key: numeric_key,
                    placeholder_key: None,
                    max_length: 32,
                    multiline: false,
                    password: false,
                    submit_command: Some(format!("project-settings.commit_numeric:{value_key}")),
                },
            )
            .with_class("project-settings-numeric-input")
            .with_layout(UiLayout::fixed(78.0, 28.0))
            .disabled(disabled),
        );
    let row = row(palette, id.clone(), label_key, control);
    if disabled {
        row.with_class("project-settings-row-disabled")
    } else {
        row
    }
}

fn text_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    _value: &str,
) -> UiNode {
    let id = id.into();
    row(
        palette,
        id.clone(),
        label_key.into(),
        UiNode::text_input(
            format!("{id}.control"),
            UiTextInput {
                value_key: "project-settings.default_scene_name".to_string(),
                placeholder_key: Some("app.default_scene_name".to_string()),
                max_length: 128,
                multiline: false,
                password: false,
                submit_command: None,
            },
        )
        .with_layout(UiLayout {
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 360.0,
                flow: Some(UiFlow::None),
                basis: Some([0.0, 30.0]),
                padding: None,
                gap: None,
                compact: None,
                grid_columns: None,
            }],
            ..UiLayout::fixed(CONTROL_WIDTH, 30.0)
        }),
    )
}

fn segment_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    values: &[(&str, &str, bool)],
) -> UiNode {
    segment_row_with_disabled_values(palette, id, label_key, values, false, &[])
}

fn segment_row_disabled(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    values: &[(&str, &str, bool)],
    disabled: bool,
) -> UiNode {
    segment_row_with_disabled_values(palette, id, label_key, values, disabled, &[])
}

fn segment_row_with_disabled_values(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    values: &[(&str, &str, bool)],
    disabled_all: bool,
    disabled_commands: &[&str],
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let tokens = palette.tokens();
    let mut toolbar =
        UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 480.0,
                flow: Some(UiFlow::RowWrap),
                basis: Some([0.0, 90.0]),
                padding: None,
                gap: Some(4.0),
                compact: Some(UiCompactMode::Wrap),
                grid_columns: None,
            }],
            ..UiLayout::fixed(CONTROL_WIDTH, 30.0)
        });
    for (label_key, command, selected) in values {
        toolbar = toolbar.with_child(
            UiNode::new(format!("{id}.{command}"), UiNodeKind::Button)
                .with_class(if *selected {
                    "project-settings-segment-active"
                } else {
                    "project-settings-segment"
                })
                .with_layout(UiLayout::fit_content())
                .with_text_key(*label_key)
                .with_text_style(UiTextStyle::button(if *selected {
                    ACCENT
                } else {
                    tokens.text_muted
                }))
                .disabled(disabled_all || disabled_commands.contains(command))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, *command)),
        );
    }
    let row = row(palette, id, label_key, toolbar);
    if disabled_all {
        row.with_class("project-settings-row-disabled")
    } else {
        row
    }
}

fn row(palette: StudioUiPalette, id: String, label_key: String, control: UiNode) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            align_items: UiAlign::Center,
            gap: 14.0,
            padding: UiSpacing::xy(4.0, 3.0),
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 520.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 78.0]),
                padding: Some(UiSpacing::xy(4.0, 6.0)),
                gap: Some(6.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, 42.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(control)
}

fn project_settings_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("project-settings-segment-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("project-settings-segment".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("project-settings-numeric-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("project-settings-toggle".to_string()),
                UiStylePatch {
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("project-settings-row-disabled".to_string()),
                UiStylePatch {
                    opacity: Some(0.55),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("project-settings-reset-panels".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
        ],
    }
}

fn script_language_id(language: ScriptLanguage) -> &'static str {
    match language {
        ScriptLanguage::Rhai => "rhai",
        ScriptLanguage::Cpp => "cpp",
        ScriptLanguage::Nodes => "nodes",
    }
}

fn script_language_key(language: ScriptLanguage) -> &'static str {
    match language {
        ScriptLanguage::Rhai => "app.project_script_language_rhai",
        ScriptLanguage::Cpp => "app.project_script_language_cpp",
        ScriptLanguage::Nodes => "app.project_script_language_nodes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_settings_surface_keeps_the_six_legacy_sections() {
        let surface = build_project_settings_surface(
            StudioUiPalette::IndustrialDark,
            &Project {
                id: uuid::Uuid::new_v4(),
                name: "Demo".to_string(),
                project_type: raf_core::project::ProjectType::Game,
                path: std::path::PathBuf::from("."),
                created_at: chrono::Utc::now(),
                modified_at: chrono::Utc::now(),
                engine_version: "0.9.0".to_string(),
                settings: Default::default(),
            },
            false,
        );
        let scroll = &surface.root.children[1];
        assert_eq!(scroll.children.len(), 6);
    }
}
