//! Retained RafUI document for the editor Console panel.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiIconSize,
    UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiSurface,
    UiTextOverflow, UiTextRole, UiTextStyle, UiToggle,
};

use crate::console::{ConsoleEntryId, ConsolePanel, LogEntry, LogLevel};
use crate::panels::editor_bottom_dock_styles::bottom_style_sheet;

pub fn build_console_surface(
    palette: StudioUiPalette,
    console: &ConsolePanel,
    input_enabled: bool,
    expanded_blocks: &[ConsoleEntryId],
    visible_range: Option<(usize, usize)>,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut log_scroll = UiNode::scroll_view("console.log-scroll", UiScrollAxis::Vertical)
        .with_class("console-log-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 2.0,
            padding: UiSpacing::xy(6.0, 4.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });

    let filtered_count = console.filtered_entries().count();
    let (start, end) = visible_range
        .map(|(start, end)| {
            let start = start.min(filtered_count);
            (start, end.min(filtered_count).max(start))
        })
        .unwrap_or((0, filtered_count));
    if start > 0 {
        log_scroll = log_scroll.with_child(spacer(
            "console.log.top-spacer",
            start as f32 * CONSOLE_ROW_ESTIMATE,
        ));
    }
    for (id, entry) in console
        .filtered_entries_with_ids()
        .skip(start)
        .take(end.saturating_sub(start))
    {
        let disclosed = expanded_blocks.contains(&id) || console.is_json_disclosed(id);
        log_scroll = log_scroll.with_child(console_entry(palette, id, entry, disclosed));
    }
    if end < filtered_count {
        log_scroll = log_scroll.with_child(spacer(
            "console.log.bottom-spacer",
            (filtered_count - end) as f32 * CONSOLE_ROW_ESTIMATE,
        ));
    }

    let input = if input_enabled {
        UiNode::new("console.input-row", UiNodeKind::Toolbar)
            .with_class("console-input-row")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 6.0,
                padding: UiSpacing::xy(6.0, 3.0),
                ..UiLayout::fixed(0.0, 30.0)
            })
            .with_child(
                UiNode::new("console.user-label", UiNodeKind::Label)
                    .with_text_key("console.user")
                    .with_class("console-user")
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::button(tokens.accent_hot)),
            )
            .with_child(
                UiNode::text_input(
                    "console.input",
                    raf_ui::UiTextInput {
                        value_key: "console.input".to_string(),
                        placeholder_key: Some("console.input_hint".to_string()),
                        max_length: 4096,
                        multiline: false,
                        password: false,
                        submit_command: Some("console.submit".to_string()),
                    },
                )
                .with_class("console-input")
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [120.0, 24.0],
                    ..UiLayout::fixed(0.0, 24.0)
                })
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
                )),
            )
            .with_child(
                UiNode::new("console.send", UiNodeKind::Button)
                    .with_class("console-send")
                    .with_layout(UiLayout::fixed(64.0, 24.0))
                    .with_text_key("console.send")
                    .with_text_style(UiTextStyle::button([255, 255, 255, 255]))
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        "console.submit",
                    )),
            )
    } else {
        UiNode::new("console.disabled-row", UiNodeKind::Panel)
            .with_class("console-disabled-row")
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::xy(8.0, 5.0),
                ..UiLayout::fixed(0.0, 42.0)
            })
            .with_child(
                UiNode::new("console.disabled", UiNodeKind::Label)
                    .with_text_key("console.commands_disabled")
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            )
            .with_child(
                UiNode::new("console.disabled-hint", UiNodeKind::Label)
                    .with_text_key("console.commands_enable_hint")
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            )
    };

    let mut root = UiNode::new("console.root", UiNodeKind::Panel)
        .with_class("bottom-panel")
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_style(palette.panel_style())
        .with_child(console_toolbar(palette, console, filtered_count));
    if filtered_count == 0 {
        root = root.with_child(console_empty_state(palette));
    } else {
        root = root.with_child(log_scroll);
    }
    if input_enabled {
        if let Some(recent) = console_recent_strip(palette, console) {
            root = root.with_child(recent);
        }
    }
    if input_enabled && console.input().trim_start().starts_with('/') {
        root = root.with_child(
            UiNode::new("console.command-hint", UiNodeKind::Label)
                .with_text_key("console.tab_hint")
                .with_layout(UiLayout::fixed(0.0, 18.0))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    root = root.with_child(input);

    let mut surface = UiSurface::new("editor.bottom.console", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

const CONSOLE_ROW_ESTIMATE: f32 = 26.0;

pub fn console_visible_range(
    console: &ConsolePanel,
    scroll_offset: f32,
    viewport_height: f32,
) -> (usize, usize) {
    let count = console.filtered_entries().count();
    let range = raf_ui::UiVirtualRange::for_vertical_list(
        count,
        scroll_offset,
        viewport_height,
        CONSOLE_ROW_ESTIMATE,
        5,
    );
    (range.start, range.end)
}

fn console_toolbar(
    palette: StudioUiPalette,
    console: &ConsolePanel,
    filtered_count: usize,
) -> UiNode {
    let tokens = palette.tokens();
    let mut toolbar = UiNode::new("console.toolbar", UiNodeKind::Toolbar)
        .with_class("console-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 2.0),
            overflow: UiOverflow::ScrollX,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        });

    toolbar = toolbar.with_child(command_button(
        "console.clear",
        "app.clear",
        UiIconId::Close,
        "console.clear",
    ));
    toolbar = toolbar.with_child(
        UiNode::toggle(
            "console.auto-scroll",
            UiToggle::new("console.auto-scroll", console.auto_scroll),
        )
        .with_class("console-toggle")
        .with_text_key("app.auto_scroll")
        .with_layout(UiLayout::fixed(110.0, 24.0).with_text_safe_area(true))
        .with_text_style(UiTextStyle::body(tokens.text_muted)),
    );
    toolbar = toolbar.with_child(
        UiNode::new("console.separator", UiNodeKind::Separator)
            .with_class("bottom-separator")
            .with_layout(UiLayout::fixed(1.0, 16.0)),
    );

    let (info_count, warn_count, error_count) = console_level_counts(console);
    let total = console.entries.len();
    for (id, key, level, width) in [
        ("all", "app.all", None, 52.0),
        ("info", "app.info", Some(LogLevel::Info), 56.0),
        ("warning", "app.warn", Some(LogLevel::Warning), 62.0),
        ("error", "app.error", Some(LogLevel::Error), 60.0),
    ] {
        let active = console.filter_level == level;
        let mut filter_button = UiNode::new(format!("console.filter.{id}"), UiNodeKind::Button)
            .with_class("console-filter")
            .with_layout(UiLayout::fixed(width, 24.0).with_text_safe_area(true))
            .with_text_key(key)
            .with_text_style(UiTextStyle::button(if active {
                tokens.text
            } else {
                tokens.text_muted
            }))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("console.filter.{id}"),
            ));
        if active {
            filter_button = filter_button.with_class("console-filter-active");
        }
        toolbar = toolbar.with_child(filter_button);
    }
    toolbar.with_child(
        UiNode::new("console.count", UiNodeKind::Label)
            .with_class("console-count")
            .with_text_value(format!(
                "{filtered_count}/{total} I:{info_count} W:{warn_count} E:{error_count}"
            ))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fit_content().with_text_safe_area(true)
            }),
    )
}

