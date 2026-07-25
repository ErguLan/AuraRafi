//! Retained RafUI surface for the Agent workbench.
//!
//! The Agent runtime, history, provider configuration, and tool executor stay
//! in `AgentPanel`. This module only projects that existing state into RafUI
//! nodes and translates input back into typed Agent actions.

use eframe::{egui, egui_wgpu};
use raf_ai::agent_runtime::AgentStatus;
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_ai::provider::AgentMode;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{
    StudioUiPalette, UiAction, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow,
    UiImage, UiImageFit, UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow,
    UiScrollAxis, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiStyleSheet, UiTextInput, UiTextStyle,
};

use super::ai_chat::{AgentPanel, AgentReadiness};
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

const SIDEBAR_WIDTH: f32 = 220.0;
const CONTROL_HEIGHT: f32 = 30.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AgentSurfaceAction {
    SetInput(String),
    SetModelLabel(String),
    SetModelId(String),
    ToggleSidebar,
    CloseSidebar,
    NewChat,
    SelectSession(usize),
    DeleteSession(usize),
    CancelDelete,
    SelectModel(usize),
    SetMode(AgentMode),
    AddModel,
    OpenSettings,
    Submit,
    Approve,
    Deny,
    UseSuggestion(String),
}

pub(crate) struct AgentSurfaceHost {
    bridge: RafUiSurfaceBridge,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    icons_registered: bool,
}

impl Default for AgentSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_agent_surface"),
            model_menu_open: false,
            mode_menu_open: false,
            add_model_open: false,
            icons_registered: false,
        }
    }
}

impl AgentSurfaceHost {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        agent: &AgentPanel,
        settings: &raf_core::config::EngineSettings,
        project: Option<&Project>,
        readiness: AgentReadiness,
    ) -> Vec<AgentSurfaceAction> {
        self.register_icons();
        let surface = build_agent_surface(
            palette,
            agent,
            settings,
            project,
            readiness,
            self.model_menu_open,
            self.mode_menu_open,
            self.add_model_open,
        );
        let input = agent.chat.input_text.clone();
        let model_label = agent.new_model_label.clone();
        let model_id = agent.new_model_id.clone();
        let actions = self.bridge.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                controls.set_text("agent.input", input.clone(), 4_096);
                controls.set_text("agent.model-label", model_label.clone(), 120);
                controls.set_text("agent.model-id", model_id.clone(), 160);
            },
            |key| t(key, settings.language),
        );

        actions
            .into_iter()
            .filter_map(|dispatched| match dispatched.action {
                UiAction::SetText { key, value } => match key.as_str() {
                    "agent.input" => Some(AgentSurfaceAction::SetInput(value)),
                    "agent.model-label" => Some(AgentSurfaceAction::SetModelLabel(value)),
                    "agent.model-id" => Some(AgentSurfaceAction::SetModelId(value)),
                    _ => None,
                },
                UiAction::Command { name } => self.parse_command(&name, settings, project),
                _ => None,
            })
            .collect()
    }

    fn parse_command(
        &mut self,
        name: &str,
        settings: &raf_core::config::EngineSettings,
        project: Option<&Project>,
    ) -> Option<AgentSurfaceAction> {
        match name {
            "agent.sidebar.toggle" => Some(AgentSurfaceAction::ToggleSidebar),
            "agent.sidebar.close" => Some(AgentSurfaceAction::CloseSidebar),
            "agent.new-chat" => Some(AgentSurfaceAction::NewChat),
            "agent.model.menu" => {
                self.model_menu_open = !self.model_menu_open;
                self.mode_menu_open = false;
                None
            }
            "agent.mode.menu" => {
                self.mode_menu_open = !self.mode_menu_open;
                self.model_menu_open = false;
                None
            }
            "agent.add-model.toggle" => {
                self.add_model_open = !self.add_model_open;
                None
            }
            "agent.add-model.confirm" => {
                self.add_model_open = false;
                Some(AgentSurfaceAction::AddModel)
            }
            "agent.settings" => Some(AgentSurfaceAction::OpenSettings),
            "agent.submit" => Some(AgentSurfaceAction::Submit),
            "agent.approve" => Some(AgentSurfaceAction::Approve),
            "agent.deny" => Some(AgentSurfaceAction::Deny),
            "agent.delete.cancel" => Some(AgentSurfaceAction::CancelDelete),
            _ => {
                if let Some(raw) = name.strip_prefix("agent.session:") {
                    return raw.parse().ok().map(AgentSurfaceAction::SelectSession);
                }
                if let Some(raw) = name.strip_prefix("agent.session.delete:") {
                    return raw.parse().ok().map(AgentSurfaceAction::DeleteSession);
                }
                if let Some(raw) = name.strip_prefix("agent.model.select:") {
                    return raw.parse().ok().map(|index| {
                        self.model_menu_open = false;
                        AgentSurfaceAction::SelectModel(index)
                    });
                }
                if let Some(raw) = name.strip_prefix("agent.mode.select:") {
                    self.mode_menu_open = false;
                    return match raw {
                        "passive" => Some(AgentSurfaceAction::SetMode(AgentMode::Passive)),
                        "active" => Some(AgentSurfaceAction::SetMode(AgentMode::Active)),
                        _ => None,
                    };
                }
                if let Some(raw) = name.strip_prefix("agent.suggestion:") {
                    let index = raw.parse::<usize>().ok()?;
                    return agent_suggestions(project, settings.language)
                        .get(index)
                        .cloned()
                        .map(AgentSurfaceAction::UseSuggestion);
                }
                None
            }
        }
    }

    fn register_icons(&mut self) {
        if self.icons_registered {
            return;
        }
        let _ = self.bridge.register_embedded_png(
            "editor.agent.send",
            include_bytes!("../../../../editor/assets/ui_icons/send.png"),
        );
        self.icons_registered = true;
    }
}

