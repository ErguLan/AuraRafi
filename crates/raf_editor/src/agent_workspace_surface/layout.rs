//! Workspace tokens, chrome primitives, dividers and style sheet.

use crate::agent_workspace_layout::WorkspaceGeometry;
use super::*;

pub(crate) fn workspace_tokens(palette: StudioUiPalette) -> UiTokens {
    let mut tokens = palette.tokens();
    if palette == StudioUiPalette::IndustrialDark {
        tokens.background = [9, 9, 10, 255];
        tokens.surface = [14, 14, 15, 255];
        tokens.surface_alt = [20, 20, 21, 255];
        tokens.surface_raised = [26, 26, 27, 255];
        tokens.border = [48, 48, 50, 255];
    }
    tokens
}

pub(crate) fn loading_ring(palette: StudioUiPalette, step: u8) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut ring = UiNode::new("agent.workspace.spinner", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(32.0, 32.0))
        .with_accessibility_role(UiAccessibilityRole::Status)
        .with_accessibility_label_key("agent_workspace.loading");
    for index in 0..12u8 {
        let angle = f32::from(index) * std::f32::consts::TAU / 12.0;
        let distance = (index + 12 - step % 12) % 12;
        let mut color = tokens.accent_hot;
        color[3] = (220.0 * (1.0 - f32::from(distance) / 15.0)).max(40.0) as u8;
        ring = ring.with_child(
            UiNode::new(
                format!("agent.workspace.spinner.{index}"),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::absolute(UiRect::new(
                14.0 + angle.cos() * 11.0,
                14.0 + angle.sin() * 11.0,
                4.0,
                4.0,
            )))
            .with_style(UiStyle {
                fill: color,
                radius: 2.0,
                ..UiStyle::transparent()
            }),
        );
    }
    ring
}

pub(crate) fn button(id: &str, label: &str, command: &str, tokens: &UiTokens) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("agent-workspace-button")
        .with_layout(UiLayout {
            padding: UiSpacing::xy(12.0, 7.0),
            min_size: [0.0, 32.0],
            ..UiLayout::fit_content()
        })
        .with_text_key(label)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            radius: 6.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_label_key(label)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(super) fn label(id: &str, key: &str, tokens: &UiTokens) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_layout(UiLayout::fit_content())
}

pub(super) fn append_dividers(
    mut root: UiNode,
    geometry: WorkspaceGeometry,
    tokens: &UiTokens,
    is_lab: bool,
) -> UiNode {
    for (divider, rect) in geometry.dividers().into_iter().filter(|_| !is_lab) {
        let name = match divider {
            WorkspaceDivider::Projects => "projects",
            WorkspaceDivider::Sessions => "sessions",
            WorkspaceDivider::Review => "review",
        };
        root = root.with_child(
            UiNode::new(
                format!("agent.workspace.divider.{name}"),
                UiNodeKind::Button,
            )
            .with_class("agent-workspace-divider")
            .with_layout(UiLayout::absolute(rect).with_z_index(50))
            .with_style(UiStyle::transparent())
            .with_accessibility_label_key("agent_workspace.resize")
            .with_tooltip_key("agent_workspace.resize")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("ArrowLeft".into()),
                format!("agent.workspace.resize.{name}:-24"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("ArrowRight".into()),
                format!("agent.workspace.resize.{name}:24"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("Home".into()),
                format!("agent.workspace.resize.{name}:reset"),
            ))
            .with_child(
                UiNode::new(
                    format!("agent.workspace.divider.{name}.line"),
                    UiNodeKind::Panel,
                )
                .with_layout(UiLayout::absolute(UiRect::new(3.5, 0.0, 1.0, rect.height)))
                .with_style(UiStyle {
                    fill: tokens.border,
                    ..UiStyle::transparent()
                }),
            ),
        );
    }
    root
}

pub(super) fn apply_style_sheet(
    mut surface: UiSurface,
    mut rules: Vec<UiStyleRule>,
    tokens: &UiTokens,
    is_lab: bool,
) -> UiSurface {
    if is_lab {
        for class in ["lab.signal-target", "lab.nav", "lab.run-primary"] {
            rules.push(UiStyleRule::new(UiStyleSelector::Class(class.into()), UiStylePatch {
                fill: Some(tokens.surface_raised), ..Default::default()
            }).when(UiStyleRuleState::Hovered));
        }
    }
    for class in ["agent-workspace-button", "agent-workspace-project"] {
        rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class(class.into()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    ..Default::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        );
        rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class(class.into()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    ..Default::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        );
    }
    surface.style_sheet.rules = rules;
    for state in [UiStyleRuleState::Hovered, UiStyleRuleState::Focused] {
        surface.style_sheet.rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class("agent-workspace-divider".into()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    ..Default::default()
                },
            )
            .when(state),
        );
    }
    surface
}