fn console_level_counts(console: &ConsolePanel) -> (usize, usize, usize) {
    let mut info = 0;
    let mut warn = 0;
    let mut error = 0;
    for entry in &console.entries {
        match entry.level {
            LogLevel::Info => info += 1,
            LogLevel::Warning => warn += 1,
            LogLevel::Error => error += 1,
        }
    }
    (info, warn, error)
}

fn console_empty_state(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("console.empty", UiNodeKind::Panel)
        .with_class("console-empty")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: raf_ui::UiJustify::Center,
            gap: 4.0,
            grow: 1.0,
            padding: UiSpacing::same(16.0),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("console.empty.icon", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::Console)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(20.0, 20.0)),
        )
        .with_child(
            UiNode::new("console.empty.label", UiNodeKind::Label)
                .with_text_key("console.empty")
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("console.empty.hint", UiNodeKind::Label)
                .with_text_key("console.empty_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn console_recent_strip(palette: StudioUiPalette, console: &ConsolePanel) -> Option<UiNode> {
    let tokens = palette.tokens();
    let recent: Vec<&String> = console.history_lines().take(5).collect();
    if recent.is_empty() {
        return None;
    }
    let mut row = UiNode::new("console.recent", UiNodeKind::Toolbar)
        .with_class("console-recent")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(6.0, 2.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("console.recent.label", UiNodeKind::Label)
                .with_text_key("console.recent")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    for (index, command) in recent.iter().enumerate() {
        row = row.with_child(
            UiNode::new(format!("console.recent.{index}"), UiNodeKind::Label)
                .with_class("console-recent-chip")
                .with_text_value((*command).clone())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: raf_ui::UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        );
    }
    Some(
        row.with_child(
            UiNode::new("console.recent.hint", UiNodeKind::Label)
                .with_text_key("console.history_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        ),
    )
}

fn console_entry(
    palette: StudioUiPalette,
    id: ConsoleEntryId,
    entry: &LogEntry,
    expanded: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let (level_icon, color) = match entry.level {
        LogLevel::Info => (UiIconId::Console, tokens.text),
        LogLevel::Warning => (UiIconId::Warning, tokens.warning),
        LogLevel::Error => (UiIconId::Error, tokens.danger),
    };
    let row_extra_class = match (&entry.level, entry.block.is_some(), entry.sender.is_some()) {
        (LogLevel::Error, _, _) => Some("console-entry-error"),
        (LogLevel::Warning, _, _) => Some("console-entry-warn"),
        (_, true, _) => Some("console-block"),
        (_, false, true) => Some("console-entry-user"),
        _ => None,
    };
    let mut row = UiNode::new(format!("console.entry.{id}"), UiNodeKind::Panel)
        .with_class("console-entry")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Stretch,
            gap: 1.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    if let Some(extra) = row_extra_class {
        row = row.with_class(extra);
    }

    let mut header = UiNode::new(format!("console.entry.{id}.header"), UiNodeKind::Toolbar)
        .with_class("console-entry-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 2.0),
            align_self: Some(UiAlign::Stretch),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("console.entry.{id}.level"), UiNodeKind::Label)
                .with_class("console-level-icon")
                .with_icon(
                    UiIcon::new(level_icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(color),
                )
                .with_layout(UiLayout::fixed(16.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("console.entry.{id}.timestamp"), UiNodeKind::Label)
                .with_text_value(entry.timestamp.clone())
                .with_layout(UiLayout::fixed(56.0, 18.0).with_text_safe_area(true))
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: raf_ui::UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                }),
        );
    if let Some(sender) = &entry.sender {
        header = header.with_child(
            UiNode::new(format!("console.entry.{id}.sender"), UiNodeKind::Label)
                .with_text_value(sender.clone())
                .with_layout(UiLayout::fixed(60.0, 18.0).with_text_safe_area(true))
                .with_text_style(UiTextStyle::button(tokens.accent_hot)),
        );
    } else {
        header = header.with_child(
            UiNode::new(
                format!("console.entry.{id}.sender-empty"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::fixed(60.0, 18.0)),
        );
    }
    header = header.with_child(
        UiNode::new(format!("console.entry.{id}.message"), UiNodeKind::Label)
            .with_text_value(entry.message.clone())
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [60.0, 18.0],
                overflow: UiOverflow::Clip,
                ..UiLayout::fixed(0.0, 18.0)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_text_safe_area(true)
            })
            .with_text_style(UiTextStyle::body(color)),
    );
    if entry.block.is_some() {
        header = header.with_child(
            UiNode::new(format!("console.entry.{id}.expand"), UiNodeKind::Button)
                .with_class("console-json-button")
                .with_text_value(if expanded { "-" } else { "+" })
                .with_layout(UiLayout::fixed(22.0, 18.0))
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("console.block.toggle:{id}"),
                )),
        );
    }
    row = row.with_child(header);

    if let Some(block) = &entry.block {
        let mut block_body = UiNode::new(format!("console.entry.{id}.block"), UiNodeKind::Panel)
            .with_class("console-block-body")
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Stretch,
                gap: 2.0,
                padding: UiSpacing {
                    left: 82.0,
                    right: 6.0,
                    top: 2.0,
                    bottom: 4.0,
                },
                align_self: Some(UiAlign::Stretch),
                ..UiLayout::fit_content()
            });
        for (line_index, line) in block.lines.iter().enumerate() {
            block_body = block_body.with_child(
                UiNode::new(
                    format!("console.entry.{id}.line.{line_index}"),
                    UiNodeKind::Label,
                )
                .with_text_value(line.clone())
                .with_layout(UiLayout {
                    align_self: Some(UiAlign::Stretch),
                    width_mode: UiSizeMode::Fill,
                    overflow: UiOverflow::Clip,
                    ..UiLayout::fit_content()
                })
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: raf_ui::UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                }),
            );
        }
        if expanded {
            block_body = block_body.with_child(
                UiNode::new(format!("console.entry.{id}.json"), UiNodeKind::Label)
                    .with_text_value(block.json.clone())
                    .with_layout(UiLayout {
                        width_mode: UiSizeMode::Fill,
                        max_size: [0.0, 96.0],
                        align_self: Some(UiAlign::Stretch),
                        ..UiLayout::fit_content()
                    })
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Monospace,
                        size_px: 9.0,
                        line_height_px: 13.0,
                        weight: raf_ui::UiFontWeight::Regular,
                        color: tokens.text_muted,
                        inherit_color: false,
                    }),
            );
        }
        row = row.with_child(block_body);
    }
    row
}

