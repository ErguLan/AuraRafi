//! Native RafUI Agent workbench.
//!
//! The Agent controller lives in `ai_chat.rs`; this module owns only the
//! retained document, transient menu state and native AGB presentation.

use raf_ai::agent_runtime::AgentStatus;
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_ai::provider::AgentMode;
use raf_core::config::{EngineSettings, Language};
use raf_core::project::Project;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::{
    UiAlign, UiDispatchedAction, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout,
    UiMotionSpec, UiNode, UiNodeKind, UiOverflow, UiRect, UiScrollAxis, UiSizeMode, UiSpacing,
    UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet,
    UiTextInput, UiTextStyle, UiTween,
};

use super::ai_chat::{AgentAction, AgentPanel, AgentReadiness};
use crate::editor_layout::EditorRect;

const MAX_INPUT_LENGTH: usize = 4_096;
const MAX_MESSAGE_DISPLAY_CHARS: usize = 16_384;
const APPROX_CHARS_PER_TOKEN: usize = 4;
const MODE_MENU_WIDTH: f32 = 156.0;
const MODE_MENU_HEIGHT: f32 = 42.0;
const AGENT_CLEAR: [u8; 4] = [14, 16, 22, 255];

pub(crate) struct AgentSurfaceHost {
    region: InputRegionId,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    surface_key: Option<AgentSurfaceKey>,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    sidebar_motion: UiTween,
    last_sync_time_seconds: f64,
    last_message_context: Option<(Option<usize>, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentSurfaceKey {
    language: Language,
    agent_mode: AgentMode,
    readiness: AgentReadiness,
    status: AgentStatus,
    sidebar_open: bool,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    sidebar_progress_bits: u32,
    selected_model: String,
    active_provider: String,
    effective_model_id: String,
    runtime_visual_revision: u64,
    streaming_enabled: bool,
    surface_width_bits: u32,
    surface_height_bits: u32,
    session_count: usize,
    active_session: Option<usize>,
    message_count: usize,
    last_message_id: Option<String>,
    new_model_label: String,
    new_model_id: String,
    new_model_error: Option<String>,
}

impl AgentSurfaceHost {
    pub(crate) fn new(
        graphics: &NativeGraphicsContext<'_>,
        region: InputRegionId,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let surface = build_agent_surface(
            palette,
            &AgentPanel::default(),
            &EngineSettings::default(),
            None,
            AgentReadiness::ProviderDisabled,
            false,
            false,
            false,
            1.0,
            rect.width.max(1.0),
            rect.height.max(1.0),
        );
        Self {
            region,
            rect,
            host: graphics.create_ui_host(surface, AGENT_CLEAR),
            surface_key: None,
            model_menu_open: false,
            mode_menu_open: false,
            add_model_open: false,
            sidebar_motion: UiTween::new(1.0, UiMotionSpec::dock()),
            last_sync_time_seconds: 0.0,
            last_message_context: None,
        }
    }

    pub(crate) fn owner(&self) -> InputOwner {
        InputOwner::RetainedUi(self.region)
    }

    pub(crate) fn rect(&self) -> EditorRect {
        self.rect
    }

    pub(crate) fn needs_surface_sync(&self) -> bool {
        self.surface_key.is_none()
    }

    pub(crate) fn has_active_motion(&self, agent: &AgentPanel) -> bool {
        !self.sidebar_motion.is_settled()
            || self.host.has_active_motion()
            || agent.has_live_output()
    }

    pub(crate) fn sync(
        &mut self,
        rect: EditorRect,
        palette: StudioUiPalette,
        agent: &AgentPanel,
        settings: &EngineSettings,
        project: Option<&Project>,
        readiness: AgentReadiness,
        now_seconds: f64,
    ) {
        self.rect = rect;
        self.sidebar_motion
            .set_target(if agent.sidebar_open { 1.0 } else { 0.0 });
        let delta = (now_seconds - self.last_sync_time_seconds).clamp(0.0, 0.25) as f32;
        self.last_sync_time_seconds = now_seconds;
        let sidebar_progress = self.sidebar_motion.advance(delta, false);
        let key = AgentSurfaceKey::new(
            agent,
            settings,
            readiness,
            self.model_menu_open,
            self.mode_menu_open,
            self.add_model_open,
            sidebar_progress,
            rect.width,
            rect.height,
        );
        let message_context = (key.active_session, key.message_count);
        if self
            .last_message_context
            .is_some_and(|previous| previous != message_context)
        {
            self.host
                .session_mut()
                .reset_interaction_for_surface_change(Some("agent.messages"));
        }
        self.last_message_context = Some(message_context);
        if self.surface_key.as_ref() == Some(&key) {
            return;
        }
        let was_add_model_open = self
            .surface_key
            .as_ref()
            .is_some_and(|previous| previous.add_model_open);
        self.host.set_surface(build_agent_surface(
            palette,
            agent,
            settings,
            project,
            readiness,
            self.model_menu_open,
            self.mode_menu_open,
            self.add_model_open,
            sidebar_progress,
            rect.width.max(1.0),
            rect.height.max(1.0),
        ));
        self.host.session_mut().interaction.controls.set_text(
            "agent.input",
            &agent.input_text,
            MAX_INPUT_LENGTH,
        );
        self.host.session_mut().interaction.controls.set_text(
            "agent.model-label",
            &agent.new_model_label,
            120,
        );
        self.host.session_mut().interaction.controls.set_text(
            "agent.model-id",
            &agent.new_model_id,
            160,
        );
        if self.add_model_open && !was_add_model_open {
            self.host
                .session_mut()
                .interaction
                .focus
                .request_focus("agent.model-label");
        }
        self.surface_key = Some(key);
    }

    pub(crate) fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
        agent: &AgentPanel,
        settings: &EngineSettings,
        project: Option<&Project>,
    ) -> Vec<AgentAction> {
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, settings.language),
            input,
            router,
            self.owner(),
            raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );
        if !actions.is_empty() {
            // Commands such as opening a model menu intentionally do not
            // become AgentAction values. Mark the retained document dirty so
            // the next frame still presents their state change.
            self.surface_key = None;
        }
        // RafUI emits Click on pointer release. Closing a popup on press
        // destroys the retained surface before that release arrives, so the
        // active button can never dispatch `Active` or `+ Add model`. Only
        // close on release when the click did not belong to the popup's own
        // command family.
        if input
            .snapshot()
            .button_released(raf_core::PointerButton::Primary)
            && (self.model_menu_open || self.mode_menu_open || self.add_model_open)
            && !has_agent_popup_command(&actions)
        {
            self.model_menu_open = false;
            self.mode_menu_open = false;
            self.add_model_open = false;
            self.surface_key = None;
        }
        let mut translated = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } => match key.as_str() {
                    "agent.input" => translated.push(AgentAction::SetInput(value)),
                    "agent.model-label" => translated.push(AgentAction::SetNewModelLabel(value)),
                    "agent.model-id" => translated.push(AgentAction::SetNewModelId(value)),
                    _ => {}
                },
                UiAction::Command { name } => {
                    if let Some(action) =
                        self.parse_command(&name, project, settings.language, agent)
                    {
                        translated.push(action);
                    }
                }
                _ => {}
            }
        }
        translated
    }

    pub(crate) fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }

    pub(crate) fn host(&self) -> &DirectUiSurfaceHost {
        &self.host
    }

    fn parse_command(
        &mut self,
        name: &str,
        project: Option<&Project>,
        language: Language,
        agent: &AgentPanel,
    ) -> Option<AgentAction> {
        match name {
            "agent.sidebar.toggle" => Some(AgentAction::ToggleSidebar),
            "agent.sidebar.close" => Some(AgentAction::CloseSidebar),
            "agent.new-chat" => Some(AgentAction::NewChat),
            "agent.model.menu" => {
                self.model_menu_open = !self.model_menu_open;
                self.mode_menu_open = false;
                self.add_model_open = false;
                None
            }
            "agent.mode.menu" => {
                self.mode_menu_open = !self.mode_menu_open;
                self.model_menu_open = false;
                self.add_model_open = false;
                None
            }
            "agent.add-model.toggle" => {
                self.add_model_open = !self.add_model_open;
                self.model_menu_open = false;
                self.mode_menu_open = false;
                None
            }
            "agent.add-model.cancel" => {
                self.add_model_open = false;
                None
            }
            "agent.add-model.confirm" => {
                let valid = !agent.new_model_label.trim().is_empty()
                    && !agent.new_model_id.trim().is_empty()
                    && agent
                        .model_registry
                        .get(agent.new_model_label.trim())
                        .is_none();
                self.add_model_open = !valid;
                Some(AgentAction::AddModel)
            }
            "agent.add-model.settings" => {
                self.add_model_open = false;
                Some(AgentAction::OpenSettings)
            }
            "agent.settings" => Some(AgentAction::OpenSettings),
            "agent.submit" => Some(AgentAction::Submit),
            "agent.stop" => Some(AgentAction::Stop),
            "agent.approve" => Some(AgentAction::Approve),
            "agent.deny" => Some(AgentAction::Deny),
            _ => {
                if let Some(index) = name
                    .strip_prefix("agent.session:")
                    .and_then(|value| value.parse().ok())
                {
                    return Some(AgentAction::SelectSession(index));
                }
                if let Some(index) = name
                    .strip_prefix("agent.session.delete:")
                    .and_then(|value| value.parse().ok())
                {
                    return Some(AgentAction::DeleteSession(index));
                }
                if let Some(label) = name.strip_prefix("agent.model.select:") {
                    self.model_menu_open = false;
                    return Some(AgentAction::SelectModel(label.to_string()));
                }
                if let Some(mode) = name.strip_prefix("agent.mode.select:") {
                    self.mode_menu_open = false;
                    return match mode {
                        "passive" => Some(AgentAction::SetMode(AgentMode::Passive)),
                        "active" => Some(AgentAction::SetMode(AgentMode::Active)),
                        _ => None,
                    };
                }
                if let Some(index) = name
                    .strip_prefix("agent.suggestion:")
                    .and_then(|value| value.parse().ok())
                {
                    return suggestion(project, language, index).map(AgentAction::UseSuggestion);
                }
                None
            }
        }
    }
}

