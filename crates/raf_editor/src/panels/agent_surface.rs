//! Retained RafUI surface for the native Agent dock.
//!
//! This module is deliberately a projection only. Runtime polling, history
//! persistence, provider configuration and tool execution stay in
//! `ai_chat.rs` and the native workbench. The surface keeps a bounded visible
//! message window so long conversations do not turn into a per-frame layout
//! workload.

use raf_ai::agent_runtime::{AgentStatus, AgentToolResult};
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_core::ai::AgentMode;
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow,
    UiFontWeight, UiIcon, UiIconId, UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiRect, UiScrollAxis, UiSelect, UiSelectOption, UiSizeMode, UiSpacing, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface,
    UiSurfaceMaterial, UiTextOverflow, UiTextRole, UiTextStyle, UiVirtualRange,
};

use super::ai_chat::{AgentPanel, AgentReadiness};

const MAX_VISIBLE_MESSAGE_CHARS: usize = 16_384;
const MESSAGE_TEXT_CHUNK_CHARS: usize = 768;
const MESSAGE_TEXT_CHUNK_LINES: usize = 32;
const MESSAGE_TEXT_TILE_GAP: f32 = 2.0;
const MESSAGE_TEXT_TILE_OVERSCAN: f32 = 180.0;
const MESSAGE_CARD_CHROME_HEIGHT: f32 = 76.0;
const HISTORY_OVERSCAN: usize = 3;
const HISTORY_VIEWPORT_ESTIMATE: f32 = 460.0;
const MESSAGE_WIDTH_RATIO: f32 = 0.78;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentSurfaceVariant {
    Embedded,
    Dock,
}

const DOCK_TOOL_ROW_HEIGHT: f32 = 34.0;
const DOCK_TOOL_ROW_GAP: f32 = 4.0;
const DOCK_MESSAGE_CHROME_HEIGHT: f32 = 50.0;
const DOCK_COMPOSER_HEIGHT: f32 = 132.0;
const APPROVAL_COMPOSER_HEIGHT: f32 = 208.0;
const REVIEW_CHIP_WIDTH: f32 = 148.0;
const SESSION_DOT_SIZE: f32 = 7.0;

fn message_chrome_height(variant: AgentSurfaceVariant) -> f32 {
    match variant {
        AgentSurfaceVariant::Embedded => MESSAGE_CARD_CHROME_HEIGHT,
        AgentSurfaceVariant::Dock => DOCK_MESSAGE_CHROME_HEIGHT,
    }
}

#[derive(Debug, Clone, Copy)]
struct MessageViewport {
    top: f32,
    bottom: f32,
}

impl MessageViewport {
    fn for_message(scroll_offset: f32, viewport_height: f32, message_top: f32) -> Self {
        Self {
            top: scroll_offset - message_top,
            bottom: scroll_offset + viewport_height - message_top,
        }
    }
}

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

fn build_sidebar(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    width: f32,
    height: f32,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let active = panel.history.active_index;
    let mut sessions = UiNode::scroll_view("agent.sessions.scroll", UiScrollAxis::Vertical)
        .with_class("agent-sessions-scroll")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 3.0,
            padding: UiSpacing::xy(8.0, 8.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });

    if panel.history.sessions.is_empty() {
        sessions = sessions.with_child(
            UiNode::new("agent.sessions.empty", UiNodeKind::Label)
                .with_text_key("app.agent_no_messages")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    } else {
        for (index, session) in panel.history.sessions.iter().enumerate() {
            let is_active = active == Some(index);
            let running = panel.session_is_running(index);
            let mut select = UiNode::new(format!("agent.session.{index}"), UiNodeKind::Button)
                .with_class(if is_active {
                    "agent-session-active"
                } else {
                    "agent-session"
                })
                .with_layout(if running && variant == AgentSurfaceVariant::Dock {
                    UiLayout {
                        flow: UiFlow::Row,
                        align_items: UiAlign::Center,
                        gap: 6.0,
                        grow: 1.0,
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 36.0],
                        padding: UiSpacing::xy(10.0, 8.0),
                        ..UiLayout::default()
                    }
                } else {
                    UiLayout {
                        grow: 1.0,
                        width_mode: UiSizeMode::Fill,
                        height_mode: UiSizeMode::Fixed,
                        basis: [0.0, 36.0],
                        padding: UiSpacing::xy(10.0, 8.0),
                        ..UiLayout::default()
                    }
                });
            if running && variant == AgentSurfaceVariant::Dock {
                select = select
                    .with_child(
                        UiNode::new(format!("agent.session.dot.{index}"), UiNodeKind::Panel)
                            .with_layout(UiLayout::fixed(SESSION_DOT_SIZE, SESSION_DOT_SIZE))
                            .with_style(UiStyle {
                                fill: tokens.accent_hot,
                                radius: 3.5,
                                ..UiStyle::transparent()
                            })
                            .with_accessibility_role(UiAccessibilityRole::Status)
                            .with_tooltip_key("agent_workspace.running"),
                    )
                    .with_child(
                        UiNode::new(format!("agent.session.name.{index}"), UiNodeKind::Label)
                            .with_text_value(short_text(&session.title, 72))
                            .with_text_overflow(UiTextOverflow::Ellipsis)
                            .with_text_style(UiTextStyle::button(if is_active {
                                tokens.text
                            } else {
                                tokens.text_muted
                            }))
                            .with_layout(UiLayout {
                                grow: 1.0,
                                width_mode: UiSizeMode::Fill,
                                ..UiLayout::fit_content()
                            }),
                    );
            } else {
                select = select
                    .with_text_value(short_text(&session.title, 72))
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_text_style(UiTextStyle::button(if is_active {
                        tokens.text
                    } else {
                        tokens.text_muted
                    }));
            }
            let select = select
                .with_accessibility_role(UiAccessibilityRole::Button)
                .with_accessibility_selected(is_active)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session.select:{index}"),
                ));
            let delete = UiNode::new(format!("agent.session.delete.{index}"), UiNodeKind::Button)
                .with_class("agent-icon-button")
                .with_layout(UiLayout::fixed(30.0, 30.0))
                .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                .with_tooltip_key("app.agent_delete")
                .with_accessibility_label_key("app.agent_delete")
                .with_accessibility_role(UiAccessibilityRole::Button)
                .focusable()
                .disabled(panel.session_is_running(index))
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session.delete:{index}"),
                ));
            sessions = sessions.with_child(
                UiNode::new(format!("agent.session.row.{index}"), UiNodeKind::Toolbar)
                    .with_class("agent-session-row")
                    .with_layout({
                        let mut layout =
                            UiLayout::fixed(0.0, 36.0).with_width_mode(UiSizeMode::Fill);
                        layout.flow = UiFlow::Row;
                        layout.gap = 4.0;
                        layout.align_items = UiAlign::Center;
                        layout
                    })
                    .with_child(select)
                    .with_child(delete),
            );
        }
    }

    let mut sidebar_layout = UiLayout::absolute(UiRect::new(0.0, 0.0, width, height));
    sidebar_layout.flow = UiFlow::Column;
    sidebar_layout.overflow = UiOverflow::Clip;
    UiNode::new("agent.sidebar", UiNodeKind::Panel)
        .with_class("agent-sidebar")
        .with_layout(sidebar_layout)
        .with_child(
            UiNode::new("agent.sidebar.header", UiNodeKind::Toolbar)
                .with_class("agent-sidebar-header")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::SpaceBetween,
                    padding: UiSpacing::xy(12.0, 8.0),
                    ..UiLayout::fixed(0.0, 48.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("agent.sessions.title", UiNodeKind::Label)
                        .with_text_key("app.agent_sessions")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(icon_button(
                    "agent.sidebar.new",
                    UiIconId::Add,
                    "app.agent_new_chat",
                    "agent.new-chat",
                )),
        )
        .with_child(sessions)
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
        if settings.agent_openai_use_plan && settings.default_ai_provider == raf_ai::provider::AiProvider::OpenAI
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
                    && (!panel.show_reasoning || message.reasoning.as_ref().is_none_or(|r| r.text.is_empty())))
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