fn build_agent_surface(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    settings: &raf_core::config::EngineSettings,
    project: Option<&Project>,
    readiness: AgentReadiness,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("agent.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.root_style());

    if agent.sidebar_open {
        root = root.with_child(sidebar(palette, agent, settings.language));
    }

    let mut main = UiNode::new("agent.main", UiNodeKind::Panel)
        .with_class("agent-main")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            padding: UiSpacing::xy(10.0, 0.0),
            gap: 6.0,
            overflow: UiOverflow::Clip,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(header(
            palette,
            agent,
            settings,
            model_menu_open,
            mode_menu_open,
        ));

    if model_menu_open {
        main = main.with_child(model_menu(palette, agent));
    }
    if add_model_open {
        main = main.with_child(add_model_form(palette));
    }
    if mode_menu_open {
        main = main.with_child(mode_menu(palette));
    }

    if readiness != AgentReadiness::Ready {
        let text = match readiness {
            AgentReadiness::Ready => "",
            AgentReadiness::ProviderDisabled => "app.agent_provider_disabled",
            AgentReadiness::ModelMissing => "app.agent_model_missing",
            AgentReadiness::AdapterRequired => "app.agent_provider_adapter_required",
        };
        main = main.with_child(banner(
            "agent.readiness",
            text,
            "agent-warning",
            tokens.warning,
            42.0,
        ));
    } else if settings.agent_mode == AgentMode::Active {
        main = main.with_child(banner(
            "agent.active-warning",
            "app.agent_active_mode_warning",
            "agent-warning",
            tokens.warning,
            36.0,
        ));
    }

    if agent.runtime.status == AgentStatus::Thinking
        || agent.runtime.status == AgentStatus::ExecutingTools
    {
        let key = if agent.runtime.status == AgentStatus::ExecutingTools {
            "app.agent_executing_tools"
        } else {
            "app.agent_thinking"
        };
        main = main.with_child(banner(
            "agent.activity",
            key,
            "agent-activity",
            tokens.accent,
            28.0,
        ));
    }

    if !agent.runtime.pending_calls.is_empty()
        && agent.runtime.status == AgentStatus::AwaitingApproval
    {
        main = main.with_child(pending_calls(palette, agent));
    }

    main = main
        .with_child(messages(palette, agent, project, settings.language))
        .with_child(suggestions(palette, agent, project, settings.language))
        .with_child(composer(palette, agent, readiness, settings.language));

    root = root.with_child(main);
    let mut surface = UiSurface::new("agent", palette, root);
    surface.style_sheet = agent_style_sheet(palette);
    surface
}

fn sidebar(palette: StudioUiPalette, agent: &AgentPanel, _lang: Language) -> UiNode {
    let tokens = palette.tokens();
    let mut sessions = UiNode::scroll_view("agent.sessions", UiScrollAxis::Vertical)
        .with_class("agent-session-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 4.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    for index in (0..agent.history.sessions.len()).rev() {
        let session = &agent.history.sessions[index];
        let active = agent.history.active_index == Some(index);
        let title = truncate(&session.title, 30);
        let mut row = UiNode::new(format!("agent.session.row.{index}"), UiNodeKind::Panel)
            .with_class(if active {
                "agent-session-active"
            } else {
                "agent-session"
            })
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 4.0,
                min_size: [0.0, 30.0],
                padding: UiSpacing::xy(4.0, 2.0),
                ..UiLayout::default()
            });
        row = row.with_child(
            UiNode::new(format!("agent.session.select.{index}"), UiNodeKind::Button)
                .with_text_key(title)
                .with_text_style(UiTextStyle::body(if active {
                    tokens.accent
                } else {
                    tokens.text
                }))
                .with_layout(UiLayout {
                    grow: 1.0,
                    padding: UiSpacing::xy(6.0, 4.0),
                    ..UiLayout::default()
                })
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session:{index}"),
                )),
        );
        if active {
            row = row.with_child(
                UiNode::new(format!("agent.session.delete.{index}"), UiNodeKind::Button)
                    .with_text_key("app.agent_delete")
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout::fixed(48.0, 24.0))
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("agent.session.delete:{index}"),
                    )),
            );
        }
        sessions = sessions.with_child(row);
    }

    let mut root = UiNode::new("agent.sidebar", UiNodeKind::Panel)
        .with_class("agent-sidebar")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(10.0),
            gap: 8.0,
            ..UiLayout::fixed(SIDEBAR_WIDTH, 0.0)
        })
        .with_child(
            UiNode::new("agent.sidebar.header", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    ..UiLayout::fixed(0.0, 28.0)
                })
                .with_child(
                    UiNode::new("agent.sessions.title", UiNodeKind::Label)
                        .with_text_key("app.agent_sessions")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                )
                .with_child(
                    UiNode::new("agent.sidebar.close", UiNodeKind::Button)
                        .with_text_key("X")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(24.0, 24.0))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "agent.sidebar.close",
                        )),
                ),
        )
        .with_child(primary_button(
            "agent.new-chat",
            "app.agent_new_chat",
            palette,
        ))
        .with_child(sessions);

    if let Some(index) = agent.pending_delete_session {
        if index < agent.history.sessions.len() {
            root = root.with_child(
                UiNode::new("agent.delete-actions", UiNodeKind::Toolbar)
                    .with_layout(UiLayout {
                        flow: UiFlow::Row,
                        gap: 4.0,
                        ..UiLayout::fixed(0.0, 28.0)
                    })
                    .with_child(command_button(
                        "agent.delete.confirm",
                        "app.agent_delete",
                        format!("agent.session.delete:{index}"),
                        "agent-danger-button",
                        palette,
                    ))
                    .with_child(command_button(
                        "agent.delete.cancel",
                        "app.agent_cancel",
                        "agent.delete.cancel",
                        "agent-secondary-button",
                        palette,
                    )),
            );
        }
    }
    root
}

