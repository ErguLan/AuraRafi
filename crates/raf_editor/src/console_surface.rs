//! Retained RafUI document for the editor Console.
//!
//! Console data, command parsing, and history remain owned by `ConsolePanel`.
//! This module only composes a typed view of that data.

use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow, UiImage,
    UiImageFit, UiImageSource, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSpacing,
    UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiTextInput, UiTextStyle, UiToggle,
};

use crate::panels::console::{ConsolePanel, LogEntry, LogLevel};

const CONTROL_HEIGHT: f32 = 30.0;

pub fn build_console_surface(
    palette: StudioUiPalette,
    console: &ConsolePanel,
    input_enabled: bool,
    expanded_blocks: &[usize],
) -> UiSurface {
    let tokens = palette.tokens();
    let mut entries = UiNode::scroll_view("console.entries", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 2.0,
            padding: UiSpacing::xy(10.0, 6.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent());
    for entry in filtered_entries(palette, console, expanded_blocks) {
        entries = entries.with_child(entry);
    }
    let root = UiNode::new("console.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(palette.root_style())
        .with_child(toolbar(palette, console))
        .with_child(input_area(
            palette,
            input_enabled,
            console.input().starts_with('/'),
        ))
        .with_child(entries);
    let mut surface = UiSurface::new("editor-console", palette, root);
    surface.style_sheet = console_style_sheet(palette, tokens);
    surface
}

fn toolbar(palette: StudioUiPalette, console: &ConsolePanel) -> UiNode {
    let tokens = palette.tokens();
    let mut toolbar = UiNode::new("console.toolbar", UiNodeKind::Toolbar)
        .with_class("console-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(10.0, 5.0),
            gap: 8.0,
            compact: UiCompactMode::Wrap,
            ..UiLayout::fixed(0.0, 38.0)
        })
        .with_child(command_button(
            "console.clear",
            "app.clear",
            "console.clear",
            "console-secondary-button",
            tokens.text,
        ))
        .with_child(
            UiNode::toggle(
                "console.auto-scroll",
                UiToggle::new("console.auto-scroll", console.auto_scroll),
            )
            .with_class(if console.auto_scroll {
                "console-toggle-on"
            } else {
                "console-toggle"
            })
            .with_layout(UiLayout::fixed(42.0, 24.0))
            .with_child(toggle_knob(
                palette,
                "console.auto-scroll",
                console.auto_scroll,
            )),
        )
        .with_child(
            UiNode::new("console.auto-scroll-label", UiNodeKind::Label)
                .with_text_key("app.auto_scroll")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(76.0, 20.0)),
        )
        .with_child(
            UiNode::new("console.filter-divider", UiNodeKind::Separator)
                .with_layout(UiLayout::fixed(1.0, 20.0)),
        );

    for (id, label_key, level) in [
        ("all", "app.all", None),
        ("info", "app.info", Some(LogLevel::Info)),
        ("warning", "app.warn", Some(LogLevel::Warning)),
        ("error", "app.error", Some(LogLevel::Error)),
    ] {
        let active = console.filter_level == level;
        toolbar = toolbar.with_child(command_button(
            format!("console.filter.{id}"),
            label_key,
            format!("console.filter.{id}"),
            if active {
                "console-filter-active"
            } else {
                "console-filter"
            },
            if active {
                [18, 18, 20, 255]
            } else {
                tokens.text_muted
            },
        ));
    }
    toolbar
}

fn input_area(palette: StudioUiPalette, input_enabled: bool, shows_command_hint: bool) -> UiNode {
    let tokens = palette.tokens();
    let mut area = UiNode::new("console.input-area", UiNodeKind::Panel)
        .with_class("console-input-area")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::xy(10.0, 5.0),
            gap: 3.0,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent());

    if !input_enabled {
        return area.with_child(
            UiNode::new("console.commands-disabled", UiNodeKind::Label)
                .with_text_key("console.commands_disabled")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 22.0)),
        );
    }

    let input = UiNode::text_input(
        "console.input",
        UiTextInput {
            value_key: "console.input".to_string(),
            placeholder_key: Some("console.input_hint".to_string()),
            max_length: 4_096,
            multiline: false,
            password: false,
            submit_command: Some("console.submit".to_string()),
        },
    )
    .with_class("console-input")
    .with_layout(UiLayout {
        grow: 1.0,
        ..UiLayout::fixed(0.0, CONTROL_HEIGHT)
    })
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("Tab".to_string()),
        "console.autocomplete",
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("ArrowUp".to_string()),
        "console.history.previous",
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("ArrowDown".to_string()),
        "console.history.next",
    ));

    area = area.with_child(
        UiNode::new("console.input-row", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                ..UiLayout::fixed(0.0, CONTROL_HEIGHT)
            })
            .with_style(UiStyle::transparent())
            .with_child(
                UiNode::new("console.user", UiNodeKind::Label)
                    .with_text_key("console.user")
                    .with_text_style(UiTextStyle::body(tokens.accent))
                    .with_layout(UiLayout::fixed(52.0, 20.0)),
            )
            .with_child(input)
            .with_child(send_button(palette)),
    );

    if shows_command_hint {
        area = area.with_child(
            UiNode::new("console.command-hint", UiNodeKind::Label)
                .with_text_key("console.tab_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 18.0)),
        );
    }
    area
}

fn filtered_entries(
    palette: StudioUiPalette,
    console: &ConsolePanel,
    expanded_blocks: &[usize],
) -> Vec<UiNode> {
    console
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            console
                .filter_level
                .map_or(true, |level| entry.level == level)
        })
        .map(|(index, entry)| entry_node(palette, index, entry, expanded_blocks.contains(&index)))
        .collect()
}

