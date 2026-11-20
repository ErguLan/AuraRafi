//! Message cards, tool rows and bounded text tiling for the Agent surface.

use raf_ai::agent_runtime::AgentToolResult;
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow,
    UiFontWeight, UiIcon, UiIconId, UiIconSize, UiLayout, UiNode, UiNodeKind, UiOverflow,
    UiSizeMode, UiSpacing, UiStyle, UiTextOverflow, UiTextRole, UiTextStyle,
};

use super::history::tool_needs_attention;
use super::layout::{
    bounded_text, copy_message_button, message_chrome_height, spacer, AgentSurfaceVariant,
    MessageViewport, DOCK_TOOL_ROW_GAP, DOCK_TOOL_ROW_HEIGHT, MAX_VISIBLE_MESSAGE_CHARS,
    MESSAGE_TEXT_CHUNK_CHARS, MESSAGE_TEXT_CHUNK_LINES, MESSAGE_TEXT_TILE_GAP,
    MESSAGE_TEXT_TILE_OVERSCAN, MESSAGE_WIDTH_RATIO,
};

pub(super) fn message_card_with_variant(
    palette: StudioUiPalette,
    message: &ChatMessage,
    width: f32,
    index: usize,
    viewport: MessageViewport,
    variant: AgentSurfaceVariant,
    tool_expanded: bool,
) -> UiNode {
    if message.role == MessageRole::Tool {
        return dock_tool_entry(
            palette,
            message,
            width,
            index,
            viewport,
            tool_expanded || tool_needs_attention(message),
        );
    }
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let content = bounded_text(&display_message_content(message), MAX_VISIBLE_MESSAGE_CHARS);
    let (class, label_key, label_color, card_alignment) = match message.role {
        MessageRole::User => (
            "agent-message-user",
            "app.agent_you",
            tokens.accent_hot,
            UiAlign::End,
        ),
        MessageRole::Assistant => (
            "agent-message-assistant",
            "app.agent_label",
            tokens.text_muted,
            UiAlign::Start,
        ),
        MessageRole::Tool => (
            "agent-message-tool",
            "app.agent_tool_result",
            tokens.positive,
            UiAlign::Start,
        ),
        MessageRole::System => (
            "agent-message-system",
            "app.agent_system_label",
            tokens.text_muted,
            UiAlign::Start,
        ),
    };
    let card_width = (width * MESSAGE_WIDTH_RATIO).clamp(220.0, 760.0);
    let body_style = UiTextStyle::body(tokens.text);
    let display_chunks = message_text_tiles(&content, message.role);
    let body_height = display_chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            estimated_text_tile_height(&chunk.0, card_width - 24.0, body_style.line_height_px)
                + if index + 1 < display_chunks.len() {
                    MESSAGE_TEXT_TILE_GAP
                } else {
                    0.0
                }
        })
        .sum::<f32>()
        .max(1.0);
    let chrome = message_chrome_height(variant);
    let card_height = chrome + body_height;
    let dock_head = variant == AgentSurfaceVariant::Dock
        && matches!(message.role, MessageRole::Assistant | MessageRole::Tool);
    let mut card = UiNode::new(format!("agent.message.{index}.card"), UiNodeKind::Panel)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(12.0, 9.0),
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fixed,
            basis: [card_width, card_height],
            ..UiLayout::default()
        });
    if dock_head {
        card = card
            .with_child(
                UiNode::new(format!("agent.message.{index}.head"), UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 6.0,
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 28.0],
                        ..UiLayout::default()
                    })
                    .with_child(
                        UiNode::new(format!("agent.message.{index}.label"), UiNodeKind::Label)
                            .with_text_key(label_key)
                            .with_text_style(UiTextStyle::button(label_color))
                            .with_layout(UiLayout {
                                grow: 1.0,
                                width_mode: UiSizeMode::Fill,
                                ..UiLayout::fit_content()
                            }),
                    )
                    .with_child(copy_message_button(
                        format!("agent.message.{index}.copy"),
                        &message.content,
                    )),
            )
            .with_child(message_body_with_chrome(
                format!("agent.message.{index}.body"),
                &content,
                body_style,
                message.role,
                card_width - 24.0,
                viewport,
                chrome,
            ));
    } else {
        card = card
            .with_child(
                UiNode::new(format!("agent.message.{index}.label"), UiNodeKind::Label)
                    .with_text_key(label_key)
                    .with_text_style(UiTextStyle::button(label_color))
                    .with_layout(UiLayout::fit_content()),
            )
            .with_child(message_body_with_chrome(
                format!("agent.message.{index}.body"),
                &content,
                body_style,
                message.role,
                card_width - 24.0,
                viewport,
                chrome,
            ));
        if matches!(message.role, MessageRole::Assistant | MessageRole::Tool) {
            card = card.with_child(copy_message_button(
                format!("agent.message.{index}.copy"),
                &message.content,
            ));
        }
    }
    UiNode::new(format!("agent.message.{index}.row"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: card_alignment,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, card_height],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(card)
}