fn header(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    settings: &raf_core::config::EngineSettings,
    model_menu_open: bool,
    mode_menu_open: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let selected = if agent.selected_model.is_empty() {
        raf_ai::agent_model_registry::AgentModelRegistry::PROVIDER_DEFAULT.to_string()
    } else {
        agent.selected_model.clone()
    };
    let mode_key = match settings.agent_mode {
        AgentMode::Passive => "app.agent_mode_passive",
        AgentMode::Active => "app.agent_mode_active",
    };
    let mut node = UiNode::new("agent.header", UiNodeKind::Toolbar)
        .with_class("agent-header")
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            align_items: UiAlign::Center,
            gap: 6.0,
            compact: UiCompactMode::Wrap,
            ..UiLayout::fixed(0.0, 38.0)
        })
        .with_child(command_button(
            "agent.sidebar.toggle",
            if agent.sidebar_open { "<" } else { ">" },
            "agent.sidebar.toggle",
            "agent-icon-button",
            palette,
        ))
        .with_child(
            UiNode::new("agent.title", UiNodeKind::Label)
                .with_text_key("app.agent_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(64.0, 24.0)),
        )
        .with_child(command_button(
            "agent.model.trigger",
            selected,
            "agent.model.menu",
            if model_menu_open {
                "agent-selected-button"
            } else {
                "agent-secondary-button"
            },
            palette,
        ))
        .with_child(command_button(
            "agent.add-model",
            "app.agent_add_model",
            "agent.add-model.toggle",
            "agent-secondary-button",
            palette,
        ))
        .with_child(command_button(
            "agent.mode.trigger",
            mode_key,
            "agent.mode.menu",
            if mode_menu_open {
                "agent-selected-button"
            } else if settings.agent_mode == AgentMode::Active {
                "agent-active-button"
            } else {
                "agent-secondary-button"
            },
            palette,
        ));

    node = node.with_child(
        UiNode::new("agent.header.spacer", UiNodeKind::Panel).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::default()
        }),
    );
    node.with_child(command_button(
        "agent.settings",
        "app.agent_settings",
        "agent.settings",
        "agent-secondary-button",
        palette,
    ))
}