fn has_agent_popup_command(actions: &[UiDispatchedAction]) -> bool {
    actions.iter().any(|action| {
        matches!(
            &action.action,
            UiAction::Command { name }
                if name.starts_with("agent.model.")
                    || name.starts_with("agent.mode.")
                    || name.starts_with("agent.add-model.")
        )
    })
}

impl AgentSurfaceKey {
    fn new(
        agent: &AgentPanel,
        settings: &EngineSettings,
        readiness: AgentReadiness,
        model_menu_open: bool,
        mode_menu_open: bool,
        add_model_open: bool,
        sidebar_progress: f32,
        surface_width: f32,
        surface_height: f32,
    ) -> Self {
        Self {
            language: settings.language,
            agent_mode: settings.agent_mode,
            readiness,
            status: agent.runtime.status.clone(),
            sidebar_open: agent.sidebar_open,
            model_menu_open,
            mode_menu_open,
            add_model_open,
            sidebar_progress_bits: sidebar_progress.to_bits(),
            selected_model: agent.selected_model.clone(),
            active_provider: agent
                .effective_provider(settings)
                .map(|provider| provider.provider.display_name().to_string())
                .unwrap_or_default(),
            effective_model_id: agent
                .effective_provider(settings)
                .map(|provider| agent.effective_model(provider))
                .unwrap_or_default(),
            runtime_visual_revision: agent.visual_revision(),
            streaming_enabled: settings.agent_streaming_enabled,
            surface_width_bits: surface_width.to_bits(),
            surface_height_bits: surface_height.to_bits(),
            session_count: agent.history.sessions.len(),
            active_session: agent.history.active_index,
            message_count: agent.runtime.messages.len(),
            last_message_id: agent
                .runtime
                .messages
                .last()
                .map(|message| message.id.to_string()),
            new_model_label: agent.new_model_label.clone(),
            new_model_id: agent.new_model_id.clone(),
            new_model_error: agent.new_model_error.clone(),
        }
    }
}

