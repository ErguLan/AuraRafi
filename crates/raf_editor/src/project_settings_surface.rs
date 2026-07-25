//! Retained RafUI document for per-project settings.
//!
//! The document describes controls only. The host applies its actions to the
//! selected `Project`, preserving the current project.ron save boundary.

use raf_core::config::{RenderPreset, ScriptExecutionMode, ScriptLanguage};
use raf_core::project::Project;
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiRange, UiResponsiveRule, UiScrollAxis, UiSpacing, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput,
    UiTextStyle, UiToggle,
};

const CONTROL_HEIGHT: f32 = 32.0;
const RANGE_WIDTH: f32 = 212.0;

pub fn build_project_settings_surface(
    palette: StudioUiPalette,
    project: &Project,
    global_console_commands_enabled: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let root = UiNode::new("project-settings.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("project-settings.header", UiNodeKind::Toolbar)
                .with_class("project-settings-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(16.0, 0.0),
                    ..UiLayout::fixed(0.0, 42.0)
                })
                .with_child(
                    UiNode::new("project-settings.title", UiNodeKind::Label)
                        .with_text_key("app.project_settings")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                )
                .with_child(
                    UiNode::new("project-settings.project-name", UiNodeKind::Label)
                        .with_text_key(project.name.clone())
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(180.0, 20.0)),
                ),
        )
        .with_child(
            UiNode::scroll_view("project-settings.scroll", UiScrollAxis::Vertical)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    padding: UiSpacing::xy(20.0, 16.0),
                    gap: 16.0,
                    overflow: UiOverflow::ScrollY,
                    ..UiLayout::fill(UiFlow::Column)
                })
                .with_style(UiStyle::transparent())
                .with_child(overview_section(palette, project))
                .with_child(layout_section(palette, project))
                .with_child(runtime_section(
                    palette,
                    project,
                    global_console_commands_enabled,
                ))
                .with_child(saving_section(palette, project))
                .with_child(scripting_section(palette, project))
                .with_child(graphics_section(palette, project)),
        );
    let mut surface = UiSurface::new("project-settings", palette, root);
    surface.style_sheet = project_settings_style_sheet(palette);
    surface
}

fn overview_section(palette: StudioUiPalette, project: &Project) -> UiNode {
    section(palette, "project-settings.overview", "app.project_overview")
        .with_child(info_row(
            palette,
            "project-settings.overview.name",
            "app.project_name",
            project.name.clone(),
        ))
        .with_child(info_row(
            palette,
            "project-settings.overview.type",
            "app.project_type",
            project.project_type.display_name().to_string(),
        ))
        .with_child(info_row(
            palette,
            "project-settings.overview.version",
            "app.project_version",
            project.engine_version.clone(),
        ))
        .with_child(info_row(
            palette,
            "project-settings.overview.path",
            "app.project_path",
            project.path.display().to_string(),
        ))
}

fn layout_section(palette: StudioUiPalette, project: &Project) -> UiNode {
    section(palette, "project-settings.layout", "app.project_layout")
        .with_child(toggle_row(
            palette,
            "project-settings.show-hierarchy",
            "app.show_hierarchy_panel",
            None,
            "project-settings.show-hierarchy",
            project.settings.show_hierarchy_panel,
            false,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.show-properties",
            "app.show_properties_panel",
            None,
            "project-settings.show-properties",
            project.settings.show_properties_panel,
            false,
        ))
}

fn runtime_section(
    palette: StudioUiPalette,
    project: &Project,
    global_console_commands_enabled: bool,
) -> UiNode {
    section(palette, "project-settings.runtime", "app.project_runtime")
        .with_child(toggle_row(
            palette,
            "project-settings.enable-audio",
            "app.enable_audio",
            None,
            "project-settings.enable-audio",
            project.settings.enable_audio,
            false,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.enable-physics",
            "app.enable_physics",
            None,
            "project-settings.enable-physics",
            project.settings.enable_physics,
            false,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.pause-unfocused",
            "app.pause_when_unfocused",
            None,
            "project-settings.pause-unfocused",
            project.settings.pause_when_unfocused,
            false,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.enable-complements",
            "app.enable_complements",
            None,
            "project-settings.enable-complements",
            project.settings.enable_complements,
            false,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.enable-console",
            "app.enable_console_commands",
            Some("app.enable_console_commands_desc"),
            "project-settings.enable-console",
            project.settings.enable_console_commands && global_console_commands_enabled,
            false,
        ))
        .with_child(text_input_row(
            palette,
            "app.default_scene_name",
            "project-settings.default-scene",
        ))
}

