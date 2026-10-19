//! Creative native Agent workspace: project rail, conversation and evidence.

use crate::agent_service::AgentService;
use crate::studio_surface::HubSurfaceProject;
use raf_ai::agent_runtime::AgentToolResult;
use raf_ai::chat::MessageRole;
use raf_core::i18n::t;
use raf_core::{EngineSettings, Language};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiAction, UiSurface};
use raf_ui::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum EvidenceTab {
    #[default]
    Changes,
    Artifacts,
    Verification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceConfirmation {
    StartAll,
    CloseAll,
    CloseProject(std::path::PathBuf),
}

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

fn label(id: &str, key: &str, tokens: &UiTokens) -> UiNode {
    UiNode::new(id, UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(UiTextStyle::body(tokens.text_muted))
        .with_layout(UiLayout::fit_content())
}

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
    let tokens = workspace_tokens(palette);
    let width = size[0].max(1.0);
    let height = size[1].max(1.0);
    let rail = if width < 900.0 { 176.0 } else { 216.0 };
    let review = if review_open && width >= 1200.0 && service.selected_workspace().is_some() {
        286.0
    } else {
        0.0
    };
    let ready = service.selected_workspace().filter(|w| !w.closing);
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
            "agent_workspace.projects",
            "agent.workspace.back",
            &tokens,
        ))
        .with_child(
            UiNode::new("agent.workspace.title", UiNodeKind::Label)
                .with_text_key("agent_workspace.title")
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
                &tokens,
            )
            .disabled(ready.is_none()),
        )
        .with_child(button(
            "agent.workspace.settings",
            "app.settings_menu",
            "agent.workspace.settings",
            &tokens,
        ))
        .with_child(
            button(
                "agent.workspace.editor",
                "agent_workspace.open_editor",
                "agent.workspace.editor",
                &tokens,
            )
            .disabled(ready.is_none()),
        );
    header = header
        .with_child(window_button(
            "minimize",
            "agent_workspace.minimize",
            UiIconId::ChevronDown,
            &tokens,
        ))
        .with_child(window_button(
            "maximize",
            "agent_workspace.maximize",
            UiIconId::Focus,
            &tokens,
        ))
        .with_child(window_button(
            "close",
            "agent_workspace.close",
            UiIconId::Close,
            &tokens,
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
            &tokens,
        ))
        .with_child(
            button(
                "agent.workspace.start-all",
                "agent_workspace.start_all",
                "agent.workspace.start-all",
                &tokens,
            )
            .disabled(projects.is_empty()),
        )
        .with_child(
            button(
                "agent.workspace.close-all",
                "agent_workspace.close_all",
                "agent.workspace.close-all",
                &tokens,
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
                    &tokens,
                )
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
            );
        }
        let close = button(
            &format!("agent.workspace.project.{index}.close"),
            "agent_workspace.close_project",
            "agent.workspace.close-project",
            &tokens,
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
    let mut root = UiNode::new("agent.workspace.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle {
            fill: tokens.background,
            ..UiStyle::transparent()
        })
        .with_child(header)
        .with_child(rail_node);
    let mut rules = Vec::new();
    if let Some(workspace) = ready {
        let panel = workspace.panel.borrow();
        let mut chat = crate::panels::agent_surface::build_agent_surface(
            palette,
            &panel,
            settings,
            panel.readiness(settings),
            workspace.project.project_type,
            [(width - rail - review).max(1.0), height - 52.0],
            scroll,
            if panel.sidebar_open { 1.0 } else { 0.0 },
        );
        chat.root.layout = UiLayout::absolute(UiRect::new(
            rail,
            52.0,
            width - rail - review,
            height - 52.0,
        ));
        root = root.with_child(chat.root);
        rules.extend(chat.style_sheet.rules);
        if review_open {
            root = root.with_child(evidence_surface(
                palette,
                &panel,
                settings.language,
                UiRect::new(width - 286.0, 52.0, 286.0, height - 52.0),
                evidence,
            ));
        }
    } else {
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
            &tokens,
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
                    &tokens,
                ));
        } else if !service.loading() && !closing {
            empty = empty.with_child(label(
                "agent.workspace.empty.hint",
                "agent_workspace.choose_project_hint",
                &tokens,
            ));
        }
        root = root.with_child(empty);
    }
    let mut surface = UiSurface::new("editor.agent.workspace", palette, root);
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
    surface
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

