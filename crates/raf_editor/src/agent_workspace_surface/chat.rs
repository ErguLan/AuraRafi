//! Center chat, laboratory body and empty/loading states.

use super::*;
use super::layout::label;

pub(super) fn laboratory_nodes(
    tokens: &UiTokens,
    settings: &EngineSettings,
    editor: &crate::electronics_controller::NativeElectronicsEditor,
    ready: Option<&crate::agent_service::AgentWorkspace>,
    lab_chat: bool,
    rail: f32,
    width: f32,
    height: f32,
) -> Vec<UiNode> {
    let mut nodes = Vec::new();
    nodes.push(crate::electronics_lab_surface::tabs(
        editor,
        UiRect::new(rail, 52.0, width - rail, 44.0),
        tokens,
    ));
    if !lab_chat {
        let body_height = (height - 140.0).max(1.0);
        nodes.push(crate::electronics_lab_surface::body(
            editor,
            UiRect::new(rail, 96.0, width - rail, body_height),
            tokens,
            settings.language,
        ));
        if let Some(workspace) = ready {
            nodes.push(crate::electronics_lab_surface::assistant(
                &workspace.panel.borrow(),
                settings,
                UiRect::new(rail, 96.0 + body_height, width - rail, 44.0),
                tokens,
            ));
        }
    }
    nodes
}

pub(super) fn workspace_chat(
    palette: StudioUiPalette,
    workspace: &crate::agent_service::AgentWorkspace,
    settings: &EngineSettings,
    scroll: f32,
    sessions: f32,
    rail: f32,
    top: f32,
    width: f32,
    height: f32,
    review: f32,
) -> (UiNode, Vec<UiStyleRule>) {
    let panel = workspace.panel.borrow();
    let mut chat = crate::panels::agent_surface::build_workspace_agent_surface(
        palette,
        &panel,
        settings,
        panel.readiness(settings),
        workspace.project.project_type,
        [(width - rail - review).max(1.0), (height - top).max(1.0)],
        scroll,
        sessions,
    );
    chat.root.layout = UiLayout::absolute(UiRect::new(
        rail,
        top,
        width - rail - review,
        (height - top).max(1.0),
    ));
    (chat.root, chat.style_sheet.rules)
}

pub(super) fn empty_state(
    palette: StudioUiPalette,
    tokens: &UiTokens,
    service: &AgentService,
    settings: &EngineSettings,
    step: u8,
    rail: f32,
    width: f32,
    height: f32,
) -> UiNode {
    let mut empty =
        UiNode::new("agent.workspace.empty", UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 16.0,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            padding: UiSpacing::same(32.0),
            ..UiLayout::absolute(UiRect::new(rail, 52.0, width - rail, height - 52.0))
        });
    let closing = service
        .selected
        .as_deref()
        .is_some_and(|p| service.is_closing(p));
    let close_error = service.selected_close_error();
    if service.loading() || (closing && close_error.is_none()) {
        empty = empty.with_child(loading_ring(palette, step));
    }
    empty = empty.with_child(label(
        "agent.workspace.empty.title",
        if closing {
            "agent_workspace.closing"
        } else if service.loading() {
            "agent_workspace.loading"
        } else if service.error().is_some() {
            "agent_workspace.load_failed"
        } else {
            "agent_workspace.choose_project"
        },
        tokens,
    ));
    if let Some(error) = close_error.as_deref().or_else(|| service.error()) {
        empty = empty
            .with_child(
                UiNode::new("agent.workspace.empty.error", UiNodeKind::Label)
                    .with_text_value(localized_error(error, settings.language))
                    .with_text_style(UiTextStyle::body(tokens.warning))
                    .with_layout(UiLayout::fit_content()),
            )
            .with_child(button(
                "agent.workspace.retry",
                "agent_workspace.retry",
                "agent.workspace.retry",
                tokens,
            ));
    } else if !service.loading() && !closing {
        empty = empty.with_child(label(
            "agent.workspace.empty.hint",
            "agent_workspace.choose_project_hint",
            tokens,
        ));
    }
    empty
}