fn build_header(
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

fn review_chip(palette: StudioUiPalette, panel: &AgentPanel, width: f32) -> UiNode {
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

fn build_status_strip(
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

fn build_live_activity(
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

fn build_tool_call_warning(
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

fn format_elapsed(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

#[derive(Debug, PartialEq)]
enum HistoryEntry {
    Message(usize),
    Reasoning(usize),
    Activity {
        first: usize,
        count: usize,
        expanded: bool,
    },
}

/// Presentation-only grouping. Domain messages and tool-call order are untouched.
/// Errors stay outside collapsed details; the final answer always stays visible.
fn history_entries(
    messages: &[&ChatMessage],
    expanded: &std::collections::HashSet<String>,
) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    while cursor < messages.len() {
        if messages[cursor].role == MessageRole::User {
            entries.push(HistoryEntry::Message(cursor));
            cursor += 1;
            continue;
        }
        let first = cursor;
        while cursor < messages.len() && messages[cursor].role != MessageRole::User {
            cursor += 1;
        }
        let segment = &messages[first..cursor];
        let count = segment
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .count();
        if count < 2 {
            entries.extend((first..cursor).map(HistoryEntry::Message));
            continue;
        }
        let open = expanded.contains(&format!("group:{}", messages[first].id));
        entries.push(HistoryEntry::Activity {
            first,
            count,
            expanded: open,
        });
        let last_tool = (first..cursor)
            .rfind(|&i| messages[i].role == MessageRole::Tool)
            .unwrap();
        for index in first..cursor {
            if open
                || index > last_tool
                || expanded.contains(&messages[index].id.to_string())
                || (messages[index].role == MessageRole::Tool
                    && tool_needs_attention(messages[index]))
            {
                entries.push(HistoryEntry::Message(index));
            }
        }
    }
    entries
}

fn activity_group(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    message: &ChatMessage,
    count: usize,
    expanded: bool,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    UiNode::new(format!("agent.activity.{}", message.id), UiNodeKind::Button)
        .with_class("agent-tool-row")
        .with_icon(
            UiIcon::new(if expanded {
                UiIconId::ChevronDown
            } else {
                UiIconId::ChevronRight
            })
            .with_size(UiIconSize::Small),
        )
        .with_text_value(format!(
            "{count} {}",
            t("agent.activity.group", panel.language)
        ))
        .with_text_style(UiTextStyle::button(tokens.text_muted))
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fixed(0.0, DOCK_TOOL_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_expanded(expanded)
        .with_accessibility_label_key("agent.activity.details")
        .with_tooltip_key("agent.activity.details")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("agent.tool.toggle:group:{}", message.id),
        ))
}

fn build_history(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    project_type: ProjectType,
    messages: &[&ChatMessage],
    width: f32,
    scroll_offset: f32,
    variant: AgentSurfaceVariant,
    tool_expansion: impl Fn(&ChatMessage) -> bool,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let mut entries = Vec::new();
    for entry in history_entries(messages, &panel.expanded_tools) {
        if let HistoryEntry::Message(index) = entry {
            let message = messages[index];
            if panel.show_reasoning && message.reasoning.as_ref().is_some_and(|r| !r.text.is_empty()) {
                entries.push(HistoryEntry::Reasoning(index));
            }
            if message.role == MessageRole::Assistant && message.content.trim().is_empty()
                && (message.tool_calls.is_some() || message.reasoning.is_some()) { continue; }
            entries.push(HistoryEntry::Message(index));
        } else {
            entries.push(entry);
        }
    }
    let message_heights = entries
        .iter()
        .map(|entry| match entry {
            HistoryEntry::Activity { .. } => DOCK_TOOL_ROW_HEIGHT,
            HistoryEntry::Reasoning(index) => {
                if panel.expanded_tools.contains(&format!("reasoning:{}", messages[*index].id)) {
                    DOCK_TOOL_ROW_HEIGHT + 126.0
                } else { DOCK_TOOL_ROW_HEIGHT }
            }
            HistoryEntry::Message(index) => estimated_message_height_with_variant(
                messages[*index],
                width,
                variant,
                tool_expansion(messages[*index]),
            ),
        })
        .collect::<Vec<_>>();
    let range = visible_message_range(&message_heights, scroll_offset, HISTORY_VIEWPORT_ESTIMATE);
    let mut scroll = UiNode::scroll_view("agent.history", UiScrollAxis::Vertical)
        .with_class("agent-history")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 10.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    let top_height = estimated_prefix_height(&message_heights, range.start);
    let bottom_height = estimated_suffix_height(&message_heights, range.end);
    if top_height > 0.0 {
        scroll = scroll.with_child(spacer("agent.history.top", top_height));
    }
    if messages.is_empty() {
        scroll = scroll.with_child(
            UiNode::new("agent.empty", UiNodeKind::Panel)
                .with_class("agent-empty")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 5.0,
                    padding: UiSpacing::same(18.0),
                    align_items: UiAlign::Center,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(if variant == AgentSurfaceVariant::Embedded {
                    UiNode::new("agent.composer.hint", UiNodeKind::Label)
                        .with_text_key("app.agent_enter_to_send")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content())
                } else {
                    spacer("agent.composer.hint.hidden", 0.0)
                })
                .with_child(
                    UiNode::new("agent.empty.subtitle", UiNodeKind::Label)
                        .with_text_key(match project_type {
                            ProjectType::Game => "app.agent_empty_subtitle_game",
                            ProjectType::Electronics => "app.agent_empty_subtitle_electronics",
                        })
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout {
                            max_size: [520.0, 0.0],
                            ..UiLayout::fit_content()
                        }),
                ),
        );
    } else {
        let mut message_top = top_height + if top_height > 0.0 { 8.0 } else { 0.0 };
        for (entry_index, entry) in entries
            .iter()
            .enumerate()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
        {
            let viewport =
                MessageViewport::for_message(scroll_offset, HISTORY_VIEWPORT_ESTIMATE, message_top);
            scroll = scroll.with_child(match entry {
                HistoryEntry::Activity {
                    first,
                    count,
                    expanded,
                } => activity_group(palette, panel, messages[*first], *count, *expanded),
                HistoryEntry::Reasoning(index) => reasoning_entry(palette, panel, messages[*index]),
                HistoryEntry::Message(index) => message_card_with_variant(
                    palette,
                    messages[*index],
                    width,
                    *index,
                    viewport,
                    variant,
                    tool_expansion(messages[*index]),
                ),
            });
            message_top += message_heights[entry_index] + 8.0;
        }
    }
    if bottom_height > 0.0 {
        scroll = scroll.with_child(spacer("agent.history.bottom", bottom_height));
    }
    if panel.runtime.status == AgentStatus::Done && panel.runtime.messages.is_empty() {
        return scroll;
    }
    scroll
}

fn reasoning_entry(palette: StudioUiPalette, panel: &AgentPanel, message: &ChatMessage) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let trace = message.reasoning.as_ref().map(|r| r.text.as_str()).unwrap_or("");
    let key = format!("reasoning:{}", message.id);
    let expanded = panel.expanded_tools.contains(&key);
    let title = format!("{} · {}", t("agent.reasoning.trace", panel.language), short_text(trace, 120));
    let button = UiNode::new(format!("agent.reasoning.{}.toggle", message.id), UiNodeKind::Button)
        .with_class("agent-tool-row")
        .with_icon(UiIcon::new(if expanded { UiIconId::ChevronDown } else { UiIconId::ChevronRight }).with_size(UiIconSize::Small))
        .with_text_value(title)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(UiTextStyle::button(tokens.text_muted))
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key("agent.reasoning.trace")
        .with_accessibility_expanded(expanded)
        .with_tooltip_key("agent.reasoning.trace_hint")
        .focusable()
        .with_layout(UiLayout::fixed(0.0, DOCK_TOOL_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill))
        .with_event(UiEventBinding::command(UiEventKind::Click, format!("agent.tool.toggle:{key}")));
    if !expanded { return button; }
    UiNode::new(format!("agent.reasoning.{}", message.id), UiNodeKind::Panel)
        .with_layout(UiLayout { flow: UiFlow::Column, gap: 4.0, overflow: UiOverflow::Clip, ..UiLayout::fixed(0.0, DOCK_TOOL_ROW_HEIGHT + 126.0).with_width_mode(UiSizeMode::Fill) })
        .with_child(button)
        .with_child(UiNode::new(format!("agent.reasoning.{}.text", message.id), UiNodeKind::Label)
            .with_text_value(bounded_text(trace, 2048))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Wrap)
            .selectable_text()
            .with_layout(UiLayout { overflow: UiOverflow::Clip, ..UiLayout::fixed(0.0, 88.0).with_width_mode(UiSizeMode::Fill) }))
        .with_child(copy_message_button(format!("agent.reasoning.{}.copy", message.id), trace))
}

