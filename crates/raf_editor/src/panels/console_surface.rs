//! Retained RafUI document for the editor Console panel.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId,
    UiIconSize, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing,
    UiSurface, UiTextOverflow, UiTextRole, UiTextStyle, UiToggle,
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
    build_console_document(
        palette,
        console,
        input_enabled,
        expanded_blocks,
        visible_range,
        true,
    )
}

/// The dock header owns its filters; the report itself starts at its columns.
pub fn build_console_table_surface(
    palette: StudioUiPalette,
    console: &ConsolePanel,
    input_enabled: bool,
    expanded_blocks: &[ConsoleEntryId],
    visible_range: Option<(usize, usize)>,
) -> UiSurface {
    build_console_document(
        palette,
        console,
        input_enabled,
        expanded_blocks,
        visible_range,
        false,
    )
}

fn build_console_document(
    palette: StudioUiPalette,
    console: &ConsolePanel,
    input_enabled: bool,
    expanded_blocks: &[ConsoleEntryId],
    visible_range: Option<(usize, usize)>,
    toolbar: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let terminal = console.terminal_palette;
    let mut log_scroll = UiNode::scroll_view("console.log-scroll", UiScrollAxis::Vertical)
        .with_class(if terminal {
            "console-log-scroll console-terminal-surface"
        } else {
            "console-log-scroll"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 0.0,
            padding: UiSpacing::xy(0.0, 0.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    if terminal {
        log_scroll = log_scroll
            .with_style(raf_ui::UiStyle::transparent().with_fill(CONSOLE_TERMINAL_BACKGROUND));
    }

    let groups = console.visible_groups();
    let filtered_count = groups.len();
    let heights = console_group_heights(&groups, console.wrap_messages);
    let (start, end) = visible_range
        .map(|(start, end)| {
            let start = start.min(filtered_count);
            (start, end.min(filtered_count).max(start))
        })
        .unwrap_or((0, filtered_count));
    if start > 0 {
        log_scroll = log_scroll.with_child(spacer(
            "console.log.top-spacer",
            heights[..start].iter().sum(),
        ));
    }
    for (id, entry, repeat) in groups.iter().skip(start).take(end.saturating_sub(start)) {
        let disclosed = expanded_blocks.contains(id) || console.is_json_disclosed(*id);
        log_scroll = log_scroll.with_child(console_entry(
            palette,
            *id,
            entry,
            *repeat,
            disclosed,
            console.wrap_messages,
            console.show_timestamps,
            terminal,
            console.entry_has_search_match(entry),
            &console.search_query,
            console.source_filter.as_deref(),
            [console.column_width(0), console.column_width(1)],
        ));
    }
    if end < filtered_count {
        log_scroll = log_scroll.with_child(spacer(
            "console.log.bottom-spacer",
            heights[end..].iter().sum(),
        ));
    }

    let input = if input_enabled {
        UiNode::new("console.input-row", UiNodeKind::Toolbar)
            .with_class(if terminal {
                "console-input-row console-terminal-input"
            } else {
                "console-input-row"
            })
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                padding: UiSpacing::xy(8.0, 2.0),
                ..UiLayout::fixed(0.0, 30.0)
            })
            .with_child(
                UiNode::new("console.prompt", UiNodeKind::Label)
                    .with_class("console-prompt")
                    .with_text_value(">")
                    .with_layout(UiLayout::fixed(10.0, 20.0))
                    .with_text_style(UiTextStyle {
                        role: UiTextRole::Monospace,
                        size_px: 13.0,
                        line_height_px: 18.0,
                        weight: raf_ui::UiFontWeight::Bold,
                        color: tokens.accent_hot,
                        inherit_color: false,
                    }),
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
            .with_text_style(UiTextStyle {
                role: UiTextRole::Monospace,
                size_px: 12.0,
                line_height_px: 16.0,
                weight: raf_ui::UiFontWeight::Regular,
                color: tokens.text,
                inherit_color: false,
            })
            .with_child(
                UiNode::new("console.send", UiNodeKind::Button)
                    .with_class("console-send")
                    .with_layout(UiLayout::fixed(26.0, 24.0))
                    .focusable()
                    .with_accessibility_label_key("console.send")
                    .with_tooltip_key("console.send")
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        "console.submit",
                    ))
                    .with_child(
                        UiNode::new("console.send.icon", UiNodeKind::Label)
                            .with_icon(
                                UiIcon::new(UiIconId::ChevronRight)
                                    .with_size(UiIconSize::Small)
                                    .with_tint(tokens.text),
                            )
                            .with_layout(UiLayout::fixed(14.0, 14.0)),
                    ),
            )
    } else {
        UiNode::new("console.disabled-row", UiNodeKind::Panel)
            .with_class("console-disabled-row")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                padding: UiSpacing::xy(8.0, 5.0),
                ..UiLayout::fixed(0.0, 40.0)
            })
            .with_child(
                UiNode::new("console.disabled", UiNodeKind::Label)
                    .with_text_key("console.commands_disabled")
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::fit_content()
                    })
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_text_overflow(UiTextOverflow::Ellipsis),
            )
            .with_child(
                UiNode::new("console.enable-commands", UiNodeKind::Button)
                    .with_class("console-enable-commands")
                    .with_layout(UiLayout::fixed(150.0, 26.0))
                    .with_text_key("console.enable_commands")
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .focusable()
                    .with_tooltip_key("console.commands_enable_hint")
                    .with_accessibility_label_key("console.enable_commands")
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        "console.commands.enable",
                    )),
            )
    };

    let mut root = UiNode::new("console.root", UiNodeKind::Panel)
        .with_class(if terminal {
            "bottom-panel console-terminal"
        } else {
            "bottom-panel"
        })
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_style(if terminal {
            raf_ui::UiStyle::transparent()
                .with_fill(CONSOLE_TERMINAL_BACKGROUND)
                .with_border(CONSOLE_TERMINAL_BORDER, 1.0)
        } else {
            palette.panel_style()
        });
    if toolbar {
        root = root.with_child(console_toolbar(
            palette,
            console,
            filtered_count,
            input_enabled,
        ));
    }
    root = root.with_child(console_columns(palette, console));
    if filtered_count == 0 {
        root = root.with_child(console_empty_state(palette));
    } else {
        root = root.with_child(log_scroll);
        if !console.auto_scroll {
            root = root.with_child(console_resume_autoscroll(palette));
        }
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
const CONSOLE_TERMINAL_BACKGROUND: [u8; 4] = [0, 0, 0, 255];
const CONSOLE_TERMINAL_BORDER: [u8; 4] = [20, 40, 24, 255];

fn console_group_heights(groups: &[(ConsoleEntryId, &LogEntry, usize)], wrap: bool) -> Vec<f32> {
    groups
        .iter()
        .map(|(_, entry, _)| {
            let text_width = entry.message.chars().count()
                + entry
                    .sender
                    .as_deref()
                    .map(|sender| sender.chars().count() + 2)
                    .unwrap_or(0);
            let message_lines = if wrap {
                text_width.div_ceil(96).max(1) as f32
            } else {
                1.0
            };
            let block_lines = entry
                .block
                .as_ref()
                .map(|block| block.lines.len())
                .unwrap_or(0) as f32;
            let row_height = if wrap {
                20.0 + (message_lines - 1.0) * 16.0
            } else {
                CONSOLE_ROW_ESTIMATE
            };
            row_height + block_lines * 14.0
        })
        .collect()
}

pub fn console_visible_range(
    console: &ConsolePanel,
    scroll_offset: f32,
    viewport_height: f32,
) -> (usize, usize) {
    let groups = console.visible_groups();
    let heights = console_group_heights(&groups, console.wrap_messages);
    let scroll_offset = scroll_offset.max(0.0);
    let viewport_height = viewport_height.max(1.0);
    let mut start = 0;
    let mut consumed = 0.0;
    while start < heights.len() && consumed + heights[start] <= scroll_offset {
        consumed += heights[start];
        start += 1;
    }
    let mut end = start;
    while end < heights.len() && consumed < scroll_offset + viewport_height {
        consumed += heights[end];
        end += 1;
    }
    (start.saturating_sub(4), (end + 4).min(heights.len()))
}

fn console_columns(palette: StudioUiPalette, console: &ConsolePanel) -> UiNode {
    let tokens = palette.tokens();
    let terminal = console.terminal_palette;
    let mut row = UiNode::new("console.columns", UiNodeKind::Panel)
        .with_class(if terminal {
            "console-columns console-terminal-header"
        } else {
            "console-columns"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 2.0),
            ..UiLayout::fixed(0.0, 30.0).fill_width()
        })
        .with_style(raf_ui::UiStyle::transparent().with_fill(if terminal {
            CONSOLE_TERMINAL_BACKGROUND
        } else {
            tokens.surface_raised
        }));
    let mut columns: Vec<(&str, f32)> =
        vec![("electronics.desktop.severity", console.column_width(0))];
    if console.show_timestamps {
        columns.push(("electronics.desktop.time", console.column_width(1)));
    }
    columns.push(("electronics.desktop.message", 0.0));
    let last = columns.len().saturating_sub(1);
    for (index, (key, width)) in columns.iter().enumerate() {
        let resizable = index < last;
        let label_width = if resizable {
            (width - 9.0).max(20.0)
        } else {
            *width
        };
        let mut label = UiNode::new(format!("console.columns.{index}"), UiNodeKind::Label)
            .with_text_key(*key)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Label,
                size_px: 12.0,
                line_height_px: 18.0,
                weight: raf_ui::UiFontWeight::Regular,
                color: if terminal {
                    [126, 200, 138, 255]
                } else {
                    tokens.text_muted
                },
                inherit_color: false,
            })
            .with_text_overflow(UiTextOverflow::Ellipsis);
        label = if resizable {
            label.with_layout(UiLayout::fixed(label_width, 22.0))
        } else {
            label.with_layout(UiLayout {
                grow: 1.0,
                min_size: [40.0, 22.0],
                ..UiLayout::fit_content().fill_width()
            })
        };
        row = row.with_child(label);
        if resizable {
            row = row.with_child(
                UiNode::new(
                    format!("console.columns.resizer.{index}"),
                    UiNodeKind::Separator,
                )
                .with_class("console-column-resizer editor-splitter-vertical")
                .with_layout(UiLayout::fixed(5.0, 22.0))
                .with_tooltip_key("console.resize_column")
                .with_accessibility_label_key("console.resize_column")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::DragStart,
                    format!("console.column.resize.start:{index}"),
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::DragMove,
                    format!("console.column.resize:{index}"),
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::DragEnd,
                    format!("console.column.resize.end:{index}"),
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::DoubleClick,
                    "console.column.reset",
                )),
            );
        }
    }
    row
}