fn localized(language: Language, english: &str, spanish: &str) -> String {
    match language {
        Language::Spanish => spanish.to_string(),
        Language::English => english.to_string(),
    }
}

fn popup_rect(
    anchor_x: f32,
    anchor_y: f32,
    width: f32,
    height: f32,
    surface_width: f32,
    surface_height: f32,
) -> UiRect {
    let horizontal_margin = 8.0;
    let available_width = (surface_width - horizontal_margin * 2.0).max(1.0);
    let width = width.min(available_width).max(1.0);
    let height = height.min((surface_height - horizontal_margin * 2.0).max(1.0));
    let max_x = (surface_width - width - horizontal_margin).max(horizontal_margin);
    let x = anchor_x.clamp(horizontal_margin, max_x);
    let below = anchor_y + height <= surface_height - horizontal_margin;
    let y = if below {
        anchor_y
    } else {
        (anchor_y - height - 8.0).max(horizontal_margin)
    };
    UiRect::new(x, y, width, height)
}

fn active_provider_model(agent: &AgentPanel, settings: &EngineSettings) -> (String, String) {
    let provider = settings.default_ai_provider.display_name().to_string();
    let model = agent
        .effective_provider(settings)
        .map(|config| config.model.clone())
        .unwrap_or_default();
    (provider, model)
}

fn model_option_details(
    label: &str,
    agent: &AgentPanel,
    settings: &EngineSettings,
) -> (String, String, bool) {
    if label == raf_ai::agent_model_registry::AgentModelRegistry::PROVIDER_DEFAULT {
        let (provider, model) = active_provider_model(agent, settings);
        return (provider, model, true);
    }
    let Some(shortcut) = agent.model_registry.get(label) else {
        let (provider, _) = active_provider_model(agent, settings);
        return (provider, String::new(), false);
    };
    (
        shortcut.provider.display_name().to_string(),
        shortcut.model_id.clone(),
        shortcut.provider == settings.default_ai_provider,
    )
}

fn effective_selection_label(agent: &AgentPanel, settings: &EngineSettings) -> String {
    if agent.selected_model != raf_ai::agent_model_registry::AgentModelRegistry::PROVIDER_DEFAULT
        && agent
            .model_registry
            .resolve_model_id(&agent.selected_model, settings.default_ai_provider)
            .is_some()
    {
        agent.selected_model.clone()
    } else {
        raf_ai::agent_model_registry::AgentModelRegistry::PROVIDER_DEFAULT.to_string()
    }
}

fn selected_model_label(agent: &AgentPanel, settings: &EngineSettings) -> String {
    let selected = effective_selection_label(agent, settings);
    if selected == raf_ai::agent_model_registry::AgentModelRegistry::PROVIDER_DEFAULT {
        localized(
            settings.language,
            "Provider default",
            "Predeterminado del proveedor",
        )
    } else {
        selected
    }
}

fn selected_model_details(agent: &AgentPanel, settings: &EngineSettings) -> String {
    let selected = effective_selection_label(agent, settings);
    let (provider, model_id, _) = model_option_details(&selected, agent, settings);
    if model_id.trim().is_empty() {
        format!(
            "{}  |  {}",
            provider,
            localized(
                settings.language,
                "Model not configured",
                "Modelo no configurado"
            )
        )
    } else {
        format!("{}  |  {}", provider, model_id)
    }
}

fn build_agent_surface(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    settings: &EngineSettings,
    project: Option<&Project>,
    readiness: AgentReadiness,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    sidebar_progress: f32,
    surface_width: f32,
    surface_height: f32,
) -> UiSurface {
    let menu_width = (surface_width - 16.0).min(360.0).max(220.0);
    let model_height = (74.0 + agent.model_registry.selector_labels().len() as f32 * 44.0)
        .min((surface_height - 16.0).max(132.0));
    let mut main = UiNode::new("agent.main", UiNodeKind::Panel)
        .with_class("agent-main")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 10.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(build_header(
            palette,
            agent,
            settings,
            model_menu_open,
            mode_menu_open,
        ));

    if readiness != AgentReadiness::Ready {
        let text = match readiness {
            AgentReadiness::ProviderDisabled => {
                raf_core::i18n::t("app.agent_provider_disabled", settings.language)
            }
            AgentReadiness::ModelMissing => {
                raf_core::i18n::t("app.agent_model_missing", settings.language)
            }
            AgentReadiness::AdapterRequired => {
                raf_core::i18n::t("app.agent_provider_adapter_required", settings.language)
            }
            AgentReadiness::Ready => String::new(),
        };
        main = main.with_child(status_card(palette, &text, true));
    } else if settings.agent_mode == AgentMode::Active {
        main = main.with_child(status_card(
            palette,
            &raf_core::i18n::t("app.agent_active_mode_warning", settings.language),
            true,
        ));
    }
    if agent.runtime.status.blocks_input() {
        main = main.with_child(activity_card(palette, agent, settings.language));
    }
    if agent.runtime.status == AgentStatus::AwaitingApproval
        && !agent.runtime.pending_calls.is_empty()
    {
        main = main.with_child(approval_card(palette));
    }
    main = main
        .with_child(build_message_summary(
            palette,
            agent,
            settings.language,
            settings.agent_max_response_tokens,
        ))
        .with_child(build_messages(palette, agent, settings.language))
        .with_child(build_suggestions(
            palette,
            agent,
            project,
            readiness,
            settings.language,
        ));
    main = main.with_child(build_composer(palette, agent, readiness, settings.language));

    let root = UiNode::new("agent.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 1.0,
            overflow: UiOverflow::Visible,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.root_style())
        .with_child(build_sidebar(palette, agent, sidebar_progress))
        .with_child(main);
    let mut root = root;
    // Menus are siblings of the workbench content instead of children of the
    // trigger buttons. This gives them a stable z-order, keeps keyboard focus
    // inside the popup and prevents the main panel's clipping from swallowing
    // the form on narrow windows.
    if add_model_open {
        root = root.with_child(
            build_add_model(palette, agent, settings, menu_width).with_layout(
                UiLayout::absolute(popup_rect(
                    (surface_width - menu_width) * 0.5,
                    58.0,
                    menu_width,
                    306.0,
                    surface_width,
                    surface_height,
                ))
                .with_z_index(400),
            ),
        );
    } else if model_menu_open {
        root = root.with_child(
            build_model_menu(palette, agent, settings, menu_width, model_height).with_layout(
                UiLayout::absolute(popup_rect(
                    (surface_width - menu_width) * 0.5,
                    58.0,
                    menu_width,
                    model_height,
                    surface_width,
                    surface_height,
                ))
                .with_z_index(390),
            ),
        );
    } else if mode_menu_open {
        root = root.with_child(
            build_mode_menu(palette, settings.agent_mode, settings.language).with_layout(
                UiLayout::absolute(popup_rect(
                    surface_width - MODE_MENU_WIDTH - 8.0,
                    58.0,
                    MODE_MENU_WIDTH,
                    MODE_MENU_HEIGHT,
                    surface_width,
                    surface_height,
                ))
                .with_z_index(390),
            ),
        );
    }
    let mut surface = UiSurface::new("agent.workbench", palette, root);
    surface.style_sheet = agent_style_sheet(palette);
    surface.with_retained_tooltips(false)
}

