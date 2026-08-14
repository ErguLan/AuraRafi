//! Retained RafUI surface for the Agent workbench.
//!
//! The surface is deliberately opinionated: a compact project-local chat
//! rail, a readable conversation, and a visible activity/approval trail. The
//! host owns only transient menu and motion state; the controller owns all AI
//! behavior.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use eframe::{egui, egui_wgpu};
use raf_ai::agent_runtime::AgentStatus;
use raf_ai::chat::{ChatMessage, MessageRole};
use raf_ai::provider::AgentMode;
use raf_core::config::{
    EngineSettings, Language, AGENT_MESSAGE_PAGE_SIZE_MAX, AGENT_MESSAGE_PAGE_SIZE_MIN,
};
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType};
use raf_render::api_graphic_basic::ui_surface::{UiIcon, UiIconId, UiIconSize, UiSurface};
use raf_ui::{
    UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiMotionSpec, UiNode,
    UiNodeKind, UiOverflow, UiRect, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput, UiTextStyle,
    UiTween,
};

use super::ai_chat::{AgentAction, AgentPanel, AgentReadiness};
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

const SIDEBAR_WIDTH: f32 = 220.0;
const HEADER_HEIGHT: f32 = 40.0;

pub(crate) struct AgentSurfaceHost {
    bridge: RafUiSurfaceBridge,
    sidebar_bridge: RafUiSurfaceBridge,
    load_messages_overlay: RafUiSurfaceBridge,
    sidebar_surface: Option<UiSurface>,
    sidebar_surface_revision: u64,
    load_messages_motion: UiTween,
    sidebar_motion: UiTween,
    sidebar_motion_initialized: bool,
    last_sidebar_time_seconds: f64,
    last_overlay_time_seconds: f64,
    surface_cache: Option<(AgentSurfaceKey, UiSurface)>,
    surface_revision: u64,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    message_history_offset: usize,
    diag_frames: u64,
    diag_cache_misses: u64,
    diag_show_ms: f64,
    diag_frame_delta_ms: f64,
    diag_last_log_seconds: f64,
    diag_prev_frame_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentSurfaceKey {
    palette: raf_ui::StudioUiPalette,
    language: Language,
    project_type: Option<ProjectType>,
    readiness: AgentReadiness,
    status: AgentStatus,
    sidebar_open: bool,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    message_history_offset: usize,
    message_page_size: usize,
    selected_model: String,
    model_labels: Vec<String>,
    active_session_title: String,
    session_count: usize,
    active_session: Option<usize>,
    message_count: usize,
    last_message_fingerprint: u64,
    pending_calls_fingerprint: u64,
}

impl Default for AgentSurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_agent_workbench"),
            sidebar_bridge: RafUiSurfaceBridge::new("raf_ui_agent_sidebar"),
            load_messages_overlay: RafUiSurfaceBridge::new("raf_ui_agent_load_messages"),
            sidebar_surface: None,
            sidebar_surface_revision: 0,
            load_messages_motion: UiTween::new(0.0, UiMotionSpec::tooltip()),
            sidebar_motion: UiTween::new(0.0, UiMotionSpec::layout()),
            sidebar_motion_initialized: false,
            last_sidebar_time_seconds: 0.0,
            last_overlay_time_seconds: 0.0,
            surface_cache: None,
            surface_revision: 0,
            model_menu_open: false,
            mode_menu_open: false,
            add_model_open: false,
            message_history_offset: 0,
            diag_frames: 0,
            diag_cache_misses: 0,
            diag_show_ms: 0.0,
            diag_frame_delta_ms: 0.0,
            diag_last_log_seconds: 0.0,
            diag_prev_frame_seconds: 0.0,
        }
    }
}

