//! Workspace header actions, window buttons and bulk confirmation modal.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceConfirmation {
    StartAll,
    CloseAll,
    CloseProject(std::path::PathBuf),
}

pub(super) fn workspace_header(
    tokens: &UiTokens,
    ready: Option<&crate::agent_service::AgentWorkspace>,
    is_lab: bool,
    width: f32,
) -> UiNode {
    let mut header = UiNode::new("agent.workspace.header", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 12.0,
            padding: UiSpacing::xy(18.0, 6.0),
            ..UiLayout::absolute(UiRect::new(0.0, 0.0, width, 52.0))
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            ..UiStyle::transparent()
        })
        .with_child(button(
            "agent.workspace.back",
            if is_lab {
                "lab.back"
            } else {
                "agent_workspace.projects"
            },
            "agent.workspace.back",
            tokens,
        ))
        .with_child(
            UiNode::new("agent.workspace.title", UiNodeKind::Label)
                .with_text_key(if is_lab {
                    "lab.title"
                } else {
                    "agent_workspace.title"
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::DragStart,
                    "window.drag",
                ))
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(
            button(
                "agent.workspace.review",
                "agent_workspace.review",
                "agent.workspace.review",
                tokens,
            )
            .disabled(ready.is_none()),
        )
        .with_child(button(
            "agent.workspace.settings",
            if is_lab {
                "lab.provider"
            } else {
                "app.settings_menu"
            },
            "agent.workspace.settings",
            tokens,
        ))
        .with_child(
            button(
                "agent.workspace.editor",
                "agent_workspace.open_editor",
                "agent.workspace.editor",
                tokens,
            )
            .disabled(ready.is_none()),
        )
        .with_child({
            let mut preview = button(
                "agent.workspace.preview",
                "agent_workspace.preview_3d",
                "agent.workspace.tab.preview",
                tokens,
            )
            .with_icon(UiIcon::new(UiIconId::ScenePreview).with_tint(tokens.text))
            .with_tooltip_key("agent_workspace.preview_3d");
            preview.text_key = None;
            preview.layout = UiLayout::fixed(34.0, 32.0);
            preview.disabled(ready.is_none())
        });
    if is_lab {
        header.children.retain(|child| {
            !matches!(
                child.id.as_str(),
                "agent.workspace.review" | "agent.workspace.preview" | "agent.workspace.editor"
            )
        });
    }
    header = header
        .with_child(window_button(
            "minimize",
            "agent_workspace.minimize",
            UiIconId::ChevronDown,
            tokens,
        ))
        .with_child(window_button(
            "maximize",
            "agent_workspace.maximize",
            UiIconId::Focus,
            tokens,
        ))
        .with_child(window_button(
            "close",
            "agent_workspace.close",
            UiIconId::Close,
            tokens,
        ));
    if let Some(workspace) = ready.filter(|_| width >= 1350.0) {
        header = header.with_child(
            UiNode::new("agent.workspace.scope", UiNodeKind::Label)
                .with_text_value(workspace.project.name.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_layout(UiLayout::fixed(150.0, 24.0)),
        );
    }
    header
}

/// The modal is window-scoped. Background controls leave the focus/hit tree.
pub(crate) fn with_confirmation(
    mut surface: UiSurface,
    size: [f32; 2],
    confirmation: Option<&WorkspaceConfirmation>,
) -> UiSurface {
    let Some(confirmation) = confirmation else {
        return surface;
    };
    fn disable(node: &mut UiNode) {
        node.disabled = true;
        node.focusable = false;
        node.interactive = false;
        node.event_handlers.clear();
        for child in &mut node.children {
            disable(child);
        }
    }
    for child in &mut surface.root.children {
        disable(child);
    }
    let tokens = workspace_tokens(surface.palette);
    let (title, message, confirm) = match confirmation {
        WorkspaceConfirmation::StartAll => (
            "agent_workspace.start_all",
            "agent_workspace.start_all_warning",
            "agent_workspace.start_all",
        ),
        WorkspaceConfirmation::CloseAll => (
            "agent_workspace.close_all",
            "agent_workspace.close_all_warning",
            "agent_workspace.close_all",
        ),
        WorkspaceConfirmation::CloseProject(_) => (
            "agent_workspace.close_project",
            "agent_workspace.close_project_warning",
            "agent_workspace.close_project",
        ),
    };
    let width = (size[0] - 40.0).clamp(240.0, 500.0);
    let card = UiNode::new("agent.workspace.confirmation.card", UiNodeKind::Panel)
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_accessibility_label_key(title)
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            border_width: 1.0,
            radius: 10.0,
            ..UiStyle::transparent()
        })
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 16.0,
            padding: UiSpacing::same(24.0),
            basis: [width, 0.0],
            width_mode: UiSizeMode::Fixed,
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new("agent.workspace.confirmation.title", UiNodeKind::Label)
                .with_text_key(title)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("agent.workspace.confirmation.message", UiNodeKind::Label)
                .with_text_key(message)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("agent.workspace.confirmation.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    gap: 12.0,
                    justify_content: UiJustify::End,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(button(
                    "agent.workspace.confirmation.cancel",
                    "app.cancel",
                    "agent.workspace.confirmation.cancel",
                    &tokens,
                ))
                .with_child(button(
                    "agent.workspace.confirmation.accept",
                    confirm,
                    "agent.workspace.confirmation.accept",
                    &tokens,
                )),
        );
    surface.root.children.push(
        UiNode::new("agent.workspace.confirmation", UiNodeKind::Overlay)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                ..UiLayout::absolute(UiRect::new(0.0, 0.0, size[0], size[1])).with_z_index(100)
            })
            .with_style(UiStyle {
                fill: [0, 0, 0, 170],
                ..UiStyle::transparent()
            })
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "agent.workspace.confirmation.cancel",
            ))
            .with_child(card),
    );
    surface
}

fn window_button(command: &str, key: &str, icon: UiIconId, tokens: &UiTokens) -> UiNode {
    let mut node = button(
        &format!("agent.workspace.window.{command}"),
        key,
        &format!("window.{command}"),
        tokens,
    );
    node.text_key = None;
    node.layout = UiLayout::fixed(30.0, 30.0);
    if command == "minimize" {
        node.with_text_value("—").with_tooltip_key(key)
    } else {
        node.with_icon(UiIcon::new(icon)).with_tooltip_key(key)
    }
}