fn saving_section(palette: StudioUiPalette, project: &Project) -> UiNode {
    let description = if project.settings.linear_save {
        "app.project_save_mode_linear_desc"
    } else {
        "app.project_save_mode_standard_desc"
    };
    section(palette, "project-settings.saving", "app.project_saving")
        .with_child(segment_row(
            palette,
            "app.project_save_mode",
            None,
            &[
                Segment::new(
                    "project-settings.save.standard",
                    "app.project_save_mode_standard",
                    !project.settings.linear_save,
                    false,
                ),
                Segment::new(
                    "project-settings.save.linear",
                    "app.project_save_mode_linear",
                    project.settings.linear_save,
                    false,
                ),
            ],
        ))
        .with_child(description_line(
            palette,
            "project-settings.save-help",
            description,
        ))
}

fn scripting_section(palette: StudioUiPalette, project: &Project) -> UiNode {
    let mut section = section(
        palette,
        "project-settings.scripting",
        "app.project_scripting",
    )
    .with_child(toggle_row(
        palette,
        "project-settings.enable-scripting",
        "app.enable_scripting",
        None,
        "project-settings.enable-scripting",
        project.settings.enable_scripting,
        false,
    ))
    .with_child(section_label(
        palette,
        "project-settings.scripting-languages",
        "app.allowed_script_languages",
    ));

    for language in ScriptLanguage::all() {
        section = section.with_child(toggle_row(
            palette,
            format!(
                "project-settings.script-language.{}",
                script_language_id(language)
            ),
            script_language_key(language),
            None,
            format!(
                "project-settings.script-language.{}",
                script_language_id(language)
            ),
            project.settings.allowed_script_languages.has(language),
            !project.settings.enable_scripting,
        ));
    }

    section
        .with_child(segment_row(
            palette,
            "app.script_execution_mode",
            None,
            &[
                Segment::new(
                    "project-settings.script-mode.disabled",
                    "app.project_script_mode_disabled",
                    project.settings.script_execution_mode == ScriptExecutionMode::Disabled,
                    !project.settings.enable_scripting,
                ),
                Segment::new(
                    "project-settings.script-mode.editor",
                    "app.project_script_mode_editor",
                    project.settings.script_execution_mode == ScriptExecutionMode::EditorOnly,
                    !project.settings.enable_scripting,
                ),
                Segment::new(
                    "project-settings.script-mode.runtime",
                    "app.project_script_mode_runtime",
                    project.settings.script_execution_mode == ScriptExecutionMode::Runtime,
                    !project.settings.enable_scripting,
                ),
            ],
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.auto-attach-scripts",
            "app.auto_attach_scripts",
            None,
            "project-settings.auto-attach-scripts",
            project.settings.auto_attach_scripts,
            !project.settings.enable_scripting,
        ))
}

