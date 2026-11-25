//! Input composer, attachments and approval controls for the Agent surface.

use raf_ai::agent_runtime::AgentStatus;
use raf_core::ai::AgentMode;
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon,
    UiIconId, UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiSizeMode,
    UiSpacing, UiStyle, UiTextOverflow, UiTextStyle,
};

use crate::panels::ai_chat::{AgentPanel, AgentReadiness};

use super::layout::{
    action_button, icon_button, short_text, AgentSurfaceVariant, APPROVAL_COMPOSER_HEIGHT,
    DOCK_COMPOSER_HEIGHT, MAX_VISIBLE_MESSAGE_CHARS,
};

pub(super) fn build_composer(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    readiness: AgentReadiness,
    width: f32,
    message_count: usize,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    if variant == AgentSurfaceVariant::Dock
        && settings.agent_mode != AgentMode::Active
        && panel.runtime.status == AgentStatus::AwaitingApproval
        && !panel.runtime.pending_calls.is_empty()
    {
        return dock_approval_composer(palette, panel, settings, width);
    }
    let running = matches!(
        panel.runtime.status,
        AgentStatus::Thinking | AgentStatus::ExecutingTools | AgentStatus::AwaitingApproval
    );
    let narrow = variant == AgentSurfaceVariant::Dock && width < 560.0;
    let composer = UiNode::new("agent.composer", UiNodeKind::Toolbar)
        .with_class("agent-composer")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(12.0, 8.0),
            ..UiLayout::fixed(
                0.0,
                if variant == AgentSurfaceVariant::Dock && !narrow {
                    DOCK_COMPOSER_HEIGHT + composer_media_height(panel)
                } else {
                    (if width < 500.0 { 180.0 } else { 168.0 }) + composer_media_height(panel)
                },
            )
            .with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("agent.composer.row", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    gap: 8.0,
                    align_items: UiAlign::End,
                    ..UiLayout::fixed(0.0, 62.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::text_input(
                        "agent.input",
                        raf_render::api_graphic_basic::ui_surface::UiTextInput {
                            value_key: "agent.input".to_string(),
                            placeholder_key: Some("app.agent_input_placeholder".to_string()),
                            max_length: MAX_VISIBLE_MESSAGE_CHARS,
                            multiline: true,
                            password: false,
                            submit_command: None,
                        },
                    )
                    .with_class("agent-input")
                    .with_layout(UiLayout {
                        grow: 1.0,
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 62.0],
                        padding: UiSpacing::xy(10.0, 8.0),
                        ..UiLayout::default()
                    })
                    .with_text_style(UiTextStyle::body(tokens.text))
                    .with_text_overflow(UiTextOverflow::Wrap)
                    .with_accessibility_role(UiAccessibilityRole::Textbox),
                )
                .with_child(if running {
                    action_button("agent.stop", "app.agent_stop", "agent.stop", false)
                        .with_icon(UiIcon::new(UiIconId::Stop).with_size(UiIconSize::Small))
                } else {
                    action_button("agent.send", "app.agent_send", "agent.submit", true).disabled(
                        !matches!(readiness, AgentReadiness::Ready) || panel.media_pending(),
                    )
                }),
        )
        .with_child(
            UiNode::new("agent.composer.footer", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    justify_content: UiJustify::SpaceBetween,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    icon_button(
                        "agent.image.paste",
                        UiIconId::Add,
                        "agent.media.paste",
                        "agent.image.paste",
                    )
                    .disabled(running || panel.media_pending()),
                )
                .with_child(
                    if settings.agent_openai_use_plan
                        && settings.default_ai_provider == raf_ai::provider::AiProvider::OpenAI
                        && raf_ai::chatgpt_auth::active_connected()
                    {
                        UiNode::new("agent.account.usage", UiNodeKind::Button)
                            .with_class("agent-button")
                            .with_text_key("agent.account.using_plan")
                            .with_tooltip_key("settings.account.usage")
                            .with_accessibility_role(UiAccessibilityRole::Button)
                            .with_text_overflow(UiTextOverflow::Ellipsis)
                            .with_layout(UiLayout::fixed(120.0, 30.0))
                            .focusable()
                            .with_event(UiEventBinding::command(
                                UiEventKind::Click,
                                "agent.account.usage",
                            ))
                    } else {
                        UiNode::new("agent.account.hidden", UiNodeKind::Panel)
                            .with_style(UiStyle::transparent())
                            .with_layout(UiLayout::fixed(0.0, 0.0))
                    },
                )
                .with_child(
                    UiNode::new("agent.reasoning.cycle", UiNodeKind::Button)
                        .with_class("agent-button")
                        .with_text_value(format!(
                            "{}: {}",
                            t("agent.reasoning.short", settings.language),
                            t(
                                match settings.agent_reasoning_effort.as_str() {
                                    "low" => "settings.reasoning.low",
                                    "medium" => "settings.reasoning.medium",
                                    "high" => "settings.reasoning.high",
                                    "none" => "settings.reasoning.none",
                                    "max" => "settings.reasoning.max",
                                    _ => "settings.reasoning.auto",
                                },
                                settings.language
                            )
                        ))
                        .with_tooltip_key("agent.reasoning.cycle")
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_accessibility_label_key("agent.reasoning.cycle")
                        .with_accessibility_role(UiAccessibilityRole::Button)
                        .focusable()
                        .disabled(running)
                        .with_layout(UiLayout::fixed(116.0, 30.0))
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "agent.reasoning.cycle",
                        )),
                )
                .with_child(
                    UiNode::new("agent.composer.metrics", UiNodeKind::Label)
                        .with_class("agent-metrics")
                        .with_text_value(metrics_text(
                            panel,
                            settings.language,
                            if settings.agent_response_limit_enabled {
                                settings.agent_max_response_tokens
                            } else {
                                0
                            },
                            message_count,
                        ))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                        }),
                ),
        )
        .with_child({
            if settings.agent_mode == AgentMode::Active
                || (variant == AgentSurfaceVariant::Dock && !narrow)
            {
                UiNode::new("agent.composer.review.hidden", UiNodeKind::Panel)
                    .with_style(UiStyle::transparent())
                    .with_layout(UiLayout::fixed(0.0, 0.0))
            } else {
                action_button(
                    "agent.review",
                    if panel.review_writes {
                        "agent_workspace.review_enabled"
                    } else {
                        "agent_workspace.review_disabled"
                    },
                    "agent.review.toggle",
                    false,
                )
            }
        });
    composer.with_child(composer_media(palette, panel, running))
}