fn model_menu(palette: StudioUiPalette, agent: &AgentPanel) -> UiNode {
    let mut menu = UiNode::new("agent.model-menu", UiNodeKind::Panel)
        .with_class("agent-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::same(6.0),
            ..UiLayout::fixed(300.0, 0.0)
        });
    for (index, label) in agent.model_registry.selector_labels().iter().enumerate() {
        menu = menu.with_child(command_button(
            format!("agent.model.option.{index}"),
            label.clone(),
            format!("agent.model.select:{index}"),
            if *label == agent.selected_model {
                "agent-option-active"
            } else {
                "agent-option"
            },
            palette,
        ));
    }
    menu
}

fn mode_menu(palette: StudioUiPalette) -> UiNode {
    UiNode::new("agent.mode-menu", UiNodeKind::Toolbar)
        .with_class("agent-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            padding: UiSpacing::same(6.0),
            ..UiLayout::fixed(220.0, 42.0)
        })
        .with_child(command_button(
            "agent.mode.passive",
            "app.agent_mode_passive",
            "agent.mode.select:passive",
            "agent-option",
            palette,
        ))
        .with_child(command_button(
            "agent.mode.active",
            "app.agent_mode_active",
            "agent.mode.select:active",
            "agent-option-active",
            palette,
        ))
}

fn add_model_form(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("agent.add-model-form", UiNodeKind::Panel)
        .with_class("agent-form")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::same(6.0),
            compact: UiCompactMode::Wrap,
            ..UiLayout::fixed(0.0, 42.0)
        })
        .with_child(
            UiNode::new("agent.model-label.caption", UiNodeKind::Label)
                .with_text_key("app.agent_model_label")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(44.0, 22.0)),
        )
        .with_child(
            UiNode::text_input(
                "agent.model-label.input",
                UiTextInput {
                    value_key: "agent.model-label".to_string(),
                    placeholder_key: Some("app.agent_model_label".to_string()),
                    max_length: 120,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("agent-input")
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fixed(150.0, CONTROL_HEIGHT)
            })
            .focusable(),
        )
        .with_child(
            UiNode::new("agent.model-id.caption", UiNodeKind::Label)
                .with_text_key("app.agent_model_id")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(52.0, 22.0)),
        )
        .with_child(
            UiNode::text_input(
                "agent.model-id.input",
                UiTextInput {
                    value_key: "agent.model-id".to_string(),
                    placeholder_key: Some("app.agent_model_id".to_string()),
                    max_length: 160,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("agent-input")
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fixed(170.0, CONTROL_HEIGHT)
            })
            .focusable(),
        )
        .with_child(command_button(
            "agent.add-model.confirm",
            "app.agent_add_model_confirm",
            "agent.add-model.confirm",
            "agent-primary-button",
            palette,
        ))
}