fn console_resume_autoscroll(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("console.resume-autoscroll", UiNodeKind::Button)
        .with_class("console-resume-pill")
        .with_layout(UiLayout {
            align_self: Some(UiAlign::End),
            padding: UiSpacing::xy(10.0, 2.0),
            ..UiLayout::fixed(164.0, 22.0)
        })
        .with_icon(UiIcon::new(UiIconId::ChevronDown).with_size(UiIconSize::Custom(12)))
        .with_text_key("console.resume_autoscroll")
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_tooltip_key("console.resume_autoscroll")
        .with_accessibility_label_key("console.resume_autoscroll")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "console.auto-scroll.resume",
        ))
}

fn console_toolbar(
    palette: StudioUiPalette,
    console: &ConsolePanel,
    filtered_count: usize,
    commands_enabled: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let mut toolbar = UiNode::new("console.toolbar", UiNodeKind::Toolbar)
        .with_class("console-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 2.0),
            overflow: UiOverflow::Clip,
            responsive: vec![raf_ui::UiResponsiveRule {
                max_width: 460.0,
                flow: Some(UiFlow::RowWrap),
                basis: Some([0.0, 60.0]),
                padding: None,
                gap: None,
                compact: None,
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        });

    toolbar = toolbar.with_child(command_button(
        "console.clear",
        "app.clear",
        UiIconId::Close,
        "console.clear",
    ));
    let mut commands_toggle = UiNode::new("console.commands-toggle", UiNodeKind::Button)
        .with_class("console-icon-button")
        .with_icon(UiIcon::new(UiIconId::Console).with_size(UiIconSize::Small))
        .with_layout(UiLayout::fixed(24.0, 24.0))
        .with_tooltip_key("settings.command_console_enabled")
        .with_accessibility_label_key("settings.command_console_enabled")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "console.commands.toggle",
        ));
    if commands_enabled {
        commands_toggle = commands_toggle.with_class("console-icon-button-active");
    }
    toolbar = toolbar.with_child(commands_toggle);
    toolbar = toolbar.with_child(
        UiNode::new("console.export", UiNodeKind::Button)
            .with_class("console-icon-button")
            .with_icon(UiIcon::new(UiIconId::File).with_size(UiIconSize::Small))
            .with_layout(UiLayout::fixed(24.0, 24.0))
            .with_tooltip_key("console.export")
            .with_accessibility_label_key("console.export")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "console.export",
            )),
    );
    toolbar = toolbar.with_child(
        UiNode::toggle(
            "console.auto-scroll",
            UiToggle::new("console.auto-scroll", console.auto_scroll),
        )
        .with_class("console-toggle")
        .with_text_key("app.auto_scroll")
        .with_layout(UiLayout::fixed(92.0, 24.0).with_text_safe_area(true))
        .with_text_style(UiTextStyle::body(tokens.text_muted)),
    );
    toolbar = toolbar.with_child(
        UiNode::new("console.separator", UiNodeKind::Separator)
            .with_class("bottom-separator")
            .with_layout(UiLayout::fixed(1.0, 16.0)),
    );

    let (info_count, success_count, warn_count, error_count, verbose_count) =
        console_level_counts(console);
    let total = console.entries.len();
    for (id, key, level, width) in [
        ("all", "app.all", None, 42.0),
        ("info", "app.info", Some(LogLevel::Info), 46.0),
        ("warning", "app.warn", Some(LogLevel::Warning), 52.0),
        ("error", "app.error", Some(LogLevel::Error), 50.0),
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
                "{filtered_count}/{total} I:{info_count} S:{success_count} W:{warn_count} E:{error_count} V:{verbose_count}"
            ))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fit_content().with_text_safe_area(true)
            }),
    )
}