pub(super) fn composer_media_height(panel: &AgentPanel) -> f32 {
    let count = panel
        .history
        .active_session()
        .map_or(0, |s| s.draft_images.len());
    (count.min(raf_ai::chat_media::MAX_IMAGES) as f32 * 30.0)
        + if panel.media_pending() || panel.media_error.is_some() {
            38.0
        } else {
            0.0
        }
}

pub(super) fn composer_media(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    running: bool,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let mut list = UiNode::new("agent.composer.media", UiNodeKind::Panel).with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 2.0,
        ..UiLayout::fixed(0.0, composer_media_height(panel)).with_width_mode(UiSizeMode::Fill)
    });
    if let Some(session) = panel.history.active_session() {
        for (index, image) in session
            .draft_images
            .iter()
            .take(raf_ai::chat_media::MAX_IMAGES)
            .enumerate()
        {
            list = list.with_child(
                UiNode::new(format!("agent.image.{index}"), UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 6.0,
                        ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child({
                        let mut thumb =
                            UiNode::new(format!("agent.image.{index}.thumb"), UiNodeKind::Image)
                                .with_layout(UiLayout::fixed(52.0, 26.0));
                        raf_ui::components::set_thumbnail_image(
                            &mut thumb,
                            crate::agent_workspace_surface::attachment_image_key(&image.file),
                            raf_ui::UiImageFit::Cover,
                            4.0,
                        );
                        thumb
                    })
                    .with_child(
                        UiNode::new(format!("agent.image.{index}.label"), UiNodeKind::Label)
                            .with_icon(UiIcon::new(UiIconId::Assets).with_size(UiIconSize::Small))
                            .with_text_value(format!(
                                "{} {} · PNG · {} × {}",
                                t("agent.media.attachment", panel.language),
                                index + 1,
                                image.width,
                                image.height
                            ))
                            .with_text_style(UiTextStyle::body(tokens.text_muted))
                            .with_layout(UiLayout {
                                grow: 1.0,
                                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                            }),
                    )
                    .with_child(
                        icon_button(
                            &format!("agent.image.{index}.remove"),
                            UiIconId::Close,
                            "agent.media.remove",
                            &format!("agent.image.remove:{}", image.file),
                        )
                        .disabled(running || panel.media_pending()),
                    ),
            );
        }
    }
    if panel.media_pending() || panel.media_error.is_some() {
        let label = UiNode::new("agent.image.status", UiNodeKind::Label)
            .with_accessibility_role(UiAccessibilityRole::Status)
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fixed(0.0, 36.0).with_width_mode(UiSizeMode::Fill));
        list = list.with_child(if let Some(error) = &panel.media_error {
            label.with_text_value(error)
        } else {
            label.with_text_key("agent.media.preparing")
        });
    }
    list
}