fn graphics_section(palette: StudioUiPalette, project: &Project) -> UiNode {
    let advanced_gpu_locked = !project.settings.allow_gpu_features;
    section(palette, "project-settings.graphics", "app.graphics_policy")
        .with_child(toggle_row(
            palette,
            "project-settings.allow-gpu-features",
            "app.allow_gpu_features",
            Some("app.allow_gpu_features_desc"),
            "project-settings.allow-gpu-features",
            project.settings.allow_gpu_features,
            false,
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.depth-accurate",
            "app.depth_accurate",
            Some("app.depth_accurate_desc"),
            "project-settings.depth-accurate",
            project.settings.depth_accurate,
            false,
        ))
        .with_child(range_row(
            palette,
            "app.depth_resolution_scale",
            None,
            "project-settings.depth-resolution-scale",
            project.settings.depth_resolution_scale,
            0.35,
            1.0,
            0.05,
            format!("{:.2}x", project.settings.depth_resolution_scale),
            !project.settings.depth_accurate,
        ))
        .with_child(segment_row(
            palette,
            "app.render_preset",
            None,
            &[
                Segment::new(
                    "project-settings.preset.potato",
                    "app.project_render_preset_potato",
                    project.settings.runtime_render_preset == RenderPreset::Potato,
                    false,
                ),
                Segment::new(
                    "project-settings.preset.low",
                    "app.project_render_preset_low",
                    project.settings.runtime_render_preset == RenderPreset::Low,
                    false,
                ),
                Segment::new(
                    "project-settings.preset.medium",
                    "app.project_render_preset_medium",
                    project.settings.runtime_render_preset == RenderPreset::Medium,
                    advanced_gpu_locked,
                ),
                Segment::new(
                    "project-settings.preset.high",
                    "app.project_render_preset_high",
                    project.settings.runtime_render_preset == RenderPreset::High,
                    advanced_gpu_locked,
                ),
            ],
        ))
        .with_child(section_divider(
            palette,
            "project-settings.streaming-divider",
        ))
        .with_child(toggle_row(
            palette,
            "project-settings.world-streaming",
            "app.world_streaming",
            Some("app.world_streaming_desc"),
            "project-settings.world-streaming",
            project.settings.world_streaming_enabled,
            false,
        ))
        .with_child(range_row(
            palette,
            "app.world_stream_region_size",
            None,
            "project-settings.stream-region-size",
            project.settings.world_stream_region_size,
            32.0,
            512.0,
            16.0,
            format!("{:.0} m", project.settings.world_stream_region_size),
            !project.settings.world_streaming_enabled,
        ))
        .with_child(range_row(
            palette,
            "app.world_stream_radius",
            None,
            "project-settings.stream-radius",
            project.settings.world_stream_load_radius as f32,
            1.0,
            8.0,
            1.0,
            project.settings.world_stream_load_radius.to_string(),
            !project.settings.world_streaming_enabled,
        ))
        .with_child(range_row(
            palette,
            "app.world_stream_lod_bias",
            None,
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
            gap: 8.0,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{id}.title"), UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 22.0)),
        )
        .with_child(section_divider(palette, format!("{id}.divider")))
}

fn info_row(palette: StudioUiPalette, id: &str, label_key: &str, value: String) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            align_items: UiAlign::Center,
            gap: 12.0,
            padding: UiSpacing::xy(0.0, 2.0),
            basis: [0.0, 24.0],
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(180.0, 20.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.value"), UiNodeKind::Label)
                .with_text_key(value)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                }),
        )
}

fn section_label(palette: StudioUiPalette, id: &str, label_key: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout::fixed(0.0, 20.0))
}

fn description_line(palette: StudioUiPalette, id: &str, text_key: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
        .with_layout(UiLayout::fixed(0.0, 30.0))
}

