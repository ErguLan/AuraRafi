//! Retained RafUI surface for the native Agent dock.
//!
//! This module is deliberately a projection only. Runtime polling, history
//! persistence, provider configuration and tool execution stay in
//! `ai_chat.rs` and the native workbench. The surface keeps a bounded visible
//! message window so long conversations do not turn into a per-frame layout
//! workload.

mod composer;
mod evidence;
mod history;
mod layout;
mod messages;
mod models;
mod settings;
#[cfg(test)]
mod tests;
mod tools;

use raf_ai::chat::{ChatMessage, MessageRole};
use raf_core::ai::AgentMode;
use raf_core::config::EngineSettings;
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiFlow, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect, UiSizeMode, UiStyle,
    UiSurface,
};

use super::ai_chat::{AgentPanel, AgentReadiness};

use self::composer::build_composer;
use self::history::{build_history, build_sidebar, build_suggestions};
use self::layout::{agent_style_sheet, build_header, short_text, AgentSurfaceVariant};
use self::models::build_model_menu;
use self::settings::build_mode_menu;
use self::tools::build_status_strip;

#[cfg(test)]
pub(crate) fn build_agent_surface(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    readiness: AgentReadiness,
    project_type: ProjectType,
    size: [f32; 2],
    scroll_offset: f32,
    sidebar_progress: f32,
) -> UiSurface {
    build_agent_surface_with_variant(
        palette,
        panel,
        settings,
        readiness,
        project_type,
        size,
        scroll_offset,
        sidebar_progress,
        AgentSurfaceVariant::Embedded,
        None,
    )
}

pub(crate) fn build_dock_agent_surface(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    readiness: AgentReadiness,
    project_type: ProjectType,
    size: [f32; 2],
    scroll_offset: f32,
    sidebar_progress: f32,
) -> UiSurface {
    build_agent_surface_with_variant(
        palette,
        panel,
        settings,
        readiness,
        project_type,
        size,
        scroll_offset,
        sidebar_progress,
        AgentSurfaceVariant::Dock,
        None,
    )
}

fn build_agent_surface_with_variant(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    readiness: AgentReadiness,
    project_type: ProjectType,
    size: [f32; 2],
    scroll_offset: f32,
    sidebar_progress: f32,
    variant: AgentSurfaceVariant,
    sidebar_override: Option<f32>,
) -> UiSurface {
    let width = size[0].max(1.0);
    let height = size[1].max(1.0);
    let requested_sidebar_width =
        sidebar_override.unwrap_or_else(|| (width * 0.30).clamp(184.0, 236.0).min(width * 0.52));
    let sidebar_width = requested_sidebar_width * sidebar_progress.clamp(0.0, 1.0);
    let main_width = (width - sidebar_width).max(1.0);
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);

    let root = UiNode::new("agent.surface.root", UiNodeKind::Root)
        .with_class("agent-root")
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle {
            fill: tokens.background,
            border: tokens.border,
            text: tokens.text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_child(build_sidebar(
            palette,
            panel,
            sidebar_width,
            height,
            variant,
        ))
        .with_child(build_main(
            palette,
            panel,
            settings,
            readiness,
            project_type,
            main_width,
            height,
            sidebar_width,
            scroll_offset,
            variant,
        ));

    let mut surface = UiSurface::new("editor.bottom.agent", palette, root);
    surface.style_sheet = agent_style_sheet(palette);
    surface
}

/// Application workspace splitters only; the compact dock keeps its own layout.
pub(crate) fn build_workspace_agent_surface(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    readiness: AgentReadiness,
    project_type: ProjectType,
    size: [f32; 2],
    scroll_offset: f32,
    sidebar_width: f32,
) -> UiSurface {
    build_agent_surface_with_variant(
        palette,
        panel,
        settings,
        readiness,
        project_type,
        size,
        scroll_offset,
        if sidebar_width > 0.0 { 1.0 } else { 0.0 },
        AgentSurfaceVariant::Embedded,
        Some(sidebar_width),
    )
}

