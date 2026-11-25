//! Conversation history, session rows and viewport paging for the Agent surface.

use raf_ai::agent_runtime::{AgentStatus, AgentToolResult};
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon,
    UiIconId, UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect,
    UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiTextOverflow, UiTextStyle, UiVirtualRange,
};

use crate::panels::ai_chat::AgentPanel;

use super::evidence::{plan_review_card, plan_review_from_message};
use super::layout::{
    bounded_text, copy_message_button, icon_button, message_chrome_height, short_text, spacer,
    AgentSurfaceVariant, MessageViewport, DOCK_TOOL_ROW_GAP, DOCK_TOOL_ROW_HEIGHT,
    HISTORY_OVERSCAN, HISTORY_VIEWPORT_ESTIMATE, MAX_VISIBLE_MESSAGE_CHARS, MESSAGE_TEXT_TILE_GAP,
    MESSAGE_WIDTH_RATIO, PLAN_REVIEW_HEIGHT, SESSION_DOT_SIZE,
};
use super::messages::{
    display_message_content, estimated_text_tile_height, message_card_with_variant,
    message_text_tiles,
};

pub(super) fn build_sidebar(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    width: f32,
    height: f32,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let active = panel.history.active_index;
    let mut sessions = UiNode::scroll_view("agent.sessions.scroll", UiScrollAxis::Vertical)
        .with_class("agent-sessions-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 3.0,
            padding: UiSpacing::xy(8.0, 8.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });

    if panel.history.sessions.is_empty() {
        sessions = sessions.with_child(
            UiNode::new("agent.sessions.empty", UiNodeKind::Label)
                .with_text_key("app.agent_no_messages")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    } else {
        for (index, session) in panel.history.sessions.iter().enumerate() {
            let is_active = active == Some(index);
            let running = panel.session_is_running(index);
            let mut select = UiNode::new(format!("agent.session.{index}"), UiNodeKind::Button)
                .with_class(if is_active {
                    "agent-session-active"
                } else {
                    "agent-session"
                })
                .with_layout(if running && variant == AgentSurfaceVariant::Dock {
                    UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 6.0,
                        grow: 1.0,
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 36.0],
                        padding: UiSpacing::xy(10.0, 8.0),
                        ..UiLayout::default()
                    }
                } else {
                    UiLayout {
                        grow: 1.0,
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 36.0],
                        padding: UiSpacing::xy(10.0, 8.0),
                        ..UiLayout::default()
                    }
                });
            if running && variant == AgentSurfaceVariant::Dock {
                select = select
                    .with_child(
                        UiNode::new(format!("agent.session.dot.{index}"), UiNodeKind::Panel)
                            .with_layout(UiLayout::fixed(SESSION_DOT_SIZE, SESSION_DOT_SIZE))
                            .with_style(UiStyle {
                                fill: tokens.accent_hot,
                                radius: 3.5,
                                ..UiStyle::transparent()
                            })
                            .with_accessibility_role(UiAccessibilityRole::Status)
                            .with_tooltip_key("agent_workspace.running"),
                    )
                    .with_child(
                        UiNode::new(format!("agent.session.name.{index}"), UiNodeKind::Label)
                            .with_text_value(short_text(&session.title, 72))
                            .with_text_overflow(UiTextOverflow::Ellipsis)
                            .with_text_style(UiTextStyle::button(if is_active {
                                tokens.text
                            } else {
                                tokens.text_muted
                            }))
                            .with_layout(UiLayout {
                                grow: 1.0,
                                width_mode: UiSizeMode::Fill,
                                ..UiLayout::fit_content()
                            }),
                    );
            } else {
                select = select
                    .with_text_value(short_text(&session.title, 72))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_text_style(UiTextStyle::button(if is_active {
                        tokens.text
                    } else {
                        tokens.text_muted
                    }));
            }
            let select = select
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_accessibility_selected(is_active)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session.select:{index}"),
                ));
            let delete = UiNode::new(format!("agent.session.delete.{index}"), UiNodeKind::Button)
                .with_class("agent-icon-button")
                .with_layout(UiLayout::fixed(30.0, 30.0))
                .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                .with_tooltip_key("app.agent_delete")
                .with_accessibility_label_key("app.agent_delete")
                .with_accessibility_role(UiAccessibilityRole::Button)
                .focusable()
                .disabled(panel.session_is_running(index))
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session.delete:{index}"),
                ));
            sessions = sessions.with_child(
                UiNode::new(format!("agent.session.row.{index}"), UiNodeKind::Toolbar)
                    .with_class("agent-session-row")
                    .with_layout({
                        let mut layout =
                            UiLayout::fixed(0.0, 36.0).with_width_mode(UiSizeMode::Fill);
                        layout.flow = UiFlow::Row;
                        layout.gap = 4.0;
                        layout.align_items = UiAlign::Center;
                        layout
                    })
                    .with_child(select)
                    .with_child(delete),
            );
        }
    }

    let mut sidebar_layout = UiLayout::absolute(UiRect::new(0.0, 0.0, width, height));
    sidebar_layout.flow = UiFlow::Column;
    sidebar_layout.overflow = UiOverflow::Clip;
    UiNode::new("agent.sidebar", UiNodeKind::Panel)
        .with_class("agent-sidebar")
        .with_layout(sidebar_layout)
        .with_child(
            UiNode::new("agent.sidebar.header", UiNodeKind::Toolbar)
                .with_class("agent-sidebar-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::SpaceBetween,
                    padding: UiSpacing::xy(12.0, 8.0),
                    ..UiLayout::fixed(0.0, 48.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("agent.sessions.title", UiNodeKind::Label)
                        .with_text_key("app.agent_sessions")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(icon_button(
                    "agent.sidebar.new",
                    UiIconId::Add,
                    "app.agent_new_chat",
                    "agent.new-chat",
                )),
        )
        .with_child(sessions)
}

#[derive(Debug, PartialEq)]
pub(super) enum HistoryEntry {
    Message(usize),
    Plan(usize),
    Reasoning(usize),
    Activity {
        first: usize,
        count: usize,
        expanded: bool,
    },
}

/// Presentation-only grouping. Domain messages and tool-call order are untouched.
/// Errors stay outside collapsed details; the final answer always stays visible.
pub(super) fn history_entries(
    messages: &[&ChatMessage],
    expanded: &std::collections::HashSet<String>,
) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    while cursor < messages.len() {
        if messages[cursor].role == MessageRole::User {
            entries.push(HistoryEntry::Message(cursor));
            cursor += 1;
            continue;
        }
        if messages[cursor].role == MessageRole::Tool
            && plan_review_from_message(messages[cursor]).is_some()
        {
            entries.push(HistoryEntry::Plan(cursor));
            cursor += 1;
            continue;
        }
        let first = cursor;
        while cursor < messages.len()
            && messages[cursor].role != MessageRole::User
            && !(messages[cursor].role == MessageRole::Tool
                && plan_review_from_message(messages[cursor]).is_some())
        {
            cursor += 1;
        }
        if cursor == first {
            // The next entry is a plan card and is handled by the branch above.
            continue;
        }
        let segment = &messages[first..cursor];
        let count = segment
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .count();
        if count < 2 {
            entries.extend((first..cursor).map(HistoryEntry::Message));
            continue;
        }
        let open = expanded.contains(&format!("group:{}", messages[first].id));
        entries.push(HistoryEntry::Activity {
            first,
            count,
            expanded: open,
        });
        let last_tool = (first..cursor)
            .rfind(|&i| messages[i].role == MessageRole::Tool)
            .unwrap();
        for index in first..cursor {
            if open
                || index > last_tool
                || expanded.contains(&messages[index].id.to_string())
                || (messages[index].role == MessageRole::Tool
                    && tool_needs_attention(messages[index]))
            {
                entries.push(HistoryEntry::Message(index));
            }
        }
    }
    entries
}

