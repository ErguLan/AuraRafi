//! Tool approval, progress and warning widgets for the Agent surface.

use raf_ai::agent_runtime::{AgentActivitySnapshot, AgentStatus};
use raf_core::ai::AgentMode;
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiFlow, UiIcon, UiIconId, UiIconSize,
    UiLayout, UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiStyle, UiTextOverflow, UiTextStyle,
};

use crate::panels::ai_chat::{AgentPanel, AgentReadiness};

use super::layout::{action_button, icon_button, short_text, status_label, AgentSurfaceVariant};

pub(super) fn build_status_strip(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    readiness: AgentReadiness,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let mut strip = UiNode::new("agent.status.strip", UiNodeKind::Panel)
        .with_class("agent-status-strip")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(12.0, 6.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });

    if panel.opening {
        return strip.with_child(
            UiNode::new("agent.context.loading", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 10.0,
                    ..UiLayout::fixed(0.0, 38.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(crate::agent_workspace_surface::loading_ring(
                    palette,
                    panel.spinner_step,
                ))
                .with_child(
                    UiNode::new("agent.context.loading.label", UiNodeKind::Label)
                        .with_text_key("agent_workspace.preparing_chat")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content()),
                ),
        );
    }
    if !panel.document_name.is_empty() {
        strip = strip.with_child(
            UiNode::new("agent.context.document", UiNodeKind::Label)
                .with_text_value(format!(
                    "{} \u{00b7} {}",
                    panel.document_name,
                    if panel.saving {
                        Some(t("agent_workspace.saving", settings.language))
                    } else {
                        panel.saved_revision.map(|revision| {
                            format!(
                                "{} {revision}",
                                t("agent_workspace.saved", settings.language)
                            )
                        })
                    }
                    .unwrap_or_else(|| t("agent_workspace.autosave", settings.language))
                ))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    if let Some(error) = &panel.save_error {
        strip = strip.with_child(
            UiNode::new("agent.save.error", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 6.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("agent.save.error.text", UiNodeKind::Label)
                        .with_text_value(crate::agent_workspace_surface::localized_error(
                            error,
                            settings.language,
                        ))
                        .with_text_style(UiTextStyle::body(tokens.warning))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(action_button(
                    "agent.save.retry",
                    "agent_workspace.retry_save",
                    "agent.workspace.save.retry",
                    true,
                )),
        );
    }

    match readiness {
        AgentReadiness::Ready => {}
        AgentReadiness::ProviderDisabled => {
            strip = strip.with_child(status_label(
                palette,
                "agent.status.provider-disabled",
                "app.agent_provider_disabled",
                UiIconId::Warning,
            ))
        }
        AgentReadiness::ModelMissing => {
            strip = strip.with_child(status_label(
                palette,
                "agent.status.model-missing",
                "app.agent_model_missing",
                UiIconId::Warning,
            ))
        }
        AgentReadiness::AdapterRequired => {
            strip = strip.with_child(status_label(
                palette,
                "agent.status.adapter-required",
                "app.agent_provider_adapter_required",
                UiIconId::Warning,
            ))
        }
    }

    if settings.agent_mode == AgentMode::Active {
        let mut warning = UiNode::new("agent.active-warning", UiNodeKind::Label)
            .with_class("agent-active-warning")
            .with_text_key("app.agent_active_mode_warning")
            .with_text_style(UiTextStyle::body(tokens.warning))
            .with_icon(UiIcon::new(UiIconId::Warning).with_size(UiIconSize::Small))
            .with_layout(UiLayout {
                padding: UiSpacing::xy(8.0, 6.0),
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            });
        if variant == AgentSurfaceVariant::Dock {
            warning = warning
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_tooltip_key("app.agent_active_mode_warning");
        } else {
            warning = warning.with_text_overflow(UiTextOverflow::Wrap);
        }
        strip = strip.with_child(warning);
    }

    match panel.runtime.status {
        AgentStatus::Done => {
            if panel.runtime.task_snapshot().is_none()
                && panel
                    .history
                    .active_session()
                    .and_then(|session| session.last_task.as_ref())
                    .is_some_and(|task| !task.status.is_terminal())
            {
                strip = strip.with_child(
                    UiNode::new("agent.interrupted", UiNodeKind::Toolbar)
                        .with_layout(UiLayout {
                            flow: UiFlow::RowWrap,
                            gap: 8.0,
                            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                        })
                        .with_child(
                            UiNode::new("agent.interrupted.label", UiNodeKind::Label)
                                .with_text_key("agent_workspace.interrupted")
                                .with_text_style(UiTextStyle::body(tokens.warning))
                                .with_layout(
                                    UiLayout::fit_content().with_width_mode(UiSizeMode::Fill),
                                ),
                        )
                        .with_child(action_button(
                            "agent.interrupted.resume",
                            "agent_workspace.resume",
                            "agent.resume",
                            true,
                        )),
                );
            }
        }
        AgentStatus::Thinking | AgentStatus::ExecutingTools => {
            if let Some(activity) = panel.runtime.activity_snapshot() {
                strip =
                    strip.with_child(build_live_activity(palette, settings.language, &activity));
                if panel.should_show_tool_call_warning() {
                    strip = strip.with_child(build_tool_call_warning(
                        palette,
                        settings.language,
                        activity.completed_tools,
                    ));
                }
            }
        }
        AgentStatus::AwaitingApproval
            if settings.agent_mode == AgentMode::Active || variant == AgentSurfaceVariant::Dock => {
        }
        AgentStatus::AwaitingApproval => {
            if let Some(call) = panel
                .runtime
                .pending_calls
                .iter()
                .find(|call| call.result.is_none())
            {
                let target = ["target", "root", "id", "parent", "name"]
                    .iter()
                    .find_map(|key| call.arguments.get(*key))
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .unwrap_or_else(|| panel.document_name.clone());
                let operation_count = call
                    .arguments
                    .get("operations")
                    .and_then(|v| v.as_array())
                    .map_or(1, Vec::len);
                strip = strip.with_child(
                    UiNode::new("agent.approval.operation", UiNodeKind::Panel)
                        .with_layout(UiLayout {
                            flow: UiFlow::Column,
                            gap: 6.0,
                            padding: UiSpacing::same(10.0),
                            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                        })
                        .with_style(UiStyle {
                            fill: tokens.surface_raised,
                            radius: 8.0,
                            ..UiStyle::transparent()
                        })
                        .with_child(
                            UiNode::new("agent.approval.operation.title", UiNodeKind::Label)
                                .with_text_value(call.name.clone())
                                .with_text_style(UiTextStyle::button(tokens.text))
                                .with_layout(UiLayout::fit_content()),
                        )
                        .with_child(
                            UiNode::new("agent.approval.operation.target", UiNodeKind::Label)
                                .with_text_value(format!(
                                    "{}: {} \u{00b7} {operation_count} {}",
                                    t("agent_workspace.target", settings.language),
                                    short_text(&target, 180),
                                    t("agent_workspace.operations", settings.language)
                                ))
                                .with_text_style(UiTextStyle::body(tokens.text_muted))
                                .with_layout(
                                    UiLayout::fit_content().with_width_mode(UiSizeMode::Fill),
                                ),
                        )
                        .with_child(
                            UiNode::new("agent.approval.operation.revision", UiNodeKind::Label)
                                .with_text_value(format!(
                                    "{} {}",
                                    t("agent_workspace.revision", settings.language),
                                    call.expected_revision
                                        .map(|r| r.to_string())
                                        .unwrap_or_else(|| "\u{2014}".into())
                                ))
                                .with_text_style(UiTextStyle::body(tokens.text_muted))
                                .with_layout(UiLayout::fit_content()),
                        )
                        .with_child(
                            UiNode::new("agent.approval.operation.preview", UiNodeKind::Label)
                                .with_text_value(
                                    call.review_preview
                                        .as_ref()
                                        .map(|preview| {
                                            format!(
                                                "{}\n{}",
                                                preview.summary,
                                                short_text(
                                                    &preview
                                                        .diff
                                                        .as_ref()
                                                        .unwrap_or(&preview.data)
                                                        .to_string(),
                                                    1200
                                                )
                                            )
                                        })
                                        .unwrap_or_else(|| {
                                            short_text(
                                                &serde_json::to_string_pretty(&call.arguments)
                                                    .unwrap_or_default(),
                                                1200,
                                            )
                                        }),
                                )
                                .with_text_overflow(UiTextOverflow::Wrap)
                                .selectable_text()
                                .with_text_style(UiTextStyle::body(tokens.text_muted))
                                .with_layout(
                                    UiLayout::fit_content().with_width_mode(UiSizeMode::Fill),
                                ),
                        )
                        .with_child(
                            UiNode::new("agent.approval.operation.actions", UiNodeKind::Toolbar)
                                .with_layout(UiLayout {
                                    flow: UiFlow::Row,
                                    gap: 8.0,
                                    ..UiLayout::fit_content()
                                })
                                .with_child(action_button(
                                    "agent.approval.once",
                                    "agent_workspace.approve_once",
                                    &format!("agent.call.approve:{}", call.id),
                                    true,
                                ))
                                .with_child(action_button(
                                    "agent.approval.skip",
                                    "agent_workspace.reject_operation",
                                    &format!("agent.call.deny:{}", call.id),
                                    false,
                                )),
                        ),
                );
            }
            let count = panel.runtime.pending_calls.len();
            strip = strip.with_child(
                UiNode::new("agent.approval", UiNodeKind::Panel)
                    .with_class("agent-approval")
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 8.0,
                        padding: UiSpacing::xy(8.0, 6.0),
                        min_size: [0.0, 34.0],
                        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(
                        UiNode::new("agent.approval.label", UiNodeKind::Label)
                            .with_text_value(format!(
                                "{} ({count})",
                                t("app.agent_pending_tools", settings.language)
                            ))
                            .with_text_style(UiTextStyle::body(tokens.warning))
                            .with_layout(UiLayout {
                                grow: 1.0,
                                width_mode: UiSizeMode::Fill,
                                ..UiLayout::fit_content()
                            }),
                    )
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
                    )),
            );
        }
        AgentStatus::Error => {
            let error = panel
                .runtime
                .last_error
                .as_deref()
                .map(|value| short_text(value, 600))
                .unwrap_or_else(|| t("app.agent_error_label", settings.language));
            strip = strip.with_child(
                UiNode::new("agent.error", UiNodeKind::Panel)
                    .with_class("agent-error")
                    .with_layout(UiLayout {
                        min_size: [0.0, 30.0],
                        ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                    })
                    .with_child(
                        UiNode::new("agent.error.label", UiNodeKind::Label)
                            .with_text_value(format!(
                                "{}: {error}",
                                t("app.agent_error_label", settings.language)
                            ))
                            .with_text_style(UiTextStyle::body(tokens.danger))
                            .with_text_overflow(UiTextOverflow::Wrap)
                            .with_icon(UiIcon::new(UiIconId::Warning).with_size(UiIconSize::Small))
                            .with_layout(UiLayout {
                                padding: UiSpacing::xy(8.0, 6.0),
                                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                            }),
                    ),
            );
        }
    }

    strip
}

pub(super) fn build_live_activity(
    palette: StudioUiPalette,
    language: Language,
    activity: &raf_ai::agent_runtime::AgentActivitySnapshot,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let tool = activity.current_tool.as_deref();
    let activity_key = match tool {
        Some("external_studio_install") => "app.agent_studio_installing",
        Some("asset_generate_image" | "asset_generate_local_png" | "external_studio_generate") => {
            "app.agent_asset_generating"
        }
        Some(name) if name.starts_with("assets_") || name == "asset_inspect" => {
            "app.agent_activity_assets"
        }
        Some("viewport_capture") => "app.agent_activity_viewport",
        Some(name)
            if name.contains("verify")
                || name.contains("audit")
                || name.contains("overlap")
                || name.contains("spatial") =>
        {
            "app.agent_activity_verifying"
        }
        Some(name)
            if name.starts_with("scene_")
                && !matches!(name, "scene_outline" | "scene_query" | "scene_inspect") =>
        {
            "app.agent_activity_building"
        }
        Some(_) => "app.agent_activity_inspecting",
        None if activity.completed_tools > 0 => "app.agent_activity_reviewing",
        None => "app.agent_activity_thinking",
    };
    let icon = match tool {
        Some(name)
            if name.starts_with("asset_generate_") || name.starts_with("external_studio_") =>
        {
            UiIconId::Assets
        }
        Some(name) if name.starts_with("assets_") || name == "asset_inspect" => UiIconId::Assets,
        Some("viewport_capture") => UiIconId::Eye,
        Some(name) if name.contains("verify") || name.contains("audit") => UiIconId::Success,
        Some(name) if name.starts_with("scene_") => UiIconId::Scene,
        _ => UiIconId::Agent,
    };
    let mut details = Vec::new();
    if let Some(tool) = tool {
        details.push(tool.to_string());
    }
    if activity.total_tools > 0 {
        details.push(format!(
            "{}/{} {}",
            activity.completed_tools,
            activity.total_tools,
            t("app.agent_activity_tools", language)
        ));
    } else {
        details.push(format!(
            "{} {}",
            t("app.agent_activity_turn", language),
            activity.turn.max(1)
        ));
    }
    UiNode::new("agent.live-activity", UiNodeKind::Panel)
        .with_class("agent-live-activity")
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_accessibility_label_key(activity_key)
        .with_accessibility_description_key("app.agent_activity_live_hint")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(10.0, 7.0),
            min_size: [0.0, 46.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("agent.live-activity.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(icon).with_size(UiIconSize::Panel))
                .with_text_value("")
                .with_layout(UiLayout::fixed(24.0, 24.0)),
        )
        .with_child(
            UiNode::new("agent.live-activity.copy", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    width_mode: UiSizeMode::Fill,
                    gap: 2.0,
                    ..UiLayout::fit_content()
                })
                .with_child(
                    UiNode::new("agent.live-activity.title", UiNodeKind::Label)
                        .with_text_key(activity_key)
                        .with_text_style(UiTextStyle::body(tokens.text))
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout {
                            width_mode: UiSizeMode::Fill,
                            ..UiLayout::fit_content()
                        }),
                )
                .with_child(
                    UiNode::new("agent.live-activity.detail", UiNodeKind::Label)
                        .with_text_value(details.join("  |  "))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout {
                            width_mode: UiSizeMode::Fill,
                            ..UiLayout::fit_content()
                        }),
                ),
        )
        .with_child(
            UiNode::new("agent.live-activity.elapsed", UiNodeKind::Label)
                .with_text_value(format_elapsed(activity.elapsed_seconds))
                .with_text_style(UiTextStyle::body(tokens.accent_hot))
                .with_layout(UiLayout::fixed(54.0, 22.0)),
        )
}

