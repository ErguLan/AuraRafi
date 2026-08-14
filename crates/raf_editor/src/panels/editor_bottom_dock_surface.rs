//! Retained RafUI documents for the beta editor downbar.
//!
//! These builders are presentation-only. They accept display models, emit
//! semantic actions, and never mutate the scene, project, or command state.

use std::path::Path;

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiIconSize,
    UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing,
    UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface,
    UiTextRole, UiTextStyle, UiToggle,
};

use crate::console::{ConsoleEntryId, ConsolePanel, LogEntry, LogLevel};
use raf_ui::{DockTab, DockTabGroup};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTreeEntry {
    pub label: String,
    pub icon: UiIconId,
    pub depth: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetFilter {
    All,
    Images,
    Models,
    Audio,
    Scripts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetsIntent {
    QueryChanged(String),
    FilterChanged(AssetFilter),
    OpenFolder,
    Refresh,
    OpenAsset(String),
    CreateScript { language: String, name: String },
}

/// Non-persistent visual state shown while a bottom tab is being dragged.
/// The host owns the gesture; the surface only renders the proposed slot.
#[derive(Debug, Clone, PartialEq)]
pub struct BottomTabDragPreview {
    pub source_group_id: String,
    pub source_tab_id: String,
    pub moving_tab: DockTab,
    pub target_group_id: String,
    pub insertion_index: usize,
    pub split_before: Option<bool>,
    pub pulse: f32,
    pub transition: f32,
    pub width: f32,
}

pub fn build_tab_strip_surface(
    palette: StudioUiPalette,
    group: &DockTabGroup,
    collapsed: bool,
    preview: Option<&BottomTabDragPreview>,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("bottom.tabs", UiNodeKind::Toolbar)
        .with_class("bottom-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            padding: UiSpacing::xy(5.0, 2.0),
            overflow: UiOverflow::ScrollX,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        });

    let same_target = preview.is_some_and(|preview| {
        preview.target_group_id == group.id && preview.split_before.is_none()
    });
    let mut inserted_preview = false;
    for (index, tab) in group.tabs.iter().enumerate() {
        if same_target
            && !inserted_preview
            && index
                >= preview
                    .expect("same target implies preview")
                    .insertion_index
        {
            root = root.with_child(drag_preview_tab(
                preview.expect("same target implies preview"),
            ));
            inserted_preview = true;
        }
        // Keep the source node in the retained document while dragging. RafUI
        // captures the pointer on that node, so removing it here would make a
        // later pointer release unable to dispatch DragEnd.
        let active = group.active_tab == tab.id;
        let mut button = tab_button(group, tab, active);
        if preview.is_some_and(|preview| {
            preview.source_group_id == group.id && preview.source_tab_id == tab.id
        }) {
            button = button.with_class("bottom-tab-dragging");
        }
        root = root.with_child(button);
    }
    if same_target && !inserted_preview {
        root = root.with_child(drag_preview_tab(
            preview.expect("same target implies preview"),
        ));
    } else if preview.is_some_and(|preview| {
        preview.target_group_id == group.id && preview.split_before.is_some()
    }) {
        root = root.with_child(drag_preview_tab(
            preview.expect("split target implies preview"),
        ));
    }

    root = root.with_child(
        UiNode::new("bottom.collapse", UiNodeKind::Button)
            .with_class("bottom-collapse-button")
            .with_layout(UiLayout {
                grow: 1.0,
                justify_content: UiJustify::End,
                align_items: UiAlign::Center,
                ..UiLayout::fit_content()
            })
            .with_icon(UiIcon::new(if collapsed {
                UiIconId::ChevronRight
            } else {
                UiIconId::ChevronDown
            }))
            .with_tooltip_key("editor.downbar.toggle")
            .with_accessibility_label_key("editor.downbar.toggle")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "bottom.toggle-collapsed",
            )),
    );

    let mut surface = UiSurface::new(format!("editor.bottom.tabs.{}", group.id), palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn drag_preview_tab(preview: &BottomTabDragPreview) -> UiNode {
    let pulse = preview.pulse.clamp(0.0, 1.0);
    let transition = preview.transition.clamp(0.0, 1.0);
    let fill_alpha = (88.0 + pulse * 72.0) as u8;
    let border_alpha = (180.0 + pulse * 75.0) as u8;
    UiNode::new("bottom.tab.drag-preview", UiNodeKind::Panel)
        .with_class("bottom-tab-drag-preview")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(preview.width.max(0.0) * transition, 28.0)
        })
        .with_style(UiStyle {
            fill: [232, 133, 28, fill_alpha],
            border: [199, 92, 174, border_alpha],
            text: [255, 237, 205, 255],
            border_width: 1.0,
            radius: 2.0,
            opacity: 0.25 + transition * 0.75,
        })
        .with_icon(UiIcon::new(preview.moving_tab.icon).with_size(UiIconSize::Small))
        .with_text_key(preview.moving_tab.title_key.clone())
        .with_text_style(UiTextStyle::button([255, 237, 205, 255]))
}