pub(super) fn activity_group(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    message: &ChatMessage,
    count: usize,
    expanded: bool,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    UiNode::new(format!("agent.activity.{}", message.id), UiNodeKind::Button)
        .with_class("agent-tool-row")
        .with_icon(
            UiIcon::new(if expanded {
                UiIconId::ChevronDown
            } else {
                UiIconId::ChevronRight
            })
            .with_size(UiIconSize::Small),
        )
        .with_text_value(format!(
            "{count} {}",
            t("agent.activity.group", panel.language)
        ))
        .with_text_style(UiTextStyle::button(tokens.text_muted))
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fixed(0.0, DOCK_TOOL_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_expanded(expanded)
        .with_accessibility_label_key("agent.activity.details")
        .with_tooltip_key("agent.activity.details")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("agent.tool.toggle:group:{}", message.id),
        ))
}

pub(super) fn build_history(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    project_type: ProjectType,
    messages: &[&ChatMessage],
    width: f32,
    scroll_offset: f32,
    variant: AgentSurfaceVariant,
    tool_expansion: impl Fn(&ChatMessage) -> bool,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let mut entries = Vec::new();
    for entry in history_entries(messages, &panel.expanded_tools) {
        if let HistoryEntry::Plan(index) = &entry {
            let dismissed = plan_review_from_message(messages[*index])
                .is_some_and(|plan| panel.dismissed_plans.contains(&plan.plan_id));
            if dismissed {
                continue;
            }
        }
        if let HistoryEntry::Message(index) = entry {
            let message = messages[index];
            if panel.show_reasoning
                && message
                    .reasoning
                    .as_ref()
                    .is_some_and(|r| !r.text.is_empty())
            {
                entries.push(HistoryEntry::Reasoning(index));
            }
            if message.role == MessageRole::Assistant
                && message.content.trim().is_empty()
                && (message.tool_calls.is_some() || message.reasoning.is_some())
            {
                continue;
            }
            entries.push(HistoryEntry::Message(index));
        } else {
            entries.push(entry);
        }
    }
    let message_heights = entries
        .iter()
        .map(|entry| match entry {
            HistoryEntry::Activity { .. } => DOCK_TOOL_ROW_HEIGHT,
            HistoryEntry::Plan(_) => PLAN_REVIEW_HEIGHT,
            HistoryEntry::Reasoning(index) => {
                if panel
                    .expanded_tools
                    .contains(&format!("reasoning:{}", messages[*index].id))
                {
                    DOCK_TOOL_ROW_HEIGHT + 126.0
                } else {
                    DOCK_TOOL_ROW_HEIGHT
                }
            }
            HistoryEntry::Message(index) => estimated_message_height_with_variant(
                messages[*index],
                width,
                variant,
                tool_expansion(messages[*index]),
            ),
        })
        .collect::<Vec<_>>();
    let range = visible_message_range(&message_heights, scroll_offset, HISTORY_VIEWPORT_ESTIMATE);
    let mut scroll = UiNode::scroll_view("agent.history", UiScrollAxis::Vertical)
        .with_class("agent-history")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 10.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    let top_height = estimated_prefix_height(&message_heights, range.start);
    let bottom_height = estimated_suffix_height(&message_heights, range.end);
    if top_height > 0.0 {
        scroll = scroll.with_child(spacer("agent.history.top", top_height));
    }
    if messages.is_empty() {
        scroll = scroll.with_child(
            UiNode::new("agent.empty", UiNodeKind::Panel)
                .with_class("agent-empty")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 5.0,
                    padding: UiSpacing::same(18.0),
                    align_items: UiAlign::Center,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(if variant == AgentSurfaceVariant::Embedded {
                    UiNode::new("agent.composer.hint", UiNodeKind::Label)
                        .with_text_key("app.agent_enter_to_send")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content())
                } else {
                    spacer("agent.composer.hint.hidden", 0.0)
                })
                .with_child(
                    UiNode::new("agent.empty.subtitle", UiNodeKind::Label)
                        .with_text_key(match project_type {
                            ProjectType::Game => "app.agent_empty_subtitle_game",
                            ProjectType::Electronics => "app.agent_empty_subtitle_electronics",
                        })
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout {
                            max_size: [520.0, 0.0],
                            ..UiLayout::fit_content()
                        }),
                ),
        );
    } else {
        let mut message_top = top_height + if top_height > 0.0 { 8.0 } else { 0.0 };
        for (entry_index, entry) in entries
            .iter()
            .enumerate()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
        {
            let viewport =
                MessageViewport::for_message(scroll_offset, HISTORY_VIEWPORT_ESTIMATE, message_top);
            scroll = scroll.with_child(match entry {
                HistoryEntry::Activity {
                    first,
                    count,
                    expanded,
                } => activity_group(palette, panel, messages[*first], *count, *expanded),
                HistoryEntry::Plan(index) => {
                    if let Some(review) = plan_review_from_message(messages[*index]) {
                        plan_review_card(palette, panel, &review, width, *index)
                    } else {
                        spacer("agent.plan.review.missing", 0.0)
                    }
                }
                HistoryEntry::Reasoning(index) => reasoning_entry(palette, panel, messages[*index]),
                HistoryEntry::Message(index) => message_card_with_variant(
                    palette,
                    messages[*index],
                    width,
                    *index,
                    viewport,
                    variant,
                    tool_expansion(messages[*index]),
                ),
            });
            message_top += message_heights[entry_index] + 8.0;
        }
    }
    if bottom_height > 0.0 {
        scroll = scroll.with_child(spacer("agent.history.bottom", bottom_height));
    }
    if panel.runtime.status == AgentStatus::Done && panel.runtime.messages.is_empty() {
        return scroll;
    }
    scroll
}