fn banner(id: &str, text_key: &str, class: &str, color: [u8; 4], height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class(class)
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::body(color))
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(10.0, 4.0),
            ..UiLayout::fixed(0.0, height)
        })
}

fn pending_calls(palette: StudioUiPalette, agent: &AgentPanel) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::new("agent.pending", UiNodeKind::Panel)
        .with_class("agent-pending")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::same(8.0),
            ..UiLayout::fixed(
                0.0,
                (82.0 + agent.runtime.pending_calls.len() as f32 * 30.0).min(210.0),
            )
        })
        .with_child(
            UiNode::new("agent.pending.title", UiNodeKind::Label)
                .with_text_key("app.agent_pending_tools")
                .with_text_style(UiTextStyle::body(tokens.warning))
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        );
    for (index, call) in agent.runtime.pending_calls.iter().enumerate() {
        let text = format!("{}({})", call.name, call.arguments);
        panel = panel.with_child(
            UiNode::new(format!("agent.pending.call.{index}"), UiNodeKind::Label)
                .with_text_key(truncate(&text, 140))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        );
        if let Some(error) = call.argument_error.as_deref() {
            panel = panel.with_child(
                UiNode::new(format!("agent.pending.error.{index}"), UiNodeKind::Label)
                    .with_text_key(truncate(error, 140))
                    .with_text_style(UiTextStyle::body(tokens.danger))
                    .with_layout(UiLayout::fixed(0.0, 20.0)),
            );
        }
    }
    panel.with_child(
        UiNode::new("agent.pending.actions", UiNodeKind::Toolbar)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 6.0,
                ..UiLayout::fixed(0.0, CONTROL_HEIGHT)
            })
            .with_child(command_button(
                "agent.approve",
                "app.agent_approve",
                "agent.approve",
                "agent-primary-button",
                palette,
            ))
            .with_child(command_button(
                "agent.deny",
                "app.agent_deny",
                "agent.deny",
                "agent-danger-button",
                palette,
            )),
    )
}

fn messages(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    project: Option<&Project>,
    lang: Language,
) -> UiNode {
    let mut list = UiNode::scroll_view("agent.messages", UiScrollAxis::Vertical)
        .with_class("agent-message-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 6.0,
            padding: UiSpacing::xy(4.0, 4.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    let visible = agent
        .runtime
        .messages
        .iter()
        .filter(|message| message.role != MessageRole::System)
        .collect::<Vec<_>>();
    if visible.is_empty() {
        list = list.with_child(empty_state(palette, project, lang));
    } else {
        for (index, message) in visible.into_iter().enumerate() {
            list = list.with_child(message_node(palette, lang, index, message));
        }
    }
    list
}

fn empty_state(palette: StudioUiPalette, project: Option<&Project>, _lang: Language) -> UiNode {
    let tokens = palette.tokens();
    let subtitle_key = match project.map(|project| project.project_type) {
        Some(ProjectType::Game) => "app.agent_empty_subtitle_game",
        Some(ProjectType::Electronics) => "app.agent_empty_subtitle_electronics",
        None => "app.agent_empty_subtitle",
    };
    UiNode::new("agent.empty", UiNodeKind::Panel)
        .with_class("agent-empty")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 110.0)
        })
        .with_child(
            UiNode::new("agent.empty.title", UiNodeKind::Label)
                .with_text_key("app.agent_empty_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 22.0)),
        )
        .with_child(
            UiNode::new("agent.empty.subtitle", UiNodeKind::Label)
                .with_text_key(subtitle_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 40.0)),
        )
}