fn build_suggestions(palette: StudioUiPalette, project_type: ProjectType) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let keys: &[&str] = match project_type {
        ProjectType::Electronics => &[
            "app.agent_hint_resistor",
            "app.agent_hint_electrical_test",
            "app.agent_hint_nets",
        ],
        ProjectType::Game => &[
            "app.agent_hint_cube",
            "app.agent_hint_light",
            "app.agent_hint_camera",
        ],
    };
    let mut row = UiNode::new("agent.suggestions", UiNodeKind::Toolbar)
        .with_class("agent-suggestions")
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            gap: 6.0,
            padding: UiSpacing::xy(12.0, 6.0),
            compact: raf_render::api_graphic_basic::ui_surface::UiCompactMode::Wrap,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("agent.suggestions.title", UiNodeKind::Label)
                .with_text_key("app.agent_suggestions_title")
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    for key in keys {
        row = row.with_child(
            UiNode::new(format!("agent.suggestion.{key}"), UiNodeKind::Button)
                .with_class("agent-suggestion")
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(10.0, 6.0),
                    ..UiLayout::fit_content()
                })
                .with_text_key(*key)
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_accessibility_role(UiAccessibilityRole::Button)
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.suggestion:{key}"),
                )),
        );
    }
    row
}

fn build_composer(
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
                .with_child(if settings.agent_openai_use_plan
                    && settings.default_ai_provider == raf_ai::provider::AiProvider::OpenAI
                    && raf_ai::chatgpt_auth::active_connected() {
                    UiNode::new("agent.account.usage", UiNodeKind::Button)
                        .with_class("agent-button")
                        .with_text_key("agent.account.using_plan")
                        .with_tooltip_key("settings.account.usage")
                        .with_accessibility_role(UiAccessibilityRole::Button)
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout::fixed(120.0, 30.0))
                        .focusable().with_event(UiEventBinding::command(UiEventKind::Click, "agent.account.usage"))
                } else {
                    UiNode::new("agent.account.hidden", UiNodeKind::Panel).with_style(UiStyle::transparent()).with_layout(UiLayout::fixed(0.0, 0.0))
                })
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

fn composer_media_height(panel: &AgentPanel) -> f32 {
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

fn composer_media(palette: StudioUiPalette, panel: &AgentPanel, running: bool) -> UiNode {
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

fn dock_approval_composer(
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

fn build_model_menu(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    width: f32,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let provider = panel
        .effective_provider(settings)
        .map(|config| config.provider)
        .unwrap_or(settings.default_ai_provider);
    let plan_owned = settings.agent_openai_use_plan
        && provider == raf_ai::provider::AiProvider::OpenAI;
    let has_catalog = plan_owned
        || matches!(
            provider,
            raf_ai::provider::AiProvider::OpenRouter
                | raf_ai::provider::AiProvider::OpenAI
                | raf_ai::provider::AiProvider::OpenCode
                | raf_ai::provider::AiProvider::OpenCodeGo
        );
    let (state, items) = crate::panels::ai_model_options::collect_items(settings, provider, "");
    let paint = crate::panels::ai_model_options::ModelRowPaint {
        text: tokens.text,
        muted: tokens.text_muted,
        accent_hot: tokens.accent_hot,
    };
    let actions = crate::panels::ai_model_options::ModelRowActions {
        node_prefix: "agent.model.".to_string(),
        select_key: "agent.model".to_string(),
        pin_command_prefix: "agent.model.pin:".to_string(),
        close_select_id: Some("agent.model.trigger".to_string()),
    };
    let status = crate::panels::ai_model_options::model_status_text(&state, settings.language);
    let pinned_count = items.iter().filter(|item| item.pinned).count();
    let custom_count = items.iter().filter(|item| item.custom).count();
    let plan_count = items
        .iter()
        .filter(|item| item.value.starts_with("account:"))
        .count();
    let catalog_count = items.iter().filter(|item| !item.pinned && !item.custom && !item.value.starts_with("account:")).count();
    let custom_rows = if plan_owned { 0 } else { 3 };
    let row_total = usize::from(has_catalog)
        + usize::from(!status.is_empty())
        + items.len()
        + usize::from(pinned_count > 0)
        + usize::from(plan_count > 0)
        + usize::from(catalog_count > 0)
        + usize::from(custom_count > 0)
        + custom_rows;
    let menu_height = (row_total as f32 * 34.0).clamp(68.0, 340.0);
    let (model_width, mode_width) = selector_widths_with_variant(width, variant);
    let right_reserved = if variant == AgentSurfaceVariant::Dock && width >= 560.0 {
        50.0 + REVIEW_CHIP_WIDTH + 8.0
    } else {
        50.0
    };
    let mut menu_layout = UiLayout::absolute(UiRect::new(
        (width - right_reserved - mode_width - 8.0 - model_width).max(8.0),
        48.0,
        model_width,
        menu_height,
    ));
    menu_layout.flow = UiFlow::Column;
    menu_layout.overflow = UiOverflow::ScrollY;
    let mut menu = UiNode::new("agent.model.menu", UiNodeKind::Menu)
        .with_class("agent-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(menu_layout.with_z_index(30))
        .with_accessibility_role(UiAccessibilityRole::Menu);

    if has_catalog {
        menu = menu.with_child(UiNode::new("agent.model.catalog.refresh", UiNodeKind::Button)
            .with_class("agent-menu-item-accent")
            .with_layout(UiLayout { padding: UiSpacing::xy(10.0, 8.0), ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill) })
            .with_text_key("agent.account.refresh_models").with_text_style(UiTextStyle::button(tokens.accent_hot))
            .with_accessibility_role(UiAccessibilityRole::MenuItem).focusable()
            .with_event(UiEventBinding::command(UiEventKind::Click, "agent.catalog.refresh")));
    }
    if !status.is_empty() {
        menu = menu.with_child(UiNode::new(
            "agent.model.catalog.status",
            UiNodeKind::Label,
        )
        .with_layout(
            UiLayout::fixed(0.0, 30.0)
                .with_width_mode(UiSizeMode::Fill)
                .padding_xy(10.0, 4.0),
        )
        .with_text_value(status)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_text_style(UiTextStyle::button(tokens.text_muted)));
    }

    // Custom id escape hatch first: one field, one confirmation, pins the
    // model. Actions precede the scrollable catalog so the entry stays
    // reachable without scrolling a long model list.
    // Plan requests may only use the selected account's live catalog.
    if !plan_owned {
        menu = menu
            .with_child(UiNode::new("agent.model.custom.divider", UiNodeKind::Separator)
                .with_layout(UiLayout::fixed(0.0, 8.0).with_width_mode(UiSizeMode::Fill)))
            .with_child(model_text_input(
                "agent.new-model.id",
                "app.agent_model_id",
                panel.new_model_id.clone(),
            ))
            .with_child(UiNode::new("agent.model.confirm", UiNodeKind::Button)
                .with_class("agent-menu-item-accent")
                .with_layout(UiLayout { padding: UiSpacing::xy(10.0, 8.0), ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill) })
                .with_text_key("app.agent_add_model_confirm")
                .with_text_style(UiTextStyle::button(tokens.accent_hot))
                .with_accessibility_role(UiAccessibilityRole::MenuItem)
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "agent.model.confirm")));
    }
    if let Some(error) = panel.new_model_error.as_deref() {
        menu = menu.with_child(UiNode::new("agent.model.error", UiNodeKind::Label)
            .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill).padding_xy(10.0, 4.0))
            .with_text_value(error.to_string())
            .with_text_style(UiTextStyle::button(tokens.danger)));
    }

    let mut last_lane = 0u8;
    for item in &items {
        let lane = if item.value.starts_with("account:") {
            3u8
        } else if item.pinned {
            1u8
        } else if item.custom {
            4u8
        } else {
            2u8
        };
        if lane != last_lane {
            let header_key = match lane {
                1 => "ai_models.pinned",
                2 => "ai_models.catalog",
                3 => "ai_models.plan",
                _ => "ai_models.custom",
            };
            menu = menu
                .with_child(crate::panels::ai_model_options::model_header_row(
                    &paint,
                    header_key,
                    settings.language,
                ));
            last_lane = lane;
        }
        menu = menu.with_child(
            crate::panels::ai_model_options::model_option_row(
                item,
                &paint,
                &actions,
                "agent-menu-item",
            ),
        );
    }

    // Custom id escape hatch: one field, one confirmation, pins the model.
    menu
}