pub(super) fn reasoning_entry(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    message: &ChatMessage,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let trace = message
        .reasoning
        .as_ref()
        .map(|r| r.text.as_str())
        .unwrap_or("");
    let key = format!("reasoning:{}", message.id);
    let expanded = panel.expanded_tools.contains(&key);
    let title = format!(
        "{} · {}",
        t("agent.reasoning.trace", panel.language),
        short_text(trace, 120)
    );
    let button = UiNode::new(
        format!("agent.reasoning.{}.toggle", message.id),
        UiNodeKind::Button,
    )
    .with_class("agent-tool-row")
    .with_icon(
        UiIcon::new(if expanded {
            UiIconId::ChevronDown
        } else {
            UiIconId::ChevronRight
        })
        .with_size(UiIconSize::Small),
    )
    .with_text_value(title)
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(UiTextStyle::button(tokens.text_muted))
    .with_accessibility_role(UiAccessibilityRole::Button)
    .with_accessibility_label_key("agent.reasoning.trace")
    .with_accessibility_expanded(expanded)
    .with_tooltip_key("agent.reasoning.trace_hint")
    .focusable()
    .with_layout(UiLayout::fixed(0.0, DOCK_TOOL_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill))
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("agent.tool.toggle:{key}"),
    ));
    if !expanded {
        return button;
    }
    UiNode::new(format!("agent.reasoning.{}", message.id), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, DOCK_TOOL_ROW_HEIGHT + 126.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(button)
        .with_child(
            UiNode::new(
                format!("agent.reasoning.{}.text", message.id),
                UiNodeKind::Label,
            )
            .with_text_value(bounded_text(trace, 2048))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Wrap)
            .selectable_text()
            .with_layout(UiLayout {
                overflow: UiOverflow::Clip,
                ..UiLayout::fixed(0.0, 88.0).with_width_mode(UiSizeMode::Fill)
            }),
        )
        .with_child(copy_message_button(
            format!("agent.reasoning.{}.copy", message.id),
            trace,
        ))
}