fn message_node(
    palette: StudioUiPalette,
    lang: Language,
    index: usize,
    message: &ChatMessage,
) -> UiNode {
    let tokens = palette.tokens();
    let is_user = message.role == MessageRole::User;
    let is_tool = message.role == MessageRole::Tool;
    let is_error = message.role == MessageRole::Assistant
        && (message.content.starts_with("Error:") || message.content.starts_with("I encountered"));
    let class = if is_user {
        "agent-message-user"
    } else if is_tool {
        "agent-message-tool"
    } else if is_error {
        "agent-message-error"
    } else {
        "agent-message-assistant"
    };
    let label_key = if is_user {
        "app.you"
    } else if is_tool {
        "app.agent_tool_result"
    } else if is_error {
        "app.agent_error_label"
    } else {
        "app.agent_label"
    };
    let content_height = estimate_text_height(&message.content, if is_user { 52 } else { 92 });
    let bubble_width = if is_user { 460.0 } else { 0.0 };
    let message_text = format!("{}\n{}", t(label_key, lang), message.content);
    let mut bubble = UiNode::new(format!("agent.message.bubble.{index}"), UiNodeKind::Panel)
        .with_class(class)
        .with_text_key(message_text)
        .with_text_style(UiTextStyle::body(if is_user {
            tokens.text
        } else if is_tool {
            tokens.text_muted
        } else if is_error {
            tokens.danger
        } else {
            tokens.text
        }))
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::xy(12.0, 8.0),
            grow: if is_user { 0.0 } else { 1.0 },
            min_size: [if is_user { bubble_width } else { 0.0 }, content_height],
            basis: [bubble_width, content_height],
            ..UiLayout::default()
        });
    if is_user {
        bubble.layout.align_self = Some(UiAlign::End);
    }
    UiNode::new(format!("agent.message.row.{index}"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            justify_content: if is_user {
                UiJustify::End
            } else {
                UiJustify::Start
            },
            ..UiLayout::fixed(0.0, content_height)
        })
        .with_child(bubble)
}

fn suggestions(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    project: Option<&Project>,
    lang: Language,
) -> UiNode {
    if !matches!(agent.runtime.status, AgentStatus::Done | AgentStatus::Error) {
        return UiNode::new("agent.suggestions.hidden", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(0.0, 0.0));
    }
    let tokens = palette.tokens();
    let mut row = UiNode::new("agent.suggestions", UiNodeKind::Toolbar)
        .with_class("agent-suggestions")
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(4.0, 2.0),
            compact: UiCompactMode::Wrap,
            ..UiLayout::fixed(0.0, 30.0)
        })
        .with_child(
            UiNode::new("agent.suggestions.title", UiNodeKind::Label)
                .with_text_key("app.agent_suggestions_title")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(90.0, 20.0)),
        );
    for (index, suggestion) in agent_suggestions(project, lang).into_iter().enumerate() {
        row = row.with_child(command_button(
            format!("agent.suggestion.{index}"),
            suggestion,
            format!("agent.suggestion:{index}"),
            "agent-suggestion",
            palette,
        ));
    }
    row
}

