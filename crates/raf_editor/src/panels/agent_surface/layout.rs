//! Layout primitives, shared metrics and chrome for the Agent surface.

use raf_core::ai::AgentMode;
use raf_core::config::EngineSettings;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow,
    UiIcon, UiIconId, UiIconSize, UiLayout, UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextOverflow,
    UiTextStyle,
};

use crate::panels::ai_chat::AgentPanel;

use super::models::model_selector_button;
use super::settings::mode_selector_button;

pub(super) const MAX_VISIBLE_MESSAGE_CHARS: usize = 16_384;
pub(super) const MESSAGE_TEXT_CHUNK_CHARS: usize = 768;
pub(super) const MESSAGE_TEXT_CHUNK_LINES: usize = 32;
pub(super) const MESSAGE_TEXT_TILE_GAP: f32 = 2.0;
pub(super) const MESSAGE_TEXT_TILE_OVERSCAN: f32 = 180.0;
pub(super) const MESSAGE_CARD_CHROME_HEIGHT: f32 = 76.0;
pub(super) const HISTORY_OVERSCAN: usize = 3;
pub(super) const HISTORY_VIEWPORT_ESTIMATE: f32 = 460.0;
pub(super) const MESSAGE_WIDTH_RATIO: f32 = 0.78;
pub(super) const PLAN_REVIEW_HEIGHT: f32 = 388.0;
pub(super) const PLAN_REVIEW_TREE_ROWS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AgentSurfaceVariant {
    Embedded,
    Dock,
}

pub(super) const DOCK_TOOL_ROW_HEIGHT: f32 = 34.0;
pub(super) const DOCK_TOOL_ROW_GAP: f32 = 4.0;
pub(super) const DOCK_MESSAGE_CHROME_HEIGHT: f32 = 50.0;
pub(super) const DOCK_COMPOSER_HEIGHT: f32 = 132.0;
pub(super) const APPROVAL_COMPOSER_HEIGHT: f32 = 208.0;
pub(super) const REVIEW_CHIP_WIDTH: f32 = 148.0;
pub(super) const SESSION_DOT_SIZE: f32 = 7.0;