fn build_sidebar(palette: StudioUiPalette, agent: &AgentPanel, progress: f32) -> UiNode {
    let tokens = palette.tokens();
    let mut sessions = UiNode::scroll_view("agent.sessions", UiScrollAxis::Vertical)
        .with_class("agent-session-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    for (index, session) in agent.history.sessions.iter().enumerate().rev() {
        let active = agent.history.active_index == Some(index);
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
                padding: UiSpacing::xy(4.0, 3.0),
                ..UiLayout::fixed(0.0, 36.0).with_width_mode(UiSizeMode::Fill)
            });
        row = row.with_child(
            UiNode::new(format!("agent.session.select.{index}"), UiNodeKind::Button)
                .with_text_value(truncate(&session.title, 36))
                .with_layout(UiLayout {
                    grow: 1.0,
                    padding: UiSpacing::xy(4.0, 2.0),
                    ..UiLayout::fixed(0.0, 28.0)
                })
                .with_text_style(UiTextStyle::body(if active {
                    tokens.accent_hot
                } else {
                    tokens.text
                }))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session:{index}"),
                )),
        );
        row = row.with_child(
            UiNode::new(format!("agent.session.delete.{index}"), UiNodeKind::Button)
                .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                .with_tooltip_key("app.agent_delete")
                .with_layout(UiLayout::fixed(24.0, 24.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session.delete:{index}"),
                )),
        );
        sessions = sessions.with_child(row);
    }
    if agent.history.sessions.is_empty() {
        sessions = sessions.with_child(
            UiNode::new("agent.sessions.empty", UiNodeKind::Label)
                .with_text_value(localized(
                    agent.language,
                    "No conversations yet",
                    "Aun no hay conversaciones",
                ))
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        );
    }
    UiNode::new("agent.sidebar", UiNodeKind::Panel)
        .with_class("agent-sidebar")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::xy(9.0, 10.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed((224.0 * progress).max(1.0), 0.0).with_height_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new("agent.sidebar.header", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::SpaceBetween,
                    ..UiLayout::fixed(0.0, 30.0)
                })
                .with_child(
                    UiNode::new("agent.sidebar.title", UiNodeKind::Label)
                        .with_text_key("app.agent_sessions")
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fit_content()
                        })
                        .with_text_style(UiTextStyle::panel_title(tokens.text)),
                )
                .with_child(
                    UiNode::new("agent.sidebar.close", UiNodeKind::Button)
                        .with_icon(UiIcon::new(UiIconId::Close).with_size(UiIconSize::Small))
                        .with_tooltip_key("app.agent_toggle_sidebar")
                        .with_layout(UiLayout::fixed(28.0, 28.0))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "agent.sidebar.close",
                        )),
                ),
        )
        .with_child(
            UiNode::new("agent.new", UiNodeKind::Button)
                .with_class("agent-new-chat")
                .with_text_key("app.agent_new_chat")
                .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "agent.new-chat",
                )),
        )
        .with_child(sessions)
}

fn build_header(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    settings: &EngineSettings,
    model_menu_open: bool,
    mode_menu_open: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let model_label = truncate(&selected_model_label(agent, settings), 24);
    let model_details = selected_model_details(agent, settings);
    let mode_label = if settings.agent_mode == AgentMode::Active {
        localized(settings.language, "Active", "Activo")
    } else {
        localized(settings.language, "Passive", "Pasivo")
    };
    let model_button = command_button(
        "agent.model",
        &model_label,
        "agent.model.menu",
        model_menu_open,
        tokens,
    )
    .with_layout(UiLayout::fixed(148.0, 30.0).with_text_safe_area(true))
    .with_tooltip_value(model_details.clone());

    let mode_button = command_button(
        "agent.mode",
        &mode_label,
        "agent.mode.menu",
        mode_menu_open,
        tokens,
    )
    .with_layout(UiLayout::fixed(88.0, 30.0).with_text_safe_area(true));

    UiNode::new("agent.header", UiNodeKind::Toolbar)
        .with_class("agent-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 34.0)
                .with_width_mode(UiSizeMode::Fill)
                .with_z_index(200)
        })
        .with_child(
            UiNode::new("agent.header.title", UiNodeKind::Label)
                .with_text_value(
                    agent
                        .history
                        .active_session()
                        .map(|session| session.title.clone())
                        .unwrap_or_else(|| {
                            raf_core::i18n::t("app.agent_new_chat_default", settings.language)
                        }),
                )
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                })
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(model_button)
        .with_child(
            UiNode::new("agent.model.details", UiNodeKind::Label)
                .with_text_value(truncate(&model_details, 42))
                .with_layout(UiLayout {
                    max_size: [230.0, 30.0],
                    ..UiLayout::fit_content()
                })
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(mode_button)
        .with_child(
            UiNode::new("agent.sidebar.toggle", UiNodeKind::Button)
                .with_icon(UiIcon::new(UiIconId::Menu).with_size(UiIconSize::Small))
                .with_tooltip_key("app.agent_toggle_sidebar")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "agent.sidebar.toggle",
                )),
        )
        .with_child(
            UiNode::new("agent.settings", UiNodeKind::Button)
                .with_icon(UiIcon::new(UiIconId::Settings).with_size(UiIconSize::Small))
                .with_text_key("app.agent_settings")
                .with_layout(UiLayout::fixed(94.0, 30.0).with_text_safe_area(true))
                .with_tooltip_key("app.settings_menu")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "agent.settings",
                )),
        )
}

