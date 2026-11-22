//! Shared settings card, row and control builders.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAction, UiAlign, UiCompactMode, UiEventBinding,
    UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRange, UiSelect,
    UiSelectOption, UiSizeMode, UiSpacing, UiStyle, UiSurfaceMaterial, UiTextInput, UiTextOverflow,
    UiTextStyle, UiToggle,
};
use raf_ui::components::select_trigger;
use raf_ui::{UiFontWeight, UiRect, UiTextRole};

use super::{
    CONTROL_WIDTH, SEGMENT_MIN_WIDTH, SETTINGS_COMPACT_BREAKPOINT, SETTINGS_CONTENT_MAX_WIDTH,
    SETTINGS_CONTROL_MIN_WIDTH, SETTINGS_RANGE_MIN_WIDTH, SETTINGS_ROW_HEIGHT,
    SETTINGS_STACKED_ROW_HEIGHT,
};

pub(super) fn group_label(palette: StudioUiPalette, text_key: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(format!("{text_key}.label"), UiNodeKind::Label)
        .with_class("settings-group-label")
        .with_layout(UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill))
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::panel_title(tokens.text_muted))
}

pub(super) fn select_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    options: Vec<UiSelectOption>,
    selected_index: usize,
    open_select: Option<&str>,
) -> UiNode {
    select_row_with_description(
        palette,
        id,
        label_key,
        value_key,
        options,
        selected_index,
        open_select,
        "",
    )
}

pub(super) fn select_row_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    options: Vec<UiSelectOption>,
    selected_index: usize,
    open_select: Option<&str>,
    description_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let value_key = value_key.into();
    let description_key = description_key.into();
    let has_description = !description_key.is_empty();
    let selected_index = selected_index.min(options.len().saturating_sub(1));
    let action_value_key = value_key.clone();
    let trigger_id = format!("{id}.control");
    let is_open = open_select == Some(trigger_id.as_str());
    let mut select = UiSelect::new(value_key, options.clone(), selected_index)
        .with_popup_id(format!("{id}.menu"));
    select.open = is_open;
    let trigger = select_trigger(trigger_id, select, palette)
        .with_class("settings-select-trigger")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(12.0, 0.0),
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH, 32.0],
            max_size: [CONTROL_WIDTH, 32.0],
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_accessibility_label_key(label_key.clone());
    let mut control = UiNode::new(format!("{id}.control-wrap"), UiNodeKind::Panel)
        .with_class("settings-select-wrap")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH, 32.0],
            max_size: [CONTROL_WIDTH, 32.0],
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(trigger);

    if is_open {
        let menu_height = (options.len() as f32 * 30.0).min(210.0);
        let tokens = palette.tokens();
        let mut menu = UiNode::new(format!("{id}.menu"), UiNodeKind::Menu)
            .with_class("settings-select-menu")
            .with_material(UiSurfaceMaterial::TranslucentRaised)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                ..UiLayout::absolute(UiRect::new(0.0, 34.0, CONTROL_WIDTH, menu_height.max(30.0)))
                    .with_z_index(30)
            })
            .with_style(UiStyle {
                fill: tokens.surface_raised,
                border: tokens.border,
                text: tokens.text,
                border_width: 1.0,
                radius: 6.0,
                opacity: 1.0,
            });
        for (index, option) in options.iter().enumerate() {
            let selected = index == selected_index;
            let option_id = format!("{id}.option.{}", option.value);
            menu = menu.with_child(
                UiNode::new(option_id, UiNodeKind::Button)
                    .with_class(if selected {
                        "settings-select-option-selected"
                    } else {
                        "settings-select-option"
                    })
                    .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill))
                    .with_text_key(option.label_key.clone())
                    .with_text_style(UiTextStyle::body(if selected {
                        tokens.text
                    } else {
                        tokens.text_muted
                    }))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_accessibility_role(UiAccessibilityRole::Option)
                    .with_accessibility_selected(selected)
                    .focusable()
                    .with_event(UiEventBinding {
                        event: UiEventKind::Click,
                        action: UiAction::SetSelect {
                            key: action_value_key.clone(),
                            value: option.value.clone(),
                            index,
                        },
                    })
                    .with_event(UiEventBinding {
                        event: UiEventKind::Click,
                        action: UiAction::SetSelectOpen {
                            id: format!("{id}.control"),
                            open: false,
                        },
                    }),
            );
        }
        control = control.with_child(menu);
    }

    if has_description {
        row_with_description(palette, id, label_key, None, description_key, control)
    } else {
        row(palette, id, label_key, None, control)
    }
}

pub(super) fn card(_palette: StudioUiPalette, title_key: &str) -> UiNode {
    UiNode::new(title_key, UiNodeKind::Panel)
        .with_class("settings-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            max_size: [SETTINGS_CONTENT_MAX_WIDTH, 0.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle::transparent())
}

pub(super) fn toggle_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: bool,
) -> UiNode {
    let label_key = label_key.into();
    toggle_row_with_key(palette, id, label_key.clone(), label_key, value)
}