fn composer(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    readiness: AgentReadiness,
    lang: Language,
) -> UiNode {
    let tokens = palette.tokens();
    let enabled = readiness == AgentReadiness::Ready
        && !agent.chat.input_text.trim().is_empty()
        && !agent.runtime.status.blocks_input();
    let node = UiNode::new("agent.composer", UiNodeKind::Panel)
        .with_class("agent-composer")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(4.0, 5.0),
            ..UiLayout::fixed(0.0, 60.0)
        })
        .with_child(
            UiNode::text_input(
                "agent.input-field",
                UiTextInput {
                    value_key: "agent.input".to_string(),
                    placeholder_key: Some("app.agent_input_placeholder".to_string()),
                    max_length: 16_384,
                    multiline: true,
                    password: false,
                    submit_command: Some("agent.submit".to_string()),
                },
            )
            .with_class("agent-input")
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fixed(0.0, 48.0)
            })
            .focusable(),
        )
        .with_child(
            UiNode::new("agent.send", UiNodeKind::Button)
                .with_class(if enabled {
                    "agent-primary-button"
                } else {
                    "agent-disabled-button"
                })
                .disabled(!enabled)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::Center,
                    gap: 5.0,
                    min_size: [82.0, 34.0],
                    padding: UiSpacing::xy(10.0, 0.0),
                    ..UiLayout::default()
                })
                .with_text_key(send_label(agent.runtime.status.clone()))
                .with_text_style(UiTextStyle::button(if enabled {
                    tokens.text
                } else {
                    tokens.text_muted
                }))
                .with_child(
                    UiNode::image(
                        "agent.send.icon",
                        UiImage {
                            source: UiImageSource::new("editor.agent.send"),
                            fit: UiImageFit::Contain,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(16.0, 16.0)),
                )
                .with_tooltip_key("app.agent_send")
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "agent.submit")),
        )
        .with_text_style(UiTextStyle::body(tokens.text));
    let _ = lang;
    // Keep the active language in the document call site even when the send
    // state is already represented by localized text keys.
    node
}

fn send_label(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::AwaitingApproval => "app.agent_awaiting_approval",
        AgentStatus::Thinking => "app.agent_thinking_dots",
        AgentStatus::ExecutingTools => "app.agent_executing_tools_dots",
        AgentStatus::Done | AgentStatus::Error => "app.agent_send",
    }
}