fn build_main(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    readiness: AgentReadiness,
    project_type: ProjectType,
    width: f32,
    height: f32,
    x: f32,
    scroll_offset: f32,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let provider_label = panel
        .effective_provider(settings)
        .map(|provider| provider.provider.display_name().to_string())
        .unwrap_or_else(|| t("app.agent_provider_default", settings.language));
    let model_label = {
        let current = panel
            .effective_provider(settings)
            .map(|provider| provider.model.clone())
            .unwrap_or_default();
        if settings.agent_openai_use_plan
            && settings.default_ai_provider == raf_ai::provider::AiProvider::OpenAI
        {
            raf_ai::chatgpt_auth::snapshot()
                .and_then(|view| {
                    view.models
                        .into_iter()
                        .find(|model| model.slug == current)
                        .map(|model| model.label)
                })
                .unwrap_or_else(|| {
                    if current.is_empty() {
                        t("app.agent_provider_default_short", settings.language)
                    } else {
                        short_text(&current, 32)
                    }
                })
        } else if current.is_empty() {
            t("app.agent_provider_default_short", settings.language)
        } else {
            short_text(&current, 32)
        }
    };
    let mode_key = match settings.agent_mode {
        AgentMode::Inspect => "app.agent_mode_inspect",
        AgentMode::Plan => "app.agent_mode_plan",
        AgentMode::Active => "app.agent_mode_active",
    };

    let mut main_layout = UiLayout::absolute(UiRect::new(x, 0.0, width, height));
    main_layout.flow = UiFlow::Column;
    main_layout.overflow = UiOverflow::Clip;
    let mut main = UiNode::new("agent.main", UiNodeKind::Panel)
        .with_class("agent-main")
        .with_layout(main_layout)
        .with_child(build_header(
            palette,
            panel,
            settings,
            provider_label,
            model_label,
            mode_key,
            width,
            variant,
        ));

    let mut content_layout = UiLayout::default();
    content_layout.flow = UiFlow::Column;
    content_layout.grow = 1.0;
    content_layout.width_mode = UiSizeMode::Fill;
    content_layout.height_mode = UiSizeMode::Fill;
    content_layout.overflow = UiOverflow::Clip;
    let mut content = UiNode::new("agent.content", UiNodeKind::Panel)
        .with_class("agent-content")
        .with_layout(content_layout)
        .with_child(build_status_strip(
            palette, panel, settings, readiness, variant,
        ));

    let visible_messages = panel
        .runtime
        .messages
        .iter()
        .filter(|message| {
            message.role != MessageRole::System
                && !(message.role == MessageRole::Assistant
                    && message.content.trim().is_empty()
                    && message.tool_calls.is_some()
                    && (!panel.show_reasoning
                        || message.reasoning.as_ref().is_none_or(|r| r.text.is_empty())))
        })
        .collect::<Vec<_>>();
    let history_width = (width - 22.0).max(180.0);
    let tool_expansion =
        |message: &ChatMessage| panel.expanded_tools.contains(&message.id.to_string());
    content = content.with_child(build_history(
        palette,
        panel,
        project_type,
        &visible_messages,
        history_width,
        scroll_offset,
        variant,
        tool_expansion,
    ));
    if visible_messages.is_empty() && matches!(readiness, AgentReadiness::Ready) {
        content = content.with_child(build_suggestions(palette, project_type));
    }
    content = content.with_child(build_composer(
        palette,
        panel,
        settings,
        readiness,
        width,
        visible_messages.len(),
        variant,
    ));
    main = main.with_child(content);

    if panel.model_menu_open {
        main = main.with_child(build_model_menu(palette, panel, settings, width, variant));
    }
    if panel.mode_menu_open {
        main = main.with_child(build_mode_menu(palette, settings, width, variant));
    }
    main
}
