//! Retained RafUI surface for the native Agent dock.
//!
//! This module is deliberately a projection only. Runtime polling, history
//! persistence, provider configuration and tool execution stay in
//! `ai_chat.rs` and the native workbench. The surface keeps a bounded visible
//! message window so long conversations do not turn into a per-frame layout
//! workload.

use raf_ai::agent_model_registry::AgentModelRegistry;
use raf_ai::agent_runtime::{AgentStatus, AgentToolResult};
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_core::ai::AgentMode;
use raf_core::config::{EngineSettings, Language};
use raf_core::i18n::t;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow,
    UiIcon, UiIconId, UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiRect,
    UiScrollAxis, UiSelect, UiSelectOption, UiSizeMode, UiSpacing, UiStyle, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface, UiSurfaceMaterial,
    UiTextOverflow, UiTextStyle, UiVirtualRange,
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
    let width = size[0].max(1.0);
    let height = size[1].max(1.0);
    let requested_sidebar_width = (width * 0.30).clamp(184.0, 236.0).min(width * 0.52);
    let sidebar_width = requested_sidebar_width * sidebar_progress.clamp(0.0, 1.0);
    let main_width = (width - sidebar_width).max(1.0);
    let tokens = palette.tokens();

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
        .with_child(build_sidebar(palette, panel, sidebar_width, height))
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
        ));

    let mut surface = UiSurface::new("editor.bottom.agent", palette, root);
    surface.style_sheet = agent_style_sheet(palette);
    surface
}