fn command_button(
    id: impl Into<String>,
    label_key: impl Into<String>,
    command: impl Into<String>,
    class: &str,
    palette: StudioUiPalette,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_layout(UiLayout {
            min_size: [48.0, CONTROL_HEIGHT],
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn primary_button(id: &str, label_key: &str, palette: StudioUiPalette) -> UiNode {
    command_button(id, label_key, id, "agent-primary-button", palette)
}

fn agent_suggestions(project: Option<&Project>, lang: Language) -> Vec<String> {
    match project.map(|project| project.project_type) {
        Some(ProjectType::Game) => vec![
            t("app.agent_hint_cube", lang),
            t("app.agent_hint_light", lang),
            t("app.agent_hint_camera", lang),
            t("app.agent_hint_explain_error", lang),
        ],
        Some(ProjectType::Electronics) => vec![
            t("app.agent_hint_resistor", lang),
            t("app.agent_hint_electrical_test", lang),
            t("app.agent_hint_nets", lang),
            t("app.agent_hint_explain_error", lang),
        ],
        None => vec![
            t("app.agent_hint_cube", lang),
            t("app.agent_hint_electrical_test", lang),
            t("app.agent_hint_explain_error", lang),
        ],
    }
}

fn estimate_text_height(text: &str, chars_per_line: usize) -> f32 {
    let lines = text
        .lines()
        .map(|line| ((line.chars().count().max(1) + chars_per_line - 1) / chars_per_line) as f32)
        .sum::<f32>()
        .max(1.0);
    (lines * 18.0 + 36.0).clamp(54.0, 260.0)
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn agent_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            style_rule("agent-sidebar", tokens.surface, tokens.border, 1.0, 0.0),
            style_rule("agent-header", tokens.surface_alt, tokens.border, 1.0, 0.0),
            style_rule("agent-session", tokens.surface, tokens.border, 1.0, 4.0),
            style_rule(
                "agent-session-active",
                tokens.surface_raised,
                tokens.accent,
                1.0,
                4.0,
            ),
            style_rule(
                "agent-message-assistant",
                tokens.surface_raised,
                tokens.border,
                1.0,
                6.0,
            ),
            style_rule(
                "agent-message-user",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                6.0,
            ),
            style_rule(
                "agent-message-tool",
                tokens.surface_alt,
                tokens.border,
                1.0,
                6.0,
            ),
            style_rule(
                "agent-message-error",
                [60, 30, 30, 255],
                tokens.danger,
                1.0,
                6.0,
            ),
            style_rule("agent-composer", tokens.surface, tokens.border, 1.0, 4.0),
            style_rule(
                "agent-input",
                tokens.surface_raised,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule("agent-menu", tokens.surface_raised, tokens.accent, 1.0, 4.0),
            style_rule("agent-form", tokens.surface, tokens.border, 1.0, 4.0),
            style_rule(
                "agent-primary-button",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                4.0,
            ),
            style_rule(
                "agent-danger-button",
                tokens.danger,
                tokens.danger,
                1.0,
                4.0,
            ),
            style_rule(
                "agent-secondary-button",
                tokens.surface_raised,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "agent-selected-button",
                tokens.selection,
                tokens.accent,
                1.0,
                4.0,
            ),
            style_rule(
                "agent-active-button",
                tokens.accent,
                tokens.accent_hot,
                1.0,
                4.0,
            ),
            style_rule("agent-option", tokens.surface, tokens.border, 1.0, 4.0),
            style_rule(
                "agent-option-active",
                tokens.selection,
                tokens.accent,
                1.0,
                4.0,
            ),
            style_rule(
                "agent-suggestion",
                tokens.surface_raised,
                tokens.border,
                1.0,
                4.0,
            ),
            style_rule(
                "agent-warning",
                tokens.surface_alt,
                tokens.warning,
                1.0,
                4.0,
            ),
            style_rule("agent-activity", tokens.surface, tokens.accent, 1.0, 4.0),
            style_rule(
                "agent-pending",
                tokens.surface_alt,
                tokens.warning,
                1.0,
                4.0,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-secondary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Kind(UiNodeKind::Button),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
}

fn style_rule(
    class: &str,
    fill: [u8; 4],
    border: [u8; 4],
    border_width: f32,
    radius: f32,
) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            border_width: Some(border_width),
            radius: Some(radius),
            ..UiStylePatch::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_surface_has_retained_root_and_message_regions() {
        let agent = AgentPanel::default();
        let settings = raf_core::config::EngineSettings::default();
        let surface = build_agent_surface(
            StudioUiPalette::IndustrialDark,
            &agent,
            &settings,
            None,
            AgentReadiness::Ready,
            false,
            false,
            false,
        );
        assert_eq!(surface.root.id, "agent.root");
        assert!(find_node(&surface.root, "agent.messages").is_some());
        assert!(find_node(&surface.root, "agent.composer").is_some());
    }

    #[test]
    fn agent_dynamic_text_nodes_receive_visible_layouts() {
        let mut agent = AgentPanel::default();
        agent.runtime.messages.push(ChatMessage::assistant(
            "The current scene contains a cube and a light.",
        ));
        let settings = raf_core::config::EngineSettings::default();
        let surface = build_agent_surface(
            StudioUiPalette::IndustrialDark,
            &agent,
            &settings,
            None,
            AgentReadiness::ProviderDisabled,
            false,
            false,
            false,
        );
        let mut session = raf_render::api_graphic_basic::ui_surface::UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 1600, 900, [0, 0, 0, 255], |key| {
                t(key, settings.language)
            });
        for id in [
            "agent.readiness",
            "agent.message.bubble.0",
            "agent.input-field",
        ] {
            let layout = frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == id)
                .unwrap_or_else(|| panic!("missing layout for {id}"));
            assert!(layout.rect.width > 1.0, "{id} has no visible width");
            assert!(layout.rect.height > 1.0, "{id} has no visible height");
        }
        assert!(frame
            .text_requests
            .iter()
            .any(|request| request.node_id == "agent.message.bubble.0"));
        assert!(frame.text_requests.iter().all(|request| {
            let resolved =
                session.resolve_text_request(&frame, request, |key| t(key, settings.language));
            session.text_atlas.slot_for(request, &resolved).is_some()
        }));
    }

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }
}