pub(super) fn dock_tool_entry(
    palette: StudioUiPalette,
    message: &ChatMessage,
    width: f32,
    index: usize,
    viewport: MessageViewport,
    tool_expanded: bool,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let content = bounded_text(&display_message_content(message), MAX_VISIBLE_MESSAGE_CHARS);
    let (status_fill, status_key) = tool_status(palette, message);
    let title = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string);
    let title_node = UiNode::new(
        format!("agent.message.{index}.row.title"),
        UiNodeKind::Label,
    )
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_text_style(UiTextStyle::button(tokens.text_muted))
    .with_layout(UiLayout {
        grow: 1.0,
        width_mode: UiSizeMode::Fill,
        ..UiLayout::fit_content()
    });
    let title_node = if let Some(title) = title {
        title_node.with_text_value(title)
    } else {
        title_node.with_text_key("app.agent_tool_result")
    };
    let mut toggle_row = UiNode::new(format!("agent.message.{index}.row"), UiNodeKind::Toolbar)
        .with_class("agent-tool-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 4.0),
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, DOCK_TOOL_ROW_HEIGHT],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new(
                format!("agent.message.{index}.row.toggle"),
                UiNodeKind::Button,
            )
            .with_class("agent-icon-button")
            .with_layout(UiLayout::fixed(24.0, 24.0))
            .with_icon(
                UiIcon::new(if tool_expanded {
                    UiIconId::ChevronDown
                } else {
                    UiIconId::ChevronRight
                })
                .with_size(UiIconSize::Small),
            )
            .with_tooltip_key("app.agent_tool_result")
            .with_accessibility_label_key("app.agent_tool_result")
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_accessibility_expanded(tool_expanded)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("agent.tool.toggle:{}", message.id),
            )),
        )
        .with_child(
            UiNode::new(
                format!("agent.message.{index}.row.status"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::fixed(8.0, 8.0))
            .with_style(UiStyle {
                fill: status_fill,
                radius: 4.0,
                ..UiStyle::transparent()
            })
            .with_accessibility_role(UiAccessibilityRole::Status)
            .with_tooltip_key(status_key),
        )
        .with_child(title_node);
    if let Some(result) = message
        .role
        .eq(&MessageRole::Tool)
        .then_some(())
        .and_then(|()| serde_json::from_str::<AgentToolResult>(&message.content).ok())
        .and_then(|result| result.revision.map(|revision| (result, revision)))
    {
        let (result, revision) = result;
        toggle_row = toggle_row.with_child(
            UiNode::new(
                format!("agent.message.{index}.row.revision"),
                UiNodeKind::Label,
            )
            .with_class("agent-tool-chip")
            .with_text_value(format!("Rev {revision}"))
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_layout(UiLayout::fit_content()),
        );
        if result.partial {
            toggle_row = toggle_row.with_child(
                UiNode::new(
                    format!("agent.message.{index}.row.partial"),
                    UiNodeKind::Label,
                )
                .with_class("agent-tool-chip")
                .with_text_key("app.agent_tool_partial")
                .with_text_style(UiTextStyle::button(tokens.warning))
                .with_layout(UiLayout::fit_content()),
            );
        }
        if let Some(plan_id) = result
            .data
            .get("plan_id")
            .and_then(serde_json::Value::as_str)
        {
            toggle_row = toggle_row.with_child(
                UiNode::new(
                    format!("agent.message.{index}.row.plan"),
                    UiNodeKind::Label,
                )
                .with_class("agent-tool-chip")
                .with_text_value(format!(
                    "{} · {}",
                    t("app.agent_tool_plan", Language::English),
                    plan_id
                ))
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_layout(UiLayout::fit_content()),
            );
        }
        if !result.artifacts.is_empty() {
            toggle_row = toggle_row.with_child(
                UiNode::new(
                    format!("agent.message.{index}.row.artifacts"),
                    UiNodeKind::Label,
                )
                .with_class("agent-tool-chip")
                .with_text_value(format!(
                    "{} · {}",
                    t("app.agent_tool_artifacts", Language::English),
                    result.artifacts.len()
                ))
                .with_text_style(UiTextStyle::button(tokens.positive))
                .with_layout(UiLayout::fit_content()),
            );
        }
    }
    if !tool_expanded {
        return toggle_row;
    }
    let body_style = UiTextStyle::body(tokens.text);
    let display_chunks = message_text_tiles(&content, MessageRole::Tool);
    let card_width = (width * MESSAGE_WIDTH_RATIO).clamp(220.0, 760.0);
    let body_height = display_chunks
        .iter()
        .enumerate()
        .map(|(chunk_index, chunk)| {
            estimated_text_tile_height(&chunk.0, card_width - 24.0, body_style.line_height_px)
                + if chunk_index + 1 < display_chunks.len() {
                    MESSAGE_TEXT_TILE_GAP
                } else {
                    0.0
                }
        })
        .sum::<f32>()
        .max(1.0);
    let chrome = message_chrome_height(AgentSurfaceVariant::Dock);
    let card_height = chrome + body_height;
    let card = UiNode::new(format!("agent.message.{index}.card"), UiNodeKind::Panel)
        .with_class("agent-message-tool")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(12.0, 9.0),
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fixed,
            basis: [card_width, card_height],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new(format!("agent.message.{index}.head"), UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    width_mode: UiSizeMode::Fill,
                    height_mode: UiSizeMode::Fixed,
                    basis: [0.0, 28.0],
                    ..UiLayout::default()
                })
                .with_child(
                    UiNode::new(format!("agent.message.{index}.label"), UiNodeKind::Label)
                        .with_text_key("app.agent_tool_result")
                        .with_text_style(UiTextStyle::button(status_fill))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            width_mode: UiSizeMode::Fill,
                            ..UiLayout::fit_content()
                        }),
                )
                .with_child(copy_message_button(
                    format!("agent.message.{index}.copy"),
                    &message.content,
                )),
        )
        .with_child(message_body_with_chrome(
            format!("agent.message.{index}.body"),
            &content,
            body_style,
            message.role,
            card_width - 24.0,
            viewport,
            chrome + DOCK_TOOL_ROW_HEIGHT + DOCK_TOOL_ROW_GAP,
        ));
    UiNode::new(format!("agent.message.{index}.wrap"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Start,
            gap: DOCK_TOOL_ROW_GAP,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, DOCK_TOOL_ROW_HEIGHT + DOCK_TOOL_ROW_GAP + card_height],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(toggle_row)
        .with_child(card)
}