pub fn build_console_filters_surface(
    palette: StudioUiPalette,
    console: &ConsolePanel,
    commands_enabled: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("console.filters.menu", UiNodeKind::Panel)
        .with_class("console-filters-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Stretch,
            gap: 4.0,
            padding: UiSpacing::same(8.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        });
    root = root.with_child(
        UiNode::text_input(
            "console.search",
            raf_ui::UiTextInput {
                value_key: "console.search".to_string(),
                placeholder_key: Some("console.search_hint".to_string()),
                max_length: 256,
                multiline: false,
                password: false,
                submit_command: None,
            },
        )
        .with_class("console-search")
        .with_layout(UiLayout::fixed(0.0, 28.0).fill_width())
        .with_text_style(UiTextStyle {
            role: UiTextRole::Monospace,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: raf_ui::UiFontWeight::Regular,
            color: tokens.text,
            inherit_color: false,
        })
        .with_accessibility_label_key("console.search_hint"),
    );
    for (id, key, enabled) in [
        (
            "console.commands-toggle",
            "settings.command_console_enabled",
            commands_enabled,
        ),
        ("console.wrap-toggle", "console.wrap", console.wrap_messages),
        (
            "console.time-toggle",
            "console.show-timestamps",
            console.show_timestamps,
        ),
        (
            "console.terminal-toggle",
            "console.terminal",
            console.terminal_palette,
        ),
    ] {
        root = root.with_child(
            UiNode::toggle(id, UiToggle::new(key, enabled))
                .with_class("console-toggle")
                .with_text_key(key)
                .with_layout(UiLayout::fixed(0.0, 26.0).fill_width())
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    root = root.with_child(
        UiNode::new("console.filters.separator", UiNodeKind::Separator)
            .with_class("bottom-separator")
            .with_layout(UiLayout::fixed(0.0, 1.0).fill_width()),
    );
    let mut severity =
        UiNode::new("console.filters.severity", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 26.0).fill_width()
        });
    for (id, key, level) in [
        ("all", "app.all", None),
        ("info", "app.info", Some(LogLevel::Info)),
        ("warning", "app.warn", Some(LogLevel::Warning)),
        ("error", "app.error", Some(LogLevel::Error)),
    ] {
        let active = console.filter_level == level;
        let mut button = UiNode::new(format!("console.filter.{id}"), UiNodeKind::Button)
            .with_class(if active {
                "console-filter console-filter-active"
            } else {
                "console-filter"
            })
            .with_text_key(key)
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [40.0, 24.0],
                ..UiLayout::fit_content()
            })
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
            button = button.with_class("console-filter-active");
        }
        severity = severity.with_child(button);
    }
    root = root.with_child(severity);
    let mut extra =
        UiNode::new("console.filters.levels", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 26.0).fill_width()
        });
    for (id, key, level, color) in [
        (
            "success",
            "console.severity.success",
            LogLevel::Success,
            tokens.positive,
        ),
        (
            "verbose",
            "console.severity.verbose",
            LogLevel::Verbose,
            tokens.text_muted,
        ),
    ] {
        let active = console.filter_level == Some(level);
        let mut button = UiNode::new(format!("console.filter.{id}"), UiNodeKind::Button)
            .with_class(if active {
                "console-filter console-filter-active"
            } else {
                "console-filter"
            })
            .with_text_key(key)
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [40.0, 24.0],
                ..UiLayout::fit_content()
            })
            .with_text_style(UiTextStyle::button(color))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("console.filter.{id}"),
            ));
        if active {
            button = button.with_class("console-filter-active");
        }
        extra = extra.with_child(button);
    }
    root = root.with_child(extra);
    root = root.with_child(
        UiNode::toggle(
            "console.auto-scroll",
            UiToggle::new("console.auto-scroll", console.auto_scroll),
        )
        .with_class("console-toggle")
        .with_text_key("app.auto_scroll")
        .with_layout(UiLayout::fixed(0.0, 26.0).fill_width())
        .with_text_style(UiTextStyle::body(tokens.text_muted)),
    );
    root = root.with_child(
        UiNode::new("console.export-option", UiNodeKind::Button)
            .with_class("console-menu-action")
            .with_text_key("console.export")
            .with_layout(UiLayout::fixed(0.0, 26.0).fill_width())
            .with_text_style(UiTextStyle::button(tokens.text))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "console.export",
            )),
    );
    root = root.with_child(
        UiNode::new("console.clear-option", UiNodeKind::Button)
            .with_class("console-menu-action")
            .with_text_key("app.clear")
            .with_layout(UiLayout::fixed(0.0, 26.0).fill_width())
            .with_text_style(UiTextStyle::button(tokens.text))
            .focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, "console.clear")),
    );
    if let Some(source) = &console.source_filter {
        root = root.with_child(
            UiNode::new("console.source-clear", UiNodeKind::Button)
                .with_class("console-filter-active")
                .with_text_value(format!("× {source}"))
                .with_layout(UiLayout::fixed(0.0, 26.0).fill_width())
                .with_text_style(UiTextStyle::button(tokens.text))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "console.source.clear",
                )),
        );
    }
    let (info_count, success_count, warn_count, error_count, verbose_count) =
        console_level_counts(console);
    root = root.with_child(
        UiNode::new("console.filters.count", UiNodeKind::Label)
            .with_class("console-count")
            .with_text_value(format!(
                "{}/{} I:{} S:{} W:{} E:{} V:{}",
                console.visible_groups().len(),
                console.entries.len(),
                info_count,
                success_count,
                warn_count,
                error_count,
                verbose_count
            ))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fixed(0.0, 20.0).fill_width()),
    );
    let mut surface = UiSurface::new(
        "console.filters.menu",
        palette,
        root.with_style(palette.panel_style()),
    );
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn console_level_counts(console: &ConsolePanel) -> (usize, usize, usize, usize, usize) {
    let mut info = 0;
    let mut success = 0;
    let mut warn = 0;
    let mut error = 0;
    let mut verbose = 0;
    for entry in &console.entries {
        match entry.level {
            LogLevel::Info => info += 1,
            LogLevel::Success => success += 1,
            LogLevel::Warning => warn += 1,
            LogLevel::Error => error += 1,
            LogLevel::Verbose => verbose += 1,
        }
    }
    (info, success, warn, error, verbose)
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
            UiNode::new(format!("console.recent.{index}"), UiNodeKind::Button)
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
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(5.0, 0.0),
                    ..UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::FitContent)
                })
                .focusable()
                .with_tooltip_key("console.history_hint")
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("console.recent.use:{index}"),
                )),
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
    repeat: usize,
    expanded: bool,
    wrap: bool,
    show_timestamps: bool,
    terminal: bool,
    search_hit: bool,
    search_query: &str,
    source_filter: Option<&str>,
    columns: [f32; 2],
) -> UiNode {
    let tokens = palette.tokens();
    let (level_icon, color, level_key, severity_class) = match entry.level {
        LogLevel::Info => (
            UiIconId::Console,
            tokens.text,
            "console.severity.info",
            "console-severity-info",
        ),
        LogLevel::Success => (
            UiIconId::Success,
            tokens.positive,
            "console.severity.success",
            "console-severity-success",
        ),
        LogLevel::Warning => (
            UiIconId::Warning,
            tokens.warning,
            "console.severity.warning",
            "console-severity-warning",
        ),
        LogLevel::Error => (
            UiIconId::Error,
            tokens.danger,
            "console.severity.error",
            "console-severity-error",
        ),
        LogLevel::Verbose => (
            UiIconId::Console,
            tokens.text_muted,
            "console.severity.verbose",
            "console-severity-verbose",
        ),
    };
    let message_color = if terminal {
        [178, 255, 188, 255]
    } else {
        color
    };
    let timestamp_color = if terminal {
        [96, 168, 108, 255]
    } else {
        tokens.text_muted
    };
    let source_color = if terminal {
        [124, 214, 140, 255]
    } else {
        match entry.level {
            LogLevel::Info => tokens.accent_hot,
            LogLevel::Success => tokens.positive,
            LogLevel::Warning => tokens.warning,
            LogLevel::Error => tokens.danger,
            LogLevel::Verbose => tokens.text_muted,
        }
    };
    let severity_row_class = match entry.level {
        LogLevel::Error => Some("console-entry-error"),
        LogLevel::Warning => Some("console-entry-warn"),
        LogLevel::Success => Some("console-entry-success"),
        LogLevel::Verbose => Some("console-entry-verbose"),
        LogLevel::Info => None,
    };
    let mut row = UiNode::new(format!("console.entry.{id}"), UiNodeKind::Panel)
        .with_class("console-entry")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Stretch,
            gap: 1.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    if terminal {
        row = row.with_class("console-terminal-entry");
    }
    if search_hit {
        row = row.with_class("console-search-hit");
    }
    if let Some(extra) = severity_row_class {
        row = row.with_class(extra);
    }
    if entry.block.is_some() {
        row = row.with_class("console-block");
    } else if entry.sender.is_some() {
        row = row.with_class("console-entry-user");
    }

    let mut header = UiNode::new(format!("console.entry.{id}.header"), UiNodeKind::Toolbar)
        .with_class("console-entry-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: if wrap {
                UiAlign::Start
            } else {
                UiAlign::Center
            },
            gap: 4.0,
            padding: UiSpacing::xy(6.0, 2.0),
            align_self: Some(UiAlign::Stretch),
            overflow: UiOverflow::Clip,
            ..if wrap {
                UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            } else {
                UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
            }
        })
        .with_child(
            UiNode::new(format!("console.entry.{id}.level"), UiNodeKind::Label)
                .with_class(format!(
                    "console-level-icon console-severity-badge {severity_class}"
                ))
                .with_text_key(level_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: raf_ui::UiFontWeight::Regular,
                    color,
                    inherit_color: false,
                })
                .with_icon(
                    UiIcon::new(level_icon)
                        .with_size(UiIconSize::Custom(14))
                        .with_tint(color),
                )
                .with_layout(UiLayout::fixed(columns[0], 20.0)),
        );
    if show_timestamps {
        header = header.with_child(
            UiNode::new(format!("console.entry.{id}.timestamp"), UiNodeKind::Label)
                .with_text_value(entry.timestamp.clone())
                .with_layout(UiLayout::fixed(columns[1], 20.0).with_text_safe_area(false))
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: raf_ui::UiFontWeight::Regular,
                    color: timestamp_color,
                    inherit_color: false,
                }),
        );
    }
    let mut message_row = UiNode::new(format!("console.entry.{id}.message-row"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: if wrap {
                UiAlign::Start
            } else {
                UiAlign::Center
            },
            gap: 8.0,
            grow: 1.0,
            min_size: [60.0, 20.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(raf_ui::UiStyle::transparent());
    if let Some(sender) = &entry.sender {
        let mut source = UiNode::new(format!("console.entry.{id}.source"), UiNodeKind::Button)
            .with_class("console-source")
            .with_text_value(sender.clone())
            .with_text_style(UiTextStyle {
                size_px: 11.0,
                line_height_px: 14.0,
                ..UiTextStyle::body(source_color)
            })
            .with_layout(UiLayout::fit_content())
            .focusable()
            .with_tooltip_key("console.filter_source")
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("console.source.filter:{sender}"),
            ));
        if source_filter == Some(sender.as_str()) {
            source = source.with_class("console-source-active");
        }
        message_row = message_row.with_child(source);
    }
    message_row = message_row.with_child(message_content(
        palette,
        id,
        &entry.message,
        wrap,
        terminal,
        search_hit,
        search_query,
        message_color,
    ));
    if repeat > 1 {
        message_row = message_row.with_child(
            UiNode::new(format!("console.entry.{id}.repeat"), UiNodeKind::Label)
                .with_class("console-repeat")
                .with_text_value(format!("×{repeat}"))
                .with_layout(UiLayout::fit_content())
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
    header = header.with_child(message_row);
    header = header.with_child(copy_icon_button(
        format!("console.entry.{id}.copy"),
        "console.copy_entry",
        entry_clipboard_text(entry),
    ));
    if let Some(block) = &entry.block {
        header = header.with_child(copy_icon_button(
            format!("console.entry.{id}.copy-json"),
            "console.copy_json",
            block.json.clone(),
        ));
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
                    left: 16.0,
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
                    color: if terminal {
                        [120, 196, 132, 255]
                    } else {
                        tokens.text_muted
                    },
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
                        color: if terminal {
                            [110, 186, 124, 255]
                        } else {
                            tokens.text_muted
                        },
                        inherit_color: false,
                    }),
            );
        }
        row = row.with_child(block_body);
    }
    row
}