pub(super) fn dock_approval_composer(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    width: f32,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let count = panel.runtime.pending_calls.len();
    let call = panel
        .runtime
        .pending_calls
        .iter()
        .find(|call| call.result.is_none());
    let target = call
        .map(|call| {
            ["target", "root", "id", "parent", "name"]
                .iter()
                .find_map(|key| call.arguments.get(*key))
                .map(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string())
                })
                .unwrap_or_else(|| panel.document_name.clone())
        })
        .unwrap_or_else(|| panel.document_name.clone());
    let operation_count = call
        .and_then(|call| call.arguments.get("operations"))
        .and_then(|v| v.as_array())
        .map_or(1, Vec::len);
    let preview_chars = (((width - 40.0).max(80.0) / 7.4) * 4.3) as usize;
    let preview = call
        .map(|call| {
            call.review_preview
                .as_ref()
                .map(|preview| {
                    format!(
                        "{}\n{}",
                        preview.summary,
                        short_text(
                            &preview.diff.as_ref().unwrap_or(&preview.data).to_string(),
                            preview_chars,
                        )
                    )
                })
                .unwrap_or_else(|| {
                    short_text(
                        &serde_json::to_string_pretty(&call.arguments).unwrap_or_default(),
                        preview_chars,
                    )
                })
        })
        .unwrap_or_default();
    let mut composer = UiNode::new("agent.composer", UiNodeKind::Toolbar)
        .with_class("agent-composer")
        .with_class("agent-approval")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(12.0, 8.0),
            ..UiLayout::fixed(0.0, APPROVAL_COMPOSER_HEIGHT).with_width_mode(UiSizeMode::Fill)
        });
    let mut header = UiNode::new("agent.approval.header", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            basis: [0.0, 26.0],
            ..UiLayout::default()
        })
        .with_child(
            UiNode::new("agent.approval.label", UiNodeKind::Label)
                .with_text_value(format!(
                    "{} ({count})",
                    t("app.agent_pending_tools", settings.language)
                ))
                .with_text_style(UiTextStyle::button(tokens.warning))
                .with_layout(UiLayout {
                    grow: 1.0,
                    width_mode: UiSizeMode::Fill,
                    height_mode: UiSizeMode::Fixed,
                    basis: [0.0, 20.0],
                    ..UiLayout::fit_content()
                })
                .with_text_overflow(UiTextOverflow::Ellipsis),
        );
    header = header
        .with_child(action_button(
            "agent.approval.deny",
            "app.agent_deny",
            "agent.deny",
            false,
        ))
        .with_child(action_button(
            "agent.approval.approve",
            "app.agent_approve",
            "agent.approve",
            true,
        ));
    composer = composer.with_child(header);
    if let Some(call) = call {
        composer = composer
            .with_child(
                UiNode::new("agent.approval.operation.title", UiNodeKind::Label)
                    .with_text_value(call.name.clone())
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_layout(UiLayout {
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 20.0],
                        ..UiLayout::fit_content()
                    }),
            )
            .with_child(
                UiNode::new("agent.approval.operation.target", UiNodeKind::Label)
                    .with_text_value(format!(
                        "{}: {} \u{00b7} {operation_count} {} \u{00b7} {} {}",
                        t("agent_workspace.target", settings.language),
                        short_text(&target, 120),
                        t("agent_workspace.operations", settings.language),
                        t("agent_workspace.revision", settings.language),
                        call.expected_revision
                            .map(|r| r.to_string())
                            .unwrap_or_else(|| "\u{2014}".into())
                    ))
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_tooltip_value(format!(
                        "{}: {} \u{00b7} {operation_count} {} \u{00b7} {} {}",
                        t("agent_workspace.target", settings.language),
                        target,
                        t("agent_workspace.operations", settings.language),
                        t("agent_workspace.revision", settings.language),
                        call.expected_revision
                            .map(|r| r.to_string())
                            .unwrap_or_else(|| "\u{2014}".into())
                    ))
                    .with_layout(UiLayout {
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 18.0],
                        ..UiLayout::fit_content()
                    }),
            )
            .with_child(
                UiNode::new("agent.approval.operation.preview", UiNodeKind::Label)
                    .with_text_value(preview)
                    .with_text_overflow(UiTextOverflow::Wrap)
                    .selectable_text()
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout {
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 84.0],
                        max_size: [width.max(1.0), 84.0],
                        overflow: UiOverflow::Clip,
                        ..UiLayout::default()
                    }),
            );
    }
    composer.with_child(
        UiNode::new("agent.approval.actions", UiNodeKind::Toolbar)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 8.0,
                width_mode: UiSizeMode::Fill,
                height_mode: UiSizeMode::Fixed,
                basis: [0.0, 30.0],
                ..UiLayout::default()
            })
            .with_child(match call {
                Some(call) => action_button(
                    "agent.approval.once",
                    "agent_workspace.approve_once",
                    &format!("agent.call.approve:{}", call.id),
                    false,
                ),
                None => UiNode::new("agent.approval.once", UiNodeKind::Panel)
                    .with_style(UiStyle::transparent())
                    .with_layout(UiLayout::fixed(0.0, 0.0)),
            })
            .with_child(match call {
                Some(call) => action_button(
                    "agent.approval.skip",
                    "agent_workspace.reject_operation",
                    &format!("agent.call.deny:{}", call.id),
                    false,
                ),
                None => UiNode::new("agent.approval.skip", UiNodeKind::Panel)
                    .with_style(UiStyle::transparent())
                    .with_layout(UiLayout::fixed(0.0, 0.0)),
            }),
    )
}

pub(super) fn metrics_text(
    panel: &AgentPanel,
    language: Language,
    max_response_tokens: u32,
    message_count: usize,
) -> String {
    let approx_tokens: usize = panel
        .runtime
        .messages
        .iter()
        .map(|message| message.content.chars().count().saturating_add(3) / 4)
        .sum();
    format!(
        "{}: {message_count}  |  {}: ~{approx_tokens} {}  |  {}: {}",
        t("app.agent_messages_label", language),
        t("app.agent_context_approx", language),
        t("app.agent_tokens", language),
        t("app.agent_response_max", language),
        if max_response_tokens == 0 {
            t("app.agent_output_auto", language)
        } else {
            max_response_tokens.to_string()
        }
    )
}