pub(super) fn tool_status(palette: StudioUiPalette, message: &ChatMessage) -> ([u8; 4], &'static str) {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let Ok(result) = serde_json::from_str::<AgentToolResult>(&message.content) else {
        return (
            if tool_needs_attention(message) {
                tokens.danger
            } else {
                tokens.text_muted
            },
            "app.agent_tool_result",
        );
    };
    if !result.ok {
        return (tokens.danger, "app.agent_tool_result");
    }
    if result.partial {
        return (tokens.warning, "app.agent_tool_partial");
    }
    let verification_status = result
        .verification
        .as_ref()
        .and_then(|verification| verification.get("status"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_ascii_lowercase);
    match verification_status.as_deref() {
        Some("ok") | Some("passed") | Some("success") | Some("reviewed") => {
            (tokens.positive, "app.agent_tool_result")
        }
        Some("failed") | Some("error") | Some("errors") => (tokens.danger, "app.agent_tool_result"),
        _ if !result.warnings.is_empty() => (tokens.warning, "app.agent_tool_result"),
        _ if result.changed => (tokens.positive, "app.agent_tool_result"),
        _ => (tokens.text_muted, "app.agent_tool_result"),
    }
}

pub(super) fn message_body_with_chrome(
    id: String,
    content: &str,
    body_style: UiTextStyle,
    role: MessageRole,
    width: f32,
    viewport: MessageViewport,
    chrome_height: f32,
) -> UiNode {
    let body_id = id;
    let chunks = message_text_tiles(content, role);
    let chunk_heights = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            estimated_text_tile_height(&chunk.0, width, body_style.line_height_px)
                + if index + 1 < chunks.len() {
                    MESSAGE_TEXT_TILE_GAP
                } else {
                    0.0
                }
        })
        .collect::<Vec<_>>();
    let body_height = chunk_heights.iter().sum::<f32>().max(1.0);
    let visible_top = viewport.top - chrome_height - MESSAGE_TEXT_TILE_OVERSCAN;
    let visible_bottom = viewport.bottom - chrome_height + MESSAGE_TEXT_TILE_OVERSCAN;
    let visible_range = visible_text_tile_range(&chunk_heights, visible_top, visible_bottom);
    let top_spacer = chunk_heights.iter().take(visible_range.start).sum::<f32>();
    let bottom_spacer = chunk_heights.iter().skip(visible_range.end).sum::<f32>();
    let mut body = UiNode::new(body_id.clone(), UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        width_mode: UiSizeMode::Fill,
        height_mode: UiSizeMode::Fixed,
        basis: [0.0, body_height],
        max_size: [width.max(1.0), 0.0],
        overflow: UiOverflow::Clip,
        ..UiLayout::default()
    });
    if top_spacer > 0.0 {
        body = body.with_child(spacer(&format!("{body_id}.top"), top_spacer));
    }
    for (index, chunk) in chunks
        .into_iter()
        .enumerate()
        .skip(visible_range.start)
        .take(visible_range.end.saturating_sub(visible_range.start))
    {
        let text_id = if index == 0 {
            format!("{body_id}.text")
        } else {
            format!("{body_id}.text.{index}")
        };
        let mut tile_style = body_style;
        match chunk.1 {
            MessageTileKind::Heading => tile_style.weight = UiFontWeight::Bold,
            MessageTileKind::Code => tile_style.role = UiTextRole::Monospace,
            MessageTileKind::Body => {}
        }
        let code_style = if chunk.1 == MessageTileKind::Code {
            UiStyle {
                fill: [0, 0, 0, 20],
                border: [
                    body_style.color[0],
                    body_style.color[1],
                    body_style.color[2],
                    40,
                ],
                border_width: 1.0,
                radius: 4.0,
                ..UiStyle::transparent()
            }
        } else {
            UiStyle::transparent()
        };
        body = body.with_child(
            UiNode::new(text_id, UiNodeKind::Label)
                .with_text_value(chunk.0)
                .selectable_text()
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_text_style(tile_style)
                .with_style(code_style)
                .with_layout(UiLayout {
                    width_mode: UiSizeMode::Fill,
                    height_mode: UiSizeMode::Fixed,
                    basis: [0.0, chunk_heights[index]],
                    max_size: [width.max(1.0), 0.0],
                    overflow: UiOverflow::Clip,
                    ..UiLayout::default()
                }),
        );
    }
    if bottom_spacer > 0.0 {
        body = body.with_child(spacer(&format!("{body_id}.bottom"), bottom_spacer));
    }
    body
}