pub(super) fn toggle_row_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: bool,
    description_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let description_key = description_key.into();
    let value_key = label_key.clone();
    let toggle = UiNode::toggle(
        format!("{id}.control"),
        UiToggle::new(value_key.clone(), value),
    )
    .with_class("settings-toggle")
    .with_layout(UiLayout::fixed(48.0, 28.0));
    row_with_description(palette, id, label_key, None, description_key, toggle).with_event(
        UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetToggle {
                key: value_key,
                value: !value,
            },
        },
    )
}

pub(super) fn toggle_row_with_key(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value_key: impl Into<String>,
    value: bool,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let value_key = value_key.into();
    let toggle = UiNode::toggle(
        format!("{id}.control"),
        UiToggle::new(value_key.clone(), value),
    )
    .with_class("settings-toggle")
    .with_layout(UiLayout::fixed(48.0, 28.0));
    row(palette, id, label_key, None, toggle).with_event(UiEventBinding {
        event: UiEventKind::Click,
        action: UiAction::SetToggle {
            key: value_key,
            value: !value,
        },
    })
}

pub(super) fn range_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
) -> UiNode {
    range_row_disabled(
        palette, id, label_key, value, min, max, step, display, false,
    )
}

pub(super) fn range_row_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
    description_key: impl Into<String>,
) -> UiNode {
    range_row_disabled_with_description(
        palette,
        id,
        label_key,
        value,
        min,
        max,
        step,
        display,
        false,
        description_key,
    )
}

pub(super) fn range_row_disabled(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
    disabled: bool,
) -> UiNode {
    range_row_disabled_with_description(
        palette, id, label_key, value, min, max, step, display, disabled, "",
    )
}

pub(super) fn range_row_disabled_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    display: String,
    disabled: bool,
    description_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let description_key = description_key.into();
    let has_description = !description_key.is_empty();
    let numeric_key = format!("{label_key}.text");
    let control = UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            grow: 1.0,
            min_size: [SETTINGS_RANGE_MIN_WIDTH, 30.0],
            max_size: [CONTROL_WIDTH + 78.0, 30.0],
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::range(
                format!("{id}.slider"),
                UiRange::new(label_key.clone(), value, min, max, step),
            )
            .with_class("settings-range")
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [96.0, 28.0],
                ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
            })
            .disabled(disabled),
        )
        .with_child(
            UiNode::text_input(
                format!("{id}.value"),
                UiTextInput {
                    value_key: numeric_key,
                    placeholder_key: Some("settings.surface.input".to_string()),
                    max_length: 32,
                    multiline: false,
                    password: false,
                    submit_command: Some(format!("settings.commit_numeric:{label_key}")),
                },
            )
            .with_class("settings-numeric-input")
            .with_layout(UiLayout::fixed(78.0, 28.0))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "settings.numeric.cancel",
            ))
            .disabled(disabled),
        );
    let row = if has_description {
        row_with_description(
            palette,
            id,
            label_key,
            Some(display),
            description_key,
            control,
        )
    } else {
        row(palette, id, label_key, Some(display), control)
    };
    if disabled {
        row.with_class("settings-row-disabled")
    } else {
        row
    }
}

pub(super) fn text_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    row(
        palette,
        id.clone(),
        label_key.clone(),
        None,
        UiNode::text_input(
            format!("{id}.control"),
            UiTextInput {
                value_key: label_key,
                placeholder_key: Some("settings.surface.input".to_string()),
                max_length: 256,
                multiline: false,
                password: false,
                submit_command: None,
            },
        )
        .with_class("settings-text-input")
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [SETTINGS_CONTROL_MIN_WIDTH, 30.0],
            max_size: [CONTROL_WIDTH, 30.0],
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        }),
    )
}

pub(super) fn segment_row(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    values: &[(&str, &str, bool)],
) -> UiNode {
    segment_row_with_description(palette, id, label_key, values, "")
}