impl AgentSurfaceHost {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: raf_ui::StudioUiPalette,
        agent: &AgentPanel,
        settings: &EngineSettings,
        project: Option<&Project>,
        readiness: AgentReadiness,
    ) -> Vec<AgentAction> {
        let now_seconds = ui.ctx().input(|input| input.time);
        let sidebar_delta_seconds = (now_seconds - self.last_sidebar_time_seconds)
            .max(0.0)
            .min(0.25) as f32;
        self.last_sidebar_time_seconds = now_seconds;
        let sidebar_target = f32::from(agent.sidebar_open);
        if !self.sidebar_motion_initialized {
            self.sidebar_motion.set_immediate(sidebar_target);
            self.sidebar_motion_initialized = true;
        } else {
            self.sidebar_motion.set_target(sidebar_target);
        }
        let sidebar_progress = self.sidebar_motion.advance(sidebar_delta_seconds, false);
        let surface_key = AgentSurfaceKey::new(
            palette,
            agent,
            settings,
            project,
            readiness,
            self.model_menu_open,
            self.mode_menu_open,
            self.add_model_open,
            self.message_history_offset,
        );
        let page_or_session_changed = self.surface_cache.as_ref().is_some_and(|(cached_key, _)| {
            cached_key.active_session != surface_key.active_session
                || cached_key.active_session_title != surface_key.active_session_title
                || cached_key.message_history_offset != surface_key.message_history_offset
                || cached_key.message_page_size != surface_key.message_page_size
        });
        let sidebar_changed = self
            .surface_cache
            .as_ref()
            .is_some_and(|(cached_key, _)| cached_key.sidebar_open != surface_key.sidebar_open);
        if page_or_session_changed {
            self.bridge
                .reset_surface_interaction(Some("agent.messages"));
            self.load_messages_overlay.reset_surface_interaction(None);
            self.sidebar_bridge.reset_surface_interaction(None);
        } else if sidebar_changed {
            self.sidebar_bridge.reset_surface_interaction(None);
        }
        let cache_miss = self
            .surface_cache
            .as_ref()
            .is_none_or(|(cached_key, _)| cached_key != &surface_key);
        if cache_miss {
            self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
            self.sidebar_surface_revision = self.sidebar_surface_revision.wrapping_add(1).max(1);
            self.surface_cache = Some((
                surface_key,
                build_agent_surface_for_host(
                    palette,
                    agent,
                    settings,
                    project,
                    readiness,
                    self.model_menu_open,
                    self.mode_menu_open,
                    self.add_model_open,
                    self.message_history_offset,
                ),
            ));
            self.sidebar_surface = Some(build_sidebar_surface(palette, agent));
        }
        let surface = &self
            .surface_cache
            .as_ref()
            .expect("Agent surface cache must be initialized")
            .1;
        let input = agent.input_text.clone();
        let new_model_label = agent.new_model_label.clone();
        let new_model_id = agent.new_model_id.clone();
        let diag_start = std::time::Instant::now();
        let mut actions = self.bridge.show_with_control_state_ref_revision(
            ui,
            render_state,
            palette,
            surface,
            self.surface_revision,
            |controls| {
                sync_text_control(controls, "agent.input", &input, 4_096);
                sync_text_control(controls, "agent.model-label", &new_model_label, 120);
                sync_text_control(controls, "agent.model-id", &new_model_id, 160);
            },
            |key| t(key, settings.language),
        );
        self.diag_show_ms += diag_start.elapsed().as_secs_f64() * 1000.0;

        if sidebar_progress > 0.001 {
            let parent_rect = ui.max_rect();
            let sidebar_size = egui::vec2(SIDEBAR_WIDTH, parent_rect.height());
            let sidebar_rect = egui::Rect::from_min_size(
                egui::pos2(
                    parent_rect.left() - SIDEBAR_WIDTH * (1.0 - sidebar_progress),
                    parent_rect.top(),
                ),
                sidebar_size,
            );
            let sidebar_surface = self
                .sidebar_surface
                .as_ref()
                .expect("Agent sidebar surface must be initialized");
            let sidebar_actions = egui::Area::new(egui::Id::new("rafui.agent.sidebar"))
                .order(egui::Order::Foreground)
                .fixed_pos(sidebar_rect.min)
                .show(ui.ctx(), |sidebar_ui| {
                    sidebar_ui
                        .allocate_ui_with_layout(
                            sidebar_rect.size(),
                            egui::Layout::top_down(egui::Align::Min),
                            |sidebar_ui| {
                                self.sidebar_bridge.show_transparent_ref_revision(
                                    sidebar_ui,
                                    render_state,
                                    palette,
                                    sidebar_surface,
                                    self.sidebar_surface_revision,
                                    |key| t(key, settings.language),
                                )
                            },
                        )
                        .inner
                })
                .inner;
            actions.extend(sidebar_actions);
        }
        if !self.sidebar_motion.is_settled() {
            ui.ctx().request_repaint();
        }

        let scroll_offset = self
            .bridge
            .with_control_state_read(|controls| controls.scroll_offset("agent.messages"))
            .unwrap_or([0.0, 0.0]);
        let message_page_size = agent_page_size(settings);
        let has_older_messages =
            has_older_message_page(agent, self.message_history_offset, message_page_size);
        let show_load_messages =
            (has_older_messages || self.message_history_offset > 0) && scroll_offset[1] <= 2.0;
        let delta_seconds = (now_seconds - self.last_overlay_time_seconds)
            .max(0.0)
            .min(0.25) as f32;
        self.last_overlay_time_seconds = now_seconds;
        self.load_messages_motion
            .set_target(if show_load_messages { 1.0 } else { 0.0 });
        let overlay_progress = self.load_messages_motion.advance(delta_seconds, false);
        if overlay_progress > 0.001 {
            let parent_rect = ui.max_rect();
            let overlay_width = if self.message_history_offset > 0 && has_older_messages {
                370.0
            } else {
                240.0
            };
            let overlay_size = egui::vec2(overlay_width, 34.0);
            let overlay_rect = egui::Rect::from_min_size(
                egui::pos2(
                    parent_rect.center().x - overlay_size.x * 0.5,
                    parent_rect.top() + HEADER_HEIGHT + 10.0 - 18.0 * (1.0 - overlay_progress),
                ),
                overlay_size,
            );
            let overlay_surface = load_messages_overlay_surface(
                palette,
                overlay_progress,
                self.message_history_offset,
                has_older_messages,
            );
            let overlay_actions = egui::Area::new(egui::Id::new("rafui.agent.load-messages"))
                .order(egui::Order::Foreground)
                .fixed_pos(overlay_rect.min)
                .show(ui.ctx(), |overlay_ui| {
                    overlay_ui
                        .allocate_ui_with_layout(
                            overlay_rect.size(),
                            egui::Layout::top_down(egui::Align::Center),
                            |overlay_ui| {
                                self.load_messages_overlay.show_transparent(
                                    overlay_ui,
                                    render_state,
                                    palette,
                                    overlay_surface,
                                    |key| t(key, settings.language),
                                )
                            },
                        )
                        .inner
                })
                .inner;
            actions.extend(overlay_actions);
        }
        if !self.load_messages_motion.is_settled() {
            ui.ctx().request_repaint();
        }
        let mut translated = Vec::new();
        for action in actions {
            match action.action {
                UiAction::SetText { key, value } => match key.as_str() {
                    "agent.input" => translated.push(AgentAction::SetInput(value)),
                    "agent.model-label" => translated.push(AgentAction::SetNewModelLabel(value)),
                    "agent.model-id" => translated.push(AgentAction::SetNewModelId(value)),
                    _ => {}
                },
                UiAction::Command { name } => {
                    if let Some(action) = self.parse_command(
                        &name,
                        project,
                        settings.language,
                        agent.runtime.messages.len(),
                        message_page_size,
                    ) {
                        translated.push(action);
                    }
                }
                _ => {}
            }
        }
        self.diag_frames += 1;
        if cache_miss {
            self.diag_cache_misses += 1;
        }
        if self.diag_prev_frame_seconds > 0.0 {
            self.diag_frame_delta_ms +=
                (now_seconds - self.diag_prev_frame_seconds).max(0.0) * 1000.0;
        }
        self.diag_prev_frame_seconds = now_seconds;
        if now_seconds - self.diag_last_log_seconds >= 1.0 {
            let key = self.surface_cache.as_ref().map(|(key, _)| key);
            tracing::info!(
                "DIAG[agent] frames={} misses={} show_ms_avg={:.3} frame_delta_avg_ms={:.2} scroll={:?} overlay={} overlay_settled={} status={:?} readiness={:?} msgs={:?} last_fp={:?} pending_fp={:?} sidebar={:?}",
                self.diag_frames,
                self.diag_cache_misses,
                self.diag_show_ms / self.diag_frames.max(1) as f64,
                self.diag_frame_delta_ms / self.diag_frames.max(1) as f64,
                scroll_offset,
                show_load_messages,
                self.load_messages_motion.is_settled(),
                key.map(|k| &k.status),
                key.map(|k| k.readiness),
                key.map(|k| k.message_count),
                key.map(|k| k.last_message_fingerprint),
                key.map(|k| k.pending_calls_fingerprint),
                key.map(|k| k.sidebar_open),
            );
            self.diag_frames = 0;
            self.diag_cache_misses = 0;
            self.diag_show_ms = 0.0;
            self.diag_frame_delta_ms = 0.0;
            self.diag_last_log_seconds = now_seconds;
        }
        translated
    }

    fn parse_command(
        &mut self,
        name: &str,
        project: Option<&Project>,
        language: Language,
        message_count: usize,
        message_page_size: usize,
    ) -> Option<AgentAction> {
        match name {
            "agent.sidebar.toggle" => Some(AgentAction::ToggleSidebar),
            "agent.sidebar.close" => Some(AgentAction::CloseSidebar),
            "agent.new-chat" => {
                self.message_history_offset = 0;
                Some(AgentAction::NewChat)
            }
            "agent.messages.older" => {
                let max_offset = message_count.saturating_sub(1);
                self.message_history_offset =
                    (self.message_history_offset + message_page_size).min(max_offset);
                None
            }
            "agent.messages.newer" => {
                self.message_history_offset = 0;
                None
            }
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
                Some(AgentAction::AddModel)
            }
            "agent.settings" => Some(AgentAction::OpenSettings),
            "agent.submit" => {
                self.message_history_offset = 0;
                Some(AgentAction::Submit)
            }
            "agent.stop" => Some(AgentAction::Stop),
            "agent.approve" => Some(AgentAction::Approve),
            "agent.deny" => Some(AgentAction::Deny),
            _ => {
                if let Some(index) = name
                    .strip_prefix("agent.session:")
                    .and_then(|v| v.parse().ok())
                {
                    self.message_history_offset = 0;
                    return Some(AgentAction::SelectSession(index));
                }
                if let Some(index) = name
                    .strip_prefix("agent.session.delete:")
                    .and_then(|v| v.parse().ok())
                {
                    self.message_history_offset = 0;
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
                    .and_then(|v| v.parse::<usize>().ok())
                {
                    return suggestion(project, language, index).map(AgentAction::UseSuggestion);
                }
                None
            }
        }
    }
}