pub(super) fn build_suggestions(palette: StudioUiPalette, project_type: ProjectType) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let keys: &[&str] = match project_type {
        ProjectType::Electronics => &[
            "app.agent_hint_resistor",
            "app.agent_hint_electrical_test",
            "app.agent_hint_nets",
        ],
        ProjectType::Game => &[
            "app.agent_hint_cube",
            "app.agent_hint_light",
            "app.agent_hint_camera",
        ],
    };
    let mut row = UiNode::new("agent.suggestions", UiNodeKind::Toolbar)
        .with_class("agent-suggestions")
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            gap: 6.0,
            padding: UiSpacing::xy(12.0, 6.0),
            compact: raf_render::api_graphic_basic::ui_surface::UiCompactMode::Wrap,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("agent.suggestions.title", UiNodeKind::Label)
                .with_text_key("app.agent_suggestions_title")
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    for key in keys {
        row = row.with_child(
            UiNode::new(format!("agent.suggestion.{key}"), UiNodeKind::Button)
                .with_class("agent-suggestion")
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(10.0, 6.0),
                    ..UiLayout::fit_content()
                })
                .with_text_key(*key)
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_accessibility_role(UiAccessibilityRole::Button)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.suggestion:{key}"),
                )),
        );
    }
    row
}

pub(super) fn visible_message_range(
    message_heights: &[f32],
    scroll_offset: f32,
    viewport_height: f32,
) -> UiVirtualRange {
    if message_heights.is_empty() {
        return UiVirtualRange { start: 0, end: 0 };
    }
    let mut cursor = 0.0;
    let target = scroll_offset.max(0.0);
    let mut first = 0;
    for (index, height) in message_heights.iter().copied().enumerate() {
        if cursor + height > target {
            first = index;
            break;
        }
        cursor += height + 8.0;
        first = index.saturating_add(1).min(message_heights.len());
    }
    let mut end = first;
    let mut visible_height = 0.0;
    while end < message_heights.len() && visible_height < viewport_height.max(0.0) {
        visible_height += message_heights[end] + 8.0;
        end += 1;
    }
    UiVirtualRange {
        start: first.saturating_sub(HISTORY_OVERSCAN),
        end: end
            .saturating_add(HISTORY_OVERSCAN)
            .min(message_heights.len()),
    }
}