pub fn build_drop_preview_surface(
    palette: StudioUiPalette,
    pulse: f32,
    split_before: bool,
) -> UiSurface {
    let alpha = (44.0 + pulse.clamp(0.0, 1.0) * 36.0) as u8;
    let border_alpha = (170.0 + pulse.clamp(0.0, 1.0) * 85.0) as u8;
    let tokens = palette.tokens();
    let direction_key = if split_before {
        "editor.downbar.split_left"
    } else {
        "editor.downbar.split_right"
    };
    let root = UiNode::new("bottom.drop-preview", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 8.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle {
            fill: [232, 133, 28, alpha],
            border: [199, 92, 174, border_alpha],
            text: tokens.text,
            border_width: 2.0,
            radius: 5.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new("bottom.drop-preview.title", UiNodeKind::Label)
                .with_text_key("editor.downbar.split_preview")
                .with_text_style(UiTextStyle::panel_title([255, 237, 205, 255]))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("bottom.drop-preview.direction", UiNodeKind::Label)
                .with_text_key(direction_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fit_content()),
        );
    UiSurface::new("editor.bottom.drop-preview", palette, root)
}

pub fn build_tab_context_menu_surface(
    palette: StudioUiPalette,
    group_id: &str,
    tab_id: &str,
    can_split: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("bottom.tab-context", UiNodeKind::Menu)
        .with_class("bottom-tab-context")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::same(6.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_accessibility_label_key("app.more_menu")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "bottom.context.close",
        ))
        .with_child(
            tab_context_button(
                "bottom.tab-context.split-left",
                "editor.downbar.split_left",
                UiIconId::ChevronLeft,
                format!("bottom.context.split-left.{group_id}.{tab_id}"),
            )
            .disabled(!can_split),
        )
        .with_child(
            tab_context_button(
                "bottom.tab-context.split-right",
                "editor.downbar.split_right",
                UiIconId::ChevronRight,
                format!("bottom.context.split-right.{group_id}.{tab_id}"),
            )
            .disabled(!can_split),
        )
        .with_child(tab_context_button(
            "bottom.tab-context.reset",
            "app.reset_panels",
            UiIconId::Rotate,
            "bottom.context.reset",
        ));
    if !can_split {
        root = root.with_child(
            UiNode::new("bottom.tab-context.limit", UiNodeKind::Label)
                .with_text_key("editor.downbar.group_limit")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(8.0, 2.0),
                    ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
                }),
        );
    }
    let mut surface = UiSurface::new("editor-bottom-tab-context", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn tab_context_button(
    id: &str,
    label_key: &str,
    icon: UiIconId,
    command: impl Into<String>,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("bottom-tab-context-action")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]).inherit_theme_color())
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn tab_button(group: &DockTabGroup, tab: &DockTab, active: bool) -> UiNode {
    let class = if active {
        "bottom-tab bottom-tab-active"
    } else {
        "bottom-tab"
    };
    let width = match tab.id.as_str() {
        "project-settings" => 104.0,
        "nodes" => 92.0,
        "console" => 96.0,
        "assets" => 86.0,
        "drc" => 86.0,
        "simulation" => 112.0,
        _ => 96.0,
    };
    UiNode::new(
        format!("bottom.tab.{}.{}", group.id, tab.id),
        UiNodeKind::Button,
    )
    .with_class(class)
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 5.0,
        padding: UiSpacing::xy(8.0, 0.0),
        ..UiLayout::fixed(width, 28.0).with_text_safe_area(true)
    })
    .with_icon(UiIcon::new(tab.icon).with_size(UiIconSize::Small))
    .with_text_key(tab.title_key.clone())
    .with_text_style(UiTextStyle::button([237, 239, 242, 255]).inherit_theme_color())
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("bottom.tab.{}.{}", group.id, tab.id),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragStart,
        format!("bottom.drag.start.{}.{}", group.id, tab.id),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragMove,
        format!("bottom.drag.move.{}.{}", group.id, tab.id),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragEnd,
        format!("bottom.drag.end.{}.{}", group.id, tab.id),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::ContextMenu,
        format!("bottom.context.open.{}.{}", group.id, tab.id),
    ))
}

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
            gap: 3.0,
            padding: UiSpacing::xy(8.0, 5.0),
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
        log_scroll = log_scroll.with_child(console_entry(
            palette,
            id,
            entry,
            expanded_blocks.contains(&id),
        ));
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
                padding: UiSpacing::xy(8.0, 4.0),
                ..UiLayout::fixed(0.0, 34.0)
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
                    min_size: [120.0, 26.0],
                    ..UiLayout::fixed(0.0, 26.0)
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
                    .with_layout(UiLayout::fixed(58.0, 26.0))
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
        .with_child(console_toolbar(palette, console))
        .with_child(log_scroll);
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