impl AgentSurfaceKey {
    fn new(
        palette: raf_ui::StudioUiPalette,
        agent: &AgentPanel,
        settings: &EngineSettings,
        project: Option<&Project>,
        readiness: AgentReadiness,
        model_menu_open: bool,
        mode_menu_open: bool,
        add_model_open: bool,
        message_history_offset: usize,
    ) -> Self {
        let last_message_fingerprint = agent
            .runtime
            .messages
            .iter()
            .rev()
            .find(|message| message.role != MessageRole::System)
            .map(message_fingerprint)
            .unwrap_or_default();
        let pending_calls_fingerprint = pending_calls_fingerprint(&agent.runtime.pending_calls);
        Self {
            palette,
            language: settings.language,
            project_type: project.map(|project| project.project_type),
            readiness,
            status: agent.runtime.status.clone(),
            sidebar_open: agent.sidebar_open,
            model_menu_open,
            mode_menu_open,
            add_model_open,
            message_history_offset,
            message_page_size: agent_page_size(settings),
            selected_model: agent.selected_model.clone(),
            model_labels: agent.model_registry.selector_labels(),
            active_session_title: agent
                .history
                .active_session()
                .map(|session| session.title.clone())
                .unwrap_or_default(),
            session_count: agent.history.sessions.len(),
            active_session: agent.history.active_index,
            message_count: agent.runtime.messages.len(),
            last_message_fingerprint,
            pending_calls_fingerprint,
        }
    }
}

fn message_fingerprint(message: &ChatMessage) -> u64 {
    let mut hasher = DefaultHasher::new();
    message.id.hash(&mut hasher);
    match message.role {
        MessageRole::User => 0_u8,
        MessageRole::Assistant => 1_u8,
        MessageRole::System => 2_u8,
        MessageRole::Tool => 3_u8,
    }
    .hash(&mut hasher);
    // The card below can expose at most 900 characters plus the truncation
    // marker. Keep the retained-surface key proportional to visible UI, not
    // to an arbitrarily large tool result stored in the chat.
    for character in message.content.chars().take(901) {
        character.hash(&mut hasher);
    }
    hasher.finish()
}

fn pending_calls_fingerprint(calls: &[raf_ai::agent_runtime::PendingToolCall]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for call in calls {
        call.id.hash(&mut hasher);
        call.name.hash(&mut hasher);
        call.arguments.to_string().hash(&mut hasher);
        call.argument_error.hash(&mut hasher);
        call.approved.hash(&mut hasher);
        call.result.hash(&mut hasher);
    }
    hasher.finish()
}

fn sync_text_control(
    controls: &mut raf_ui::UiControlState,
    key: &str,
    value: &str,
    max_length: usize,
) {
    if !controls.has_text(key) || controls.text(key) != value {
        controls.set_text(key, value, max_length);
    }
}

#[cfg(test)]
fn build_agent_surface(
    palette: raf_ui::StudioUiPalette,
    agent: &AgentPanel,
    settings: &EngineSettings,
    project: Option<&Project>,
    readiness: AgentReadiness,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    message_history_offset: usize,
) -> UiSurface {
    build_agent_surface_with_sidebar(
        palette,
        agent,
        settings,
        project,
        readiness,
        model_menu_open,
        mode_menu_open,
        add_model_open,
        message_history_offset,
        true,
    )
}

fn build_agent_surface_for_host(
    palette: raf_ui::StudioUiPalette,
    agent: &AgentPanel,
    settings: &EngineSettings,
    project: Option<&Project>,
    readiness: AgentReadiness,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    message_history_offset: usize,
) -> UiSurface {
    build_agent_surface_with_sidebar(
        palette,
        agent,
        settings,
        project,
        readiness,
        model_menu_open,
        mode_menu_open,
        add_model_open,
        message_history_offset,
        false,
    )
}

