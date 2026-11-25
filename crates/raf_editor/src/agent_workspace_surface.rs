//! Creative native Agent workspace: project rail, conversation and evidence.

use crate::agent_service::AgentService;
use crate::agent_workspace_layout::{WorkspaceDivider, WorkspaceLayout};
use crate::studio_surface::HubSurfaceProject;
use raf_ai::agent_runtime::AgentToolResult;
use raf_ai::chat::MessageRole;
use raf_core::i18n::t;
use raf_core::{EngineSettings, Language};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiAction, UiSurface};
use raf_ui::*;

mod chat;
mod evidence;
mod layout;
mod sessions;
mod settings;
mod tools;

pub(crate) use layout::{button, loading_ring, workspace_tokens};
pub(crate) use settings::{with_confirmation, WorkspaceConfirmation};
pub(crate) use tools::{
    attachment_image_key, collect_evidence_artifacts, evidence_image_key, EvidenceArtifact,
    EvidenceKind, EvidenceTab,
};

pub(crate) fn build_workspace_surface(
    palette: StudioUiPalette,
    service: &AgentService,
    projects: &[HubSurfaceProject],
    settings: &EngineSettings,
    size: [f32; 2],
    scroll: f32,
    step: u8,
    evidence: EvidenceTab,
    review_open: bool,
) -> UiSurface {
    build_workspace_surface_with_layout(
        palette,
        service,
        projects,
        settings,
        size,
        scroll,
        step,
        evidence,
        review_open,
        &WorkspaceLayout::default(),
    )
}

pub(crate) fn build_workspace_surface_with_layout(
    palette: StudioUiPalette,
    service: &AgentService,
    projects: &[HubSurfaceProject],
    settings: &EngineSettings,
    size: [f32; 2],
    scroll: f32,
    step: u8,
    evidence: EvidenceTab,
    review_open: bool,
    layout: &WorkspaceLayout,
) -> UiSurface {
    build_workspace_surface_with_laboratory(
        palette,
        service,
        projects,
        settings,
        size,
        scroll,
        step,
        evidence,
        review_open,
        layout,
        service.selected_lab(None),
    )
}

pub(crate) fn build_workspace_surface_with_laboratory(
    palette: StudioUiPalette,
    service: &AgentService,
    projects: &[HubSurfaceProject],
    settings: &EngineSettings,
    size: [f32; 2],
    scroll: f32,
    step: u8,
    evidence: EvidenceTab,
    review_open: bool,
    layout: &WorkspaceLayout,
    laboratory: Option<&crate::electronics_controller::NativeElectronicsEditor>,
) -> UiSurface {
    let tokens = workspace_tokens(palette);
    let width = size[0].max(1.0);
    let height = size[1].max(1.0);
    let ready = service.selected_workspace().filter(|w| !w.closing);
    let is_lab = laboratory.is_some();
    let lab_chat =
        laboratory.is_some_and(|editor| editor.lab.view == crate::electronics_lab::LabView::Chat);
    let geometry = layout.resolve(
        size,
        review_open && ready.is_some() && !is_lab,
        ready.is_some_and(|w| w.panel.borrow().sidebar_open),
    );
    let rail = geometry.projects;
    let review = geometry.review.map_or(0.0, |r| r.width);
    let header = settings::workspace_header(&tokens, ready, is_lab, width);
    let rail_node = sessions::projects_rail(&tokens, service, projects, settings, rail, height);
    let mut root = UiNode::new("agent.workspace.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle {
            fill: tokens.background,
            ..UiStyle::transparent()
        })
        .with_child(header)
        .with_child(rail_node);
    let mut rules = Vec::new();
    if let Some(editor) = laboratory {
        for node in chat::laboratory_nodes(
            &tokens, settings, editor, ready, lab_chat, rail, width, height,
        ) {
            root = root.with_child(node);
        }
    }
    if let Some(workspace) = ready.filter(|_| !is_lab || lab_chat) {
        let top = if is_lab { 96.0 } else { 52.0 };
        let (chat_node, chat_rules) = chat::workspace_chat(
            palette,
            workspace,
            settings,
            scroll,
            geometry.sessions,
            rail,
            top,
            width,
            height,
            review,
        );
        root = root.with_child(chat_node);
        rules.extend(chat_rules);
        if let Some(review_rect) = geometry.review {
            root = root.with_child(evidence::evidence_surface(
                palette,
                &workspace.panel.borrow(),
                settings.language,
                review_rect,
                evidence,
                workspace.project.project_type == raf_core::ProjectType::Game,
                Some(&workspace.project),
            ));
            if evidence == EvidenceTab::Preview
                && workspace.project.project_type == raf_core::ProjectType::Game
            {
                root.style.fill = [0; 4];
                root.children.insert(
                    0,
                    UiNode::new("agent.workspace.background", UiNodeKind::Panel)
                        .with_layout(UiLayout::absolute(UiRect::new(
                            0.0,
                            52.0,
                            review_rect.x,
                            height - 52.0,
                        )))
                        .with_style(UiStyle {
                            fill: tokens.background,
                            ..UiStyle::transparent()
                        }),
                );
            }
        }
    } else if !is_lab {
        root = root.with_child(chat::empty_state(
            palette, &tokens, service, settings, step, rail, width, height,
        ));
    }
    let root = layout::append_dividers(root, geometry, &tokens, is_lab);
    let surface = UiSurface::new("editor.agent.workspace", palette, root);
    layout::apply_style_sheet(surface, rules, &tokens, is_lab)
}

pub(crate) fn localized_error(error: &str, language: Language) -> String {
    if error.starts_with("agent_workspace.") {
        t(error, language)
    } else {
        format!(
            "{}: {error}",
            t("agent_workspace.operation_failed", language)
        )
    }
}

#[cfg(test)]
mod tests;