const CONSOLE_ROW_ESTIMATE: f32 = 32.0;

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

fn console_toolbar(palette: StudioUiPalette, console: &ConsolePanel) -> UiNode {
    let tokens = palette.tokens();
    let mut toolbar = UiNode::new("console.toolbar", UiNodeKind::Toolbar)
        .with_class("console-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            padding: UiSpacing::xy(6.0, 3.0),
            overflow: UiOverflow::ScrollX,
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
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
        .with_layout(UiLayout::fixed(132.0, 27.0).with_text_safe_area(true))
        .with_text_style(UiTextStyle::body(tokens.text_muted)),
    );
    toolbar = toolbar.with_child(
        UiNode::new("console.separator", UiNodeKind::Separator)
            .with_class("bottom-separator")
            .with_layout(UiLayout::fixed(1.0, 18.0)),
    );

    for (id, key, level, width) in [
        ("all", "app.all", None, 48.0),
        ("info", "app.info", Some(LogLevel::Info), 52.0),
        ("warning", "app.warn", Some(LogLevel::Warning), 60.0),
        ("error", "app.error", Some(LogLevel::Error), 58.0),
    ] {
        let active = console.filter_level == level;
        let class = if active {
            "console-filter console-filter-active"
        } else {
            "console-filter"
        };
        toolbar = toolbar.with_child(
            UiNode::new(format!("console.filter.{id}"), UiNodeKind::Button)
                .with_class(class)
                .with_layout(UiLayout::fixed(width, 27.0).with_text_safe_area(true))
                .with_text_key(key)
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("console.filter.{id}"),
                )),
        );
    }
    toolbar
}