fn build_agent_surface_with_sidebar(
    palette: raf_ui::StudioUiPalette,
    agent: &AgentPanel,
    settings: &EngineSettings,
    project: Option<&Project>,
    readiness: AgentReadiness,
    model_menu_open: bool,
    mode_menu_open: bool,
    add_model_open: bool,
    message_history_offset: usize,
    render_sidebar: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    // Keep Agent geometry discrete. Animating the sidebar/activity width in
    // the retained document changes every text request's wrapping width and
    // causes a new atlas slot on each animation tick.
    let mut root = UiNode::new("agent.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::Row))
        .with_style(palette.root_style());

    if agent.sidebar_open {
        root = root.with_child(if render_sidebar {
            sidebar(palette, agent, SIDEBAR_WIDTH)
        } else {
            sidebar_spacer(SIDEBAR_WIDTH)
        });
    }

    let mut main = UiNode::new("agent.main", UiNodeKind::Panel)
        .with_class("agent-main")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 7.0,
            padding: UiSpacing::xy(10.0, 8.0),
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
    if mode_menu_open {
        main = main.with_child(mode_menu(palette, settings.agent_mode));
    }
    if add_model_open {
        main = main.with_child(add_model_form(palette));
    }
    if readiness != AgentReadiness::Ready {
        let key = match readiness {
            AgentReadiness::ProviderDisabled => "app.agent_provider_disabled",
            AgentReadiness::ModelMissing => "app.agent_model_missing",
            AgentReadiness::AdapterRequired => "app.agent_provider_adapter_required",
            AgentReadiness::Ready => "",
        };
        main = main.with_child(status_card(
            palette,
            "agent.readiness",
            key,
            tokens.warning,
            Some("agent.settings"),
        ));
    } else if settings.agent_mode == AgentMode::Active {
        main = main.with_child(status_card(
            palette,
            "agent.active-warning",
            "app.agent_active_mode_warning",
            tokens.warning,
            None,
        ));
    }
    if agent.runtime.status.blocks_input() {
        main = main.with_child(activity_strip(palette, agent));
    }
    if agent.runtime.status == AgentStatus::AwaitingApproval
        && !agent.runtime.pending_calls.is_empty()
    {
        main = main.with_child(approval_card(palette, agent));
    }
    main = main
        .with_child(messages(
            palette,
            agent,
            project,
            message_history_offset,
            agent_page_size(settings),
        ))
        .with_child(suggestions(palette, agent, project, settings.language))
        .with_child(composer(palette, agent));
    root = root.with_child(main);

    let mut surface = UiSurface::new("agent.workbench", palette, root);
    surface.style_sheet = agent_style_sheet(palette);
    surface.with_retained_tooltips(false)
}

fn sidebar(palette: raf_ui::StudioUiPalette, agent: &AgentPanel, width: f32) -> UiNode {
    let tokens = palette.tokens();
    let mut list = UiNode::scroll_view("agent.sessions", UiScrollAxis::Vertical)
        .with_class("agent-session-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 3.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    for index in (0..agent.history.sessions.len()).rev() {
        let session = &agent.history.sessions[index];
        let active = agent.history.active_index == Some(index);
        let title = truncate(&session.title, 28);
        let mut row = UiNode::new(format!("agent.session.row.{index}"), UiNodeKind::Panel)
            .with_class(if active {
                "agent-session-active"
            } else {
                "agent-session"
            })
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                gap: 3.0,
                padding: UiSpacing::xy(5.0, 2.0),
                ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
            });
        row = row.with_child(
            UiNode::new(format!("agent.session.select.{index}"), UiNodeKind::Button)
                .with_text_value(title)
                .with_text_style(UiTextStyle::button(if active {
                    tokens.accent
                } else {
                    tokens.text
                }))
                .with_layout(UiLayout {
                    grow: 1.0,
                    padding: UiSpacing::xy(4.0, 2.0),
                    ..UiLayout::default()
                })
                .with_accessibility_label_key("app.agent_sessions")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("agent.session:{index}"),
                )),
        );
        row = row.with_child(icon_command_button(
            format!("agent.session.delete.{index}"),
            UiIconId::Close,
            format!("agent.session.delete:{index}"),
            "agent-icon-button",
            24.0,
            "app.agent_delete",
        ));
        list = list.with_child(row);
    }

    UiNode::new("agent.sidebar", UiNodeKind::Panel)
        .with_class("agent-sidebar")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::same(10.0),
            ..UiLayout::fixed(width, 0.0).with_height_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
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
                    ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("agent.sidebar.title", UiNodeKind::Label)
                        .with_text_key("app.agent_sessions")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                )
                .with_child(icon_command_button(
                    "agent.sidebar.close",
                    UiIconId::Close,
                    "agent.sidebar.close",
                    "agent-icon-button",
                    26.0,
                    "app.agent_title",
                )),
        )
        .with_child(command_button(
            palette,
            "agent.new-chat",
            "app.agent_new_chat",
            "agent.new-chat",
            "agent-primary-button",
            0.0,
        ))
        .with_child(list)
}

fn sidebar_spacer(width: f32) -> UiNode {
    UiNode::new("agent.sidebar.spacer", UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(width, 0.0).with_height_mode(UiSizeMode::Fill))
        .with_style(UiStyle::transparent())
}

fn build_sidebar_surface(palette: raf_ui::StudioUiPalette, agent: &AgentPanel) -> UiSurface {
    let root = UiNode::new("agent.sidebar.overlay.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::Row))
        .with_child(sidebar(palette, agent, SIDEBAR_WIDTH));
    let mut surface = UiSurface::new("agent.sidebar.overlay", palette, root);
    surface.style_sheet = agent_style_sheet(palette);
    surface.with_retained_tooltips(false)
}
fn header(
    palette: raf_ui::StudioUiPalette,
    agent: &AgentPanel,
    settings: &EngineSettings,
    model_menu_open: bool,
    mode_menu_open: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let model_label = if agent.selected_model.is_empty()
        || agent.selected_model == raf_ai::AgentModelRegistry::PROVIDER_DEFAULT
    {
        t("app.agent_provider_default", settings.language)
    } else {
        agent.selected_model.clone()
    };
    let mode_label = match settings.agent_mode {
        AgentMode::Passive => "app.agent_mode_passive",
        AgentMode::Active => "app.agent_mode_active",
    };
    UiNode::new("agent.header", UiNodeKind::Toolbar)
        .with_class("agent-header")
        .with_layout(UiLayout {
            flow: UiFlow::RowWrap,
            align_items: UiAlign::Center,
            gap: 6.0,
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, HEADER_HEIGHT)
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::Fixed)
        })
        .with_child(icon_command_button(
            "agent.sidebar.toggle",
            if agent.sidebar_open {
                UiIconId::ChevronLeft
            } else {
                UiIconId::ChevronRight
            },
            "agent.sidebar.toggle",
            "agent-icon-button",
            28.0,
            "app.agent_title",
        ))
        .with_child(
            UiNode::new("agent.header.title", UiNodeKind::Label)
                .with_text_key("app.agent_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(56.0, 24.0)),
        )
        .with_child(command_button(
            palette,
            "agent.model.menu",
            &model_label,
            "agent.model.menu",
            if model_menu_open {
                "agent-selected-button"
            } else {
                "agent-secondary-button"
            },
            148.0,
        ))
        .with_child(command_button(
            palette,
            "agent.mode.menu",
            mode_label,
            "agent.mode.menu",
            if mode_menu_open {
                "agent-selected-button"
            } else {
                "agent-secondary-button"
            },
            82.0,
        ))
        .with_child(command_button(
            palette,
            "agent.add-model",
            "app.agent_add_model",
            "agent.add-model.toggle",
            "agent-secondary-button",
            110.0,
        ))
        .with_child(
            UiNode::new("agent.header.spacer", UiNodeKind::Panel).with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::default()
            }),
        )
        .with_child(command_button(
            palette,
            "agent.settings",
            "app.agent_settings",
            "agent.settings",
            "agent-secondary-button",
            82.0,
        ))
}

fn model_menu(palette: raf_ui::StudioUiPalette, agent: &AgentPanel) -> UiNode {
    let tokens = palette.tokens();
    let item_count = agent.model_registry.selector_labels().len();
    let menu_height =
        24.0 + item_count as f32 * 30.0 + item_count.saturating_sub(1) as f32 * 4.0 + 26.0;
    let mut panel = popup_panel(palette, "agent.model-menu").with_layout(UiLayout {
        flow: UiFlow::Column,
        gap: 4.0,
        padding: UiSpacing::same(7.0),
        ..UiLayout::absolute(UiRect::new(106.0, 42.0, 276.0, menu_height)).with_z_index(200)
    });
    for label in agent.model_registry.selector_labels() {
        let selected = label == agent.selected_model;
        panel = panel.with_child(command_button(
            palette,
            &format!("agent.model.select.{label}"),
            if label == raf_ai::AgentModelRegistry::PROVIDER_DEFAULT {
                "app.agent_provider_default"
            } else {
                &label
            },
            &format!("agent.model.select:{label}"),
            if selected {
                "agent-menu-selected"
            } else {
                "agent-menu-button"
            },
            260.0,
        ));
    }
    panel.with_child(
        UiNode::new("agent.model-menu.hint", UiNodeKind::Label)
            .with_text_key("app.agent_model_settings_hint")
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fit_content()),
    )
}