fn command_button(
    id: &str,
    label: &str,
    command: &str,
    selected: bool,
    tokens: raf_ui::UiTokens,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if selected {
            "agent-command-selected"
        } else {
            "agent-command"
        })
        .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::FitContent))
        .with_text_value(label.to_string())
        .with_text_style(UiTextStyle::button(tokens.text))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn build_model_menu(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    settings: &EngineSettings,
    width: f32,
    height: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let active_label = effective_selection_label(agent, settings);
    let mut options = UiNode::scroll_view("agent.model.options", UiScrollAxis::Vertical)
        .with_class("agent-model-options")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            grow: 1.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::default()
        });
    for label in agent.model_registry.selector_labels() {
        let (provider, model_id, compatible) = model_option_details(&label, agent, settings);
        let current = label == active_label;
        let class = if !compatible {
            "agent-model-option-disabled"
        } else if current {
            "agent-model-option-current"
        } else {
            "agent-model-option"
        };
        let mut row = UiNode::new(format!("agent.model.{label}"), UiNodeKind::Button)
            .with_class(class)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Start,
                gap: 1.0,
                padding: UiSpacing::xy(8.0, 5.0),
                ..UiLayout::fixed(0.0, 40.0).with_width_mode(UiSizeMode::Fill)
            })
            .focusable()
            .disabled(!compatible)
            .with_child(
                UiNode::new(format!("agent.model.{label}.label"), UiNodeKind::Label)
                    .with_text_value(if current {
                        format!(
                            "{}  {}",
                            localized(settings.language, "Selected", "Seleccionado"),
                            label
                        )
                    } else {
                        label.clone()
                    })
                    .with_text_style(UiTextStyle::button(tokens.text)),
            )
            .with_child(
                UiNode::new(format!("agent.model.{label}.details"), UiNodeKind::Label)
                    .with_text_value(if compatible {
                        format!("{}  |  {}", provider, truncate(&model_id, 92))
                    } else {
                        format!(
                            "{}  |  {}",
                            provider,
                            localized(
                                settings.language,
                                "Provider mismatch",
                                "Proveedor incompatible",
                            )
                        )
                    })
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            );
        if compatible {
            row = row.with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("agent.model.select:{label}"),
            ));
        }
        options = options.with_child(row);
    }
    return UiNode::new("agent.model.menu.panel", UiNodeKind::Menu)
        .with_class("agent-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::same(6.0),
            ..UiLayout::fixed(width, height)
        })
        .with_child(
            UiNode::new("agent.model.heading", UiNodeKind::Label)
                .with_text_value(localized(
                    settings.language,
                    "Configured models",
                    "Modelos configurados",
                ))
                .with_text_style(UiTextStyle::panel_title(tokens.text_muted)),
        )
        .with_child(options)
        .with_child(
            UiNode::new("agent.model.separator", UiNodeKind::Separator)
                .with_class("agent-menu-separator")
                .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("agent.model.add", UiNodeKind::Button)
                .with_class("agent-add-model-button")
                .with_text_key("app.agent_add_model")
                // Give the modal action a concrete hitbox. A zero-basis Fill
                // child was visually painted by some layouts but could lose
                // its release target after the popup rebuilt.
                .with_layout(UiLayout::fixed((width - 12.0).max(1.0), 32.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "agent.add-model.toggle",
                )),
        );
}

fn build_mode_menu(palette: StudioUiPalette, mode: AgentMode, language: Language) -> UiNode {
    let tokens = palette.tokens();
    let mut menu = UiNode::new("agent.mode.menu.panel", UiNodeKind::Toolbar)
        .with_class("agent-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 4.0,
            padding: UiSpacing::xy(5.0, 5.0),
            ..UiLayout::fixed(MODE_MENU_WIDTH, MODE_MENU_HEIGHT)
        });
    let option_width = ((MODE_MENU_WIDTH - 10.0 - 4.0) * 0.5).max(1.0);
    for value in ["passive", "active"] {
        let is_active = value == "active";
        let label = if is_active {
            localized(language, "Active", "Activo")
        } else {
            localized(language, "Passive", "Pasivo")
        };
        let current = is_active == (mode == AgentMode::Active);
        menu = menu.with_child(
            UiNode::new(format!("agent.mode.{value}"), UiNodeKind::Button)
                .with_class(if current {
                    "agent-mode-option-current"
                } else {
                    "agent-mode-option"
                })
                .with_layout(UiLayout::fixed(option_width, 30.0))
                .with_text_value(label)
                .with_text_style(UiTextStyle::button(tokens.text))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.mode.select:{value}"),
                )),
        );
    }
    menu
}