fn console_entry(
    palette: StudioUiPalette,
    id: ConsoleEntryId,
    entry: &LogEntry,
    expanded: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let color = match entry.level {
        LogLevel::Info => tokens.text,
        LogLevel::Warning => tokens.warning,
        LogLevel::Error => tokens.danger,
    };
    let mut row = UiNode::new(format!("console.entry.{id}"), UiNodeKind::Panel)
        .with_class(if entry.block.is_some() {
            "console-entry console-block"
        } else {
            "console-entry"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Stretch,
            gap: 1.0,
            ..UiLayout::fit_content()
        });

    let mut header = UiNode::new(format!("console.entry.{id}.header"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(6.0, 4.0),
            align_self: Some(UiAlign::Stretch),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("console.entry.{id}.timestamp"), UiNodeKind::Label)
                .with_text_value(entry.timestamp.clone())
                .with_layout(UiLayout::fixed(64.0, 20.0).with_text_safe_area(true))
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
                .with_layout(UiLayout::fixed(66.0, 20.0).with_text_safe_area(true))
                .with_text_style(UiTextStyle::button(tokens.accent_hot)),
        );
    }
    header = header.with_child(
        UiNode::new(format!("console.entry.{id}.message"), UiNodeKind::Label)
            .with_text_value(entry.message.clone())
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [80.0, 20.0],
                overflow: UiOverflow::Clip,
                ..UiLayout::fixed(0.0, 20.0)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_text_safe_area(true)
            })
            .with_text_style(UiTextStyle::body(color)),
    );
    row = row.with_child(header);

    if let Some(block) = &entry.block {
        let mut block_body = UiNode::new(format!("console.entry.{id}.block"), UiNodeKind::Panel)
            .with_class("console-block-body")
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Stretch,
                gap: 1.0,
                padding: UiSpacing {
                    left: 68.0,
                    right: 4.0,
                    top: 0.0,
                    bottom: 2.0,
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
                    ..UiLayout::fit_content()
                })
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
        block_body = block_body.with_child(
            UiNode::new(
                format!("console.entry.{id}.json-toggle"),
                UiNodeKind::Button,
            )
            .with_class("console-json-button")
            .with_text_key("console.json")
            .with_layout(UiLayout::fixed(52.0, 20.0))
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("console.block.toggle.{id}"),
            )),
        );
        if expanded {
            block_body = block_body.with_child(
                UiNode::new(format!("console.entry.{id}.json"), UiNodeKind::Label)
                    .with_text_value(block.json.clone())
                    .with_layout(UiLayout {
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

pub fn build_assets_surface(
    palette: StudioUiPalette,
    asset_rows: &[String],
    query: &str,
    filter: AssetFilter,
    visible_range: Option<(usize, usize)>,
    script_menu_open: bool,
    script_name: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    let filtered_rows = asset_rows
        .iter()
        .enumerate()
        .filter(|(_, row)| asset_matches(row.as_str(), query, filter))
        .collect::<Vec<_>>();
    let total_rows = filtered_rows.len();
    let (start, end) = visible_range
        .map(|(start, end)| {
            let start = start.min(total_rows);
            (start, end.min(total_rows).max(start))
        })
        .unwrap_or((0, total_rows));

    let mut list = UiNode::scroll_view("assets.list", UiScrollAxis::Vertical)
        .with_class("assets-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: ASSET_ROW_GAP,
            padding: UiSpacing::xy(8.0, 8.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    if start > 0 {
        list = list.with_child(spacer(
            "assets.top-spacer",
            asset_virtual_spacer_height(start),
        ));
    }
    for (source_index, row) in filtered_rows.iter().skip(start).take(end - start) {
        list = list.with_child(asset_row(palette, *source_index, row.as_str()));
    }
    if end < total_rows {
        list = list.with_child(spacer(
            "assets.bottom-spacer",
            asset_virtual_spacer_height(total_rows - end),
        ));
    }
    if filtered_rows.is_empty() {
        let mut empty = UiNode::new("assets.empty", UiNodeKind::Panel)
            .with_class("assets-empty")
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                grow: 1.0,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                gap: 4.0,
                min_size: [0.0, 72.0],
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_child(
                UiNode::new("assets.empty.title", UiNodeKind::Label)
                    .with_text_key(if asset_rows.is_empty() {
                        "app.no_assets"
                    } else {
                        "app.search_no_results"
                    })
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            );
        if asset_rows.is_empty() {
            empty = empty.with_child(
                UiNode::new("assets.empty.hint", UiNodeKind::Label)
                    .with_text_key("app.drag_drop_hint")
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            );
        }
        list = list.with_child(empty);
    }

    let mut root = UiNode::new("assets.root", UiNodeKind::Panel)
        .with_class("bottom-panel")
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_style(palette.panel_style())
        .with_child(
            UiNode::new("assets.toolbar", UiNodeKind::Toolbar)
                .with_class("assets-toolbar")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 5.0,
                    padding: UiSpacing::xy(8.0, 3.0),
                    ..UiLayout::fixed(0.0, 34.0)
                })
                .with_child(
                    UiNode::text_input(
                        "assets.search",
                        raf_ui::UiTextInput {
                            value_key: "assets.search".to_string(),
                            placeholder_key: Some("app.search".to_string()),
                            max_length: 256,
                            multiline: false,
                            password: false,
                            submit_command: None,
                        },
                    )
                    .with_class("asset-search")
                    .with_icon(UiIcon::new(UiIconId::Search).with_size(UiIconSize::Small))
                    .with_accessibility_label_key("app.search")
                    .with_layout(UiLayout {
                        grow: 1.0,
                        min_size: [80.0, 28.0],
                        ..UiLayout::fixed(0.0, 28.0)
                    }),
                )
                .with_child(
                    UiNode::new("assets.open-folder", UiNodeKind::Button)
                        .with_class("asset-action")
                        .with_icon(UiIcon::new(UiIconId::Folder).with_size(UiIconSize::Small))
                        .with_tooltip_key("app.open_folder")
                        .with_accessibility_label_key("app.open_folder")
                        .with_layout(UiLayout::fixed(28.0, 28.0))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "assets.open-folder",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("enter".to_string()),
                            "assets.open-folder",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("space".to_string()),
                            "assets.open-folder",
                        )),
                )
                .with_child(
                    UiNode::new("assets.refresh", UiNodeKind::Button)
                        .with_class("asset-action")
                        .with_icon(UiIcon::new(UiIconId::Undo).with_size(UiIconSize::Small))
                        .with_tooltip_key("app.refresh_assets")
                        .with_accessibility_label_key("app.refresh_assets")
                        .with_layout(UiLayout::fixed(28.0, 28.0))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "assets.refresh",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("enter".to_string()),
                            "assets.refresh",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("space".to_string()),
                            "assets.refresh",
                        )),
                )
                .with_child(
                    UiNode::new("assets.create-script", UiNodeKind::Button)
                        .with_class("asset-action asset-create-script")
                        .with_icon(UiIcon::new(UiIconId::Node).with_size(UiIconSize::Small))
                        .with_text_key("app.create_script")
                        .with_accessibility_label_key("app.create_script")
                        .with_layout(UiLayout::fit_content().with_text_safe_area(true))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "assets.create-script",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("enter".to_string()),
                            "assets.create-script",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("space".to_string()),
                            "assets.create-script",
                        )),
                ),
        );

    root = root.with_child(
        UiNode::new("assets.filters", UiNodeKind::Toolbar)
            .with_class("assets-filters")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 4.0,
                padding: UiSpacing::xy(8.0, 1.0),
                ..UiLayout::fixed(0.0, 29.0)
            })
            .with_child(
                UiNode::new("assets.filters.icon", UiNodeKind::Label)
                    .with_icon(UiIcon::new(UiIconId::Filter).with_size(UiIconSize::Small))
                    .with_layout(UiLayout::fixed(18.0, 24.0)),
            )
            .with_child(asset_filter_button(
                "all",
                "app.all",
                filter == AssetFilter::All,
            ))
            .with_child(asset_filter_button(
                "images",
                "app.images",
                filter == AssetFilter::Images,
            ))
            .with_child(asset_filter_button(
                "models",
                "app.models",
                filter == AssetFilter::Models,
            ))
            .with_child(asset_filter_button(
                "audio",
                "app.audio",
                filter == AssetFilter::Audio,
            ))
            .with_child(asset_filter_button(
                "scripts",
                "app.scripts_filter",
                filter == AssetFilter::Scripts,
            )),
    );

    if script_menu_open {
        root = root.with_child(script_template_popover(palette, script_name));
    }
    root = root.with_child(list);

    let mut surface = UiSurface::new("editor.bottom.assets", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

const ASSET_ROW_HEIGHT: f32 = 32.0;
const ASSET_ROW_GAP: f32 = 3.0;
const ASSET_ROW_ESTIMATE: f32 = ASSET_ROW_HEIGHT + ASSET_ROW_GAP;

fn asset_virtual_spacer_height(row_count: usize) -> f32 {
    if row_count == 0 {
        0.0
    } else {
        row_count as f32 * ASSET_ROW_ESTIMATE - ASSET_ROW_GAP
    }
}

fn asset_row(palette: StudioUiPalette, index: usize, row: &str) -> UiNode {
    let tokens = palette.tokens();
    let command = format!("assets.open:{row}");
    UiNode::new(format!("assets.row.{index}"), UiNodeKind::Panel)
        .with_class("asset-row")
        .with_text_value(row)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(8.0, 3.0),
            align_self: Some(UiAlign::Stretch),
            ..UiLayout::fixed(0.0, ASSET_ROW_HEIGHT)
        })
        .with_icon(UiIcon::new(asset_icon(row)).with_size(UiIconSize::Small))
        .with_text_style(UiTextStyle::body(tokens.text))
        .interactive()
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::DoubleClick,
            command.clone(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command.clone(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command,
        ))
}