fn entry_node(palette: StudioUiPalette, index: usize, entry: &LogEntry, expanded: bool) -> UiNode {
    let tokens = palette.tokens();
    let (class, color) = match entry.level {
        LogLevel::Info => ("console-entry-info", tokens.text),
        LogLevel::Warning => ("console-entry-warning", tokens.warning),
        LogLevel::Error => ("console-entry-error", tokens.danger),
    };
    let id = format!("console.entry.{index}");
    let mut node = UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::xy(8.0, 4.0),
            gap: 3.0,
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new(format!("{id}.head"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    ..UiLayout::default()
                })
                .with_style(UiStyle::transparent())
                .with_child(
                    UiNode::new(format!("{id}.time"), UiNodeKind::Label)
                        .with_text_key(entry.timestamp.clone())
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(64.0, 18.0)),
                )
                .with_child(sender_node(palette, &id, entry.sender.as_deref()))
                .with_child(
                    UiNode::new(format!("{id}.message"), UiNodeKind::Label)
                        .with_text_key(entry.message.clone())
                        .with_text_style(UiTextStyle::body(color))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                ),
        );

    if let Some(block) = &entry.block {
        for (line_index, line) in block.lines.iter().enumerate() {
            node = node.with_child(
                UiNode::new(format!("{id}.line.{line_index}"), UiNodeKind::Label)
                    .with_text_key(line.clone())
                    .with_text_style(UiTextStyle::body(tokens.text))
                    .with_layout(UiLayout::fixed(0.0, 18.0)),
            );
        }
        node = node.with_child(command_button(
            format!("{id}.json-toggle"),
            "console.json",
            format!("console.block.toggle.{index}"),
            "console-json-button",
            tokens.text_muted,
        ));
        if expanded {
            node = node.with_child(
                UiNode::new(format!("{id}.json"), UiNodeKind::Label)
                    .with_text_key(block.json.clone())
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fixed(0.0, 80.0)),
            );
        }
    }
    node
}

fn sender_node(palette: StudioUiPalette, entry_id: &str, sender: Option<&str>) -> UiNode {
    let tokens = palette.tokens();
    let value = sender.map(|sender| format!("{sender}:"));
    UiNode::new(format!("{entry_id}.sender"), UiNodeKind::Label)
        .with_text_key(value.unwrap_or_default())
        .with_text_style(UiTextStyle::body(tokens.accent))
        .with_layout(UiLayout::fixed(72.0, 18.0))
}

fn toggle_knob(palette: StudioUiPalette, id: &str, value: bool) -> UiNode {
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
                palette.tokens().text_muted
            },
            border: [0, 0, 0, 0],
            text: palette.tokens().text,
            border_width: 0.0,
            radius: 9.0,
            opacity: 1.0,
        })
}

fn send_button(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("console.send", UiNodeKind::Button)
        .with_class("console-primary-button")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: raf_ui::UiJustify::Center,
            gap: 6.0,
            min_size: [78.0, CONTROL_HEIGHT],
            padding: UiSpacing::xy(10.0, 0.0),
            ..UiLayout::default()
        })
        .with_child(
            UiNode::image(
                "console.send.icon",
                UiImage {
                    source: UiImageSource::new("editor.console.send"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new("console.send.label", UiNodeKind::Label)
                .with_text_key("console.send")
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout::fixed(38.0, 20.0)),
        )
        .with_tooltip_key("console.send")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "console.submit",
        ))
}

fn command_button(
    id: impl Into<String>,
    label_key: &str,
    command: impl Into<String>,
    class: &str,
    color: [u8; 4],
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(color))
        .with_class(class)
        .with_layout(UiLayout {
            basis: [0.0, CONTROL_HEIGHT],
            min_size: [58.0, CONTROL_HEIGHT],
            padding: UiSpacing::xy(9.0, 0.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn console_style_sheet(palette: StudioUiPalette, tokens: raf_ui::UiTokens) -> UiStyleSheet {
    let toggle_fill = match palette {
        StudioUiPalette::IndustrialDark => [58, 67, 77, 255],
        StudioUiPalette::PaperLight => [190, 194, 198, 255],
    };
    UiStyleSheet {
        rules: vec![
            style_rule(
                "console-toolbar",
                tokens.surface_alt,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "console-secondary-button",
                tokens.surface_alt,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "console-primary-button",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                4.0,
            ),
            style_rule("console-filter", [0, 0, 0, 0], [0, 0, 0, 0], 0.0, 4.0),
            style_rule(
                "console-filter-active",
                tokens.surface_raised,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "console-input",
                tokens.surface_raised,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "console-input-area",
                tokens.surface,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule("console-toggle", toggle_fill, tokens.border, 1.0, 12.0),
            style_rule(
                "console-toggle-on",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                12.0,
            ),
            style_rule("console-entry-info", [0, 0, 0, 0], tokens.border, 0.0, 2.0),
            style_rule(
                "console-entry-warning",
                [0, 0, 0, 0],
                tokens.warning,
                1.0,
                2.0,
            ),
            style_rule("console-entry-error", [0, 0, 0, 0], tokens.danger, 1.0, 2.0),
            style_rule("console-json-button", [0, 0, 0, 0], [0, 0, 0, 0], 0.0, 2.0),
            UiStyleRule::new(
                UiStyleSelector::Class("console-secondary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
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
    fn console_surface_keeps_input_and_entries_as_retained_nodes() {
        let console = ConsolePanel::default();
        let surface = build_console_surface(StudioUiPalette::IndustrialDark, &console, true, &[]);
        assert!(find_node(&surface.root, "console.input").is_some());
        assert!(find_node(&surface.root, "console.entries").is_some());
        assert!(find_node(&surface.root, "console.entry.0").is_some());
    }
}