fn build_mode_menu(
    palette: StudioUiPalette,
    settings: &EngineSettings,
    width: f32,
    variant: AgentSurfaceVariant,
) -> UiNode {
    let tokens = crate::agent_workspace_surface::workspace_tokens(palette);
    let (_, mode_width) = selector_widths_with_variant(width, variant);
    let right_reserved = if variant == AgentSurfaceVariant::Dock && width >= 560.0 {
        50.0 + REVIEW_CHIP_WIDTH + 8.0
    } else {
        50.0
    };
    let mut menu_layout = UiLayout::absolute(UiRect::new(
        (width - right_reserved - mode_width).max(8.0),
        48.0,
        mode_width,
        102.0,
    ));
    menu_layout.flow = UiFlow::Column;
    menu_layout.overflow = UiOverflow::Clip;
    UiNode::new("agent.mode.menu", UiNodeKind::Menu)
        .with_class("agent-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(menu_layout.with_z_index(30))
        .with_accessibility_role(UiAccessibilityRole::Menu)
        .with_child(mode_option(
            tokens,
            "app.agent_mode_inspect",
            "inspect",
            0,
            settings.agent_mode == AgentMode::Inspect,
        ))
        .with_child(mode_option(
            tokens,
            "app.agent_mode_plan",
            "plan",
            1,
            settings.agent_mode == AgentMode::Plan,
        ))
        .with_child(mode_option(
            tokens,
            "app.agent_mode_active",
            "active",
            2,
            settings.agent_mode == AgentMode::Active,
        ))
}

fn message_card_with_variant(
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

fn dock_tool_entry(
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
        let (_result, revision) = result;
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

fn tool_status(palette: StudioUiPalette, message: &ChatMessage) -> ([u8; 4], &'static str) {
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
    let verification_status = result
        .verification
        .as_ref()
        .and_then(|verification| verification.get("status"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_ascii_lowercase);
    match verification_status.as_deref() {
        Some("ok") | Some("passed") | Some("success") => (tokens.positive, "app.agent_tool_result"),
        Some("failed") | Some("error") | Some("errors") => (tokens.danger, "app.agent_tool_result"),
        _ if !result.warnings.is_empty() => (tokens.warning, "app.agent_tool_result"),
        _ if result.changed => (tokens.positive, "app.agent_tool_result"),
        _ => (tokens.text_muted, "app.agent_tool_result"),
    }
}

fn message_body_with_chrome(
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

fn visible_text_tile_range(
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

fn estimated_text_tile_height(content: &str, width: f32, line_height: f32) -> f32 {
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
fn message_text_chunks(content: &str) -> Vec<String> {
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
enum MessageTileKind {
    Body,
    Heading,
    Code,
}

/// A deliberately small, non-HTML Markdown subset. Never opens links, loads
/// remote images or executes code. Fenced code keeps punctuation and indentation.
fn message_text_tiles(content: &str, role: MessageRole) -> Vec<(String, MessageTileKind)> {
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
fn lightweight_markdown_text(content: &str) -> String {
    message_text_tiles(content, MessageRole::Assistant)
        .into_iter()
        .map(|(s, _)| s)
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_inline_markdown(value: &str) -> String {
    value.replace("**", "").replace("__", "").replace('`', "")
}

fn status_label(palette: StudioUiPalette, id: &str, key: &str, icon: UiIconId) -> UiNode {
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

fn mode_option(
    tokens: raf_render::api_graphic_basic::ui_surface::UiTokens,
    label_key: &str,
    value: &str,
    index: usize,
    selected: bool,
) -> UiNode {
    UiNode::new(format!("agent.mode.option.{value}"), UiNodeKind::Button)
        .with_class("agent-menu-item")
        .with_layout(UiLayout {
            padding: UiSpacing::xy(10.0, 8.0),
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_accessibility_role(UiAccessibilityRole::Option)
        .with_accessibility_selected(selected)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetSelect {
                key: "agent.mode".to_string(),
                value: value.to_string(),
                index,
            },
        })
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::SetSelectOpen {
                id: "agent.mode.trigger".to_string(),
                open: false,
            },
        })
}

fn model_text_input(id: &str, label_key: &str, value: String) -> UiNode {
    UiNode::new(format!("{id}.field"), UiNodeKind::Panel)
        .with_class("agent-form-field")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::text_input(
                id,
                raf_render::api_graphic_basic::ui_surface::UiTextInput {
                    value_key: id.to_string(),
                    placeholder_key: None,
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("agent-form-input")
            .focusable()
            .with_layout(UiLayout {
                padding: UiSpacing::xy(9.0, 7.0),
                ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_text_value(value),
        )
}

fn selector_widths_with_variant(width: f32, variant: AgentSurfaceVariant) -> (f32, f32) {
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

fn model_selector_button(
    id: &str,
    value: &str,
    tooltip: &str,
    open: bool,
    width: f32,
) -> UiNode {
    let options = vec![UiSelectOption::new(
        crate::panels::ai_chat::MODEL_SENTINEL,
        crate::panels::ai_chat::MODEL_SENTINEL,
    )];
    agent_select_trigger(
        id,
        "agent.model",
        options,
        0,
        value,
        tooltip,
        open,
        width,
        "agent.model.menu",
        None,
    )
}

fn mode_selector_button(
    id: &str,
    settings: &EngineSettings,
    mode_key: &str,
    open: bool,
    width: f32,
) -> UiNode {
    let options = vec![
        UiSelectOption::new("inspect", "app.agent_mode_inspect"),
        UiSelectOption::new("plan", "app.agent_mode_plan"),
        UiSelectOption::new("active", "app.agent_mode_active"),
    ];
    let (selected_index, mode_class) = match settings.agent_mode {
        AgentMode::Inspect => (0, "agent-mode-inspect"),
        AgentMode::Plan => (1, "agent-mode-plan"),
        AgentMode::Active => (2, "agent-mode-active"),
    };
    let value = t(mode_key, settings.language);
    let tooltip = t("app.agent_mode", settings.language);
    agent_select_trigger(
        id,
        "agent.mode",
        options,
        selected_index,
        &value,
        &tooltip,
        open,
        width,
        "agent.mode.menu",
        Some(mode_class),
    )
}

fn agent_select_trigger(
    id: &str,
    value_key: &str,
    options: Vec<UiSelectOption>,
    selected_index: usize,
    value: &str,
    tooltip: &str,
    open: bool,
    width: f32,
    popup_id: &str,
    extra_class: Option<&str>,
) -> UiNode {
    let mut select = UiSelect::new(value_key, options, selected_index).with_popup_id(popup_id);
    select.open = open;
    let node = UiNode::select(id, select).with_class("agent-selector");
    let node = if let Some(extra_class) = extra_class {
        node.with_class(extra_class)
    } else {
        node
    };
    node.with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 4.0,
        padding: UiSpacing::xy(8.0, 5.0),
        width_mode: UiSizeMode::Fixed,
        height_mode: UiSizeMode::Fixed,
        basis: [width, 32.0],
        ..UiLayout::default()
    })
    .with_text_value(value.to_string())
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_icon(UiIcon::new(UiIconId::ChevronDown).with_size(UiIconSize::Small))
    .with_tooltip_value(tooltip.to_string())
    .with_accessibility_role(UiAccessibilityRole::Combobox)
    .with_accessibility_expanded(open)
    .focusable()
}

fn icon_button(id: &str, icon: UiIconId, tooltip_key: &str, command: &str) -> UiNode {
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

fn action_button(id: &str, text_key: &str, command: &str, primary: bool) -> UiNode {
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

fn copy_message_button(id: String, text: &str) -> UiNode {
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

fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
        .with_style(UiStyle::transparent())
}

fn metrics_text(
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

fn visible_message_range(
    message_heights: &[f32],
    scroll_offset: f32,
    viewport_height: f32,
) -> UiVirtualRange {
    if message_heights.is_empty() {
        return UiVirtualRange { start: 0, end: 0 };
    }
    let mut cursor = 0.0;
    let target = scroll_offset.max(0.0);
    let mut first = 0;
    for (index, height) in message_heights.iter().copied().enumerate() {
        if cursor + height > target {
            first = index;
            break;
        }
        cursor += height + 8.0;
        first = index.saturating_add(1).min(message_heights.len());
    }
    let mut end = first;
    let mut visible_height = 0.0;
    while end < message_heights.len() && visible_height < viewport_height.max(0.0) {
        visible_height += message_heights[end] + 8.0;
        end += 1;
    }
    UiVirtualRange {
        start: first.saturating_sub(HISTORY_OVERSCAN),
        end: end
            .saturating_add(HISTORY_OVERSCAN)
            .min(message_heights.len()),
    }
}

fn estimated_prefix_height(message_heights: &[f32], end: usize) -> f32 {
    let count = end.min(message_heights.len());
    let total = message_heights
        .iter()
        .take(count)
        .map(|height| height + 8.0)
        .sum::<f32>();
    if count > 0 {
        total - 8.0
    } else {
        0.0
    }
}

fn estimated_suffix_height(message_heights: &[f32], start: usize) -> f32 {
    let count = message_heights.len().saturating_sub(start);
    let total = message_heights
        .iter()
        .skip(start)
        .map(|height| height + 8.0)
        .sum::<f32>();
    if count > 0 {
        total - 8.0
    } else {
        0.0
    }
}

fn tool_needs_attention(message: &ChatMessage) -> bool {
    serde_json::from_str::<AgentToolResult>(&message.content)
        .map(|result| {
            !result.ok
                || result
                    .verification
                    .as_ref()
                    .and_then(|v| v.get("status"))
                    .and_then(serde_json::Value::as_str)
                    == Some("failed")
        })
        // Legacy unstructured errors must remain visible too.
        .unwrap_or_else(|_| message.content.trim_start().starts_with("Error:"))
}

fn estimated_message_height_with_variant(
    message: &ChatMessage,
    width: f32,
    variant: AgentSurfaceVariant,
    tool_expanded: bool,
) -> f32 {
    let content = bounded_text(&display_message_content(message), MAX_VISIBLE_MESSAGE_CHARS);
    let card_width = (width * MESSAGE_WIDTH_RATIO).clamp(220.0, 760.0);
    let line_height = 18.0;
    let chunks = message_text_tiles(&content, message.role);
    let body_height = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            estimated_text_tile_height(&chunk.0, card_width - 24.0, line_height)
                + if index + 1 < chunks.len() {
                    MESSAGE_TEXT_TILE_GAP
                } else {
                    0.0
                }
        })
        .sum::<f32>();
    let chrome = message_chrome_height(if message.role == MessageRole::Tool {
        AgentSurfaceVariant::Dock
    } else {
        variant
    });
    if message.role == MessageRole::Tool {
        let summary = DOCK_TOOL_ROW_HEIGHT + DOCK_TOOL_ROW_GAP;
        if !tool_expanded && !tool_needs_attention(message) {
            return DOCK_TOOL_ROW_HEIGHT;
        }
        return summary + chrome + body_height;
    }
    (chrome + body_height).max(70.0)
}

fn display_message_content(message: &ChatMessage) -> String {
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
    lines.extend(
        result
            .warnings
            .into_iter()
            .map(|warning| format!("Warning: {warning}")),
    );
    lines.join("\n")
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    let mut result = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        result.push('\u{2026}');
    }
    result
}

fn short_text(value: &str, max_chars: usize) -> String {
    bounded_text(value.trim(), max_chars)
}

fn agent_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
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

fn style(
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

fn state_patch(
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use raf_render::api_graphic_basic::ui_surface::{
        CpuUiSurfaceHost, UiInputState, UiSurfaceSession,
    };
    use uuid::Uuid;

    fn message(content: &str) -> ChatMessage {
        ChatMessage {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.to_string(),
            images: Vec::new(),
            reasoning: None,
            timestamp: Utc::now(),
            tool_calls: None,
        }
    }

    #[test]
    fn harness_turn_groups_keep_errors_and_the_final_answer_outside_details() {
        let mut user = message("Build a world");
        user.role = MessageRole::User;
        let comment = message("Inspecting first");
        let mut success = message("Created objects");
        success.role = MessageRole::Tool;
        let mut failure = message("Error: layout check failed");
        failure.role = MessageRole::Tool;
        let answer = message("The layout needs one correction.");
        let messages = [&user, &comment, &success, &failure, &answer];
        let mut expanded = std::collections::HashSet::new();
        assert_eq!(
            history_entries(&messages, &expanded),
            vec![
                HistoryEntry::Message(0),
                HistoryEntry::Activity {
                    first: 1,
                    count: 2,
                    expanded: false
                },
                HistoryEntry::Message(3),
                HistoryEntry::Message(4)
            ]
        );
        expanded.insert(format!("group:{}", comment.id));
        let entries = history_entries(&messages, &expanded);
        assert_eq!(entries.len(), 6);
        assert_eq!(entries.last(), Some(&HistoryEntry::Message(4)));
    }

    #[test]
    fn harness_reasoning_visibility_and_expansion_follow_settings() {
        let mut panel = AgentPanel::default();
        let mut answer = message("Done");
        answer.reasoning = Some(raf_ai::chat::ProviderReasoning {
            endpoint: "https://openrouter.ai/api/v1".into(), model: "test/model".into(),
            transport: String::new(), text: "Provider reasoning".into(), details: Vec::new(),
        });
        let id = format!("agent.reasoning.{}", answer.id);
        let render = |panel: &AgentPanel| build_history(StudioUiPalette::IndustrialDark, panel,
            ProjectType::Game, &[&answer], 600.0, 0.0, AgentSurfaceVariant::Embedded, |_| false);
        let collapsed = render(&panel);
        assert!(node(&collapsed, &format!("{id}.toggle")).is_some());
        assert!(node(&collapsed, &format!("{id}.text")).is_none());
        panel.expanded_tools.insert(format!("reasoning:{}", answer.id));
        assert!(node(&render(&panel), &format!("{id}.text")).is_some());
        panel.show_reasoning = false;
        assert!(node(&render(&panel), &format!("{id}.toggle")).is_none());
    }

    #[test]
    fn harness_markdown_preserves_code_and_user_literals() {
        let source = "## Summary\nDone\n```rust\n  let snake_case = \"**literal**\";\n```";
        let tiles = message_text_tiles(source, MessageRole::Assistant);
        assert_eq!(tiles[0], ("Summary".into(), MessageTileKind::Heading));
        assert_eq!(
            tiles.last().unwrap(),
            &(
                "  let snake_case = \"**literal**\";".into(),
                MessageTileKind::Code
            )
        );
        assert_eq!(
            message_text_tiles("**literal**", MessageRole::User)[0].0,
            "**literal**"
        );
    }

    #[test]
    fn harness_composer_exposes_removable_images_and_automatic_output() {
        let mut panel = AgentPanel::default();
        panel.history.start_session("Images");
        panel
            .history
            .active_session_mut()
            .unwrap()
            .draft_images
            .push(raf_ai::chat_media::ChatImage {
                file: "fixture.png".into(),
                width: 320,
                height: 240,
            });
        let settings = EngineSettings::default();
        let composer = build_composer(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            AgentReadiness::Ready,
            720.0,
            0,
            AgentSurfaceVariant::Embedded,
        );
        assert!(node(&composer, "agent.image.paste").is_some());
        assert!(node(&composer, "agent.image.0.remove").is_some());
        assert!(node(&composer, "agent.reasoning.cycle").is_some());
        assert_eq!(composer_media_height(&panel), 30.0);
        assert!(metrics_text(&panel, Language::English, 0, 0).contains("Automatic"));
    }

    #[test]
    fn harness_cpu_layout_keeps_grouped_activity_and_composer_inside_the_panel() {
        // UI fixture, not evidence of a model run or of native window input.
        let mut panel = AgentPanel::default();
        panel.language = Language::Spanish;
        panel.document_name = "Vista de prueba".into();
        panel.sidebar_open = false;
        let mut user = message("Revisa la estructura de esta escena.");
        user.role = MessageRole::User;
        let comment = message("Consulto la escena y reviso sus objetos.");
        let mut tool = message(
            &serde_json::to_string(&AgentToolResult::success(
                "Consulta de escena",
                serde_json::json!({}),
            ))
            .unwrap(),
        );
        tool.role = MessageRole::Tool;
        panel.runtime.messages = vec![user, comment, tool.clone(), {let mut second=tool;second.id=Uuid::new_v4();second},
            message("## Resumen\nRespuesta de prueba con actividad agrupada.\n```rust\nlet scene_name = \"World\";\n```")];
        let mut settings = EngineSettings::default();
        settings.language = Language::Spanish;
        settings.agent_mode = AgentMode::Inspect;
        let surface = build_dock_agent_surface(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            AgentReadiness::Ready,
            ProjectType::Game,
            [760.0, 640.0],
            0.0,
            0.0,
        );
        let mut host = CpuUiSurfaceHost::new(surface, [9, 9, 10, 255]);
        let rendered = host.render([760, 640], |key| t(key, Language::Spanish));
        let rect = |id: &str| {
            rendered
                .frame
                .layout_boxes
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .rect
        };
        let composer = rect("agent.composer");
        let input = rect("agent.input");
        let reasoning = rect("agent.reasoning.cycle");
        assert!(input.y >= composer.y && input.bottom() <= composer.bottom() + 1.0);
        assert!(reasoning.x >= composer.x && reasoning.right() <= composer.right() + 1.0);
        assert!(composer.bottom() <= 641.0);
        assert!(rendered.frame.text_requests.iter().any(|request| request
            .text_key
            .contains("Vista de prueba \u{00b7}")));
        assert!(rendered
            .frame
            .layout_boxes
            .iter()
            .any(|item| item.id.starts_with("agent.activity.")));
        assert!(!rendered
            .frame
            .layout_boxes
            .iter()
            .any(|item| item.id == "agent.message.2.row"));
        let path = std::env::temp_dir().join(format!("raf-harness-ui-{}.png", Uuid::new_v4()));
        image::save_buffer(&path, rendered.pixels, 760, 640, image::ColorType::Rgba8).unwrap();
        eprintln!("Harness CPU fixture: {}", path.display());
    }

    #[test]
    fn harness_workspace_tools_are_compact_and_failures_remain_visible() {
        let mut tool = message(
            &serde_json::to_string(&AgentToolResult::success(
                "Created objects",
                serde_json::json!({}),
            ))
            .unwrap(),
        );
        tool.role = MessageRole::Tool;
        let height = estimated_message_height_with_variant(
            &tool,
            700.0,
            AgentSurfaceVariant::Embedded,
            false,
        );
        assert!(height < 70.0);
        let card = message_card_with_variant(
            StudioUiPalette::IndustrialDark,
            &tool,
            700.0,
            0,
            MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
            AgentSurfaceVariant::Embedded,
            false,
        );
        assert!(node(&card, "agent.message.0.row.toggle").is_some());
        assert!(node(&card, "agent.message.0.copy").is_none());
        tool.content = "Error: Verification failed".into();
        assert!(tool_needs_attention(&tool));
        assert!(
            estimated_message_height_with_variant(
                &tool,
                700.0,
                AgentSurfaceVariant::Embedded,
                false
            ) > height
        );
    }

    fn node<'a>(root: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if root.id == id {
            return Some(root);
        }
        root.children.iter().find_map(|child| node(child, id))
    }

    #[test]
    fn assistant_messages_expose_selectable_text_and_a_copy_action() {
        let message = message("response from the agent");
        let card = message_card_with_variant(
            StudioUiPalette::IndustrialDark,
            &message,
            700.0,
            0,
            MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
            AgentSurfaceVariant::Embedded,
            false,
        );
        let text = node(&card, "agent.message.0.body.text").expect("message text");
        let copy = node(&card, "agent.message.0.copy").expect("copy button");

        assert!(text.text_selectable);
        assert!(text.focusable);
        assert!(copy.focusable);
        assert!(copy.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            UiAction::SetClipboard { text } if text == "response from the agent"
        )));
    }

    #[test]
    fn virtual_range_keeps_small_overscan_for_long_history() {
        let messages = (0..200)
            .map(|index| message(&format!("message {index}")))
            .collect::<Vec<_>>();
        let heights = messages
            .iter()
            .map(|message| {
                estimated_message_height_with_variant(
                    message,
                    700.0,
                    AgentSurfaceVariant::Embedded,
                    false,
                )
            })
            .collect::<Vec<_>>();
        let range = visible_message_range(&heights, 5_000.0, 360.0);
        assert!(range.end - range.start < 30);
        assert!(range.start > 0);
    }

    #[test]
    fn virtual_history_spacers_do_not_add_a_trailing_gap() {
        let heights = [100.0, 200.0, 300.0];

        assert_eq!(estimated_prefix_height(&heights, 2), 308.0);
        assert_eq!(estimated_suffix_height(&heights, 1), 508.0);
    }

    #[test]
    fn long_message_height_estimate_preserves_its_full_scroll_extent() {
        let message = message(&"Long Agent line ".repeat(1_000));
        let height = estimated_message_height_with_variant(
            &message,
            700.0,
            AgentSurfaceVariant::Embedded,
            false,
        );

        assert!(height > 1_000.0);
    }

    #[test]
    fn message_text_is_bounded_without_splitting_unicode() {
        let source = "\u{00e9}".repeat(MAX_VISIBLE_MESSAGE_CHARS + 10);
        let bounded = bounded_text(&source, MAX_VISIBLE_MESSAGE_CHARS);
        assert_eq!(bounded.chars().count(), MAX_VISIBLE_MESSAGE_CHARS + 1);
        assert!(bounded.ends_with('\u{2026}'));
    }

    #[test]
    fn lightweight_markdown_keeps_readable_text_without_parser_cost() {
        let text = lightweight_markdown_text("### **Scene**\n- `Create` a cube\n\nReady");

        assert_eq!(text, "Scene\n\u{2022} Create a cube\n\nReady");
    }

    #[test]
    fn long_message_text_is_split_into_bounded_atlas_tiles() {
        let source = "Long Agent response ".repeat(180);
        let chunks = message_text_chunks(&source);

        assert!(chunks.len() > 1);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.chars().count() <= MESSAGE_TEXT_CHUNK_CHARS));
        assert_eq!(chunks.concat(), source);
    }

    #[test]
    fn maximum_agent_message_fits_the_bounded_text_atlas_as_tiles() {
        let source = "Agent output ".repeat(1_500);
        let mut message = message(&source);
        message.role = MessageRole::Tool;
        let surface = UiSurface::new(
            "agent-long-message-atlas",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(message_card_with_variant(
                    StudioUiPalette::IndustrialDark,
                    &message,
                    700.0,
                    0,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                    AgentSurfaceVariant::Embedded,
                    true,
                ))
                .with_child(
                    UiNode::new("status.fps", UiNodeKind::Label)
                        .with_text_value("FPS: 60")
                        .with_layout(UiLayout::fixed(76.0, 18.0)),
                ),
        );
        let mut session = UiSurfaceSession::default();
        let frame = session.build_frame_with_resolved_text_at_scale(
            &surface,
            760,
            7_000,
            [0, 0, 0, 255],
            1.5,
            |key| key.to_string(),
        );
        let body_requests = frame
            .text_requests
            .iter()
            .filter(|request| request.node_id.starts_with("agent.message.0.body.text"))
            .collect::<Vec<_>>();
        let fps_request = frame
            .text_requests
            .iter()
            .find(|request| request.node_id == "status.fps")
            .expect("status text remains in the frame after a long message");

        assert!((2..=8).contains(&body_requests.len()));
        assert!(body_requests.iter().all(|request| session
            .text_atlas
            .slot_for(request, &request.text_key)
            .is_some()));
        assert!(session
            .text_atlas
            .slot_for(fps_request, &fps_request.text_key)
            .is_some());
    }

    #[test]
    fn deep_scroll_keeps_only_a_bounded_window_of_one_large_message() {
        let source = "Tool output row with several values and paths\n".repeat(500);
        let mut message = message(&source);
        message.role = MessageRole::Tool;
        let surface = UiSurface::new(
            "agent-deep-message-window",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(message_card_with_variant(
                    StudioUiPalette::IndustrialDark,
                    &message,
                    700.0,
                    0,
                    MessageViewport {
                        top: 2_500.0,
                        bottom: 2_960.0,
                    },
                    AgentSurfaceVariant::Embedded,
                    true,
                )),
        );
        let mut session = UiSurfaceSession::default();
        let frame = session.build_frame_with_resolved_text_at_scale(
            &surface,
            760,
            7_000,
            [0, 0, 0, 255],
            1.5,
            |key| key.to_string(),
        );
        let body_requests = frame
            .text_requests
            .iter()
            .filter(|request| request.node_id.starts_with("agent.message.0.body.text"))
            .collect::<Vec<_>>();
        let body = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.body")
            .expect("virtualized message body");

        assert!(!body_requests.is_empty());
        assert!(body_requests.len() <= 8);
        assert!(body_requests
            .iter()
            .all(|request| request.node_id != "agent.message.0.body.text"));
        assert!(body.rect.height > 3_000.0);
    }

    #[test]
    fn session_rows_keep_distinct_vertical_tracks() {
        let mut panel = AgentPanel::default();
        panel.history.start_session("First chat");
        panel.history.start_session("Second chat");
        let surface = UiSurface::new(
            "agent-sessions-layout",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::None))
                .with_child(build_sidebar(
                    StudioUiPalette::IndustrialDark,
                    &panel,
                    280.0,
                    180.0,
                    AgentSurfaceVariant::Embedded,
                )),
        );
        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 280, 180, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        let first = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.session.row.0")
            .expect("first session row");
        let second = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.session.row.1")
            .expect("second session row");

        assert_eq!(first.rect.height, 36.0);
        assert_eq!(second.rect.height, 36.0);
        assert!(second.rect.y >= first.rect.bottom());
    }

    #[test]
    fn message_rows_reserve_height_instead_of_overlapping() {
        let first = message("first");
        let second = message("second");
        let surface = UiSurface::new(
            "agent-history-layout",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(message_card_with_variant(
                    StudioUiPalette::IndustrialDark,
                    &first,
                    360.0,
                    0,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                    AgentSurfaceVariant::Embedded,
                    false,
                ))
                .with_child(message_card_with_variant(
                    StudioUiPalette::IndustrialDark,
                    &second,
                    360.0,
                    1,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                    AgentSurfaceVariant::Embedded,
                    false,
                )),
        );
        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 400, 240, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        let first_row = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.row")
            .expect("first message row");
        let second_row = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.1.row")
            .expect("second message row");

        assert!(second_row.rect.y >= first_row.rect.bottom());
    }

    #[test]
    fn wrapped_message_rows_advance_by_their_measured_height() {
        let first = message(
            "A long assistant response that must wrap across several lines inside the message card. \
             It must keep the following message below it instead of painting over it.",
        );
        let second = message("second");
        let surface = UiSurface::new(
            "agent-wrapped-history-layout",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(message_card_with_variant(
                    StudioUiPalette::IndustrialDark,
                    &first,
                    260.0,
                    0,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                    AgentSurfaceVariant::Embedded,
                    false,
                ))
                .with_child(message_card_with_variant(
                    StudioUiPalette::IndustrialDark,
                    &second,
                    260.0,
                    1,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                    AgentSurfaceVariant::Embedded,
                    false,
                )),
        );
        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 300, 240, [0, 0, 0, 255], |key| {
                match key {
                    "app.agent_label" => "Agent".to_string(),
                    _ => key.to_string(),
                }
            });
        let first_row = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.row")
            .expect("first wrapped message row");
        let second_row = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.1.row")
            .expect("second message row");
        let first_body = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.body.text")
            .expect("first body text");
        assert!(first_body.rect.height > 18.0);
        assert!(second_row.rect.y >= first_row.rect.bottom());
    }

    #[test]
    fn dropdown_menus_have_vertical_flow_and_visible_bounds() {
        let panel = AgentPanel::default();
        let settings = EngineSettings::default();
        let model_menu = build_model_menu(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            720.0,
            AgentSurfaceVariant::Embedded,
        );
        let mode_menu = build_mode_menu(
            StudioUiPalette::IndustrialDark,
            &settings,
            720.0,
            AgentSurfaceVariant::Embedded,
        );

        assert_eq!(model_menu.layout.flow, UiFlow::Column);
        assert_eq!(mode_menu.layout.flow, UiFlow::Column);
        assert!(model_menu.layout.rect.is_some_and(|rect| rect.height > 0.0));
        assert!(mode_menu.layout.rect.is_some_and(|rect| rect.height > 0.0));
    }

    #[test]
    fn dock_mode_select_survives_the_real_input_protocol() {
        let size = [760u32, 520];
        let mut panel = AgentPanel::default();
        let mut settings = EngineSettings::default();
        settings.agent_mode = AgentMode::Active;
        let resolve = |key: &str| key.to_string();
        let idle = UiInputState::default();
        let mut host =
            CpuUiSurfaceHost::new(dock_wrapper_surface(&panel, &settings), [10, 10, 20, 255]);
        let _ = host.process_input(size, resolve, &idle);
        let trigger = host
            .layout_rect("agent.mode.trigger")
            .expect("mode trigger rect");
        let trigger_point = [
            trigger.x + trigger.width * 0.5,
            trigger.y + trigger.height * 0.5,
        ];
        let open_click = UiInputState {
            pointer_position: Some(trigger_point),
            pointer_down: true,
            pointer_buttons_down: vec![raf_ui::UiPointerButton::Primary],
            pointer_pressed_buttons: vec![raf_ui::UiPointerButton::Primary],
            pointer_released_buttons: vec![raf_ui::UiPointerButton::Primary],
            ..UiInputState::default()
        };
        let opened = host.process_input(size, resolve, &open_click);
        assert!(opened.iter().any(|dispatched| matches!(
            &dispatched.action,
            raf_ui::UiAction::SetSelectOpen {
                id,
                open: true
            } if id == "agent.mode.trigger"
        )));
        panel.mode_menu_open = true;
        host.set_surface(dock_wrapper_surface(&panel, &settings));
        let _ = host.process_input(size, resolve, &idle);
        let option = host
            .layout_rect("agent.mode.option.plan")
            .expect("plan option rect");
        let option_point = [
            option.x + option.width * 0.5,
            option.y + option.height * 0.5,
        ];
        let press = UiInputState {
            pointer_position: Some(option_point),
            pointer_down: true,
            pointer_buttons_down: vec![raf_ui::UiPointerButton::Primary],
            pointer_pressed_buttons: vec![raf_ui::UiPointerButton::Primary],
            ..UiInputState::default()
        };
        let pressed = host.process_input(size, resolve, &press);
        assert!(pressed.iter().all(|dispatched| !matches!(
            &dispatched.action,
            raf_ui::UiAction::SetSelectOpen { open: false, .. }
        )));
        let release = UiInputState {
            pointer_position: Some(option_point),
            pointer_released_buttons: vec![raf_ui::UiPointerButton::Primary],
            ..UiInputState::default()
        };
        let released = host.process_input(size, resolve, &release);
        assert!(released.iter().any(|dispatched| matches!(
            &dispatched.action,
            raf_ui::UiAction::SetSelect {
                key,
                value,
                ..
            } if key == "agent.mode" && value == "plan"
        )));
        assert!(released.iter().any(|dispatched| matches!(
            &dispatched.action,
            raf_ui::UiAction::SetSelectOpen {
                id,
                open: false
            } if id == "agent.mode.trigger"
        )));
    }

    #[test]
    fn selector_click_dispatches_the_shared_select_contract() {
        let panel = AgentPanel::default();
        let settings = EngineSettings::default();
        let surface = build_agent_surface(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            AgentReadiness::ProviderDisabled,
            ProjectType::Game,
            [720.0, 400.0],
            0.0,
            1.0,
        );
        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 720, 400, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        let trigger = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.model.trigger")
            .expect("model selector");
        let point = [trigger.rect.x + 12.0, trigger.rect.y + 12.0];
        let _ = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some(point),
                pointer_down: true,
                time_seconds: 0.1,
                ..UiInputState::default()
            },
        );
        let actions = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some(point),
                time_seconds: 0.2,
                ..UiInputState::default()
            },
        );

        assert!(actions.iter().any(|action| {
            matches!(
                action.action,
                raf_ui::UiAction::SetSelectOpen { ref id, open: true }
                    if id == "agent.model.trigger"
            )
        }));
    }

    #[test]
    fn model_menu_custom_id_action_is_reachable_and_compact_selectors_keep_their_width() {
        let mut panel = AgentPanel::default();
        panel.model_menu_open = true;
        let settings = EngineSettings::default();
        let surface = build_agent_surface(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            AgentReadiness::ProviderDisabled,
            ProjectType::Game,
            [720.0, 400.0],
            0.0,
            1.0,
        );
        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 720, 400, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        // The single-field custom model entry lives inline in the picker: the
        // old separate add-model dialog is gone.
        let confirm = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.model.confirm")
            .expect("custom model confirmation");
        assert!(confirm.interactive);

        let point = [confirm.rect.x + 12.0, confirm.rect.y + 12.0];
        let _ = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some(point),
                pointer_down: true,
                time_seconds: 0.1,
                ..UiInputState::default()
            },
        );
        let actions = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some(point),
                time_seconds: 0.2,
                ..UiInputState::default()
            },
        );
        assert!(actions.iter().any(|action| matches!(
            action.action,
            raf_ui::UiAction::Command { ref name } if name == "agent.model.confirm"
        )));

        let narrow_header = build_header(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            "OpenRouter".to_string(),
            "Default".to_string(),
            "app.agent_mode_plan",
            340.0,
            AgentSurfaceVariant::Embedded,
        );
        let narrow_surface = UiSurface::new(
            "agent-narrow-header",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_child(narrow_header),
        );
        let narrow_frame = narrow_surface.build_frame(340, 400, [0, 0, 0, 255]);
        let model = narrow_frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.model.trigger")
            .expect("narrow model selector");
        let mode = narrow_frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.mode.trigger")
            .expect("narrow mode selector");
        assert!(model.rect.width >= 100.0);
        assert!(mode.rect.width >= 80.0);
        assert!(mode.rect.x >= model.rect.right());
    }

    fn dock_wrapper_surface(
        panel: &AgentPanel,
        settings: &EngineSettings,
    ) -> raf_render::api_graphic_basic::ui_surface::UiSurface {
        build_dock_agent_surface(
            StudioUiPalette::IndustrialDark,
            panel,
            settings,
            AgentReadiness::ProviderDisabled,
            ProjectType::Game,
            [760.0, 520.0],
            0.0,
            1.0,
        )
    }

    fn tool_message(content: &str) -> ChatMessage {
        let mut message = message(content);
        message.role = MessageRole::Tool;
        message
    }

    fn dock_frame(
        panel: &AgentPanel,
        settings: &EngineSettings,
        width: f32,
        height: f32,
    ) -> raf_render::api_graphic_basic::ui_surface::UiSurfaceFrame {
        let surface = build_dock_agent_surface(
            StudioUiPalette::IndustrialDark,
            panel,
            settings,
            AgentReadiness::ProviderDisabled,
            ProjectType::Game,
            [width, height],
            0.0,
            1.0,
        );
        let mut session = UiSurfaceSession::default();
        session.build_frame_with_resolved_text(&surface, 800, 800, [0, 0, 0, 255], |key| {
            key.to_string()
        })
    }

    #[test]
    fn dock_collapses_tool_results_into_rows_until_expanded() {
        let mut panel = AgentPanel::default();
        panel
            .runtime
            .messages
            .push(tool_message("Tool result ... Revision 7 ..."));

        let frame = dock_frame(&panel, &EngineSettings::default(), 720.0, 520.0);
        let row = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.row")
            .expect("collapsed tool row in dock");
        assert!(row.rect.height <= 40.0);
        assert!(row.rect.height.min(1.0).gt(&0.0));
        assert!(frame
            .layout_boxes
            .iter()
            .all(|layout| layout.id != "agent.message.0.body"));
    }

    #[test]
    fn dock_expands_tool_results_after_the_panel_marks_them_expanded() {
        let mut panel = AgentPanel::default();
        panel
            .runtime
            .messages
            .push(tool_message("Tool result ... Revision 7 ..."));
        let expanded_id = panel.runtime.messages[0].id.to_string();
        panel.expanded_tools.insert(expanded_id);

        let frame = dock_frame(&panel, &EngineSettings::default(), 720.0, 520.0);
        let body = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.body")
            .expect("expanded tool body in dock");
        let row = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.row")
            .expect("tool toggle row stays visible while expanded");
        assert!(row.rect.height <= 40.0);
        assert!(body.rect.y >= row.rect.bottom() - 1.0);
    }

    #[test]
    fn dock_keeps_assistant_messages_fully_visible_inside_the_tool_viewport() {
        let mut panel = AgentPanel::default();
        panel
            .runtime
            .messages
            .push(message("assistant answer body"));

        let frame = dock_frame(&panel, &EngineSettings::default(), 720.0, 520.0);
        let body = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.body.text")
            .expect("assistant body text in dock");
        let card = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.card")
            .expect("assistant card in dock");
        assert!(body.rect.bottom() <= card.rect.bottom() + 1.0);
    }

    #[test]
    fn dock_shows_the_review_chip_only_when_there_is_room_for_it() {
        let panel = AgentPanel::default();
        let settings = EngineSettings::default();

        let wide = dock_frame(&panel, &settings, 900.0, 520.0);
        assert!(wide
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "agent.review.chip"));

        let narrow = dock_frame(&panel, &settings, 540.0, 520.0);
        assert!(narrow
            .layout_boxes
            .iter()
            .all(|layout| layout.id != "agent.review.chip"));
    }

    #[test]
    fn active_permissions_hide_review_and_approval_controls_in_workspace_and_dock() {
        let mut panel = AgentPanel::default();
        panel.runtime.status = AgentStatus::AwaitingApproval;
        panel
            .runtime
            .pending_calls
            .push(raf_ai::agent_runtime::PendingToolCall::new(
                "legacy-review",
                "scene_create",
                serde_json::json!({}),
            ));
        for language in [Language::English, Language::Spanish] {
            let settings = EngineSettings {
                agent_mode: AgentMode::Active,
                language,
                ..Default::default()
            };
            for palette in [StudioUiPalette::IndustrialDark, StudioUiPalette::PaperLight] {
                for variant in [AgentSurfaceVariant::Embedded, AgentSurfaceVariant::Dock] {
                    for width in [540u32, 900] {
                        let surface = build_agent_surface_with_variant(
                            palette,
                            &panel,
                            &settings,
                            AgentReadiness::Ready,
                            ProjectType::Game,
                            [width as f32, 640.0],
                            0.0,
                            0.0,
                            variant,
                            None,
                        );
                        let mut session = UiSurfaceSession::default();
                        let frame = session.build_frame_with_resolved_text(
                            &surface,
                            width,
                            640,
                            [0; 4],
                            |key| t(key, language),
                        );
                        assert!(frame
                            .layout_boxes
                            .iter()
                            .any(|item| item.id == "agent.input"));
                        assert!(frame
                            .layout_boxes
                            .iter()
                            .all(|item| !item.id.starts_with("agent.approval")
                                && !item.id.starts_with("agent.review")));
                    }
                }
            }
        }
    }

    #[test]
    fn embedded_header_never_shows_the_dock_review_chip() {
        let panel = AgentPanel::default();
        let settings = EngineSettings::default();
        let surface = build_agent_surface(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            AgentReadiness::ProviderDisabled,
            ProjectType::Game,
            [1100.0, 640.0],
            0.0,
            1.0,
        );
        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 1100, 640, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        assert!(frame
            .layout_boxes
            .iter()
            .all(|layout| layout.id != "agent.review.chip"));
    }

    #[test]
    fn dock_replaces_the_composer_with_the_approval_composer_when_requested() {
        let mut panel = AgentPanel::default();
        panel.runtime.status = AgentStatus::AwaitingApproval;
        panel
            .runtime
            .pending_calls
            .push(raf_ai::agent_runtime::PendingToolCall::new(
                "op-1",
                "scene.create_node",
                serde_json::json!({ "name": "Cube" }),
            ));
        let settings = EngineSettings::default();

        let frame = dock_frame(&panel, &settings, 720.0, 520.0);
        let composer = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.composer")
            .expect("dock approval composer");
        let input = frame
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "agent.input");
        assert!(composer.rect.height >= 180.0 && !input);

        let embedded_surface = build_agent_surface(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            AgentReadiness::ProviderDisabled,
            ProjectType::Game,
            [820.0, 560.0],
            0.0,
            1.0,
        );
        let mut session = UiSurfaceSession::default();
        let embedded = session.build_frame_with_resolved_text(
            &embedded_surface,
            820,
            560,
            [0, 0, 0, 255],
            |key| key.to_string(),
        );
        let embedded_keeps_live_composer = embedded
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "agent.input");
        assert!(embedded_keeps_live_composer);
        assert!(embedded
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "agent.approval.operation"));
    }
}