fn message_content(
    palette: StudioUiPalette,
    id: ConsoleEntryId,
    message: &str,
    wrap: bool,
    terminal: bool,
    search_hit: bool,
    search_query: &str,
    color: [u8; 4],
) -> UiNode {
    let tokens = palette.tokens();
    let query = search_query.trim();
    let match_color = if terminal {
        [255, 231, 146, 255]
    } else {
        tokens.accent_hot
    };
    if !query.is_empty() && !wrap {
        let mut segments = UiNode::new(format!("console.entry.{id}.message"), UiNodeKind::Panel)
            .with_class("console-message-segments")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 0.0,
                grow: 1.0,
                min_size: [60.0, 20.0],
                overflow: UiOverflow::Clip,
                ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
            });
        for (index, (fragment, matched)) in
            highlight_segments(message, query).into_iter().enumerate()
        {
            let mut label = UiNode::new(
                format!("console.entry.{id}.message.segment.{index}"),
                UiNodeKind::Label,
            )
            .with_text_value(fragment)
            .with_text_style(UiTextStyle {
                role: UiTextRole::Monospace,
                size_px: 12.0,
                line_height_px: 16.0,
                weight: if matched {
                    raf_ui::UiFontWeight::Bold
                } else {
                    raf_ui::UiFontWeight::Regular
                },
                color: if matched { match_color } else { color },
                inherit_color: false,
            })
            .with_layout(UiLayout::fit_content());
            if matched {
                label = label.with_class("console-search-match");
            }
            segments = segments.with_child(label);
        }
        return segments;
    }

    let mut label = UiNode::new(format!("console.entry.{id}.message"), UiNodeKind::Label)
        .with_text_value(message.to_string())
        .with_text_style(UiTextStyle {
            role: UiTextRole::Monospace,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: raf_ui::UiFontWeight::Regular,
            color: if search_hit && !query.is_empty() {
                match_color
            } else {
                color
            },
            inherit_color: false,
        })
        .with_text_overflow(if wrap {
            UiTextOverflow::Wrap
        } else {
            UiTextOverflow::Ellipsis
        })
        .with_layout(UiLayout {
            grow: 1.0,
            min_size: [60.0, 20.0],
            width_mode: UiSizeMode::Fill,
            ..if wrap {
                UiLayout::fit_content()
            } else {
                UiLayout::fixed(0.0, 20.0)
            }
        });
    if search_hit && !query.is_empty() {
        label = label.with_class("console-search-match");
    }
    label
}