pub(super) fn segment_row_with_description(
    palette: StudioUiPalette,
    id: impl Into<String>,
    label_key: impl Into<String>,
    values: &[(&str, &str, bool)],
    description_key: impl Into<String>,
) -> UiNode {
    let id = id.into();
    let label_key = label_key.into();
    let description_key = description_key.into();
    let has_description = !description_key.is_empty();
    let tokens = palette.tokens();
    let count = values.len().max(1) as f32;
    let gap = 2.0;
    let segment_width = ((CONTROL_WIDTH - gap * (count - 1.0)) / count).max(SEGMENT_MIN_WIDTH);
    let toolbar_width = segment_width * count + gap * (count - 1.0);
    let mut toolbar = UiNode::new(format!("{id}.control"), UiNodeKind::Toolbar)
        .with_class("settings-segments")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap,
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(toolbar_width, 30.0)
        });
    for (value_key, command, selected) in values {
        toolbar = toolbar.with_child(
            UiNode::new(format!("{id}.{command}"), UiNodeKind::Button)
                .with_class(if *selected {
                    "settings-segment-active"
                } else {
                    "settings-segment"
                })
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(8.0, 0.0),
                    ..UiLayout::fixed(segment_width, 30.0)
                })
                .with_text_key(*value_key)
                .with_text_style(UiTextStyle::button(if *selected {
                    tokens.text
                } else {
                    tokens.text_muted
                }))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, *command)),
        );
    }
    if has_description {
        row_with_description(palette, id, label_key, None, description_key, toolbar)
    } else {
        row(palette, id, label_key, None, toolbar)
    }
}

pub(super) fn row(
    palette: StudioUiPalette,
    id: String,
    label_key: String,
    value: Option<String>,
    control: UiNode,
) -> UiNode {
    row_with_breakpoint(
        palette,
        id,
        label_key,
        value,
        None,
        control,
        SETTINGS_COMPACT_BREAKPOINT,
    )
}

pub(super) fn row_with_description(
    palette: StudioUiPalette,
    id: String,
    label_key: String,
    value: Option<String>,
    description_key: String,
    control: UiNode,
) -> UiNode {
    row_with_breakpoint(
        palette,
        id,
        label_key,
        value,
        Some(description_key),
        control,
        SETTINGS_COMPACT_BREAKPOINT,
    )
}

pub(super) fn row_with_breakpoint(
    palette: StudioUiPalette,
    id: String,
    label_key: String,
    value: Option<String>,
    description_key: Option<String>,
    control: UiNode,
    compact_breakpoint: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let mut copy = UiNode::new(format!("{id}.copy"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 2.0,
            min_size: [96.0, 0.0],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fit_content()),
        );
    if let Some(value) = value {
        copy = copy.with_child(
            UiNode::new(format!("{id}.display"), UiNodeKind::Label)
                .with_text_value(value)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    if let Some(description_key) = description_key {
        copy = copy.with_child(
            UiNode::new(format!("{id}.description"), UiNodeKind::Label)
                .with_text_key(description_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 11.0,
                    line_height_px: 14.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    UiNode::new(format!("{id}.row"), UiNodeKind::Panel)
        .with_class("settings-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::None,
            align_items: UiAlign::Center,
            gap: 14.0,
            padding: UiSpacing::xy(12.0, 4.0),
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: compact_breakpoint,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, SETTINGS_STACKED_ROW_HEIGHT]),
                padding: Some(UiSpacing::xy(8.0, 6.0)),
                gap: Some(6.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, SETTINGS_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(copy)
        .with_child(control)
}

pub(super) fn row_with_reset(
    palette: StudioUiPalette,
    row: UiNode,
    changed: bool,
    reset_key: &str,
) -> UiNode {
    if !changed {
        return row;
    }
    let tokens = palette.tokens();
    row.with_child(
        UiNode::new(format!("{reset_key}.reset"), UiNodeKind::Button)
            .with_class("settings-row-reset")
            .with_layout(UiLayout::fixed(22.0, 22.0))
            .with_tooltip_key("settings.reset_row")
            .with_accessibility_label_key("settings.reset_row")
            .with_accessibility_role(UiAccessibilityRole::Button)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("settings.reset:{reset_key}"),
            ))
            .with_child(
                UiNode::new(format!("{reset_key}.reset-icon"), UiNodeKind::Label)
                    .with_icon(
                        raf_ui::UiIcon::new(raf_ui::UiIconId::Refresh)
                            .with_size(raf_ui::UiIconSize::Small)
                            .with_tint(tokens.text_muted),
                    )
                    .with_layout(UiLayout::fixed(14.0, 14.0)),
            ),
    )
}

pub(super) fn quiet_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Button)
        .with_class("settings-quiet-button")
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_text_key(label_key)
        .with_text_style(UiTextStyle {
            inherit_color: true,
            ..UiTextStyle::button(tokens.text_muted)
        })
        .with_accessibility_label_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(super) fn action_button(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    command: &str,
    primary: bool,
) -> UiNode {
    action_button_with_width(
        palette,
        id,
        text_key,
        command,
        primary,
        if primary { 136.0 } else { 94.0 },
    )
}

pub(super) fn action_button_with_width(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    command: &str,
    primary: bool,
    width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if primary {
            "settings-primary-button"
        } else {
            "settings-secondary-button"
        })
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button(if primary {
            [18, 18, 20, 255]
        } else {
            tokens.text
        }))
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(super) fn command_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    class: &str,
    width: f32,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_accessibility_label_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}