fn command_button(id: &str, text_key: &str, icon: UiIconId, command: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("console-command-button")
        .with_layout(UiLayout::fixed(68.0, 24.0).with_text_safe_area(true))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button([196, 201, 209, 255]))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::ConsoleBlock;

    fn child<'a>(node: &'a UiNode, id: &str) -> &'a UiNode {
        if node.id == id {
            return node;
        }
        node.children
            .iter()
            .find_map(|candidate| find_node(candidate, id))
            .unwrap_or_else(|| panic!("missing node {id}"))
    }

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|candidate| find_node(candidate, id))
    }

    #[test]
    fn console_toolbar_has_enough_width_for_fixed_controls() {
        let surface = build_console_surface(
            StudioUiPalette::IndustrialDark,
            &ConsolePanel::default(),
            false,
            &[],
            None,
        );
        let toolbar = child(&surface.root, "console.toolbar");
        let fixed_width: f32 = toolbar
            .children
            .iter()
            .map(|node| node.layout.basis[0])
            .sum();
        let gaps = toolbar.layout.gap * toolbar.children.len().saturating_sub(1) as f32;
        let padding = toolbar.layout.padding.left + toolbar.layout.padding.right;

        assert!(fixed_width + gaps + padding <= 460.0);
        assert_eq!(child(toolbar, "console.clear").layout.basis, [68.0, 24.0]);
        assert_eq!(
            child(toolbar, "console.auto-scroll").layout.basis,
            [110.0, 24.0]
        );
    }

    #[test]
    fn console_messages_use_columns_instead_of_literal_wrapping() {
        let mut console = ConsolePanel::default();
        console.log_user("User", "a message that must remain one horizontal row");
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);
        let log_scroll = child(&surface.root, "console.log-scroll");
        let entry = log_scroll
            .children
            .last()
            .expect("user entry should be present");
        let header = child(entry, "console.entry.1.header");
        let message = child(header, "console.entry.1.message");

        assert_eq!(message.layout.grow, 1.0);
        assert_eq!(message.layout.min_size[0], 60.0);
        assert_eq!(message.layout.basis[1], 18.0);
        assert_eq!(
            child(entry, "console.entry.1.sender").layout.basis,
            [60.0, 18.0]
        );
    }

    #[test]
    fn console_json_toggle_uses_the_command_consumed_by_the_workbench() {
        let mut console = ConsolePanel::default();
        console.entries.push(LogEntry {
            id: 1,
            level: LogLevel::Info,
            message: "Result".to_string(),
            timestamp: "12:00:00".to_string(),
            sender: None,
            block: Some(ConsoleBlock {
                title: "Result".to_string(),
                lines: vec!["ok".to_string()],
                json: "{}".to_string(),
            }),
        });
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);
        let toggle = child(&surface.root, "console.entry.1.expand");

        assert!(toggle.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "console.block.toggle:1"
        )));
    }

    #[test]
    fn console_empty_state_appears_when_filter_hides_everything() {
        let mut console = ConsolePanel::default();
        console.set_filter_level(Some(LogLevel::Error));
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, true, &[], None);

        assert!(find_node(&surface.root, "console.empty").is_some());
        assert!(find_node(&surface.root, "console.log-scroll").is_none());
    }

    #[test]
    fn console_recent_strip_shows_session_command_history() {
        let mut console = ConsolePanel::default();
        console.log_user("User", "/help");
        console.log_user("User", "/project.info");
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, true, &[], None);

        let recent = child(&surface.root, "console.recent");
        assert!(find_node(recent, "console.recent.0").is_some());
        assert!(find_node(recent, "console.recent.hint").is_some());
    }

    #[test]
    fn console_error_entries_use_severity_row_class() {
        let mut console = ConsolePanel::default();
        console.log(LogLevel::Error, "boom");
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);
        let log_scroll = child(&surface.root, "console.log-scroll");
        let entry = log_scroll.children.last().expect("error entry present");

        assert!(entry.classes.iter().any(|class| class == "console-entry"));
        assert!(entry
            .classes
            .iter()
            .any(|class| class == "console-entry-error"));
    }

    #[test]
    fn console_entries_without_sender_keep_message_column_aligned() {
        let console = ConsolePanel::default();
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);
        let log_scroll = child(&surface.root, "console.log-scroll");
        let entry = log_scroll.children.last().expect("engine entry present");
        let header = child(entry, "console.entry.0.header");

        assert!(find_node(header, "console.entry.0.sender-empty").is_some());
        assert!(find_node(header, "console.entry.0.message").is_some());
    }

    #[test]
    fn console_disclosed_json_renders_without_caller_param() {
        let mut console = ConsolePanel::default();
        console.entries.push(LogEntry {
            id: 7,
            level: LogLevel::Info,
            message: "Console".to_string(),
            timestamp: "12:00:00".to_string(),
            sender: None,
            block: Some(ConsoleBlock {
                title: "Console".to_string(),
                lines: vec!["xd".to_string()],
                json: "{}".to_string(),
            }),
        });
        assert!(console.toggle_json_disclosure(7));
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);

        assert!(find_node(&surface.root, "console.entry.7.json").is_some());
    }
}