pub(super) fn message_chrome_height(variant: AgentSurfaceVariant) -> f32 {
    match variant {
        AgentSurfaceVariant::Embedded => MESSAGE_CARD_CHROME_HEIGHT,
        AgentSurfaceVariant::Dock => DOCK_MESSAGE_CHROME_HEIGHT,
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MessageViewport {
    pub(super) top: f32,
    pub(super) bottom: f32,
}

impl MessageViewport {
    pub(super) fn for_message(scroll_offset: f32, viewport_height: f32, message_top: f32) -> Self {
        Self {
            top: scroll_offset - message_top,
            bottom: scroll_offset + viewport_height - message_top,
        }
    }
}

pub(super) fn build_header(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    provider_label: String,
    model_label: String,
    mode_key: &str,
    width: f32,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let title = panel
        .history
        .active_index
        .and_then(|index| panel.history.sessions.get(index))
        .map(|session| short_text(&session.title, 54))
        .unwrap_or_else(|| t("app.agent_title", settings.language));
    let (model_width, mode_width) = selector_widths_with_variant(width, variant);
    let mut header = UiNode::new("agent.header", UiNodeKind::Toolbar)
        .with_class("agent-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 7.0),
            ..UiLayout::fixed(0.0, 52.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(icon_button(
            "agent.sidebar.toggle",
            if panel.sidebar_open {
                UiIconId::ChevronLeft
            } else {
                UiIconId::Menu
            },
            "app.agent_sessions",
            "agent.sidebar.toggle",
        ));
    if width >= 420.0 {
        header = header.with_child(
            UiNode::new("agent.header.icon", UiNodeKind::Image)
                .with_icon(UiIcon::new(UiIconId::Agent).with_size(UiIconSize::Small))
                .with_layout(UiLayout::fixed(20.0, 20.0)),
        );
    }
    if width >= 360.0 {
        header = header.with_child(
            UiNode::new("agent.header.title", UiNodeKind::Label)
                .with_text_value(title)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    width_mode: UiSizeMode::Fill,
                    ..UiLayout::fit_content()
                }),
        );
    }
    header = header
        .with_child(model_selector_button(
            "agent.model.trigger",
            &model_label,
            &provider_label,
            panel.model_menu_open,
            model_width,
        ))
        .with_child(mode_selector_button(
            "agent.mode.trigger",
            settings,
            mode_key,
            panel.mode_menu_open,
            mode_width,
        ));
    if variant == AgentSurfaceVariant::Dock
        && width >= 560.0
        && settings.agent_mode != AgentMode::Active
    {
        header = header.with_child(review_chip(palette, panel, REVIEW_CHIP_WIDTH));
    }
    header = header.with_child(icon_button(
        "agent.settings",
        UiIconId::Settings,
        "app.agent_settings",
        "agent.open-settings",
    ));

    if width < 620.0 {
        header = header.with_class("agent-header-compact");
    }
    header
}

pub(super) fn review_chip(palette: StudioUiPalette, panel: &AgentPanel, width: f32) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let (class, text_key, tooltip_key, accent) = if panel.review_writes {
        (
            "agent-review-on",
            "agent_workspace.review",
            "agent_workspace.review_enabled",
            tokens.positive,
        )
    } else {
        (
            "agent-review-off",
            "agent_workspace.review",
            "agent_workspace.review_disabled",
            tokens.warning,
        )
    };
    UiNode::new("agent.review.chip", UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(10.0, 5.0),
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fixed,
            basis: [width, 32.0],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new("agent.review.dot", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(8.0, 8.0))
                .with_style(UiStyle {
                    fill: accent,
                    radius: 4.0,
                    ..UiStyle::transparent()
                }),
        )
        .with_child(
            UiNode::new("agent.review.label", UiNodeKind::Label)
                .with_text_key(text_key)
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    width_mode: UiSizeMode::Fill,
                    ..UiLayout::fit_content()
                }),
        )
        .with_tooltip_key(tooltip_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "agent.review.toggle",
        ))
}

pub(super) fn status_label(
    palette: StudioUiPalette,
    id: &str,
    key: &str,
    icon: UiIconId,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    UiNode::new(id, UiNodeKind::Label)
        .with_class("agent-status")
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_text_overflow(UiTextOverflow::Wrap)
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 5.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
}

pub(super) fn selector_widths_with_variant(width: f32, variant: AgentSurfaceVariant) -> (f32, f32) {
    if variant == AgentSurfaceVariant::Dock {
        if width < 360.0 {
            (108.0, 112.0)
        } else if width < 560.0 {
            (116.0, 104.0)
        } else {
            (118.0, 102.0)
        }
    } else if width < 360.0 {
        (108.0, 112.0)
    } else if width < 560.0 {
        (132.0, 116.0)
    } else {
        (168.0, 124.0)
    }
}

pub(super) fn icon_button(id: &str, icon: UiIconId, tooltip_key: &str, command: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("agent-icon-button")
        .with_layout(UiLayout::fixed(30.0, 30.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(super) fn action_button(id: &str, text_key: &str, command: &str, primary: bool) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if primary {
            "agent-button-primary"
        } else {
            "agent-button"
        })
        .with_layout(UiLayout {
            padding: UiSpacing::xy(12.0, 7.0),
            ..UiLayout::fit_content()
        })
        .with_text_key(text_key)
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(super) fn copy_message_button(id: String, text: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("agent-copy-button")
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fit_content()
        })
        .with_text_key("app.agent_copy")
        .with_tooltip_key("app.agent_copy")
        .with_accessibility_label_key("app.agent_copy")
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetClipboard {
                text: text.to_string(),
            },
        })
}