fn script_template_popover(palette: StudioUiPalette, _script_name: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("assets.script-popover", UiNodeKind::Menu)
        .with_class("asset-script-popover")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 6.0),
            align_self: Some(UiAlign::Stretch),
            ..UiLayout::fit_content()
                .with_width_mode(UiSizeMode::Fill)
                .with_z_index(20)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.accent,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 0.98,
        })
        .with_accessibility_label_key("app.create_script")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.script.cancel",
        ))
        .with_child(
            UiNode::new("assets.script-popover.title", UiNodeKind::Label)
                .with_text_key("app.create_script")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::text_input(
                "assets.script-name",
                raf_ui::UiTextInput {
                    value_key: "assets.script-name".to_string(),
                    placeholder_key: Some("app.script_name".to_string()),
                    max_length: 96,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("asset-script-name")
            .with_accessibility_label_key("app.script_name")
            .with_layout(UiLayout {
                align_self: Some(UiAlign::Stretch),
                ..UiLayout::fixed(0.0, 28.0)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_text_safe_area(true)
            })
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "assets.script.cancel",
            )),
        )
        .with_child(
            UiNode::new("assets.script-popover.hint", UiNodeKind::Label)
                .with_text_key("app.script_language_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("assets.script-popover.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 5.0,
                    min_size: [0.0, 30.0],
                    ..UiLayout::fit_content()
                })
                .with_child(script_template_button(
                    "rust",
                    "app.script_language_rust",
                    "assets.script.create:rust",
                ))
                .with_child(script_template_button(
                    "rhai",
                    "app.script_rhai",
                    "assets.script.create:rhai",
                ))
                .with_child(script_template_button(
                    "cpp",
                    "app.script_cpp",
                    "assets.script.create:cpp",
                ))
                .with_child(
                    UiNode::new("assets.script.cancel", UiNodeKind::Button)
                        .with_class("asset-script-cancel")
                        .with_text_key("app.cancel")
                        .with_layout(UiLayout::fit_content().with_text_safe_area(true))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "assets.script.cancel",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("enter".to_string()),
                            "assets.script.cancel",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("space".to_string()),
                            "assets.script.cancel",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("escape".to_string()),
                            "assets.script.cancel",
                        )),
                ),
        )
}