fn highlight_segments(message: &str, query: &str) -> Vec<(String, bool)> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return vec![(message.to_string(), false)];
    }
    let haystack = message.to_ascii_lowercase();
    let mut segments = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = haystack[cursor..].find(&query) {
        let start = cursor + relative;
        let end = start + query.len();
        if start > cursor {
            segments.push((message[cursor..start].to_string(), false));
        }
        segments.push((message[start..end].to_string(), true));
        cursor = end;
    }
    if cursor < message.len() {
        segments.push((message[cursor..].to_string(), false));
    }
    if segments.is_empty() {
        segments.push((message.to_string(), false));
    }
    segments
}

fn entry_clipboard_text(entry: &LogEntry) -> String {
    format!(
        "{} [{}]{}{}",
        entry.timestamp,
        entry.level.as_str().to_ascii_uppercase(),
        entry
            .sender
            .as_deref()
            .map(|sender| format!(" {sender}: "))
            .unwrap_or_default(),
        entry.message
    )
}

fn copy_icon_button(id: impl Into<String>, tooltip_key: &str, text: String) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("console-copy")
        .with_layout(UiLayout::fixed(18.0, 18.0))
        .with_icon(
            UiIcon::new(UiIconId::Copy)
                .with_size(UiIconSize::Custom(12))
                .with_tint([196, 201, 209, 255]),
        )
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetClipboard { text },
        })
}