fn build_sidebar(palette: StudioUiPalette, panel: &AgentPanel, width: f32, height: f32) -> UiNode {
    let tokens = palette.tokens();
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
            let select = UiNode::new(format!("agent.session.{index}"), UiNodeKind::Button)
                .with_class(if is_active {
                    "agent-session-active"
                } else {
                    "agent-session"
                })
                .with_layout(UiLayout {
                    grow: 1.0,
                    width_mode: UiSizeMode::Fill,
                    height_mode: UiSizeMode::Fixed,
                    basis: [0.0, 40.0],
                    padding: UiSpacing::xy(10.0, 8.0),
                    ..UiLayout::default()
                })
                .with_text_value(short_text(&session.title, 72))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(if is_active {
                    tokens.text
                } else {
                    tokens.text_muted
                }))
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
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session.delete:{index}"),
                ));
            sessions = sessions.with_child(
                UiNode::new(format!("agent.session.row.{index}"), UiNodeKind::Toolbar)
                    .with_class("agent-session-row")
                    .with_layout({
                        let mut layout =
                            UiLayout::fixed(0.0, 40.0).with_width_mode(UiSizeMode::Fill);
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
) -> UiNode {
    let provider_label = panel
        .effective_provider(settings)
        .map(|provider| provider.provider.display_name().to_string())
        .unwrap_or_else(|| t("app.agent_provider_default", settings.language));
    let model_label = if panel.selected_model == AgentModelRegistry::PROVIDER_DEFAULT {
        t("app.agent_provider_default_short", settings.language)
    } else {
        short_text(&panel.selected_model, 32)
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
        .with_child(build_status_strip(palette, panel, settings, readiness));

    let visible_messages = panel
        .runtime
        .messages
        .iter()
        .filter(|message| {
            message.role != MessageRole::System
                && !(message.role == MessageRole::Assistant
                    && message.content.trim().is_empty()
                    && message.tool_calls.is_some())
        })
        .collect::<Vec<_>>();
    let history_width = (width - 22.0).max(180.0);
    content = content.with_child(build_history(
        palette,
        panel,
        project_type,
        &visible_messages,
        history_width,
        scroll_offset,
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
    ));
    main = main.with_child(content);

    if panel.model_menu_open {
        main = main.with_child(build_model_menu(palette, panel, settings, width));
    }
    if panel.mode_menu_open {
        main = main.with_child(build_mode_menu(palette, settings, width));
    }
    if panel.add_model_open {
        main = main.with_child(build_add_model_menu(
            palette, panel, settings, width, height,
        ));
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
) -> UiNode {
    let tokens = palette.tokens();
    let title = panel
        .history
        .active_index
        .and_then(|index| panel.history.sessions.get(index))
        .map(|session| short_text(&session.title, 54))
        .unwrap_or_else(|| t("app.agent_title", settings.language));
    let (model_width, mode_width) = selector_widths(width);
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
            &panel.model_registry,
            &panel.selected_model,
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
        ))
        .with_child(icon_button(
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

fn build_status_strip(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    readiness: AgentReadiness,
) -> UiNode {
    let tokens = palette.tokens();
    let mut strip = UiNode::new("agent.status.strip", UiNodeKind::Panel)
        .with_class("agent-status-strip")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(12.0, 6.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });

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
        strip = strip.with_child(
            UiNode::new("agent.active-warning", UiNodeKind::Label)
                .with_class("agent-active-warning")
                .with_text_key("app.agent_active_mode_warning")
                .with_text_style(UiTextStyle::body(tokens.warning))
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_icon(UiIcon::new(UiIconId::Warning).with_size(UiIconSize::Small))
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(8.0, 6.0),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                }),
        );
    }

    match panel.runtime.status {
        AgentStatus::Done => {}
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
        AgentStatus::AwaitingApproval => {
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
    let tokens = palette.tokens();
    let tool = activity.current_tool.as_deref();
    let activity_key = match tool {
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
    let tokens = palette.tokens();
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

fn build_history(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    project_type: ProjectType,
    messages: &[&ChatMessage],
    width: f32,
    scroll_offset: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let message_heights = messages
        .iter()
        .map(|message| estimated_message_height(message, width))
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
                .with_child(
                    UiNode::new("agent.empty.title", UiNodeKind::Label)
                        .with_text_key("app.agent_empty_title")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fit_content()),
                )
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
        for (index, message) in messages
            .iter()
            .enumerate()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
        {
            let viewport =
                MessageViewport::for_message(scroll_offset, HISTORY_VIEWPORT_ESTIMATE, message_top);
            scroll = scroll.with_child(message_card(palette, message, width, index, viewport));
            message_top += message_heights[index] + 8.0;
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

fn build_suggestions(palette: StudioUiPalette, project_type: ProjectType) -> UiNode {
    let tokens = palette.tokens();
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
) -> UiNode {
    let tokens = palette.tokens();
    let running = matches!(
        panel.runtime.status,
        AgentStatus::Thinking | AgentStatus::ExecutingTools | AgentStatus::AwaitingApproval
    );
    let composer = UiNode::new("agent.composer", UiNodeKind::Toolbar)
        .with_class("agent-composer")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(12.0, 8.0),
            ..UiLayout::fixed(0.0, if width < 500.0 { 112.0 } else { 96.0 })
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
                    action_button("agent.send", "app.agent_send", "agent.submit", true)
                        .disabled(!matches!(readiness, AgentReadiness::Ready))
                }),
        )
        .with_child(
            UiNode::new("agent.composer.footer", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::SpaceBetween,
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("agent.composer.hint", UiNodeKind::Label)
                        .with_text_key("app.agent_enter_to_send")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(
                    UiNode::new("agent.composer.metrics", UiNodeKind::Label)
                        .with_class("agent-metrics")
                        .with_text_value(metrics_text(
                            panel,
                            settings.language,
                            settings.agent_max_response_tokens,
                            message_count,
                        ))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content()),
                ),
        );
    composer
}

fn build_model_menu(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let labels = panel.model_registry.selector_labels();
    let options = labels
        .iter()
        .cloned()
        .map(|label| UiSelectOption::new(label.clone(), label))
        .collect::<Vec<_>>();
    let (model_width, mode_width) = selector_widths(width);
    let menu_height = (labels.len().saturating_add(1) as f32 * 34.0).clamp(68.0, 300.0);
    let mut menu_layout = UiLayout::absolute(UiRect::new(
        (width - 50.0 - mode_width - 8.0 - model_width).max(8.0),
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
    for label in labels {
        let value = if label == AgentModelRegistry::PROVIDER_DEFAULT {
            t("app.agent_provider_default_short", settings.language)
        } else {
            panel
                .model_registry
                .get(&label)
                .map(|shortcut| {
                    format!(
                        "{}  |  {}  ·  {}",
                        shortcut.label,
                        shortcut.model_id,
                        shortcut.provider.display_name()
                    )
                })
                .unwrap_or_else(|| label.clone())
        };
        menu = menu.with_child(
            UiNode::new(format!("agent.model.option.{label}"), UiNodeKind::Button)
                .with_class("agent-menu-item")
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(10.0, 8.0),
                    ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_text_value(value.clone())
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_tooltip_value(value)
                .with_accessibility_role(UiAccessibilityRole::Option)
                .with_accessibility_selected(panel.selected_model == label)
                .focusable()
                .with_event(UiEventBinding {
                    event: UiEventKind::Click,
                    action: UiAction::SetSelect {
                        key: "agent.model".to_string(),
                        value: label.clone(),
                        index: options
                            .iter()
                            .position(|option| option.value == label)
                            .unwrap_or(0),
                    },
                })
                .with_event(UiEventBinding {
                    event: UiEventKind::Click,
                    action: UiAction::SetSelectOpen {
                        id: "agent.model.trigger".to_string(),
                        open: false,
                    },
                }),
        );
    }
    menu.with_child(
        UiNode::new("agent.model.add", UiNodeKind::Button)
            .with_class("agent-menu-item-accent")
            .with_layout(UiLayout {
                padding: UiSpacing::xy(10.0, 8.0),
                ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
            })
            .with_text_key("app.agent_add_model")
            .with_text_style(UiTextStyle::button(tokens.accent_hot))
            .with_accessibility_role(UiAccessibilityRole::MenuItem)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "agent.model.add",
            )),
    )
}

fn build_mode_menu(palette: StudioUiPalette, settings: &EngineSettings, width: f32) -> UiNode {
    let tokens = palette.tokens();
    let (_, mode_width) = selector_widths(width);
    let mut menu_layout = UiLayout::absolute(UiRect::new(
        (width - 50.0 - mode_width).max(8.0),
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

fn build_add_model_menu(
    palette: StudioUiPalette,
    panel: &AgentPanel,
    settings: &EngineSettings,
    width: f32,
    height: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let provider_name = panel
        .effective_provider(settings)
        .map(|provider| provider.provider.display_name().to_string())
        .unwrap_or_else(|| t("app.agent_provider_default", settings.language));
    let menu_height = if panel.new_model_error.is_some() {
        292.0
    } else {
        264.0
    };
    let mut menu_layout = UiLayout::absolute(UiRect::new(
        (width - 360.0).max(8.0),
        ((height - menu_height) * 0.5).max(58.0),
        352.0_f32.min((width - 16.0).max(1.0)),
        menu_height,
    ));
    menu_layout.flow = UiFlow::Column;
    menu_layout.gap = 8.0;
    menu_layout.padding = UiSpacing::same(12.0);
    menu_layout.overflow = UiOverflow::Clip;
    let mut menu = UiNode::new("agent.add-model.menu", UiNodeKind::Menu)
        .with_class("agent-menu agent-add-model")
        .with_material(UiSurfaceMaterial::ModalSurface)
        .with_layout(menu_layout.with_z_index(40))
        .with_accessibility_role(UiAccessibilityRole::Dialog)
        .with_child(
            UiNode::new("agent.add-model.title", UiNodeKind::Label)
                .with_text_key("app.agent_add_model_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("agent.add-model.provider", UiNodeKind::Label)
                .with_text_value(format!(
                    "{}: {}",
                    t("app.agent_add_model_provider", settings.language),
                    provider_name
                ))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(model_text_input(
            "agent.new-model.label",
            "app.agent_model_label",
            panel.new_model_label.clone(),
        ))
        .with_child(model_text_input(
            "agent.new-model.id",
            "app.agent_model_id",
            panel.new_model_id.clone(),
        ));
    if let Some(error) = panel.new_model_error.as_deref() {
        menu = menu.with_child(
            UiNode::new("agent.add-model.error", UiNodeKind::Label)
                .with_text_value(error.to_string())
                .with_text_style(UiTextStyle::body(tokens.danger))
                .with_layout(UiLayout::fit_content()),
        );
    }
    menu.with_child(
        UiNode::new("agent.add-model.actions", UiNodeKind::Toolbar)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 6.0,
                justify_content: UiJustify::End,
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            })
            .with_child(action_button(
                "agent.add-model.cancel",
                "app.agent_cancel",
                "agent.model.cancel",
                false,
            ))
            .with_child(action_button(
                "agent.add-model.settings",
                "app.agent_open_ai_settings",
                "agent.open-settings",
                false,
            ))
            .with_child(action_button(
                "agent.add-model.confirm",
                "app.agent_add_model_confirm",
                "agent.model.confirm",
                true,
            )),
    )
}

fn message_card(
    palette: StudioUiPalette,
    message: &ChatMessage,
    width: f32,
    index: usize,
    viewport: MessageViewport,
) -> UiNode {
    let tokens = palette.tokens();
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
    let display_text = if matches!(message.role, MessageRole::Assistant | MessageRole::Tool) {
        lightweight_markdown_text(&content)
    } else {
        content.clone()
    };
    let display_chunks = message_text_chunks(&display_text);
    let body_height = display_chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            estimated_text_tile_height(chunk, card_width - 24.0, body_style.line_height_px)
                + if index + 1 < display_chunks.len() {
                    MESSAGE_TEXT_TILE_GAP
                } else {
                    0.0
                }
        })
        .sum::<f32>()
        .max(1.0);
    let card_height = MESSAGE_CARD_CHROME_HEIGHT + body_height;
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
        })
        .with_child(
            UiNode::new(format!("agent.message.{index}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(label_color))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(message_body(
            format!("agent.message.{index}.body"),
            &content,
            body_style,
            message.role,
            card_width - 24.0,
            viewport,
        ));
    if matches!(message.role, MessageRole::Assistant | MessageRole::Tool) {
        card = card.with_child(copy_message_button(
            format!("agent.message.{index}.copy"),
            &message.content,
        ));
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

fn message_body(
    id: String,
    content: &str,
    body_style: UiTextStyle,
    role: MessageRole,
    width: f32,
    viewport: MessageViewport,
) -> UiNode {
    let body_id = id;
    let display_text = if matches!(role, MessageRole::Assistant | MessageRole::Tool) {
        lightweight_markdown_text(content)
    } else {
        content.to_string()
    };
    let chunks = message_text_chunks(&display_text);
    let chunk_heights = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            estimated_text_tile_height(chunk, width, body_style.line_height_px)
                + if index + 1 < chunks.len() {
                    MESSAGE_TEXT_TILE_GAP
                } else {
                    0.0
                }
        })
        .collect::<Vec<_>>();
    let body_height = chunk_heights.iter().sum::<f32>().max(1.0);
    let visible_top = viewport.top - MESSAGE_CARD_CHROME_HEIGHT - MESSAGE_TEXT_TILE_OVERSCAN;
    let visible_bottom = viewport.bottom - MESSAGE_CARD_CHROME_HEIGHT + MESSAGE_TEXT_TILE_OVERSCAN;
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
        body = body.with_child(
            UiNode::new(text_id, UiNodeKind::Label)
                .with_text_value(chunk)
                .selectable_text()
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_text_style(body_style)
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

fn lightweight_markdown_text(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return String::new();
            }
            let without_heading = trimmed.trim_start_matches('#').trim();
            let (bullet, text) = if let Some(value) = without_heading.strip_prefix("- ") {
                (true, value)
            } else if let Some(value) = without_heading.strip_prefix("* ") {
                (true, value)
            } else {
                (false, without_heading)
            };
            let prefix = bullet.then_some("\u{2022} ").unwrap_or_default();
            format!("{prefix}{}", strip_inline_markdown(text))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_inline_markdown(value: &str) -> String {
    value.replace("**", "").replace("__", "").replace('`', "")
}

fn status_label(palette: StudioUiPalette, id: &str, key: &str, icon: UiIconId) -> UiNode {
    let tokens = palette.tokens();
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

fn selector_widths(width: f32) -> (f32, f32) {
    if width < 360.0 {
        (108.0, 112.0)
    } else if width < 560.0 {
        (132.0, 116.0)
    } else {
        (168.0, 124.0)
    }
}

fn model_selector_button(
    id: &str,
    registry: &AgentModelRegistry,
    selected_model: &str,
    value: &str,
    tooltip: &str,
    open: bool,
    width: f32,
) -> UiNode {
    let labels = registry.selector_labels();
    let selected_index = labels
        .iter()
        .position(|label| label == selected_model)
        .unwrap_or(0);
    let options = labels
        .into_iter()
        .map(|label| UiSelectOption::new(label.clone(), label))
        .collect();
    agent_select_trigger(
        id,
        "agent.model",
        options,
        selected_index,
        value,
        tooltip,
        open,
        width,
        "agent.model.menu",
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
    let selected_index = match settings.agent_mode {
        AgentMode::Inspect => 0,
        AgentMode::Plan => 1,
        AgentMode::Active => 2,
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
) -> UiNode {
    let mut select = UiSelect::new(value_key, options, selected_index).with_popup_id(popup_id);
    select.open = open;
    UiNode::select(id, select)
        .with_class("agent-selector")
        .with_layout(UiLayout {
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
        max_response_tokens
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

fn estimated_message_height(message: &ChatMessage, width: f32) -> f32 {
    let content = bounded_text(&display_message_content(message), MAX_VISIBLE_MESSAGE_CHARS);
    let display_text = if matches!(message.role, MessageRole::Assistant | MessageRole::Tool) {
        lightweight_markdown_text(&content)
    } else {
        content
    };
    let card_width = (width * MESSAGE_WIDTH_RATIO).clamp(220.0, 760.0);
    let line_height = 18.0;
    let chunks = message_text_chunks(&display_text);
    let body_height = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            estimated_text_tile_height(chunk, card_width - 24.0, line_height)
                + if index + 1 < chunks.len() {
                    MESSAGE_TEXT_TILE_GAP
                } else {
                    0.0
                }
        })
        .sum::<f32>();
    (MESSAGE_CARD_CHROME_HEIGHT + body_height).max(70.0)
}

fn display_message_content(message: &ChatMessage) -> String {
    if message.role != MessageRole::Tool {
        return message.content.clone();
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
    let tokens = palette.tokens();
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
                tokens.surface,
                tokens.border,
                tokens.text,
                1.0,
                6.0,
            ),
            style(
                "agent-message-user",
                tokens.surface_raised,
                tokens.accent,
                tokens.text,
                1.0,
                6.0,
            ),
            style(
                "agent-message-tool",
                tokens.surface_alt,
                tokens.positive,
                tokens.text,
                1.0,
                4.0,
            ),
            style(
                "agent-message-system",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
                1.0,
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
                tokens.selection,
                tokens.accent,
                tokens.text,
                1.0,
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
    use raf_render::api_graphic_basic::ui_surface::{UiInputState, UiSurfaceSession};
    use uuid::Uuid;

    fn message(content: &str) -> ChatMessage {
        ChatMessage {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_calls: None,
        }
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
        let card = message_card(
            StudioUiPalette::IndustrialDark,
            &message,
            700.0,
            0,
            MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
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
            .map(|message| estimated_message_height(message, 700.0))
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
        let height = estimated_message_height(&message, 700.0);

        assert!(height > 1_000.0);
    }

    #[test]
    fn message_text_is_bounded_without_splitting_unicode() {
        let source = "é".repeat(MAX_VISIBLE_MESSAGE_CHARS + 10);
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
                .with_child(message_card(
                    StudioUiPalette::IndustrialDark,
                    &message,
                    700.0,
                    0,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
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
                .with_child(message_card(
                    StudioUiPalette::IndustrialDark,
                    &message,
                    700.0,
                    0,
                    MessageViewport {
                        top: 2_500.0,
                        bottom: 2_960.0,
                    },
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

        assert_eq!(first.rect.height, 40.0);
        assert_eq!(second.rect.height, 40.0);
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
                .with_child(message_card(
                    StudioUiPalette::IndustrialDark,
                    &first,
                    360.0,
                    0,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                ))
                .with_child(message_card(
                    StudioUiPalette::IndustrialDark,
                    &second,
                    360.0,
                    1,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
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
                .with_child(message_card(
                    StudioUiPalette::IndustrialDark,
                    &first,
                    260.0,
                    0,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
                ))
                .with_child(message_card(
                    StudioUiPalette::IndustrialDark,
                    &second,
                    260.0,
                    1,
                    MessageViewport::for_message(0.0, HISTORY_VIEWPORT_ESTIMATE, 0.0),
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
        let model_menu =
            build_model_menu(StudioUiPalette::IndustrialDark, &panel, &settings, 720.0);
        let mode_menu = build_mode_menu(StudioUiPalette::IndustrialDark, &settings, 720.0);

        assert_eq!(model_menu.layout.flow, UiFlow::Column);
        assert_eq!(mode_menu.layout.flow, UiFlow::Column);
        assert!(model_menu.layout.rect.is_some_and(|rect| rect.height > 0.0));
        assert!(mode_menu.layout.rect.is_some_and(|rect| rect.height > 0.0));
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
    fn model_menu_add_action_is_reachable_and_compact_selectors_keep_their_width() {
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
        let add = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.model.add")
            .expect("add model menu item");
        assert!(add.interactive);

        let point = [add.rect.x + 12.0, add.rect.y + 12.0];
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
            raf_ui::UiAction::Command { ref name } if name == "agent.model.add"
        )));

        let narrow_header = build_header(
            StudioUiPalette::IndustrialDark,
            &panel,
            &settings,
            "OpenRouter".to_string(),
            "Default".to_string(),
            "app.agent_mode_plan",
            340.0,
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
}