fn script_template_button(id: &str, label_key: &str, command: &str) -> UiNode {
    UiNode::new(format!("assets.script.{id}"), UiNodeKind::Button)
        .with_class("asset-script-template")
        .with_text_key(label_key)
        .with_layout(UiLayout::fit_content().with_text_safe_area(true))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.script.cancel",
        ))
}

pub fn asset_visible_range(
    asset_rows: &[String],
    query: &str,
    filter: AssetFilter,
    scroll_offset: f32,
    viewport_height: f32,
) -> (usize, usize) {
    let count = asset_rows
        .iter()
        .filter(|row| asset_matches(row, query, filter))
        .count();
    let range = raf_ui::UiVirtualRange::for_vertical_list(
        count,
        scroll_offset,
        viewport_height,
        ASSET_ROW_ESTIMATE,
        4,
    );
    (range.start, range.end)
}

pub fn build_project_surface(
    palette: StudioUiPalette,
    project_name: &str,
    session_name: &str,
    entries: &[ProjectTreeEntry],
) -> UiSurface {
    let tokens = palette.tokens();
    let mut tree = UiNode::scroll_view("project.tree", UiScrollAxis::Vertical)
        .with_class("project-tree")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 2.0,
            padding: UiSpacing::xy(8.0, 6.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new("project.root", UiNodeKind::Label)
                .with_class("project-root-row")
                .with_text_value(project_name.to_string())
                .with_icon(UiIcon::new(UiIconId::Project).with_size(UiIconSize::Small))
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(6.0, 3.0),
                    align_self: Some(UiAlign::Stretch),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        );

    if !session_name.trim().is_empty() {
        tree = tree.with_child(
            UiNode::new("project.session", UiNodeKind::Label)
                .with_class("project-session-row")
                .with_text_value(session_name.to_string())
                .with_icon(UiIcon::new(UiIconId::Scene).with_size(UiIconSize::Small))
                .with_layout(UiLayout {
                    padding: UiSpacing {
                        left: 20.0,
                        right: 6.0,
                        top: 3.0,
                        bottom: 3.0,
                    },
                    align_self: Some(UiAlign::Stretch),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }

    for (index, entry) in entries.iter().enumerate() {
        tree = tree.with_child(
            UiNode::new(format!("project.entry.{index}"), UiNodeKind::Label)
                .with_class("project-tree-row")
                .with_text_value(entry.label.clone())
                .with_icon(UiIcon::new(entry.icon).with_size(UiIconSize::Small))
                .with_layout(UiLayout {
                    padding: UiSpacing {
                        left: 6.0 + f32::from(entry.depth) * 14.0,
                        right: 6.0,
                        top: 2.0,
                        bottom: 2.0,
                    },
                    align_self: Some(UiAlign::Stretch),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }

    let root = UiNode::new("project.root-surface", UiNodeKind::Panel)
        .with_class("bottom-panel")
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_style(palette.panel_style())
        .with_child(
            UiNode::new("project.toolbar", UiNodeKind::Toolbar)
                .with_class("project-toolbar")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(8.0, 3.0),
                    ..UiLayout::fixed(0.0, 31.0)
                })
                .with_text_key("app.studio_project")
                .with_text_style(UiTextStyle::panel_title(tokens.text_muted)),
        )
        .with_child(tree);

    let mut surface = UiSurface::new("editor.bottom.project", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn asset_filter_button(id: &str, label_key: &str, active: bool) -> UiNode {
    let command = format!("assets.filter.{id}");
    UiNode::new(format!("assets.filter.{id}"), UiNodeKind::Button)
        .with_class(if active {
            "asset-filter asset-filter-active"
        } else {
            "asset-filter"
        })
        .with_text_key(label_key)
        .with_layout(UiLayout::fit_content().with_text_safe_area(true))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command.clone()))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command.clone(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command,
        ))
}

fn asset_matches(row: &str, query: &str, filter: AssetFilter) -> bool {
    let lower = row.to_lowercase();
    let query = query.trim().to_lowercase();
    let query_matches = query.is_empty() || lower.contains(&query);
    if !query_matches {
        return false;
    }
    let extension = Path::new(row)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    match filter {
        AssetFilter::All => true,
        AssetFilter::Images => matches!(
            extension.as_deref(),
            Some("png")
                | Some("jpg")
                | Some("jpeg")
                | Some("bmp")
                | Some("tga")
                | Some("webp")
                | Some("svg")
        ),
        AssetFilter::Models => matches!(
            extension.as_deref(),
            Some("obj") | Some("gltf") | Some("glb") | Some("fbx") | Some("stl")
        ),
        AssetFilter::Audio => matches!(
            extension.as_deref(),
            Some("wav") | Some("mp3") | Some("ogg") | Some("flac")
        ),
        AssetFilter::Scripts => matches!(
            extension.as_deref(),
            Some("rhai")
                | Some("rs")
                | Some("lua")
                | Some("py")
                | Some("cpp")
                | Some("cc")
                | Some("cxx")
                | Some("js")
                | Some("ts")
        ),
    }
}

pub fn build_status_surface(palette: StudioUiPalette, status: &[String]) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("editor.status.root", UiNodeKind::Toolbar)
        .with_class("editor-status-bar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 0.0,
            padding: UiSpacing::xy(10.0, 0.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(UiStyle {
            fill: [13, 24, 34, 255],
            border: tokens.border,
            text: tokens.text_muted,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        });

    for (index, item) in status.iter().enumerate() {
        if index > 0 {
            root = root.with_child(
                UiNode::new(
                    format!("editor.status.separator.{index}"),
                    UiNodeKind::Separator,
                )
                .with_class("status-separator")
                .with_layout(UiLayout::fixed(1.0, 14.0)),
            );
        }
        root = root.with_child(
            UiNode::new(format!("editor.status.item.{index}"), UiNodeKind::Label)
                .with_text_value(item.clone())
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    root = root.with_child(
        UiNode::new("editor.status.menu", UiNodeKind::Button)
            .with_class("status-menu")
            .with_layout(UiLayout {
                grow: 1.0,
                justify_content: UiJustify::End,
                ..UiLayout::fit_content()
            })
            .with_icon(UiIcon::new(UiIconId::Menu).with_size(UiIconSize::Small))
            .with_tooltip_key("editor.status.menu")
            .focusable(),
    );

    let mut surface = UiSurface::new("editor.status", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn command_button(id: &str, text_key: &str, icon: UiIconId, command: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("console-command-button")
        .with_layout(UiLayout::fixed(82.0, 27.0).with_text_safe_area(true))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::button([196, 201, 209, 255]))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn asset_icon(name: &str) -> UiIconId {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("ron") | Some("toml") | Some("json") => UiIconId::Project,
        Some("scene") | Some("world") => UiIconId::Scene,
        Some("rhai") | Some("rs") | Some("lua") | Some("py") | Some("cpp") | Some("cc")
        | Some("cxx") | Some("js") | Some("ts") => UiIconId::Node,
        Some("pcb") => UiIconId::Pcb,
        Some("sch") | Some("kicad_sch") => UiIconId::Schematic,
        Some("png") | Some("jpg") | Some("jpeg") | Some("bmp") | Some("tga") | Some("webp")
        | Some("svg") | Some("obj") | Some("gltf") | Some("glb") | Some("fbx") | Some("stl")
        | Some("wav") | Some("mp3") | Some("ogg") | Some("flac") => UiIconId::Assets,
        _ => UiIconId::Assets,
    }
}

fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}

fn bottom_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-tab".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-tab-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-tab-dragging".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    opacity: Some(0.55),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-tab".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-tab-context".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-tab-context-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-tab-context-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("console-send".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([255, 255, 255, 255]),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("console-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("console-command-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("console-command-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("console-filter".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("console-block".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("console-filter-active".to_string()),
                UiStylePatch {
                    fill: Some([116, 67, 24, 54]),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-search".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-search".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-search".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(0.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-filter".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-filter-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-filter".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-filter".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-action".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-create-script".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-create-script".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-create-script".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-empty".to_string()),
                UiStylePatch {
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-popover".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-name".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-name".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-template".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-template".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-template".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-cancel".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-cancel".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("asset-script-cancel".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("project-root-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("project-tree-row".to_string()),
                UiStylePatch {
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("project-tree-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("project-session-row".to_string()),
                UiStylePatch {
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("editor-status-bar".to_string()),
                UiStylePatch {
                    fill: Some([13, 24, 34, 255]),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(child(toolbar, "console.clear").layout.basis, [82.0, 27.0]);
        assert_eq!(
            child(toolbar, "console.auto-scroll").layout.basis,
            [132.0, 27.0]
        );
    }

    #[test]
    fn bottom_tabs_expose_a_retained_context_menu_contract() {
        let group = DockTabGroup::new(
            "workspace",
            vec![DockTab::new(
                "console",
                "app.studio_console",
                UiIconId::Console,
            )],
        );
        let tabs = build_tab_strip_surface(StudioUiPalette::IndustrialDark, &group, false, None);
        let console_tab = child(&tabs.root, "bottom.tab.workspace.console");
        assert!(console_tab.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::ContextMenu
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name }
                        if name == "bottom.context.open.workspace.console"
                )
        }));

        let menu = build_tab_context_menu_surface(
            StudioUiPalette::IndustrialDark,
            "workspace",
            "console",
            true,
        );
        for (node_id, expected_command) in [
            (
                "bottom.tab-context.split-left",
                "bottom.context.split-left.workspace.console",
            ),
            (
                "bottom.tab-context.split-right",
                "bottom.context.split-right.workspace.console",
            ),
            ("bottom.tab-context.reset", "bottom.context.reset"),
        ] {
            let action = child(&menu.root, node_id);
            assert!(action.event_handlers.iter().any(|binding| {
                matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name } if name == expected_command
                )
            }));
        }
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
        assert_eq!(message.layout.min_size[0], 80.0);
        assert_eq!(message.layout.basis[1], 20.0);
        assert_eq!(
            child(entry, "console.entry.1.sender").layout.basis,
            [66.0, 20.0]
        );
    }

    #[test]
    fn project_hierarchy_uses_padding_not_leading_spaces() {
        let surface = build_project_surface(
            StudioUiPalette::IndustrialDark,
            "HELLO WORLD",
            "Main",
            &[ProjectTreeEntry {
                label: "scene.ron".to_string(),
                icon: UiIconId::Scene,
                depth: 1,
            }],
        );
        let session = child(&surface.root, "project.session");
        let entry = child(&surface.root, "project.entry.0");

        assert_eq!(session.text_value.as_deref(), Some("Main"));
        assert_eq!(session.layout.padding.left, 20.0);
        assert_eq!(entry.layout.padding.left, 20.0);
        assert_eq!(
            child(&surface.root, "project.root").layout.width_mode,
            UiSizeMode::Fill
        );
        assert_eq!(
            child(&surface.root, "project.root").layout.height_mode,
            UiSizeMode::FitContent
        );
    }

    #[test]
    fn bottom_tabs_use_stable_text_safe_widths() {
        let group = DockTabGroup::new(
            "main",
            vec![DockTab::new(
                "project-settings",
                "app.studio_project",
                UiIconId::Settings,
            )],
        );
        let surface = build_tab_strip_surface(StudioUiPalette::IndustrialDark, &group, false, None);
        let tab = child(&surface.root, "bottom.tab.main.project-settings");

        assert_eq!(tab.layout.basis, [104.0, 28.0]);
        assert_eq!(tab.text_key.as_deref(), Some("app.studio_project"));
    }

    #[test]
    fn status_labels_measure_natural_width_without_growing_below_the_bar() {
        let surface = build_status_surface(
            StudioUiPalette::IndustrialDark,
            &["Long project name".to_string(), "Selected: 1".to_string()],
        );
        let mut session = raf_render::api_graphic_basic::ui_surface::UiSurfaceSession::default();
        let frame = session.build_frame_with_resolved_text(
            &surface,
            800,
            28,
            [0, 0, 0, 255],
            str::to_string,
        );
        let project = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "editor.status.item.0")
            .expect("project status item");
        let selected = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "editor.status.item.1")
            .expect("selected status item");

        assert!(project.rect.height <= 28.0);
        assert!(selected.rect.y >= project.rect.y);
        assert!(selected.rect.x >= project.rect.x + project.rect.width);
    }

    #[test]
    fn asset_icons_follow_known_project_file_types() {
        assert_eq!(asset_icon("main.scene"), UiIconId::Scene);
        assert_eq!(asset_icon("board.pcb"), UiIconId::Pcb);
        assert_eq!(asset_icon("config.ron"), UiIconId::Project);
        assert_eq!(asset_icon("Building_A.glb"), UiIconId::Assets);
    }
}