pub(super) fn build_tool_call_warning(
    palette: StudioUiPalette,
    language: Language,
    completed_tools: usize,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    UiNode::new("agent.tool-call-warning", UiNodeKind::Panel)
        .with_class("agent-tool-call-warning")
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_accessibility_label_key("app.agent_tool_call_warning")
        .with_accessibility_description_key("app.agent_tool_warning_hint")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::xy(10.0, 7.0),
            min_size: [0.0, 62.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("agent.tool-call-warning.top", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("agent.tool-call-warning.icon", UiNodeKind::Label)
                        .with_icon(UiIcon::new(UiIconId::Warning).with_size(UiIconSize::Small))
                        .with_text_value("")
                        .with_layout(UiLayout::fixed(22.0, 22.0)),
                )
                .with_child(
                    UiNode::new("agent.tool-call-warning.copy", UiNodeKind::Panel)
                        .with_layout(UiLayout {
                            flow: UiFlow::Column,
                            grow: 1.0,
                            width_mode: UiSizeMode::Fill,
                            gap: 2.0,
                            ..UiLayout::fit_content()
                        })
                        .with_child(
                            UiNode::new("agent.tool-call-warning.title", UiNodeKind::Label)
                                .with_text_value(format!(
                                    "{} ({completed_tools})",
                                    t("app.agent_tool_call_warning", language)
                                ))
                                .with_text_style(UiTextStyle::body(tokens.warning))
                                .with_text_overflow(UiTextOverflow::Ellipsis)
                                .with_layout(UiLayout {
                                    width_mode: UiSizeMode::Fill,
                                    ..UiLayout::fit_content()
                                }),
                        )
                        .with_child(
                            UiNode::new("agent.tool-call-warning.hint", UiNodeKind::Label)
                                .with_text_key("app.agent_tool_warning_hint")
                                .with_text_style(UiTextStyle::body(tokens.text_muted))
                                .with_text_overflow(UiTextOverflow::Wrap)
                                .with_layout(UiLayout {
                                    width_mode: UiSizeMode::Fill,
                                    ..UiLayout::fit_content()
                                }),
                        ),
                )
                .with_child(icon_button(
                    "agent.tool-call-warning.dismiss",
                    UiIconId::Close,
                    "app.agent_tool_warning_dismiss",
                    "agent.tool-warning.dismiss",
                )),
        )
        .with_child(
            UiNode::new("agent.tool-call-warning.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    gap: 6.0,
                    ..UiLayout::fit_content()
                })
                .with_child(action_button(
                    "agent.tool-call-warning.settings",
                    "app.agent_tool_warning_settings",
                    "agent.open-settings",
                    false,
                ))
                .with_child(action_button(
                    "agent.tool-call-warning.stop",
                    "app.agent_tool_warning_stop",
                    "agent.stop",
                    false,
                )),
        )
}

pub(super) fn format_elapsed(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}