fn mode_menu(palette: raf_ui::StudioUiPalette, selected: AgentMode) -> UiNode {
    let mut panel = popup_panel(palette, "agent.mode-menu");
    for (id, label, mode) in [
        ("passive", "app.agent_mode_passive", AgentMode::Passive),
        ("active", "app.agent_mode_active", AgentMode::Active),
    ] {
        panel = panel.with_child(command_button(
            palette,
            &format!("agent.mode.select.{id}"),
            label,
            &format!("agent.mode.select:{id}"),
            if mode == selected {
                "agent-menu-selected"
            } else {
                "agent-menu-button"
            },
            180.0,
        ));
    }
    panel
}

fn add_model_form(palette: raf_ui::StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    popup_panel(palette, "agent.add-model-form")
        .with_child(field_input(
            palette,
            "agent.model-label",
            "app.agent_model_label",
            132.0,
        ))
        .with_child(field_input(
            palette,
            "agent.model-id",
            "app.agent_model_id",
            170.0,
        ))
        .with_child(command_button(
            palette,
            "agent.add-model.confirm",
            "app.agent_add_model_confirm",
            "agent.add-model.confirm",
            "agent-primary-button",
            92.0,
        ))
        .with_child(
            UiNode::new("agent.add-model-hint", UiNodeKind::Label)
                .with_text_key("app.agent_shortcut_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn activity_strip(palette: raf_ui::StudioUiPalette, agent: &AgentPanel) -> UiNode {
    let tokens = palette.tokens();
    let (key, detail) = match agent.runtime.status {
        AgentStatus::Thinking => ("app.agent_thinking", "app.agent_activity_waiting"),
        AgentStatus::ExecutingTools => ("app.agent_executing_tools", "app.agent_activity_applying"),
        AgentStatus::AwaitingApproval => {
            ("app.agent_awaiting_approval", "app.agent_activity_review")
        }
        _ => ("app.agent_title", ""),
    };
    UiNode::new("agent.activity", UiNodeKind::Panel)
        .with_class("agent-activity")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(9.0, 4.0),
            ..UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.accent,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new("agent.activity.edge", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(3.0, 18.0))
                .with_style(UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 0.0,
                    opacity: 1.0,
                }),
        )
        .with_child(
            UiNode::new("agent.activity.label", UiNodeKind::Label)
                .with_text_key(key)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("agent.activity.detail", UiNodeKind::Label)
                .with_text_key(detail)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn status_card(
    palette: raf_ui::StudioUiPalette,
    id: &str,
    text_key: &str,
    color: [u8; 4],
    action: Option<&str>,
) -> UiNode {
    let tokens = palette.tokens();
    let mut node = UiNode::new(id, UiNodeKind::Panel)
        .with_class("agent-status-card")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(9.0, 4.0),
            ..UiLayout::fixed(0.0, 42.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: color,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new(format!("{id}.message"), UiNodeKind::Label)
                .with_text_key(text_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        );
    if let Some(action) = action {
        node = node.with_child(command_button(
            palette,
            &format!("{id}.action"),
            "app.agent_settings",
            action,
            "agent-secondary-button",
            82.0,
        ));
    }
    node
}

fn approval_card(palette: raf_ui::StudioUiPalette, agent: &AgentPanel) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::new("agent.approval", UiNodeKind::Panel)
        .with_class("agent-approval")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::same(9.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.warning,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::new("agent.approval.title", UiNodeKind::Label)
                .with_text_key("app.agent_pending_tools")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        );
    for (index, call) in agent.runtime.pending_calls.iter().enumerate() {
        panel = panel.with_child(
            UiNode::new(format!("agent.approval.tool.{index}"), UiNodeKind::Label)
                .with_text_value(format!(
                    "- {}  {}",
                    call.name,
                    truncate(&call.arguments.to_string(), 120)
                ))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    panel.with_child(
        UiNode::new("agent.approval.actions", UiNodeKind::Toolbar)
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                gap: 6.0,
                ..UiLayout::fit_content()
            })
            .with_child(command_button(
                palette,
                "agent.approve",
                "app.agent_approve",
                "agent.approve",
                "agent-primary-button",
                94.0,
            ))
            .with_child(command_button(
                palette,
                "agent.deny",
                "app.agent_deny",
                "agent.deny",
                "agent-danger-button",
                82.0,
            )),
    )
}

fn messages(
    palette: raf_ui::StudioUiPalette,
    agent: &AgentPanel,
    project: Option<&Project>,
    message_history_offset: usize,
    message_page_size: usize,
) -> UiNode {
    let tokens = palette.tokens();
    let mut list = UiNode::scroll_view("agent.messages", UiScrollAxis::Vertical)
        .with_class("agent-message-list")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 7.0,
            padding: UiSpacing::xy(4.0, 2.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        });
    let visible_messages = agent
        .runtime
        .messages
        .iter()
        .filter(|message| message.role != MessageRole::System)
        .collect::<Vec<_>>();
    if visible_messages.is_empty() {
        let subtitle_key =
            if project.is_some_and(|project| project.project_type == ProjectType::Electronics) {
                "app.agent_empty_subtitle_electronics"
            } else {
                "app.agent_empty_subtitle_game"
            };
        list = list.with_child(
            UiNode::new("agent.empty", UiNodeKind::Panel)
                .with_class("agent-empty")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 5.0,
                    padding: UiSpacing::same(18.0),
                    ..UiLayout::fixed(0.0, 78.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("agent.empty.title", UiNodeKind::Label)
                        .with_text_key("app.agent_empty_title")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 22.0).with_width_mode(UiSizeMode::Fill)),
                )
                .with_child(
                    UiNode::new("agent.empty.subtitle", UiNodeKind::Label)
                        .with_text_key(subtitle_key)
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)),
                ),
        );
    } else {
        let clamped_offset = message_history_offset.min(visible_messages.len().saturating_sub(1));
        let end = visible_messages.len().saturating_sub(clamped_offset);
        let start = end.saturating_sub(message_page_size);

        for (index, message) in visible_messages[start..end].iter().enumerate() {
            list = list.with_child(message_card(palette, start + index, message));
        }
    }
    list
}

fn has_older_message_page(
    agent: &AgentPanel,
    message_history_offset: usize,
    message_page_size: usize,
) -> bool {
    let message_count = agent
        .runtime
        .messages
        .iter()
        .filter(|message| message.role != MessageRole::System)
        .count();
    let clamped_offset = message_history_offset.min(message_count.saturating_sub(1));
    let end = message_count.saturating_sub(clamped_offset);
    end.saturating_sub(message_page_size) > 0
}