fn build_add_model(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    settings: &EngineSettings,
    width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let input_width = (width - 16.0).clamp(180.0, 320.0);
    let mut panel = UiNode::new("agent.add-model", UiNodeKind::Panel)
        .with_class("agent-popup")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 8.0),
            ..UiLayout::fixed(width, 306.0)
        })
        .with_child(
            UiNode::new("agent.add-model.title", UiNodeKind::Label)
                .with_text_key("app.agent_add_model_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text)),
        )
        .with_child(
            UiNode::new("agent.add-model.provider", UiNodeKind::Label)
                .with_text_value(format!(
                    "{}: {}",
                    raf_core::i18n::t("app.agent_add_model_provider", settings.language),
                    settings.default_ai_provider.display_name()
                ))
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::new("agent.add-model.label-caption", UiNodeKind::Label)
                .with_text_key("app.agent_model_label")
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::text_input(
                "agent.model-label",
                UiTextInput {
                    value_key: "agent.model-label".to_string(),
                    placeholder_key: Some("app.agent_model_label".to_string()),
                    max_length: 120,
                    multiline: false,
                    password: false,
                    submit_command: Some("agent.add-model.confirm".to_string()),
                },
            )
            .with_layout(UiLayout::fixed(input_width, 28.0)),
        )
        .with_child(
            UiNode::new("agent.add-model.id-caption", UiNodeKind::Label)
                .with_text_key("app.agent_model_id")
                .with_text_style(UiTextStyle::body(tokens.text_muted)),
        )
        .with_child(
            UiNode::text_input(
                "agent.model-id",
                UiTextInput {
                    value_key: "agent.model-id".to_string(),
                    placeholder_key: Some("app.agent_model_id".to_string()),
                    max_length: 160,
                    multiline: false,
                    password: false,
                    submit_command: Some("agent.add-model.confirm".to_string()),
                },
            )
            .with_layout(UiLayout::fixed(input_width, 28.0)),
        )
        .with_child(
            UiNode::new("agent.add-model.settings", UiNodeKind::Button)
                .with_class("agent-secondary-button")
                .with_text_key("app.agent_open_ai_settings")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "agent.add-model.settings",
                )),
        );
    if let Some(error) = agent.new_model_error.as_deref() {
        panel = panel.with_child(
            UiNode::new("agent.add-model.error", UiNodeKind::Label)
                .with_text_value(error)
                .with_text_style(UiTextStyle::body(tokens.danger)),
        );
    }
    panel
        .with_child(
            UiNode::new("agent.add-model.confirm", UiNodeKind::Button)
                .with_class("agent-primary-button")
                .with_text_key("app.agent_add_model_confirm")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "agent.add-model.confirm",
                )),
        )
        .with_child(
            UiNode::new("agent.add-model.cancel", UiNodeKind::Button)
                .with_class("agent-secondary-button")
                .with_text_key("app.agent_cancel")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "agent.add-model.cancel",
                )),
        )
}

fn build_messages(palette: StudioUiPalette, agent: &AgentPanel, language: Language) -> UiNode {
    let mut list = UiNode::scroll_view("agent.messages", UiScrollAxis::Vertical)
        .with_class("agent-messages")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 7.0,
            grow: 1.0,
            min_size: [0.0, 96.0],
            overflow: UiOverflow::ScrollY,
            // Reserve space for RafUI's generated scrollbar so the thumb
            // never covers the last glyph of a wrapped response.
            padding: UiSpacing {
                left: 4.0,
                right: 16.0,
                top: 2.0,
                bottom: 2.0,
            },
            ..UiLayout::default()
        });
    let visible = agent
        .runtime
        .messages
        .iter()
        .filter(|message| message.role != MessageRole::System)
        .collect::<Vec<_>>();
    // The transcript is one continuous scroll surface. The old page buttons
    // made the chat feel clipped and competed with the scrollbar, so older
    // messages stay in the same list instead of requiring a second pager.
    for (index, message) in visible.iter().enumerate() {
        list = list.with_child(message_card(palette, index, message, language));
    }
    if visible.is_empty() {
        list = list.with_child(
            UiNode::new("agent.empty", UiNodeKind::Panel)
                .with_class("agent-empty")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 4.0,
                    padding: UiSpacing::xy(14.0, 12.0),
                    ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("agent.empty.title", UiNodeKind::Label)
                        .with_text_key("app.agent_empty_title")
                        .with_text_style(UiTextStyle::panel_title(palette.tokens().text)),
                )
                .with_child(
                    UiNode::new("agent.empty.subtitle", UiNodeKind::Label)
                        .with_text_key("app.agent_empty_subtitle")
                        .with_text_style(UiTextStyle::body(palette.tokens().text_muted)),
                ),
        );
    }
    list
}

fn build_message_summary(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    language: Language,
    max_response_tokens: u32,
) -> UiNode {
    let tokens = palette.tokens();
    let visible = agent
        .runtime
        .messages
        .iter()
        .filter(|message| message.role != MessageRole::System)
        .collect::<Vec<_>>();
    let count = visible.len();
    let end = count;
    let total_chars = visible
        .iter()
        .map(|message| message.content.chars().count())
        .sum::<usize>();
    let approx_tokens = total_chars.div_ceil(APPROX_CHARS_PER_TOKEN);
    let text = if count == 0 {
        format!(
            "{} | {} {}",
            raf_core::i18n::t("app.agent_no_messages", language),
            raf_core::i18n::t("app.agent_response_max", language),
            max_response_tokens
        )
    } else {
        format!(
            "{} 1-{} {} {} | {} ~{} {} | {} {}",
            raf_core::i18n::t("app.agent_messages_label", language),
            end,
            raf_core::i18n::t("app.agent_messages_of", language),
            count,
            raf_core::i18n::t("app.agent_context_approx", language),
            approx_tokens,
            raf_core::i18n::t("app.agent_tokens", language),
            raf_core::i18n::t("app.agent_response_max", language),
            max_response_tokens
        )
    };
    UiNode::new("agent.messages.summary", UiNodeKind::Toolbar)
        .with_class("agent-message-summary")
        .with_layout(UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill))
        .with_text_value(text)
        .with_text_style(UiTextStyle::body(tokens.text_muted))
}

