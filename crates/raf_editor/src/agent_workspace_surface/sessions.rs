//! Project rail and project residency rows.

use super::*;
use super::layout::label;

pub(super) fn projects_rail(
    tokens: &UiTokens,
    service: &AgentService,
    projects: &[HubSurfaceProject],
    settings: &EngineSettings,
    rail: f32,
    height: f32,
) -> UiNode {
    let mut rail_node = UiNode::scroll_view("agent.workspace.projects", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(10.0, 16.0),
            ..UiLayout::absolute(UiRect::new(0.0, 52.0, rail, height - 52.0))
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            ..UiStyle::transparent()
        })
        .with_child(label(
            "agent.workspace.projects.title",
            "agent_workspace.projects",
            tokens,
        ))
        .with_child(
            button(
                "agent.workspace.start-all",
                "agent_workspace.start_all",
                "agent.workspace.start-all",
                tokens,
            )
            .disabled(projects.is_empty()),
        )
        .with_child(
            button(
                "agent.workspace.close-all",
                "agent_workspace.close_all",
                "agent.workspace.close-all",
                tokens,
            )
            .disabled(!service.has_projects()),
        );
    for (index, project) in projects.iter().enumerate().take(128) {
        let selected = service.is_selected(&project.path);
        let open = service.is_open(&project.path);
        let loading = service.is_loading(&project.path);
        let closing = service.is_closing(&project.path);
        let status_key = if closing {
            "agent_workspace.closing"
        } else if loading {
            "agent_workspace.loading"
        } else if open {
            "agent_workspace.loaded"
        } else {
            "agent_workspace.unloaded"
        };
        let mut row = UiNode::new(
            format!("agent.workspace.project.{index}"),
            UiNodeKind::Button,
        )
        .with_class("agent-workspace-project")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(10.0, 9.0),
            ..UiLayout::fixed(rail - 48.0, 54.0)
        })
        .with_style(UiStyle {
            fill: if selected {
                tokens.surface_raised
            } else {
                [0; 4]
            },
            radius: 6.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_selected(selected)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::Custom {
                channel: "agent.workspace.project".into(),
                payload: serde_json::json!({"path": project.path}),
            },
        })
        .with_child(
            UiNode::new(
                format!("agent.workspace.project.{index}.heading"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 6.0,
                align_items: UiAlign::Center,
                ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_child(
                UiNode::new(
                    format!("agent.workspace.project.{index}.status"),
                    UiNodeKind::Panel,
                )
                .with_layout(UiLayout::fixed(7.0, 7.0))
                .with_style(UiStyle {
                    fill: if closing || loading {
                        tokens.warning
                    } else if open {
                        tokens.positive
                    } else {
                        tokens.text_muted
                    },
                    radius: 3.5,
                    ..UiStyle::transparent()
                })
                .with_accessibility_role(UiAccessibilityRole::Status)
                .with_accessibility_label_key(status_key)
                .with_tooltip_key(status_key),
            )
            .with_child(
                UiNode::new(
                    format!("agent.workspace.project.{index}.name"),
                    UiNodeKind::Label,
                )
                .with_text_value(project.name.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed((rail - 87.0).max(1.0), 20.0)
                }),
            ),
        );
        if let Some(workspace) = service.workspace(&project.path) {
            let panel = workspace.panel.borrow();
            row = row.with_child(
                UiNode::new(
                    format!("agent.workspace.project.{index}.detail"),
                    UiNodeKind::Label,
                )
                .with_text_value(format!(
                    "{} · {}",
                    panel.history.sessions.len(),
                    t(
                        if closing {
                            "agent_workspace.closing"
                        } else if panel.has_pending_review() {
                            "agent_workspace.awaiting_review"
                        } else if panel.has_live_output() {
                            "agent_workspace.running"
                        } else {
                            "agent_workspace.conversations"
                        },
                        settings.language
                    )
                ))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
            );
        } else {
            row = row.with_child(
                label(
                    &format!("agent.workspace.project.{index}.detail"),
                    status_key,
                    tokens,
                )
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
            );
        }
        let close = button(
            &format!("agent.workspace.project.{index}.close"),
            "agent_workspace.close_project",
            "agent.workspace.close-project",
            tokens,
        );
        let close = UiNode {
            text_key: None,
            layout: UiLayout::fixed(24.0, 28.0),
            event_handlers: vec![UiEventBinding {
                event: UiEventKind::Click,
                action: UiAction::Custom {
                    channel: "agent.workspace.close-project".into(),
                    payload: serde_json::json!({"path": project.path}),
                },
            }],
            ..close
        }
        .with_icon(UiIcon::new(UiIconId::Close))
        .with_tooltip_key("agent_workspace.close_project")
        .disabled((!open && !loading) || closing);
        rail_node = rail_node.with_child(
            UiNode::new(
                format!("agent.workspace.project.{index}.row"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 4.0,
                align_items: UiAlign::Center,
                ..UiLayout::fixed(rail - 20.0, 54.0)
            })
            .with_child(row)
            .with_child(close),
        );
    }
    rail_node
}