fn command_button(id: &str, text_key: &str, icon: UiIconId, command: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("console-command-button")
        .with_layout(UiLayout::fixed(60.0, 24.0).with_text_safe_area(false))
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
        assert_eq!(child(toolbar, "console.clear").layout.basis, [60.0, 24.0]);
        assert_eq!(
            child(toolbar, "console.auto-scroll").layout.basis,
            [92.0, 24.0]
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
        assert_eq!(message.layout.basis[1], 20.0);
        assert!(find_node(entry, "console.entry.1.source").is_some());
    }

    #[test]
    fn report_columns_align_with_rows_and_leave_full_height_for_message_glyphs() {
        let surface = build_console_surface(
            StudioUiPalette::IndustrialDark,
            &ConsolePanel::default(),
            false,
            &[],
            None,
        );
        let frame = surface.build_frame(1000, 220, [0, 0, 0, 255]);
        let rect = |id: &str| frame.layout_boxes.iter().find(|b| b.id == id).unwrap().rect;
        for (heading, cell) in [
            ("console.columns.0", "console.entry.0.level"),
            ("console.columns.1", "console.entry.0.timestamp"),
            ("console.columns.2", "console.entry.0.message"),
        ] {
            assert!(
                (rect(heading).x - rect(cell).x).abs() < 1.0,
                "{heading} vs {cell}"
            );
        }
        let message = surface.root.find("console.entry.0.message").unwrap();
        assert!(
            message.layout.basis[1] - message.layout.padding.top - message.layout.padding.bottom
                >= message.text_style.unwrap().line_height_px
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
            code: Some("console.runtime".to_string()),
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

        assert!(find_node(header, "console.entry.0.source").is_none());
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
            code: Some("console.runtime".to_string()),
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

    #[test]
    fn runtime_entries_render_severity_and_source_badges() {
        let mut console = ConsolePanel::default();
        console.log_structured(
            LogLevel::Error,
            "runtime",
            "The runtime did not become ready in time.",
            Some("console.runtime_timeout".to_string()),
            None,
        );
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);
        let log_scroll = child(&surface.root, "console.log-scroll");
        let entry = log_scroll.children.last().expect("runtime entry present");
        let level = child(entry, "console.entry.1.level");
        let source = child(entry, "console.entry.1.source");

        assert!(level
            .classes
            .iter()
            .any(|class| class == "console-severity-badge"));
        assert!(level
            .classes
            .iter()
            .any(|class| class == "console-severity-error"));
        assert_eq!(level.text_key.as_deref(), Some("console.severity.error"));
        assert!(source.classes.iter().any(|class| class == "console-source"));
    }

    #[test]
    fn repeated_entries_render_one_row_with_a_repeat_count() {
        let mut console = ConsolePanel::default();
        console.log(LogLevel::Info, "Lighting unchanged.");
        console.log(LogLevel::Info, "Lighting unchanged.");
        console.log(LogLevel::Info, "Lighting unchanged.");
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);
        let repeat = child(&surface.root, "console.entry.1.repeat");

        assert_eq!(repeat.text_value.as_deref(), Some("×3"));
    }

    #[test]
    fn search_highlights_matching_rows_and_segments() {
        let mut console = ConsolePanel::default();
        console.log(LogLevel::Error, "boom goes the dynamite");
        console.set_search_query("boom");
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);
        let entry = child(&surface.root, "console.entry.1");

        assert!(entry
            .classes
            .iter()
            .any(|class| class == "console-search-hit"));
        let matched = child(entry, "console.entry.1.message.segment.0");
        assert!(matched
            .classes
            .iter()
            .any(|class| class == "console-search-match"));
        assert_eq!(matched.text_value.as_deref(), Some("boom"));
    }

    #[test]
    fn disabled_commands_offer_an_enable_action_inside_the_console() {
        let surface = build_console_surface(
            StudioUiPalette::IndustrialDark,
            &ConsolePanel::default(),
            false,
            &[],
            None,
        );
        let enable = child(&surface.root, "console.enable-commands");

        assert!(enable.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "console.commands.enable"
        )));
    }

    #[test]
    fn paused_autoscroll_exposes_a_resume_pill() {
        let mut console = ConsolePanel::default();
        console.set_auto_scroll(false);
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, true, &[], None);
        let resume = child(&surface.root, "console.resume-autoscroll");

        assert!(resume.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name } if name == "console.auto-scroll.resume"
        )));
    }

    #[test]
    fn terminal_palette_marks_the_log_and_rows() {
        let mut console = ConsolePanel::default();
        console.set_terminal_palette(true);
        let surface =
            build_console_surface(StudioUiPalette::IndustrialDark, &console, false, &[], None);

        assert!(surface
            .root
            .classes
            .iter()
            .any(|class| class == "console-terminal"));
        let entry = child(&surface.root, "console.entry.0");
        assert!(entry
            .classes
            .iter()
            .any(|class| class == "console-terminal-entry"));
    }

    #[test]
    fn column_resizers_expose_drag_and_reset_commands() {
        let surface = build_console_surface(
            StudioUiPalette::IndustrialDark,
            &ConsolePanel::default(),
            false,
            &[],
            None,
        );
        let resizer = child(&surface.root, "console.columns.resizer.0");

        assert!(resizer.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::DragMove
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name } if name == "console.column.resize:0"
                )
        }));
        assert!(resizer.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::DoubleClick
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name } if name == "console.column.reset"
                )
        }));
    }
}