fn message_card(
    palette: StudioUiPalette,
    index: usize,
    message: &ChatMessage,
    language: Language,
) -> UiNode {
    let tokens = palette.tokens();
    let (class, role) = match message.role {
        MessageRole::User => ("agent-message-user", localized(language, "You", "Tu")),
        MessageRole::Assistant => (
            "agent-message-assistant",
            localized(language, "Agent", "Agent"),
        ),
        MessageRole::Tool => (
            "agent-message-tool",
            localized(language, "Tool result", "Resultado de herramienta"),
        ),
        MessageRole::System => (
            "agent-message-system",
            localized(language, "System", "Sistema"),
        ),
    };
    UiNode::new(format!("agent.message.{index}"), UiNodeKind::Panel)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::xy(9.0, 8.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("agent.message.{index}.role"), UiNodeKind::Label)
                .with_text_value(role)
                .with_text_style(UiTextStyle::button(tokens.accent_hot)),
        )
        .with_child(
            UiNode::new(format!("agent.message.{index}.content"), UiNodeKind::Label)
                .with_text_value(truncate(&message.content, MAX_MESSAGE_DISPLAY_CHARS))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill))
                .with_text_style(UiTextStyle::body(tokens.text)),
        )
}

fn build_suggestions(
    _palette: StudioUiPalette,
    agent: &AgentPanel,
    project: Option<&Project>,
    readiness: AgentReadiness,
    language: Language,
) -> UiNode {
    let mut row = UiNode::new("agent.suggestions", UiNodeKind::Toolbar).with_layout(UiLayout {
        flow: UiFlow::RowWrap,
        gap: 5.0,
        ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
    });
    if readiness != AgentReadiness::Ready {
        return row;
    }
    for index in 0..3 {
        if let Some(value) = suggestion(project, language, index) {
            row = row.with_child(
                UiNode::new(format!("agent.suggestion.{index}"), UiNodeKind::Button)
                    .with_class("agent-suggestion")
                    .with_text_value(value)
                    .with_layout(UiLayout::fit_content())
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("agent.suggestion:{index}"),
                    )),
            );
        }
    }
    if agent.runtime.messages.is_empty() {
        row
    } else {
        row.with_layout(UiLayout::fixed(0.0, 0.0))
    }
}

fn build_composer(
    palette: StudioUiPalette,
    agent: &AgentPanel,
    readiness: AgentReadiness,
    language: Language,
) -> UiNode {
    let tokens = palette.tokens();
    let running = agent.runtime.status.blocks_input();
    let command = if running {
        "agent.stop"
    } else {
        "agent.submit"
    };
    let enabled =
        running || (readiness == AgentReadiness::Ready && !agent.input_text.trim().is_empty());
    let button_class = if running {
        "agent-stop-button"
    } else if enabled {
        "agent-primary-button"
    } else {
        "agent-disabled-button"
    };
    let button_label = if running {
        localized(language, "Stop", "Detener")
    } else {
        localized(language, "Send", "Enviar")
    };
    UiNode::new("agent.composer", UiNodeKind::Toolbar)
        .with_class("agent-composer")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::End,
            gap: 6.0,
            padding: UiSpacing::xy(0.0, 4.0),
            ..UiLayout::fixed(0.0, 52.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::text_input(
                "agent.input",
                UiTextInput {
                    value_key: "agent.input".to_string(),
                    placeholder_key: Some("app.agent_input_placeholder".to_string()),
                    max_length: MAX_INPUT_LENGTH,
                    multiline: true,
                    password: false,
                    submit_command: Some("agent.submit".to_string()),
                },
            )
            .with_class("agent-input")
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [120.0, 44.0],
                ..UiLayout::fixed(0.0, 44.0)
            })
            .with_text_style(UiTextStyle::body(tokens.text)),
        )
        .with_child(
            UiNode::new("agent.submit", UiNodeKind::Button)
                .with_class(button_class)
                .disabled(!enabled)
                .with_icon(
                    UiIcon::new(if running {
                        UiIconId::Close
                    } else {
                        UiIconId::ChevronRight
                    })
                    .with_size(UiIconSize::Small),
                )
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::Center,
                    gap: 5.0,
                    ..UiLayout::fixed(82.0, 36.0)
                })
                .with_text_value(button_label)
                .with_text_style(UiTextStyle::button(tokens.text))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        )
}

fn status_card(palette: StudioUiPalette, text: &str, warning: bool) -> UiNode {
    UiNode::new("agent.status", UiNodeKind::Panel)
        .with_class(if warning {
            "agent-status-warning"
        } else {
            "agent-status"
        })
        .with_layout(UiLayout {
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_text_value(text.to_string())
        .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
}

fn activity_card(palette: StudioUiPalette, agent: &AgentPanel, language: Language) -> UiNode {
    let text = match &agent.runtime.status {
        AgentStatus::Thinking => localized(
            language,
            "Thinking while waiting for the provider...",
            "Pensando mientras espera al proveedor...",
        ),
        AgentStatus::ExecutingTools => localized(
            language,
            "Applying tool changes...",
            "Aplicando cambios de herramienta...",
        ),
        AgentStatus::AwaitingApproval => localized(
            language,
            "Approval required before continuing.",
            "Se requiere aprobación para continuar.",
        ),
        AgentStatus::Done | AgentStatus::Error => String::new(),
    };
    status_card(palette, &text, false)
}

fn approval_card(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("agent.approval", UiNodeKind::Toolbar)
        .with_class("agent-approval")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(8.0, 6.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("agent.approval.label", UiNodeKind::Label)
                .with_text_key("app.agent_pending_tools")
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                })
                .with_text_style(UiTextStyle::body(tokens.warning)),
        )
        .with_child(
            UiNode::new("agent.approve", UiNodeKind::Button)
                .with_text_key("app.agent_approve")
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "agent.approve")),
        )
        .with_child(
            UiNode::new("agent.deny", UiNodeKind::Button)
                .with_text_key("app.agent_deny")
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "agent.deny")),
        )
}