pub(super) fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
        .with_style(UiStyle::transparent())
}

pub(super) fn bounded_text(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push('\u{2026}');
    }
    result
}

pub(super) fn short_text(value: &str, max_chars: usize) -> String {
    bounded_text(value.trim(), max_chars)
}

pub(super) fn agent_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    UiStyleSheet {
        rules: vec![
            style(
                "agent-sidebar",
                tokens.surface,
                tokens.border,
                tokens.text,
                1.0,
                0.0,
            ),
            style(
                "agent-main",
                tokens.background,
                tokens.border,
                tokens.text,
                0.0,
                0.0,
            ),
            style(
                "agent-header",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
                1.0,
                0.0,
            ),
            style(
                "agent-sidebar-header",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
                1.0,
                0.0,
            ),
            style(
                "agent-composer",
                tokens.surface,
                tokens.border,
                tokens.text,
                1.0,
                0.0,
            ),
            style(
                "agent-input",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
                1.0,
                4.0,
            ),
            style(
                "agent-status",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
                1.0,
                4.0,
            ),
            style(
                "agent-live-activity",
                tokens.surface_alt,
                tokens.accent,
                tokens.text,
                1.0,
                5.0,
            ),
            style(
                "agent-tool-call-warning",
                tokens.surface_raised,
                tokens.warning,
                tokens.text,
                1.0,
                5.0,
            ),
            style(
                "agent-active-warning",
                tokens.surface_alt,
                tokens.warning,
                tokens.warning,
                1.0,
                4.0,
            ),
            style(
                "agent-approval",
                tokens.surface_alt,
                tokens.warning,
                tokens.text,
                1.0,
                4.0,
            ),
            style(
                "agent-error",
                tokens.surface_alt,
                tokens.danger,
                tokens.danger,
                1.0,
                4.0,
            ),
            style(
                "agent-empty",
                tokens.surface,
                tokens.border,
                tokens.text,
                1.0,
                6.0,
            ),
            style(
                "agent-message-assistant",
                [0; 4],
                tokens.border,
                tokens.text,
                0.0,
                6.0,
            ),
            style(
                "agent-message-user",
                tokens.surface_raised,
                tokens.accent,
                tokens.text,
                0.0,
                6.0,
            ),
            style(
                "agent-message-tool",
                tokens.surface_alt,
                tokens.positive,
                tokens.text,
                0.0,
                4.0,
            ),
            style(
                "agent-message-system",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
                0.0,
                4.0,
            ),
            style(
                "agent-menu",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
                1.0,
                6.0,
            ),
            style(
                "agent-form-field",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
                1.0,
                4.0,
            ),
            style(
                "agent-form-input",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
                1.0,
                4.0,
            ),
            style(
                "agent-button",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
                1.0,
                4.0,
            ),
            style(
                "agent-button-primary",
                tokens.accent,
                tokens.accent_hot,
                tokens.background,
                1.0,
                4.0,
            ),
            style(
                "agent-icon-button",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
                1.0,
                4.0,
            ),
            style(
                "agent-copy-button",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
                1.0,
                4.0,
            ),
            style(
                "agent-selector",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
                1.0,
                4.0,
            ),
            style(
                "agent-session",
                tokens.surface,
                tokens.border,
                tokens.text_muted,
                0.0,
                4.0,
            ),
            style(
                "agent-session-active",
                tokens.surface_raised,
                tokens.accent,
                tokens.text,
                0.0,
                4.0,
            ),
            style(
                "agent-menu-item",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
                0.0,
                4.0,
            ),
            style(
                "agent-menu-item-accent",
                tokens.surface_raised,
                tokens.accent,
                tokens.accent_hot,
                1.0,
                4.0,
            ),
            style(
                "agent-suggestion",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
                1.0,
                4.0,
            ),
            style(
                "agent-metrics",
                tokens.surface,
                tokens.border,
                tokens.text_muted,
                0.0,
                0.0,
            ),
            style(
                "agent-tool-row",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
                0.0,
                6.0,
            ),
            style(
                "agent-tool-chip",
                tokens.surface_raised,
                tokens.border,
                tokens.text_muted,
                0.0,
                8.0,
            ),
            style(
                "agent-review-on",
                tokens.surface_raised,
                tokens.positive,
                tokens.text,
                1.0,
                4.0,
            ),
            style(
                "agent-review-off",
                tokens.surface_raised,
                tokens.warning,
                tokens.warning,
                1.0,
                4.0,
            ),
            style(
                "agent-mode-inspect",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
                1.0,
                4.0,
            ),
            style(
                "agent-mode-plan",
                tokens.surface_alt,
                tokens.warning,
                tokens.warning,
                1.0,
                4.0,
            ),
            style(
                "agent-mode-active",
                tokens.surface_alt,
                tokens.accent_hot,
                tokens.accent_hot,
                1.0,
                4.0,
            ),
            state_patch(
                "agent-button",
                UiStyleRuleState::Hovered,
                tokens.surface_raised,
                tokens.accent_hot,
            ),
            state_patch(
                "agent-button-primary",
                UiStyleRuleState::Hovered,
                tokens.accent_hot,
                tokens.accent_hot,
            ),
            state_patch(
                "agent-icon-button",
                UiStyleRuleState::Hovered,
                tokens.surface_raised,
                tokens.accent_hot,
            ),
            state_patch(
                "agent-copy-button",
                UiStyleRuleState::Hovered,
                tokens.surface_raised,
                tokens.accent_hot,
            ),
            state_patch(
                "agent-selector",
                UiStyleRuleState::Hovered,
                tokens.surface_raised,
                tokens.accent_hot,
            ),
            state_patch(
                "agent-menu-item",
                UiStyleRuleState::Hovered,
                tokens.surface,
                tokens.accent_hot,
            ),
            state_patch(
                "agent-suggestion",
                UiStyleRuleState::Hovered,
                tokens.surface_raised,
                tokens.accent_hot,
            ),
            state_patch(
                "agent-button",
                UiStyleRuleState::Focused,
                tokens.surface_raised,
                tokens.focus,
            ),
            state_patch(
                "agent-button-primary",
                UiStyleRuleState::Focused,
                tokens.accent_hot,
                tokens.focus,
            ),
            state_patch(
                "agent-icon-button",
                UiStyleRuleState::Focused,
                tokens.surface_raised,
                tokens.focus,
            ),
            state_patch(
                "agent-copy-button",
                UiStyleRuleState::Focused,
                tokens.surface_raised,
                tokens.focus,
            ),
            state_patch(
                "agent-selector",
                UiStyleRuleState::Focused,
                tokens.surface_raised,
                tokens.focus,
            ),
            state_patch(
                "agent-input",
                UiStyleRuleState::Focused,
                tokens.surface_alt,
                tokens.focus,
            ),
            state_patch(
                "agent-form-input",
                UiStyleRuleState::Focused,
                tokens.surface_alt,
                tokens.focus,
            ),
            state_patch(
                "agent-menu-item",
                UiStyleRuleState::Selected,
                tokens.surface_alt,
                tokens.accent_hot,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-button-primary".to_string()),
                UiStylePatch {
                    text: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Disabled),
        ],
    }
}

pub(super) fn style(
    class: &str,
    fill: [u8; 4],
    border: [u8; 4],
    text: [u8; 4],
    border_width: f32,
    radius: f32,
) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(border_width),
            radius: Some(radius),
            ..UiStylePatch::default()
        },
    )
}

pub(super) fn state_patch(
    class: &str,
    state: UiStyleRuleState,
    fill: [u8; 4],
    border: [u8; 4],
) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            border_width: Some(1.0),
            ..UiStylePatch::default()
        },
    )
    .when(state)
}
