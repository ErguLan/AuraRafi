//! Native input and retained presentation for the application-wide Agent workspace.

use crate::agent_preview::{AgentPreview, PreviewTarget};
use crate::agent_service::AgentService;
use crate::agent_workspace_layout::{
    WorkspaceDivider, WorkspaceGeometry, WorkspaceLayout, WorkspaceResize,
};
use crate::agent_workspace_surface::{
    build_workspace_surface, build_workspace_surface_with_layout, with_confirmation, EvidenceTab,
    WorkspaceConfirmation,
};
use crate::editor_layout::EditorRect;
use crate::panels::ai_chat::AgentAction;
use crate::studio_surface::HubSurfaceProject;
use raf_core::{EngineSettings, InputKey, InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::UiRect;
use std::path::PathBuf;
use std::time::Instant;

pub(crate) enum AgentWorkspaceIntent {
    Back,
    OpenEditor(PathBuf),
    Settings,
    Window(raf_ui::UiWindowCommand),
    StartAllProjects,
    CloseProjects(Option<PathBuf>),
}

pub(crate) struct AgentWorkspaceSurfaceHost {
    host: DirectUiSurfaceHost,
    rect: EditorRect,
    started: Instant,
    last_key: String,
    scroll: f32,
    evidence: EvidenceTab,
    review_open: bool,
    confirmation: Option<WorkspaceConfirmation>,
    layout: WorkspaceLayout,
    resize: WorkspaceResize,
    pub preview: AgentPreview,
    release_canvas: bool,
    frame_requested: bool,
}

impl AgentWorkspaceSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
        service: &AgentService,
        projects: &[HubSurfaceProject],
        settings: &EngineSettings,
    ) -> Self {
        let surface = build_workspace_surface(
            palette,
            service,
            projects,
            settings,
            rect.logical_size().map(|value| value as f32),
            0.0,
            0,
            EvidenceTab::Changes,
            rect.width >= 1200.0,
        );
        Self {
            host: graphics.create_ui_host(surface, [9, 9, 10, 255]),
            rect,
            started: Instant::now(),
            last_key: String::new(),
            scroll: 0.0,
            evidence: EvidenceTab::Changes,
            review_open: rect.width >= 1200.0,
            confirmation: None,
            layout: WorkspaceLayout::default(),
            resize: WorkspaceResize::default(),
            preview: AgentPreview::default(),
            release_canvas: false,
            frame_requested: false,
        }
    }

    fn owner(&self) -> InputOwner {
        InputOwner::RetainedUi(InputRegionId::from_static("native.agent.workspace"))
    }

    pub fn reset(&mut self, router: &mut InputRouter) {
        self.resize.cancel(router);
        self.preview.cancel_input(router);
        router.cancel_owner(self.owner());
        self.host
            .session_mut()
            .reset_interaction_for_surface_change(None);
        self.scroll = 0.0;
        self.last_key.clear();
        self.confirmation = None;
        self.frame_requested = true;
    }

    pub fn geometry(&self, service: &AgentService) -> WorkspaceGeometry {
        let workspace = service.selected_workspace().filter(|w| !w.closing);
        self.layout.resolve(
            [self.rect.width, self.rect.height],
            self.review_open && workspace.is_some(),
            workspace.is_some_and(|w| w.panel.borrow().sidebar_open),
        )
    }

    pub fn sync_preview(&mut self, service: &AgentService, now: Instant) {
        let target = service
            .selected_workspace()
            .filter(|w| !w.closing && w.project.project_type == raf_core::ProjectType::Game)
            .and_then(|w| {
                let document = w
                    .panel
                    .borrow()
                    .document_id()
                    .unwrap_or(w.registry.active_session);
                w.documents
                    .contains_key(&document.0.to_string())
                    .then(|| PreviewTarget {
                        root: w.project.path.clone(),
                        document,
                    })
            });
        self.release_canvas |= self.preview.sync(
            target,
            self.review_open && self.evidence == EvidenceTab::Preview,
            now,
        );
    }
    pub fn close_preview(&mut self) {
        if self.evidence == EvidenceTab::Preview {
            self.evidence = EvidenceTab::Changes;
            self.last_key.clear();
        }
        self.release_canvas |= self.preview.close();
    }
    pub fn suspend_preview_input(&mut self, router: &mut InputRouter) {
        self.preview.cancel_input(router);
        self.resize.cancel(router);
    }
    pub fn preview_rect(&self, service: &AgentService) -> Option<EditorRect> {
        self.preview
            .visible
            .then(|| self.geometry(service).preview_rect())
            .flatten()
            .map(|r| EditorRect::new(self.rect.x + r.x, self.rect.y + r.y, r.width, r.height))
    }
    pub fn take_release_canvas(&mut self) -> bool {
        std::mem::take(&mut self.release_canvas)
    }
    pub fn take_frame_request(&mut self) -> bool {
        std::mem::take(&mut self.frame_requested)
    }

    pub fn sync(
        &mut self,
        rect: EditorRect,
        palette: StudioUiPalette,
        service: &AgentService,
        projects: &[HubSurfaceProject],
        settings: &EngineSettings,
    ) {
        self.rect = rect;
        self.sync_preview(service, Instant::now());
        let loading = service.loading()
            || (service
                .selected
                .as_deref()
                .is_some_and(|p| service.is_closing(p))
                && service.selected_close_error().is_none())
            || service
                .selected_workspace()
                .is_some_and(|w| w.panel.borrow().opening);
        let phase = if loading && !settings.prefers_reduced_motion {
            (self.started.elapsed().as_millis() / 80 % 12) as u8
        } else {
            0
        };
        let panel_key = service.selected_workspace().map(|w| {
            let mut panel = w.panel.borrow_mut();
            panel.spinner_step = phase;
            (
                panel.visual_revision(),
                panel.history.active_index,
                panel.runtime.message_revision(),
                panel.input_text.clone(),
                panel.model_menu_open,
                panel.mode_menu_open,
                panel.add_model_open,
                panel.sidebar_open,
                panel.review_writes,
                panel.opening,
                panel.saved_revision,
                panel.save_error.clone(),
            )
        });
        let key = format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
            rect,
            palette,
            settings.language,
            service.revision,
            service.selected,
            panel_key,
            self.scroll,
            phase,
            self.evidence,
            self.review_open,
            self.confirmation,
            self.layout,
            (
                settings.font_size,
                settings.ui_scale,
                settings.prefers_reduced_motion,
                settings.high_contrast,
                settings.reduce_transparency
            )
        );
        if key == self.last_key {
            return;
        }
        self.host
            .set_environment(crate::agent_workspace_surface_host::environment(
                settings,
                rect.logical_size().map(|value| value as f32),
            ));
        self.host.set_surface(with_confirmation(
            build_workspace_surface_with_layout(
                palette,
                service,
                projects,
                settings,
                rect.logical_size().map(|value| value as f32),
                self.scroll,
                phase,
                self.evidence,
                self.review_open,
                &self.layout,
            ),
            rect.logical_size().map(|v| v as f32),
            self.confirmation.as_ref(),
        ));
        if self.confirmation.is_some()
            && !self
                .host
                .session()
                .interaction
                .focus
                .has_focus("agent.workspace.confirmation.accept")
        {
            self.host
                .session_mut()
                .interaction
                .focus
                .request_focus("agent.workspace.confirmation.cancel");
        }
        if let Some(workspace) = service.selected_workspace() {
            let panel = workspace.panel.borrow();
            let controls = &mut self.host.session_mut().interaction.controls;
            controls.set_text("agent.input", &panel.input_text, 16_384);
            controls.set_text("agent.new-model.label", &panel.new_model_label, 256);
            controls.set_text("agent.new-model.id", &panel.new_model_id, 256);
        }
        self.last_key = key;
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
        service: &mut AgentService,
        settings: &mut EngineSettings,
    ) -> Vec<AgentWorkspaceIntent> {
        let language = settings.language;
        if self.confirmation.is_none() {
            let geometry = self.geometry(service);
            // The splitter and camera acquire their own pointer ownership before
            // RafUI sees the snapshot, preventing click-through into the chat.
            let before = self.layout;
            if self
                .resize
                .input(input.snapshot(), router, geometry, &mut self.layout)
            {
                if self.layout != before {
                    self.last_key.clear();
                    self.frame_requested = true;
                }
                return Vec::new();
            }
            if let Some(rect) = self.preview_rect(service) {
                let snapshot = input.snapshot();
                let hovered = snapshot.pointer_position.is_some_and(|p| {
                    UiRect::new(rect.x, rect.y, rect.width, rect.height).contains(p)
                });
                if hovered && snapshot.button_pressed(raf_core::PointerButton::Secondary) {
                    self.host.session_mut().interaction.focus.clear_focus();
                    router.release_keyboard(self.owner());
                }
                let changed = self.preview.input(
                    snapshot,
                    router,
                    UiRect::new(rect.x, rect.y, rect.width, rect.height),
                );
                self.frame_requested |= changed;
                if self.preview.navigating || (hovered && snapshot.scroll_delta[1] != 0.0) {
                    return Vec::new();
                }
            }
        } else {
            self.suspend_preview_input(router);
        }
        let actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, language),
            input,
            router,
            self.owner(),
            UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );
        let mut intents = Vec::new();
        let modal_was_open = self.confirmation.is_some();
        if modal_was_open && input.snapshot().key_pressed(InputKey::Escape) {
            self.reset(router);
            return intents;
        }
        for action in actions {
            if modal_was_open || self.confirmation.is_some() {
                if let UiAction::Command { name } = &action.action {
                    match name.as_str() {
                        "agent.workspace.confirmation.accept" => {
                            if let Some(confirmation) = self.confirmation.take() {
                                intents.push(match confirmation {
                                    WorkspaceConfirmation::StartAll => {
                                        AgentWorkspaceIntent::StartAllProjects
                                    }
                                    WorkspaceConfirmation::CloseAll => {
                                        AgentWorkspaceIntent::CloseProjects(None)
                                    }
                                    WorkspaceConfirmation::CloseProject(path) => {
                                        AgentWorkspaceIntent::CloseProjects(Some(path))
                                    }
                                });
                                self.reset(router);
                            }
                        }
                        "agent.workspace.confirmation.cancel" => self.reset(router),
                        _ => {}
                    }
                }
                continue;
            }
            match action.action {
                UiAction::Custom { channel, payload }
                    if channel == "agent.workspace.close-project" =>
                {
                    if let Some(path) = payload.get("path").and_then(|v| v.as_str()) {
                        self.reset(router);
                        self.confirmation =
                            Some(WorkspaceConfirmation::CloseProject(PathBuf::from(path)));
                    }
                }
                UiAction::Custom { channel, payload } if channel == "agent.workspace.project" => {
                    if let Some(path) = payload.get("path").and_then(|v| v.as_str()) {
                        service.request(PathBuf::from(path));
                        self.reset(router);
                    }
                }
                UiAction::Command { name } => match name.as_str() {
                    "agent.workspace.tab.preview" => {
                        self.evidence = EvidenceTab::Preview;
                        self.review_open = true;
                        self.layout.review = self.layout.review.max(480.0);
                    }
                    "agent.workspace.preview.close" => self.review_open = false,
                    "agent.workspace.preview.fit" => {
                        self.preview.request_fit();
                        self.frame_requested = true;
                    }
                    name if name.starts_with("agent.workspace.resize.") => {
                        if let Some((part, value)) = name
                            .trim_start_matches("agent.workspace.resize.")
                            .split_once(':')
                        {
                            let divider = match part {
                                "projects" => Some(WorkspaceDivider::Projects),
                                "sessions" => Some(WorkspaceDivider::Sessions),
                                "review" => Some(WorkspaceDivider::Review),
                                _ => None,
                            };
                            if let Some(divider) = divider {
                                if value == "reset" {
                                    let defaults = WorkspaceLayout::default();
                                    match divider {
                                        WorkspaceDivider::Projects => {
                                            self.layout.projects = defaults.projects
                                        }
                                        WorkspaceDivider::Sessions => {
                                            self.layout.sessions = defaults.sessions
                                        }
                                        WorkspaceDivider::Review => {
                                            self.layout.review = defaults.review
                                        }
                                    }
                                } else if let Ok(delta) = value.parse::<f32>() {
                                    self.layout.adjust(divider, delta, self.geometry(service));
                                }
                            }
                        }
                    }
                    "agent.workspace.start-all" | "agent.workspace.close-all" => {
                        self.reset(router);
                        self.confirmation = Some(if name == "agent.workspace.start-all" {
                            WorkspaceConfirmation::StartAll
                        } else {
                            WorkspaceConfirmation::CloseAll
                        });
                    }
                    "window.drag" => intents.push(AgentWorkspaceIntent::Window(
                        raf_ui::UiWindowCommand::BeginDrag,
                    )),
                    "window.minimize" => intents.push(AgentWorkspaceIntent::Window(
                        raf_ui::UiWindowCommand::Minimize,
                    )),
                    "window.maximize" => intents.push(AgentWorkspaceIntent::Window(
                        raf_ui::UiWindowCommand::ToggleMaximize,
                    )),
                    "window.close" => {
                        intents.push(AgentWorkspaceIntent::Window(raf_ui::UiWindowCommand::Close))
                    }
                    "agent.workspace.back" => intents.push(AgentWorkspaceIntent::Back),
                    "agent.workspace.settings" => intents.push(AgentWorkspaceIntent::Settings),
                    "agent.workspace.editor" => {
                        if let Some(path) = service.selected.clone() {
                            intents.push(AgentWorkspaceIntent::OpenEditor(path));
                        }
                    }
                    "agent.workspace.retry" => service.retry_open(),
                    "agent.workspace.review" => self.review_open = !self.review_open,
                    "agent.workspace.tab.changes" => self.evidence = EvidenceTab::Changes,
                    "agent.workspace.tab.artifacts" => self.evidence = EvidenceTab::Artifacts,
                    "agent.workspace.tab.verification" => self.evidence = EvidenceTab::Verification,
                    "agent.workspace.save.retry" => {
                        if let Some(workspace) = service.selected_workspace_mut() {
                            for document in workspace.documents.values() {
                                document.save.borrow_mut().retry();
                            }
                        }
                    }
                    _ => {
                        if let Some(action) =
                            crate::native_workbench::parse_agent_action(&name, language)
                        {
                            if let Some(workspace) = service.selected_workspace() {
                                let reset = matches!(
                                    action,
                                    AgentAction::NewChat | AgentAction::SelectSession(_)
                                );
                                workspace.panel.borrow_mut().apply_action(action, settings);
                                if reset {
                                    self.reset(router);
                                    self.host
                                        .session_mut()
                                        .interaction
                                        .focus
                                        .request_focus("agent.input");
                                }
                                if workspace.panel.borrow().add_model_open {
                                    self.host
                                        .session_mut()
                                        .interaction
                                        .focus
                                        .request_focus("agent.new-model.label");
                                }
                            }
                        }
                    }
                },
                UiAction::SetText { key, value } => {
                    let action = match key.as_str() {
                        "agent.input" => Some(AgentAction::SetInput(value)),
                        "agent.new-model.label" => Some(AgentAction::SetNewModelLabel(value)),
                        "agent.new-model.id" => Some(AgentAction::SetNewModelId(value)),
                        _ => None,
                    };
                    if let (Some(action), Some(workspace)) = (action, service.selected_workspace())
                    {
                        workspace.panel.borrow_mut().apply_action(action, settings);
                    }
                }
                UiAction::SetSelect { key, value, .. } => {
                    let action = match key.as_str() {
                        "agent.model" => Some(AgentAction::SelectModel(value)),
                        "agent.mode" => match value.as_str() {
                            "inspect" => {
                                Some(AgentAction::SetMode(raf_ai::provider::AgentMode::Inspect))
                            }
                            "plan" => Some(AgentAction::SetMode(raf_ai::provider::AgentMode::Plan)),
                            "active" => {
                                Some(AgentAction::SetMode(raf_ai::provider::AgentMode::Active))
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    if let (Some(action), Some(workspace)) = (action, service.selected_workspace())
                    {
                        workspace.panel.borrow_mut().apply_action(action, settings);
                    }
                }
                UiAction::SetSelectOpen { id, open } => {
                    if let Some(workspace) = service.selected_workspace() {
                        let mut panel = workspace.panel.borrow_mut();
                        panel.close_menus();
                        match id.as_str() {
                            "agent.model.trigger" => panel.model_menu_open = open,
                            "agent.mode.trigger" => panel.mode_menu_open = open,
                            _ => {}
                        }
                    }
                }
                UiAction::ScrollTo { id, offset } if id == "agent.history" => {
                    if (offset[1] - self.scroll).abs() >= 48.0 {
                        self.scroll = offset[1].max(0.0);
                    } else {
                        continue;
                    }
                }
                _ => {}
            }
            self.last_key.clear();
            self.frame_requested = true;
        }
        if !modal_was_open && self.confirmation.is_none() {
            if let Some(workspace) = service.selected_workspace().filter(|w| !w.closing) {
                let mut panel = workspace.panel.borrow_mut();
                if input.snapshot().key_pressed(InputKey::Escape) {
                    panel.close_menus();
                    if self.rect.width < 1200.0 {
                        self.review_open = false;
                    }
                    self.last_key.clear();
                }
                if input.snapshot().key_pressed(InputKey::Enter)
                    && !input.snapshot().modifiers.shift
                    && self
                        .host
                        .session()
                        .interaction
                        .focus
                        .has_focus("agent.input")
                {
                    panel.apply_action(AgentAction::Submit, settings);
                    self.last_key.clear();
                }
                if panel.take_open_settings_request() {
                    intents.push(AgentWorkspaceIntent::Settings);
                }
            }
        }
        intents
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.host.has_active_text_repeat()
    }
    pub fn cursor_hint(&self) -> raf_ui::UiCursorIcon {
        if self.resize.active() || self.resize.hovered.is_some() {
            return raf_ui::UiCursorIcon::ResizeHorizontal;
        }
        self.host.cursor_hint()
    }
    pub fn compositor_layer(&mut self, scale: f32, size: [u32; 2]) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale, size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale.max(1.0),
        }
    }
}

fn environment(settings: &EngineSettings, size: [f32; 2]) -> raf_ui::UiEnvironment {
    let mut environment = raf_ui::UiEnvironment::default();
    environment.viewport_size = size;
    environment.font_size = settings.font_size;
    environment.ui_scale = settings.ui_scale;
    environment.prefers_reduced_motion = settings.prefers_reduced_motion;
    environment.high_contrast = settings.high_contrast;
    environment.reduce_transparency = settings.reduce_transparency;
    environment.color_mode = match settings.theme {
        raf_core::Theme::Light => raf_ui::UiColorMode::Light,
        _ => raf_ui::UiColorMode::Dark,
    };
    environment
}