fn section_divider(palette: StudioUiPalette, id: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Separator)
        .with_layout(UiLayout::fixed(0.0, 1.0))
        .with_style(UiStyle {
            fill: palette.tokens().border,
            border: [0, 0, 0, 0],
            text: palette.tokens().text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

fn form_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    help_key: Option<&str>,
    control: UiNode,
) -> UiNode {
    let tokens = palette.tokens();
    let row_height = if help_key.is_some() { 64.0 } else { 42.0 };
    let mut label = UiNode::new(format!("{id}.copy"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [260.0, 0.0],
            min_size: [180.0, 0.0],
            gap: 2.0,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 18.0)),
        );
    if let Some(help_key) = help_key {
        label = label.with_child(
            UiNode::new(format!("{id}.help"), UiNodeKind::Label)
                .with_text_key(help_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 34.0)),
        );
    }
    UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
        .with_class("project-settings-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            align_items: UiAlign::Center,
            gap: 18.0,
            padding: UiSpacing::xy(0.0, 5.0),
            basis: [0.0, row_height],
            responsive: vec![UiResponsiveRule {
                max_width: 680.0,
                flow: Some(UiFlow::Column),
                basis: None,
                padding: None,
                gap: Some(6.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(label)
        .with_child(
            UiNode::new(format!("{id}.control"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    grow: 1.0,
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    basis: [0.0, CONTROL_HEIGHT],
                    ..UiLayout::default()
                })
                .with_style(UiStyle::transparent())
                .with_child(control),
        )
}

fn toggle_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: &str,
    help_key: Option<&str>,
    value_key: impl Into<String>,
    value: bool,
    disabled: bool,
) -> UiNode {
    let id = id.into();
    form_row(
        palette,
        &id,
        label_key,
        help_key,
        toggle_control(palette, id.clone(), value_key, value, disabled),
    )
}

fn toggle_control(
    palette: StudioUiPalette,
    id: String,
    value_key: impl Into<String>,
    value: bool,
    disabled: bool,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::toggle(id.clone(), UiToggle::new(value_key, value))
        .with_class(if value {
            "project-settings-toggle-on"
        } else {
            "project-settings-toggle"
        })
        .disabled(disabled)
        .with_layout(UiLayout::fixed(42.0, 24.0))
        .with_child(
            UiNode::new(format!("{id}.knob"), UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(
                    raf_render::api_graphic_basic::ui_surface::UiRect::new(
                        if value { 21.0 } else { 3.0 },
                        3.0,
                        18.0,
                        18.0,
                    ),
                ))
                .with_style(UiStyle {
                    fill: if value {
                        [255, 255, 255, 255]
                    } else {
                        tokens.text_muted
                    },
                    border: [0, 0, 0, 0],
                    text: tokens.text,
                    border_width: 0.0,
                    radius: 9.0,
                    opacity: 1.0,
                }),
        )
}

fn text_input_row(palette: StudioUiPalette, label_key: &str, value_key: &str) -> UiNode {
    form_row(
        palette,
        value_key,
        label_key,
        None,
        UiNode::text_input(
            format!("{value_key}.input"),
            UiTextInput {
                value_key: value_key.to_string(),
                placeholder_key: None,
                max_length: 512,
                multiline: false,
                password: false,
                submit_command: None,
            },
        )
        .with_class("project-settings-input")
        .with_layout(UiLayout::fixed(248.0, CONTROL_HEIGHT)),
    )
}

fn range_row(
    palette: StudioUiPalette,
    label_key: &str,
    help_key: Option<&str>,
    value_key: &str,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    value_label: String,
    disabled: bool,
) -> UiNode {
    let range = UiRange::new(value_key, value, min, max, step);
    let tokens = palette.tokens();
    let track_width = RANGE_WIDTH - 14.0;
    let fill_width = (track_width * range.fraction()).max(2.0);
    let thumb_x = (track_width * range.fraction()).clamp(0.0, track_width) + 3.0;
    let control = UiNode::new(format!("{value_key}.range-control"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            ..UiLayout::fixed(0.0, CONTROL_HEIGHT)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::range(format!("{value_key}.range"), range)
                .with_class("project-settings-range")
                .disabled(disabled)
                .with_layout(UiLayout::fixed(RANGE_WIDTH, CONTROL_HEIGHT))
                .with_child(
                    UiNode::new(format!("{value_key}.track"), UiNodeKind::Panel)
                        .with_layout(UiLayout::absolute(
                            raf_render::api_graphic_basic::ui_surface::UiRect::new(
                                4.0,
                                14.0,
                                track_width,
                                4.0,
                            ),
                        ))
                        .with_class("project-settings-range-track"),
                )
                .with_child(
                    UiNode::new(format!("{value_key}.fill"), UiNodeKind::Panel)
                        .with_layout(UiLayout::absolute(
                            raf_render::api_graphic_basic::ui_surface::UiRect::new(
                                4.0, 14.0, fill_width, 4.0,
                            ),
                        ))
                        .with_class("project-settings-range-fill"),
                )
                .with_child(
                    UiNode::new(format!("{value_key}.thumb"), UiNodeKind::Panel)
                        .with_layout(UiLayout::absolute(
                            raf_render::api_graphic_basic::ui_surface::UiRect::new(
                                thumb_x, 8.0, 16.0, 16.0,
                            ),
                        ))
                        .with_style(UiStyle {
                            fill: [255, 255, 255, 255],
                            border: tokens.border,
                            text: tokens.text,
                            border_width: 1.0,
                            radius: 8.0,
                            opacity: 1.0,
                        }),
                ),
        )
        .with_child(
            UiNode::new(format!("{value_key}.value"), UiNodeKind::Label)
                .with_text_key(value_label)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(64.0, 20.0)),
        );
    form_row(palette, value_key, label_key, help_key, control)
}

#[derive(Debug, Clone, Copy)]
struct Segment<'a> {
    command: &'a str,
    label_key: &'a str,
    selected: bool,
    disabled: bool,
}

impl<'a> Segment<'a> {
    const fn new(command: &'a str, label_key: &'a str, selected: bool, disabled: bool) -> Self {
        Self {
            command,
            label_key,
            selected,
            disabled,
        }
    }
}

fn segment_row(
    palette: StudioUiPalette,
    label_key: &str,
    help_key: Option<&str>,
    segments: &[Segment<'_>],
) -> UiNode {
    let tokens = palette.tokens();
    let mut control = UiNode::new(format!("{label_key}.segments"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            align_items: UiAlign::Center,
            gap: 6.0,
            compact: UiCompactMode::Wrap,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent());
    for segment in segments {
        control = control.with_child(
            UiNode::new(
                format!("{label_key}.{}", segment.command),
                UiNodeKind::Button,
            )
            .with_text_key(segment.label_key)
            .with_text_style(UiTextStyle::button(if segment.selected {
                [18, 18, 20, 255]
            } else {
                tokens.text
            }))
            .with_class(if segment.selected {
                "project-settings-segment-active"
            } else {
                "project-settings-segment"
            })
            .disabled(segment.disabled)
            .with_layout(UiLayout {
                basis: [0.0, CONTROL_HEIGHT],
                min_size: [76.0, CONTROL_HEIGHT],
                padding: UiSpacing::xy(10.0, 0.0),
                ..UiLayout::default()
            })
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, segment.command)),
        );
    }
    form_row(palette, label_key, label_key, help_key, control)
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

fn project_settings_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let toggle_fill = match palette {
        StudioUiPalette::IndustrialDark => [58, 67, 77, 255],
        StudioUiPalette::PaperLight => [190, 194, 198, 255],
    };
    UiStyleSheet {
        rules: vec![
            style_rule(
                "project-settings-header",
                tokens.surface,
                tokens.border,
                1.0,
                0.0,
            ),
            style_rule(
                "project-settings-segment",
                tokens.surface_alt,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "project-settings-segment-active",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                4.0,
            ),
            style_rule(
                "project-settings-input",
                tokens.surface_raised,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "project-settings-toggle",
                toggle_fill,
                tokens.border,
                1.0,
                12.0,
            ),
            style_rule(
                "project-settings-toggle-on",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                12.0,
            ),
            style_rule(
                "project-settings-range-track",
                tokens.surface_alt,
                [0, 0, 0, 0],
                0.0,
                2.0,
            ),
            style_rule(
                "project-settings-range-fill",
                [255, 255, 255, 255],
                [0, 0, 0, 0],
                0.0,
                2.0,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("project-settings-segment".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("project-settings-range".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
}

fn style_rule(
    class: &str,
    fill: [u8; 4],
    border: [u8; 4],
    border_width: f32,
    radius: f32,
) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            border_width: Some(border_width),
            radius: Some(radius),
            ..UiStylePatch::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }

    #[test]
    fn surface_contains_every_project_setting_group() {
        let project = Project {
            id: uuid::Uuid::nil(),
            name: "test-project-settings-surface".to_string(),
            project_type: raf_core::project::ProjectType::Game,
            path: std::path::PathBuf::from("test-project-settings-surface"),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            engine_version: "0.9.0".to_string(),
            settings: raf_core::project::ProjectSettings::default(),
        };
        let surface =
            build_project_settings_surface(StudioUiPalette::IndustrialDark, &project, true);

        for id in [
            "project-settings.overview",
            "project-settings.layout",
            "project-settings.runtime",
            "project-settings.saving",
            "project-settings.scripting",
            "project-settings.graphics",
            "project-settings.enable-audio",
            "project-settings.depth-resolution-scale.range",
            "project-settings.world-streaming",
        ] {
            assert!(find_node(&surface.root, id).is_some(), "missing {id}");
        }
    }
}