fn suggestion(project: Option<&Project>, language: Language, index: usize) -> Option<String> {
    let name =
        project
            .map(|project| project.name.as_str())
            .unwrap_or(if language == Language::Spanish {
                "este proyecto"
            } else {
                "this project"
            });
    [
        localized(
            language,
            &format!("Inspect {name}"),
            &format!("Inspecciona {name}"),
        ),
        localized(
            language,
            "Create a starter blockout",
            "Crea un blockout inicial",
        ),
        localized(
            language,
            "Explain the current scene",
            "Explica la escena actual",
        ),
    ]
    .get(index)
    .cloned()
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

fn agent_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("agent-command".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-command".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-command".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-command-selected".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-session".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-session".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-session-active".to_string()),
                UiStylePatch {
                    fill: Some([55, 43, 26, 255]),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-suggestion".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-suggestion".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-suggestion".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-new-chat".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-new-chat".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-empty".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-model-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-model-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-model-option-current".to_string()),
                UiStylePatch {
                    fill: Some([55, 43, 26, 255]),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-model-option-current".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-model-option-disabled".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    text: Some(tokens.text_muted),
                    opacity: Some(0.48),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Disabled),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-add-model-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-add-model-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-primary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-primary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-stop-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.warning),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.warning),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-disabled-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    text: Some(tokens.text_muted),
                    opacity: Some(0.55),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Disabled),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-composer".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-secondary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
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
                UiStyleSelector::Class("agent-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-mode-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-mode-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-mode-option-current".to_string()),
                UiStylePatch {
                    fill: Some([55, 43, 26, 255]),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-mode-option-current".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-popup".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-message-summary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-sidebar".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-message-user".to_string()),
                UiStylePatch {
                    fill: Some([45, 36, 24, 255]),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-message-assistant".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-message-tool".to_string()),
                UiStylePatch {
                    fill: Some([22, 28, 35, 255]),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-status-warning".to_string()),
                UiStylePatch {
                    fill: Some([66, 44, 20, 255]),
                    border: Some(tokens.warning),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-approval".to_string()),
                UiStylePatch {
                    fill: Some([63, 43, 24, 255]),
                    border: Some(tokens.warning),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_ai::chat::ChatMessage;
    use raf_core::config::EngineSettings;
    use raf_render::api_graphic_basic::ui_surface::{UiHitTestMode, UiSurface};

    fn test_surface(
        agent: &AgentPanel,
        settings: &EngineSettings,
        model_menu_open: bool,
        mode_menu_open: bool,
        add_model_open: bool,
    ) -> UiSurface {
        build_agent_surface(
            StudioUiPalette::IndustrialDark,
            agent,
            settings,
            None,
            AgentReadiness::ProviderDisabled,
            model_menu_open,
            mode_menu_open,
            add_model_open,
            1.0,
            640.0,
            480.0,
        )
    }

    #[test]
    fn mode_popup_is_compact_and_active_has_a_real_hitbox() {
        let agent = AgentPanel::default();
        let settings = EngineSettings::default();
        let surface = test_surface(&agent, &settings, false, true, false);
        let frame = surface.build_frame(640, 480, [0, 0, 0, 255]);
        let active = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.mode.active")
            .expect("active mode option");

        assert!(active.rect.width < 90.0);
        let hit = raf_ui::hit_test(
            &frame.hit_regions,
            [
                active.rect.x + active.rect.width * 0.5,
                active.rect.y + 15.0,
            ],
            UiHitTestMode::InteractiveOnly,
        )
        .expect("active mode hit");
        assert_eq!(hit.id, "agent.mode.active");
    }

    #[test]
    fn model_menu_add_action_and_form_are_present_in_independent_layers() {
        let agent = AgentPanel::default();
        let settings = EngineSettings::default();
        let menu = test_surface(&agent, &settings, true, false, false);
        let menu_frame = menu.build_frame(640, 480, [0, 0, 0, 255]);
        let add = menu_frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.model.add")
            .expect("add model action");
        let hit = raf_ui::hit_test(
            &menu_frame.hit_regions,
            [
                add.rect.x + add.rect.width * 0.5,
                add.rect.y + add.rect.height * 0.5,
            ],
            UiHitTestMode::InteractiveOnly,
        )
        .expect("add model hit");
        assert_eq!(hit.id, "agent.model.add");

        let form = test_surface(&agent, &settings, false, false, true);
        assert!(form
            .root
            .children
            .iter()
            .any(|node| node.id == "agent.add-model"));
    }

    #[test]
    fn popup_release_keeps_internal_mode_and_add_model_commands_alive() {
        let internal = UiDispatchedAction {
            target_id: "agent.mode.active".to_string(),
            event: UiEventKind::Click,
            action: UiAction::Command {
                name: "agent.mode.select:active".to_string(),
            },
        };
        assert!(has_agent_popup_command(&[internal]));

        let outside = UiDispatchedAction {
            target_id: "agent.settings".to_string(),
            event: UiEventKind::Click,
            action: UiAction::Command {
                name: "agent.settings".to_string(),
            },
        };
        assert!(!has_agent_popup_command(&[outside]));
    }

    #[test]
    fn message_summary_exposes_continuous_range_and_response_limit() {
        let mut agent = AgentPanel::default();
        for index in 0..24 {
            agent
                .runtime
                .messages
                .push(ChatMessage::user(&format!("Prompt {index}")));
        }
        let settings = EngineSettings::default();
        let surface = test_surface(&agent, &settings, false, false, false);
        let frame = surface.build_frame(640, 480, [0, 0, 0, 255]);
        let summary = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.messages.summary")
            .expect("message summary");
        let text = summary.text_value.as_deref().expect("summary text");
        assert!(text.contains("24"));
        assert!(text.contains(&settings.agent_max_response_tokens.to_string()));
        assert!(text.contains("response"));
        assert_eq!(
            frame
                .layout_boxes
                .iter()
                .filter(|layout| layout.id.starts_with("agent.message.")
                    && layout.kind == UiNodeKind::Panel)
                .count(),
            24
        );
    }
}