pub(super) fn visible_text_tile_range(
    chunk_heights: &[f32],
    visible_top: f32,
    visible_bottom: f32,
) -> std::ops::Range<usize> {
    let mut cursor = 0.0;
    let mut first = None;
    let mut end = 0;
    for (index, height) in chunk_heights.iter().copied().enumerate() {
        let tile_bottom = cursor + height;
        if tile_bottom > visible_top && cursor < visible_bottom {
            first.get_or_insert(index);
            end = index + 1;
        }
        cursor = tile_bottom;
    }
    first.map_or(0..0, |first| first..end)
}

pub(super) fn estimated_text_tile_height(content: &str, width: f32, line_height: f32) -> f32 {
    // Deliberately conservative: a slightly taller fixed tile is cheaper than
    // clipping a wide glyph and keeps virtual spacers stable across DPI.
    let chars_per_line = (width.max(1.0) / 8.0).max(16.0) as usize;
    let lines = content
        .split('\n')
        .map(|line| {
            (line.chars().count().saturating_add(chars_per_line - 1) / chars_per_line).max(1)
        })
        .sum::<usize>()
        .max(1);
    lines as f32 * line_height.max(14.0)
}

/// Keeps a single retained text request below the atlas tile limit. Normal
/// messages remain one node; only long responses are split into bounded tiles.
pub(super) fn message_text_chunks(content: &str) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }
    let mut chunks = Vec::new();
    let mut current = String::with_capacity(MESSAGE_TEXT_CHUNK_CHARS);
    let mut chars = 0usize;
    let mut lines = 1usize;
    for character in content.chars() {
        current.push(character);
        chars += 1;
        if character == '\n' {
            lines += 1;
        }
        if chars >= MESSAGE_TEXT_CHUNK_CHARS || lines >= MESSAGE_TEXT_CHUNK_LINES {
            chunks.push(std::mem::take(&mut current));
            chars = 0;
            lines = 1;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum MessageTileKind {
    Body,
    Heading,
    Code,
}

/// A deliberately small, non-HTML Markdown subset. Never opens links, loads
/// remote images or executes code. Fenced code keeps punctuation and indentation.
pub(super) fn message_text_tiles(content: &str, role: MessageRole) -> Vec<(String, MessageTileKind)> {
    if !matches!(role, MessageRole::Assistant | MessageRole::Tool) {
        return message_text_chunks(content)
            .into_iter()
            .map(|s| (s, MessageTileKind::Body))
            .collect();
    }
    let mut blocks: Vec<(String, MessageTileKind)> = Vec::new();
    let mut fence: Option<&str> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
                continue;
            }
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(if trimmed.starts_with("```") {
                "```"
            } else {
                "~~~"
            });
            continue;
        }
        let (text, kind) = if fence.is_some() {
            (line.to_string(), MessageTileKind::Code)
        } else {
            let heading = trimmed.chars().take_while(|c| *c == '#').count();
            let heading =
                heading > 0 && heading <= 6 && trimmed.as_bytes().get(heading) == Some(&b' ');
            let text = if heading {
                trimmed.trim_start_matches('#').trim_start()
            } else {
                trimmed
            };
            let text =
                if let Some(bullet) = text.strip_prefix("- ").or_else(|| text.strip_prefix("* ")) {
                    format!("\u{2022} {}", strip_inline_markdown(bullet))
                } else {
                    strip_inline_markdown(text)
                };
            (
                text,
                if heading {
                    MessageTileKind::Heading
                } else {
                    MessageTileKind::Body
                },
            )
        };
        if let Some((previous, previous_kind)) = blocks.last_mut().filter(|(_, k)| *k == kind) {
            let _ = previous_kind;
            previous.push('\n');
            previous.push_str(&text);
        } else {
            blocks.push((text, kind));
        }
    }
    if blocks.is_empty() {
        blocks.push((String::new(), MessageTileKind::Body));
    }
    blocks
        .into_iter()
        .flat_map(|(text, kind)| {
            message_text_chunks(&text)
                .into_iter()
                .map(move |s| (s, kind))
        })
        .collect()
}