fn evidence_surface(
    palette: StudioUiPalette,
    panel: &crate::panels::ai_chat::AgentPanel,
    language: Language,
    rect: UiRect,
    tab: EvidenceTab,
) -> UiNode {
    let tokens = workspace_tokens(palette);
    let mut root = UiNode::new("agent.workspace.evidence", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 12.0,
            padding: UiSpacing::same(12.0),
            ..UiLayout::absolute(rect).with_z_index(30)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            ..UiStyle::transparent()
        });
    let mut tabs =
        UiNode::new("agent.workspace.evidence.tabs", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        });
    for (id, key, value) in [
        ("changes", "agent_workspace.changes", EvidenceTab::Changes),
        (
            "artifacts",
            "agent_workspace.artifacts",
            EvidenceTab::Artifacts,
        ),
        (
            "verification",
            "agent_workspace.verification",
            EvidenceTab::Verification,
        ),
    ] {
        let mut item = button(
            &format!("agent.workspace.tab.{id}"),
            key,
            &format!("agent.workspace.tab.{id}"),
            &tokens,
        )
        .with_accessibility_selected(tab == value);
        item.layout = UiLayout {
            padding: UiSpacing::xy(4.0, 6.0),
            ..UiLayout::fixed(
                match value {
                    EvidenceTab::Changes => 72.0,
                    EvidenceTab::Artifacts => 80.0,
                    EvidenceTab::Verification => 102.0,
                },
                32.0,
            )
        };
        item.style.fill = if tab == value {
            tokens.surface_raised
        } else {
            [0; 4]
        };
        tabs = tabs.with_child(item);
    }
    root = root.with_child(tabs);
    let mut list = UiNode::scroll_view("agent.workspace.evidence.scroll", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 12.0,
            ..UiLayout::fill(UiFlow::Column)
        });
    let mut count = 0;
    for message in panel
        .runtime
        .messages
        .iter()
        .rev()
        .filter(|m| m.role == MessageRole::Tool)
        .take(24)
    {
        let Ok(result) = serde_json::from_str::<AgentToolResult>(&message.content) else {
            continue;
        };
        let relevant = match tab {
            EvidenceTab::Changes => result.diff.is_some() || result.changed,
            EvidenceTab::Artifacts => {
                result.data.get("artifact").is_some()
                    || result
                        .data
                        .pointer("/receipt/artifacts")
                        .and_then(|v| v.as_array())
                        .is_some_and(|artifacts| !artifacts.is_empty())
                    || result
                        .references
                        .iter()
                        .any(|r| r.contains("generated") || r.contains("agent_artifacts"))
            }
            EvidenceTab::Verification => result.verification.is_some(),
        };
        if !relevant {
            continue;
        }
        count += 1;
        let id = format!("agent.workspace.evidence.{}", message.id);
        let mut entry = UiNode::new(&id, UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 6.0,
                padding: UiSpacing::same(10.0),
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            })
            .with_style(UiStyle {
                fill: tokens.surface_alt,
                radius: 8.0,
                ..UiStyle::transparent()
            })
            .with_child(
                UiNode::new(format!("{id}.summary"), UiNodeKind::Label)
                    .with_text_value(result.summary)
                    .with_text_overflow(UiTextOverflow::Wrap)
                    .with_text_style(UiTextStyle::body(tokens.text))
                    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        if let Some(revision) = result.revision {
            entry = entry.with_child(
                UiNode::new(format!("{id}.revision"), UiNodeKind::Label)
                    .with_text_value(format!(
                        "{} {revision}",
                        t("agent_workspace.revision", language)
                    ))
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content()),
            );
        }
        if result
            .data
            .pointer("/persistence/saved")
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            entry = entry.with_child(label(
                &format!("{id}.saved"),
                "agent_workspace.autosave",
                &tokens,
            ));
        }
        let detail = match tab {
            EvidenceTab::Changes => result.diff.as_ref(),
            EvidenceTab::Artifacts => result
                .data
                .get("artifact")
                .or_else(|| result.data.pointer("/receipt/artifacts")),
            EvidenceTab::Verification => result.verification.as_ref(),
        };
        if let Some(detail) = detail {
            entry = entry.with_child(button(
                &format!("{id}.expand"),
                "agent_workspace.details",
                &format!("agent.tool.toggle:{}", message.id),
                &tokens,
            ));
            if panel.expanded_tools.contains(&message.id.to_string()) {
                let text = serde_json::to_string_pretty(detail)
                    .unwrap_or_default()
                    .chars()
                    .take(4096)
                    .collect::<String>();
                entry = entry.with_child(
                    UiNode::new(format!("{id}.detail"), UiNodeKind::Label)
                        .with_text_value(text)
                        .with_text_overflow(UiTextOverflow::Wrap)
                        .selectable_text()
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
                );
            }
        }
        for (index, reference) in result.references.iter().take(12).enumerate() {
            entry = entry.with_child(
                UiNode::new(format!("{id}.reference.{index}"), UiNodeKind::Label)
                    .with_text_value(reference.clone())
                    .selectable_text()
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        }
        if let Some(verification) = result.verification {
            let status = verification
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let key = match status {
                "passed" | "Passed" => "agent_workspace.verified",
                "failed" | "Failed" => "agent_workspace.validation_failed",
                "preview" => "agent_workspace.preview",
                _ => "agent_workspace.unverified",
            };
            entry = entry.with_child(label(&format!("{id}.verification"), key, &tokens));
        }
        list = list.with_child(entry);
    }
    if count == 0 {
        list = list.with_child(
            label(
                "agent.workspace.evidence.empty",
                "agent_workspace.no_evidence",
                &tokens,
            )
            .with_text_overflow(UiTextOverflow::Wrap)
            .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    root.with_child(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_workspace_bulk_modals_block_background_and_fit_both_languages() {
        use raf_render::api_graphic_basic::ui_surface::{CpuUiSurfaceHost, UiSurfaceSession};
        for language in [Language::English, Language::Spanish] {
            for confirmation in [
                WorkspaceConfirmation::StartAll,
                WorkspaceConfirmation::CloseAll,
            ] {
                let settings = EngineSettings {
                    language,
                    ..Default::default()
                };
                let surface = with_confirmation(
                    build_workspace_surface(
                        StudioUiPalette::IndustrialDark,
                        &AgentService::default(),
                        &[],
                        &settings,
                        [900.0, 600.0],
                        0.0,
                        0,
                        EvidenceTab::Changes,
                        false,
                    ),
                    [900.0, 600.0],
                    Some(&confirmation),
                );
                fn focusable(node: &UiNode, result: &mut Vec<String>) {
                    if node.focusable && !node.disabled {
                        result.push(node.id.clone());
                    }
                    for child in &node.children {
                        focusable(child, result);
                    }
                }
                let mut controls = Vec::new();
                focusable(&surface.root, &mut controls);
                assert_eq!(
                    controls,
                    [
                        "agent.workspace.confirmation.cancel",
                        "agent.workspace.confirmation.accept"
                    ]
                );
                let mut session = UiSurfaceSession::default();
                let frame = session.build_frame_with_resolved_text(
                    &surface,
                    900,
                    600,
                    [9, 9, 10, 255],
                    |key| t(key, language),
                );
                let rect = |id: &str| frame.layout_boxes.iter().find(|b| b.id == id).unwrap().rect;
                let card = rect("agent.workspace.confirmation.card");
                let cancel = rect("agent.workspace.confirmation.cancel");
                let accept = rect("agent.workspace.confirmation.accept");
                assert!(
                    card.x >= 0.0
                        && card.y >= 0.0
                        && card.x + card.width <= 900.0
                        && card.y + card.height <= 600.0,
                    "{card:?}"
                );
                assert!(
                    cancel.x + cancel.width <= accept.x + 0.5,
                    "{cancel:?} {accept:?}"
                );
                assert!(accept.y + accept.height <= card.y + card.height);
                if let Some(directory) = std::env::var_os("RAF_AGENT_UI_QA_DIR") {
                    std::fs::create_dir_all(&directory).unwrap();
                    let name = match confirmation {
                        WorkspaceConfirmation::StartAll => "start",
                        _ => "close",
                    };
                    let mut host = CpuUiSurfaceHost::new(surface, [9, 9, 10, 255]);
                    let rendered = host.render([900, 600], |key| t(key, language));
                    image::save_buffer(
                        std::path::PathBuf::from(directory)
                            .join(format!("agents-{name}-{language:?}.png")),
                        rendered.pixels,
                        900,
                        600,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }

    #[test]
    fn agent_workspace_copy_has_english_and_spanish_parity() {
        let en: serde_json::Value =
            serde_json::from_str(include_str!("../../raf_core/locales/en.json")).unwrap();
        let es: serde_json::Value =
            serde_json::from_str(include_str!("../../raf_core/locales/es.json")).unwrap();
        let en_keys: std::collections::BTreeSet<_> = en
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("agent_workspace."))
            .collect();
        let es_keys: std::collections::BTreeSet<_> = es
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("agent_workspace."))
            .collect();
        assert_eq!(en_keys, es_keys);
        for key in en_keys {
            assert_ne!(t(key, Language::English), *key);
            assert_ne!(t(key, Language::Spanish), *key);
        }
        let first = loading_ring(StudioUiPalette::IndustrialDark, 0);
        let next = loading_ring(StudioUiPalette::IndustrialDark, 1);
        assert_eq!(first.children.len(), 12);
        assert_ne!(first.children[0].style.fill, next.children[0].style.fill);
        assert!(first.accessibility_label_key.is_some());
    }
}