pub(super) fn estimated_prefix_height(message_heights: &[f32], end: usize) -> f32 {
    let count = end.min(message_heights.len());
    let total = message_heights
        .iter()
        .take(count)
        .map(|height| height + 8.0)
        .sum::<f32>();
    if count > 0 {
        total - 8.0
    } else {
        0.0
    }
}

pub(super) fn estimated_suffix_height(message_heights: &[f32], start: usize) -> f32 {
    let count = message_heights.len().saturating_sub(start);
    let total = message_heights
        .iter()
        .skip(start)
        .map(|height| height + 8.0)
        .sum::<f32>();
    if count > 0 {
        total - 8.0
    } else {
        0.0
    }
}

pub(super) fn tool_needs_attention(message: &ChatMessage) -> bool {
    serde_json::from_str::<AgentToolResult>(&message.content)
        .map(|result| {
            !result.ok
                || result
                    .verification
                    .as_ref()
                    .and_then(|v| v.get("status"))
                    .and_then(serde_json::Value::as_str)
                    == Some("failed")
        })
        // Legacy unstructured errors must remain visible too.
        .unwrap_or_else(|_| message.content.trim_start().starts_with("Error:"))
}

pub(super) fn estimated_message_height_with_variant(
    message: &ChatMessage,
    width: f32,
    variant: AgentSurfaceVariant,
    tool_expanded: bool,
) -> f32 {
    let content = bounded_text(&display_message_content(message), MAX_VISIBLE_MESSAGE_CHARS);
    let card_width = (width * MESSAGE_WIDTH_RATIO).clamp(220.0, 760.0);
    let line_height = 18.0;
    let chunks = message_text_tiles(&content, message.role);
    let body_height = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            estimated_text_tile_height(&chunk.0, card_width - 24.0, line_height)
                + if index + 1 < chunks.len() {
                    MESSAGE_TEXT_TILE_GAP
                } else {
                    0.0
                }
        })
        .sum::<f32>();
    let chrome = message_chrome_height(if message.role == MessageRole::Tool {
        AgentSurfaceVariant::Dock
    } else {
        variant
    });
    if message.role == MessageRole::Tool {
        let summary = DOCK_TOOL_ROW_HEIGHT + DOCK_TOOL_ROW_GAP;
        if !tool_expanded && !tool_needs_attention(message) {
            return DOCK_TOOL_ROW_HEIGHT;
        }
        return summary + chrome + body_height;
    }
    (chrome + body_height).max(70.0)
}