#[cfg(test)]
pub(super) fn lightweight_markdown_text(content: &str) -> String {
    message_text_tiles(content, MessageRole::Assistant)
        .into_iter()
        .map(|(s, _)| s)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn strip_inline_markdown(value: &str) -> String {
    value.replace("**", "").replace("__", "").replace('`', "")
}

pub(super) fn display_message_content(message: &ChatMessage) -> String {
    if message.role != MessageRole::Tool {
        let mut content = message.content.clone();
        for image in message.images.iter().take(raf_ai::chat_media::MAX_IMAGES) {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&format!("[PNG · {} × {}]", image.width, image.height));
        }
        return content;
    }
    let Ok(result) = serde_json::from_str::<AgentToolResult>(&message.content) else {
        return message.content.clone();
    };
    let mut lines = vec![result.summary];
    if let Some(revision) = result.revision {
        lines.push(format!("Revision {revision}"));
    }
    if let Some(status) = result
        .verification
        .as_ref()
        .and_then(|verification| verification.get("status"))
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("Verification: {status}"));
    }
    if result.partial {
        lines.push("Partial result: some requested targets were not found.".to_string());
    }
    if let Some(diff) = result.diff.as_ref() {
        let count = |key: &str| {
            diff.get(key)
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len)
        };
        let created = count("created");
        let updated = count("updated");
        let deleted = count("deleted");
        if created + updated + deleted > 0 {
            lines.push(format!(
                "Changes: {created} created, {updated} updated, {deleted} deleted."
            ));
        }
    }
    if let Some(plan_id) = result
        .data
        .get("plan_id")
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("Plan: {plan_id}"));
    }
    if let Some(preview_lines) = result
        .data
        .get("preview")
        .and_then(|preview| preview.get("lines"))
        .and_then(serde_json::Value::as_array)
    {
        lines.push("Plan preview:".to_string());
        lines.extend(
            preview_lines
                .iter()
                .filter_map(serde_json::Value::as_str)
                .take(8)
                .map(|line| format!("- {line}")),
        );
    }
    if let Some(status) = result
        .data
        .get("design_audit")
        .and_then(|audit| audit.get("status"))
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("Design audit: {status}."));
    }
    if let Some(overlaps) = result
        .data
        .get("overlap_count")
        .and_then(serde_json::Value::as_u64)
    {
        if overlaps > 0 {
            lines.push(format!("Conservative overlaps: {overlaps}."));
        }
    }
    if !result.artifacts.is_empty() {
        lines.push(format!("Evidence artifacts: {}.", result.artifacts.len()));
    }
    lines.extend(
        result
            .warnings
            .into_iter()
            .map(|warning| format!("Warning: {warning}")),
    );
    lines.join("\n")
}