fn agent_page_size(settings: &EngineSettings) -> usize {
    settings
        .agent_message_page_size
        .clamp(AGENT_MESSAGE_PAGE_SIZE_MIN, AGENT_MESSAGE_PAGE_SIZE_MAX) as usize
}

fn load_messages_overlay_surface(
    palette: raf_ui::StudioUiPalette,
    opacity: f32,
    message_history_offset: usize,
    has_older_messages: bool,
) -> UiSurface {
    let mut root =
        UiNode::new("agent.load-messages.overlay.root", UiNodeKind::Root).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: raf_ui::UiJustify::Center,
            gap: 6.0,
            ..UiLayout::fill(UiFlow::Row)
        });
    if message_history_offset > 0 {
        root = root.with_child(load_messages_overlay_button(
            "agent.load-messages.latest",
            "app.agent_back_to_latest",
            "agent.messages.newer",
            160.0,
            [42, 48, 58, 255],
            [112, 124, 142, 255],
            opacity,
        ));
    }
    if message_history_offset == 0 || has_older_messages {
        root = root.with_child(load_messages_overlay_button(
            "agent.load-messages.older",
            "app.agent_load_older",
            "agent.messages.older",
            if message_history_offset > 0 {
                198.0
            } else {
                240.0
            },
            [235, 133, 28, 255],
            [255, 188, 90, 255],
            opacity,
        ));
    }
    UiSurface::new("agent.load-messages.overlay", palette, root).with_retained_tooltips(false)
}

fn load_messages_overlay_button(
    id: &str,
    label: &str,
    command: &str,
    width: f32,
    fill: [u8; 4],
    border: [u8; 4],
    opacity: f32,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_text_key(label)
        .with_text_style(UiTextStyle::button([18, 18, 20, 255]))
        .with_style(UiStyle {
            fill,
            border,
            text: [18, 18, 20, 255],
            border_width: 1.0,
            radius: 7.0,
            opacity: opacity.clamp(0.0, 1.0),
        })
        .with_accessibility_label_key(label)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn message_card(palette: raf_ui::StudioUiPalette, index: usize, message: &ChatMessage) -> UiNode {
    let tokens = palette.tokens();
    let (class, label, color) = match message.role {
        MessageRole::User => ("agent-message-user", "app.agent_you", tokens.text),
        MessageRole::Assistant => ("agent-message-assistant", "app.agent_label", tokens.text),
        MessageRole::Tool => (
            "agent-message-tool",
            "app.agent_tool_result",
            tokens.text_muted,
        ),
        MessageRole::System => (
            "agent-message-system",
            "app.agent_system_label",
            tokens.text_muted,
        ),
    };
    UiNode::new(format!("agent.message.{index}"), UiNodeKind::Panel)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::same(9.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("agent.message.{index}.label"), UiNodeKind::Label)
                .with_text_key(label)
                .with_text_style(UiTextStyle::panel_title(color))
                .with_layout(
                    UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .with_height_mode(UiSizeMode::FitContent),
                ),
        )
        .with_child(
            UiNode::new(format!("agent.message.{index}.content"), UiNodeKind::Label)
                .with_text_value(truncate(&message.content, 900))
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(
                    UiLayout::fit_content()
                        .with_width_mode(UiSizeMode::Fill)
                        .with_height_mode(UiSizeMode::FitContent),
                ),
        )
}

fn suggestions(
    palette: raf_ui::StudioUiPalette,
    agent: &AgentPanel,
    project: Option<&Project>,
    language: Language,
) -> UiNode {
    if !agent.runtime.messages.is_empty() {
        return UiNode::new("agent.suggestions.hidden", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(0.0, 0.0));
    }
    let tokens = palette.tokens();
    let mut row = UiNode::new("agent.suggestions", UiNodeKind::Toolbar).with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 5.0,
        overflow: UiOverflow::Clip,
        ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
    });
    row = row.with_child(
        UiNode::new("agent.suggestions.title", UiNodeKind::Label)
            .with_text_key("app.agent_suggestions_title")
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fit_content()),
    );
    let count = if project.is_some_and(|project| project.project_type == ProjectType::Electronics) {
        3
    } else {
        3
    };
    for index in 0..count {
        let Some(text) = suggestion(project, language, index) else {
            continue;
        };
        row = row.with_child(command_button(
            palette,
            &format!("agent.suggestion.{index}"),
            &truncate(&text, 24),
            &format!("agent.suggestion:{index}"),
            "agent-suggestion-button",
            120.0,
        ));
    }
    row
}

fn composer(palette: raf_ui::StudioUiPalette, agent: &AgentPanel) -> UiNode {
    let tokens = palette.tokens();
    let busy = agent.runtime.status.blocks_input();
    UiNode::new("agent.composer", UiNodeKind::Toolbar)
        .with_class("agent-composer")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(2.0, 2.0),
            ..UiLayout::fixed(0.0, 40.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::text_input(
                "agent.input.control",
                UiTextInput {
                    value_key: "agent.input".to_string(),
                    placeholder_key: Some("app.agent_input_placeholder".to_string()),
                    max_length: 4_096,
                    multiline: false,
                    password: false,
                    submit_command: Some("agent.submit".to_string()),
                },
            )
            .with_class("agent-input")
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [80.0, 34.0],
                ..UiLayout::fixed(0.0, 34.0)
            })
            .disabled(busy),
        )
        .with_child(command_button(
            palette,
            "agent.submit-or-stop",
            if busy {
                "app.agent_cancel"
            } else {
                "app.agent_send"
            },
            if busy { "agent.stop" } else { "agent.submit" },
            if busy {
                "agent-danger-button"
            } else {
                "agent-primary-button"
            },
            86.0,
        ))
        .with_child(
            UiNode::new("agent.composer.hint", UiNodeKind::Label)
                .with_text_key(if busy {
                    "app.agent_working"
                } else {
                    "app.agent_enter_to_send"
                })
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn popup_panel(palette: raf_ui::StudioUiPalette, id: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("agent-popup")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::same(7.0),
            ..UiLayout::fit_content()
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        })
}

fn field_input(palette: raf_ui::StudioUiPalette, id: &str, label_key: &str, width: f32) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(format!("{id}.field"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(width + 112.0, 30.0)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(100.0, 20.0)),
        )
        .with_child(
            UiNode::text_input(
                format!("{id}.input"),
                UiTextInput {
                    value_key: id.to_string(),
                    placeholder_key: Some("settings.surface.input".to_string()),
                    max_length: 200,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("agent-input")
            .with_layout(UiLayout::fixed(width, 28.0)),
        )
}

fn command_button(
    palette: raf_ui::StudioUiPalette,
    id: &str,
    label: &str,
    command: &str,
    class: &str,
    width: f32,
) -> UiNode {
    let mut layout = UiLayout::fixed(width, 30.0);
    if width <= 0.0 {
        layout = UiLayout::fit_content();
    }
    let label_is_key = label.starts_with("app.") || label.starts_with("settings.");
    let mut node = UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(layout)
        .with_text_style(UiTextStyle::button(palette.tokens().text))
        .with_accessibility_label_key(if label_is_key {
            label
        } else {
            "app.agent_title"
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command));
    node = if label_is_key {
        node.with_text_key(label)
    } else {
        node.with_text_value(label)
    };
    node
}

fn icon_command_button(
    id: impl Into<String>,
    icon: UiIconId,
    command: impl Into<String>,
    class: &str,
    width: f32,
    accessibility_key: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout::fixed(width, 30.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_accessibility_label_key(accessibility_key)
        .with_tooltip_key(accessibility_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn suggestion(project: Option<&Project>, language: Language, index: usize) -> Option<String> {
    let key = if project.is_some_and(|project| project.project_type == ProjectType::Electronics) {
        [
            "app.agent_hint_resistor",
            "app.agent_hint_electrical_test",
            "app.agent_hint_nets",
        ]
    } else {
        [
            "app.agent_hint_cube",
            "app.agent_hint_light",
            "app.agent_hint_explain_error",
        ]
    };
    key.get(index).map(|key| t(key, language))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut output = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        output.push('…');
    }
    output
}

fn agent_style_sheet(palette: raf_ui::StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            class_rule("agent-session", tokens.surface, tokens.border, tokens.text),
            class_rule(
                "agent-session-active",
                tokens.surface_raised,
                tokens.accent,
                tokens.text,
            ),
            class_rule(
                "agent-message-user",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "agent-message-assistant",
                tokens.canvas,
                tokens.accent,
                tokens.text,
            ),
            class_rule(
                "agent-message-tool",
                tokens.surface,
                tokens.border,
                tokens.text_muted,
            ),
            class_rule("agent-empty", tokens.surface, tokens.border, tokens.text),
            class_rule(
                "agent-suggestion-button",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
            ),
            class_rule(
                "agent-secondary-button",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "agent-icon-button",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
            ),
            class_rule(
                "agent-menu-button",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "agent-menu-selected",
                tokens.surface_raised,
                tokens.accent,
                tokens.text,
            ),
            filled_rule(
                "agent-primary-button",
                tokens.accent,
                tokens.accent,
                [18, 18, 20, 255],
            ),
            filled_rule(
                "agent-danger-button",
                tokens.danger,
                tokens.danger,
                [255, 255, 255, 255],
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-secondary-button".to_string()),
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
                UiStyleSelector::Class("agent-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("agent-input".to_string()),
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

fn class_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(1.0),
            radius: Some(3.0),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}

fn filled_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    class_rule(class, fill, border, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_render::api_graphic_basic::ui_surface::{
        StudioUiPalette, UiSurfaceDrawList, UiSurfaceSession,
    };

    #[test]
    fn long_runtime_values_are_truncated_for_dense_cards() {
        assert_eq!(truncate("abcdef", 4), "abcd…");
        assert_eq!(truncate("abc", 4), "abc");
    }

    #[test]
    fn message_fingerprint_is_bounded_to_visible_card_content() {
        let message = ChatMessage::assistant(&"a".repeat(10_000));
        let fingerprint = message_fingerprint(&message);

        let mut visible_change = message.clone();
        visible_change.content.replace_range(20..21, "b");
        assert_ne!(fingerprint, message_fingerprint(&visible_change));

        let mut hidden_change = message.clone();
        hidden_change.content.replace_range(5_000..5_001, "b");
        assert_eq!(fingerprint, message_fingerprint(&hidden_change));
    }

    #[test]
    fn agent_input_control_resynchronizes_after_submit_clears_the_model() {
        let mut controls = raf_ui::UiControlState::default();
        sync_text_control(&mut controls, "agent.input", "sent prompt", 4_096);
        sync_text_control(&mut controls, "agent.input", "", 4_096);

        assert_eq!(controls.text("agent.input"), "");
    }

    #[test]
    fn model_menu_is_an_elevated_overlay() {
        let palette = raf_ui::StudioUiPalette::IndustrialDark;
        let agent = AgentPanel::default();
        let menu = model_menu(palette, &agent);

        assert_eq!(menu.kind, UiNodeKind::Panel);
        assert_eq!(menu.layout.position_mode, raf_ui::UiPositionMode::Absolute);
        assert_eq!(menu.layout.z_index, 200);
    }

    #[test]
    fn animated_sidebar_host_surface_reserves_a_stable_layout_slot() {
        let palette = StudioUiPalette::IndustrialDark;
        let agent = AgentPanel::default();
        let settings = EngineSettings::default();
        let workbench = build_agent_surface_for_host(
            palette,
            &agent,
            &settings,
            None,
            AgentReadiness::ProviderDisabled,
            false,
            false,
            false,
            0,
        );
        let overlay = build_sidebar_surface(palette, &agent);

        assert_eq!(workbench.root.children[0].id, "agent.sidebar.spacer");
        assert_eq!(workbench.root.children[1].id, "agent.main");
        assert_eq!(overlay.root.children[0].id, "agent.sidebar");
        assert_eq!(overlay.root.children[0].layout.basis[0], SIDEBAR_WIDTH);
    }

    #[test]
    fn agent_surface_keeps_all_declared_text_in_the_draw_list() {
        let palette = StudioUiPalette::IndustrialDark;
        let agent = AgentPanel::default();
        let settings = EngineSettings::default();
        let surface = build_agent_surface(
            palette,
            &agent,
            &settings,
            None,
            AgentReadiness::ProviderDisabled,
            false,
            false,
            false,
            0,
        );
        let mut session = UiSurfaceSession::default();
        let frame = session.build_layout_frame_at_scale(&surface, 1200, 420, [0; 4], 1.0);
        let resolved = session.resolve_text_requests(&frame, |key| t(key, settings.language));
        session.sync_resolved_text(&frame, &resolved);
        let draw_list = UiSurfaceDrawList::build_with_resolved_text_values_at_scale(
            &frame,
            &session.text_atlas,
            1.0,
            &resolved,
        );

        assert!(!resolved.is_empty());
        assert_eq!(resolved.len(), draw_list.text.len());
    }

    #[test]
    fn long_agent_history_is_limited_to_one_message_page() {
        let palette = StudioUiPalette::IndustrialDark;
        let mut agent = AgentPanel::default();
        agent.runtime.messages = (0..35)
            .map(|index| ChatMessage::assistant(&format!("Agent response {index}")))
            .collect();
        let settings = EngineSettings::default();
        let surface = build_agent_surface(
            palette,
            &agent,
            &settings,
            None,
            AgentReadiness::Ready,
            false,
            false,
            false,
            0,
        );
        let mut session = UiSurfaceSession::default();
        let frame = session.build_layout_frame_at_scale(&surface, 1200, 900, [0; 4], 1.0);
        let message_content_count = frame
            .text_requests
            .iter()
            .filter(|request| {
                request.node_id.starts_with("agent.message.")
                    && request.node_id.ends_with(".content")
            })
            .count();
        let resolved = session.resolve_text_requests(&frame, |key| t(key, settings.language));
        session.sync_resolved_text(&frame, &resolved);
        let draw_list = UiSurfaceDrawList::build_with_resolved_text_values_at_scale(
            &frame,
            &session.text_atlas,
            1.0,
            &resolved,
        );

        assert_eq!(message_content_count, agent_page_size(&settings));
        assert_eq!(resolved.len(), draw_list.text.len());
    }

    #[test]
    fn load_messages_overlay_is_orange_translucent_and_actionable() {
        let surface = load_messages_overlay_surface(StudioUiPalette::IndustrialDark, 0.62, 0, true);
        let button = &surface.root.children[0];

        assert_eq!(button.style.fill, [235, 133, 28, 255]);
        assert_eq!(button.style.opacity, 0.62);
        assert!(button.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::Click
                && binding.action
                    == UiAction::Command {
                        name: "agent.messages.older".to_string(),
                    }
        }));
    }

    #[test]
    #[ignore]
    fn temp_bench_agent_render_pipeline() {
        let palette = StudioUiPalette::IndustrialDark;
        let mut agent = AgentPanel::default();
        let long_content = (0..35)
            .map(|index| {
                format!(
                    "Agent response {index}: This is a long message with enough words to wrap across several lines inside the conversation column, including some code-like tokens like game_add(), scene.set_position(), wire_net(VCC, GND) and a trailing explanation of what happened and what the user should do next in the editor."
                )
            })
            .collect::<Vec<_>>();
        for content in &long_content {
            agent.runtime.messages.push(ChatMessage::assistant(content));
        }
        let settings = EngineSettings::default();
        let surface = build_agent_surface(
            palette,
            &agent,
            &settings,
            None,
            AgentReadiness::Ready,
            false,
            false,
            false,
            0,
        );
        let mut session = UiSurfaceSession::default();
        let mark = |name: &str, started: std::time::Instant| {
            println!(
                "[bench] {name}: {:.2} ms",
                started.elapsed().as_secs_f64() * 1000.0
            );
        };

        let timer = std::time::Instant::now();
        let frame = session.build_layout_frame_at_scale(&surface, 1280, 400, [0; 4], 1.25);
        mark("layout initial", timer);
        println!(
            "[bench] nodes={} text_requests={}",
            frame.layout_boxes.len(),
            frame.text_requests.len()
        );

        let timer = std::time::Instant::now();
        let resolved = session.resolve_text_requests(&frame, |key| t(key, settings.language));
        mark("resolve text", timer);

        let timer = std::time::Instant::now();
        let stats = session.text_atlas.sync_resolved(
            frame
                .text_requests
                .iter()
                .zip(resolved.iter().map(|v| v.as_str())),
        );
        mark("atlas sync (first, rasterize)", timer);
        println!(
            "[bench] atlas sync stats: allocated={} reused={} overflowed={} evicted={} size={:?} slots={}",
            stats.allocated,
            stats.reused,
            stats.overflowed,
            stats.evicted,
            session.text_atlas.size(),
            session.text_atlas.slot_count()
        );

        let timer = std::time::Instant::now();
        let stats = session.text_atlas.sync_resolved(
            frame
                .text_requests
                .iter()
                .zip(resolved.iter().map(|v| v.as_str())),
        );
        mark("atlas sync (second, reuse)", timer);
        println!(
            "[bench] second sync allocated={} reused={}",
            stats.allocated, stats.reused
        );

        let timer = std::time::Instant::now();
        let _draw_list = UiSurfaceDrawList::build_with_resolved_text_values_at_scale(
            &frame,
            &session.text_atlas,
            1.25,
            &resolved,
        );
        mark("draw list build", timer);

        let timer = std::time::Instant::now();
        session
            .interaction
            .controls
            .scroll_by("agent.messages", [0.0, 279.0]);
        let frame2 = session.build_layout_frame_at_scale(&surface, 1280, 400, [0; 4], 1.25);
        mark("layout rebuild after scroll", timer);

        let timer = std::time::Instant::now();
        let resolved2 = session.resolve_text_requests(&frame2, |key| t(key, settings.language));
        let stats = session.text_atlas.sync_resolved(
            frame2
                .text_requests
                .iter()
                .zip(resolved2.iter().map(|v| v.as_str())),
        );
        mark("atlas sync after scroll", timer);
        println!(
            "[bench] post-scroll sync allocated={} reused={} resolved_same={}",
            stats.allocated,
            stats.reused,
            resolved == resolved2
        );
    }

    #[test]
    fn message_content_wraps_inside_the_card_instead_of_using_intrinsic_width() {
        let palette = StudioUiPalette::IndustrialDark;
        let message = ChatMessage::assistant(
            "This is a long Agent response that must wrap inside the available conversation width instead of being clipped by an intrinsic text column.",
        );
        let surface = UiSurface::new(
            "agent-message-layout",
            palette,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(message_card(palette, 0, &message)),
        );
        let mut session = UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 320, 160, [0; 4], |_| String::new());
        let card = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0")
            .expect("message card layout");
        let content = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.0.content")
            .expect("message content layout");

        assert!(content.rect.width < 320.0);
        assert!(content.rect.width > 200.0);
        assert!(content.rect.bottom() <= card.content_rect.bottom());
        assert!(content.rect.height > 18.0);
    }

    #[test]
    fn agent_transcript_scroll_extent_reaches_the_bottom_of_wrapped_messages() {
        let palette = StudioUiPalette::IndustrialDark;
        let mut agent = AgentPanel::default();
        let response = "This Agent response contains enough wrapped content to force the conversation viewport to scroll before the composer. ".repeat(18);
        agent.runtime.messages = (0..8).map(|_| ChatMessage::assistant(&response)).collect();
        let settings = EngineSettings::default();
        let surface = build_agent_surface(
            palette,
            &agent,
            &settings,
            None,
            AgentReadiness::Ready,
            false,
            false,
            false,
            0,
        );
        let mut session = UiSurfaceSession::default();
        let frame = session.build_frame_with_resolved_text(&surface, 720, 562, [0; 4], |key| {
            t(key, settings.language)
        });
        let metrics = frame
            .scroll_metrics
            .iter()
            .find(|metrics| metrics.id == "agent.messages")
            .expect("Agent transcript scroll metrics");
        assert!(
            metrics.max_offset[1] > 0.0,
            "wrapped Agent messages must expose vertical overflow"
        );

        let max_offset = metrics.max_offset;
        session
            .interaction
            .controls
            .scroll_by("agent.messages", max_offset);
        let bottom_frame =
            session.build_frame_with_resolved_text(&surface, 720, 562, [0; 4], |key| {
                t(key, settings.language)
            });
        let list = bottom_frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.messages")
            .expect("Agent transcript layout");
        let last_card = bottom_frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "agent.message.7")
            .expect("last Agent message layout");
        assert!(
            last_card.rect.bottom() <= list.content_rect.bottom() + 1.0,
            "bottom message must be reachable inside the transcript viewport"
        );
    }
}
