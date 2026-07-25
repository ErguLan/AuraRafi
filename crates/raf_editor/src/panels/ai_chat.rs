//! Agent panel - conversational AI interface for the editor.
//!
//! Layout: left sidebar (chat sessions) | right area (conversation + input).
//!
//! The runtime is non-blocking: HTTP requests run on a background thread
//! so the editor stays responsive. Call `poll()` each frame.

use std::collections::HashMap;

use egui::Ui;
use raf_ai::agent_history::AgentHistory;
use raf_ai::agent_model_registry::AgentModelRegistry;
use raf_ai::agent_runtime::{AgentEvent, AgentRuntime, AgentStatus};
use raf_ai::chat::{ChatMessage, ChatPanel, MessageRole};
use raf_ai::openai_client::{OpenAiConfig, OpenAiTool};
use raf_ai::provider::{AgentMode, AiProvider, AiProviderConfig};
use raf_core::config::EngineSettings;
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType};
use raf_core::Language;

use crate::agent_executor::{build_tool_name_map, AgentToolExecutor};
use crate::commands::catalog::CommandCatalog;
use crate::theme;

/// State for the Agent panel in the editor.
pub struct AgentPanel {
    pub chat: ChatPanel,
    pub lang: Language,
    pub open_settings_requested: bool,
    pub runtime: AgentRuntime,
    pub history: AgentHistory,
    pub model_registry: AgentModelRegistry,
    pub tools: Vec<OpenAiTool>,
    pub tool_name_map: HashMap<String, String>,
    loaded_project_path: Option<std::path::PathBuf>,
    tools_language: Option<Language>,
    pub(crate) selected_model: String,
    last_runtime_config: Option<OpenAiConfig>,
    pub(crate) new_model_label: String,
    pub(crate) new_model_id: String,
    pub settings_changed: bool,
    pub(crate) sidebar_open: bool,
    pub(crate) pending_delete_session: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentReadiness {
    Ready,
    ProviderDisabled,
    ModelMissing,
    AdapterRequired,
}

impl Default for AgentPanel {
    fn default() -> Self {
        let runtime = AgentRuntime::new(OpenAiConfig::default());
        Self {
            chat: welcome_chat_panel(),
            lang: Language::English,
            open_settings_requested: false,
            runtime,
            history: AgentHistory::default(),
            model_registry: AgentModelRegistry::default(),
            tools: Vec::new(),
            tool_name_map: HashMap::new(),
            loaded_project_path: None,
            tools_language: None,
            selected_model: String::new(),
            last_runtime_config: None,
            new_model_label: String::new(),
            new_model_id: String::new(),
            settings_changed: false,
            sidebar_open: true,
            pending_delete_session: None,
        }
    }
}

impl AgentPanel {
    pub(crate) fn prepare_retained_surface(
        &mut self,
        settings: &mut EngineSettings,
        project: Option<&Project>,
        catalog: &CommandCatalog,
        executor: &mut AgentToolExecutor<'_>,
    ) -> AgentReadiness {
        self.lang = settings.language;
        self.ensure_tools(catalog, self.lang);
        self.ensure_model_registry(settings);
        self.ensure_project_history(project);
        self.ensure_runtime_config(settings);

        let provider_config = settings
            .ai_providers
            .iter()
            .find(|config| config.provider == settings.default_ai_provider)
            .cloned()
            .unwrap_or_default();
        let effective_model_id = self
            .model_registry
            .resolve_model_id(&self.selected_model, provider_config.provider)
            .unwrap_or_else(|| provider_config.model.clone());

        let _ = self.runtime.poll(Some(executor));
        if self.flush_runtime_events() {
            self.save_history();
        }

        agent_readiness(&provider_config, &effective_model_id)
    }

    pub(crate) fn apply_retained_action(
        &mut self,
        action: crate::panels::agent_surface::AgentSurfaceAction,
        settings: &mut EngineSettings,
        project: Option<&Project>,
        executor: &mut AgentToolExecutor<'_>,
    ) -> bool {
        use crate::panels::agent_surface::AgentSurfaceAction;

        match action {
            AgentSurfaceAction::SetInput(value) => self.chat.input_text = value,
            AgentSurfaceAction::SetModelLabel(value) => self.new_model_label = value,
            AgentSurfaceAction::SetModelId(value) => self.new_model_id = value,
            AgentSurfaceAction::ToggleSidebar => self.sidebar_open = !self.sidebar_open,
            AgentSurfaceAction::CloseSidebar => self.sidebar_open = false,
            AgentSurfaceAction::NewChat => self.start_new_chat(project),
            AgentSurfaceAction::SelectSession(index) => self.select_session(index),
            AgentSurfaceAction::DeleteSession(index) => self.delete_session(index),
            AgentSurfaceAction::CancelDelete => self.pending_delete_session = None,
            AgentSurfaceAction::SelectModel(index) => {
                if let Some(label) = self.model_registry.selector_labels().get(index) {
                    self.selected_model = label.clone();
                }
            }
            AgentSurfaceAction::SetMode(mode) => {
                if settings.agent_mode != mode {
                    settings.agent_mode = mode;
                    self.settings_changed = true;
                }
            }
            AgentSurfaceAction::AddModel => {
                if self.model_registry.add(
                    self.new_model_label.clone(),
                    settings.default_ai_provider,
                    self.new_model_id.clone(),
                ) {
                    settings.agent_model_shortcuts = self.model_registry.shortcuts.clone();
                    self.settings_changed = true;
                    self.new_model_label.clear();
                    self.new_model_id.clear();
                }
            }
            AgentSurfaceAction::OpenSettings => self.open_settings_requested = true,
            AgentSurfaceAction::Submit => {
                let input = self.chat.input_text.trim().to_string();
                if !input.is_empty() && !self.runtime.status.blocks_input() {
                    self.chat.input_text.clear();
                    self.submit(input, settings.agent_mode == AgentMode::Active);
                }
            }
            AgentSurfaceAction::Approve => self.approve_all(executor),
            AgentSurfaceAction::Deny => {
                self.deny_all(t("app.agent_denied_by_user", self.lang), executor)
            }
            AgentSurfaceAction::UseSuggestion(value) => self.chat.input_text = value,
        }

        self.open_settings_requested
    }

    fn select_session(&mut self, index: usize) {
        if let Some(session) = self.history.sessions.get(index) {
            self.runtime.clear();
            self.runtime.messages = session.messages.clone();
            self.runtime
                .set_system_prompt(build_agent_prompt(&self.tools));
            self.history.active_index = Some(index);
            self.chat = welcome_chat_panel();
            for message in &self.runtime.messages {
                self.chat.messages.push(message.clone());
            }
        }
    }

    fn delete_session(&mut self, index: usize) {
        if index >= self.history.sessions.len() {
            self.pending_delete_session = None;
            return;
        }
        self.history.sessions.remove(index);
        self.history.active_index = self.history.active_index.and_then(|active| {
            if active == index {
                None
            } else {
                Some(active.min(self.history.sessions.len().saturating_sub(1)))
            }
        });
        self.pending_delete_session = None;
        if let Some(path) = self.loaded_project_path.as_ref() {
            let _ = self.history.save(path);
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        settings: &mut EngineSettings,
        project: Option<&Project>,
        catalog: &CommandCatalog,
        executor: &mut AgentToolExecutor<'_>,
    ) {
        let lang = self.lang;
        let palette = crate::theme::palette_for_visuals(
            ui.ctx().style().visuals.dark_mode,
            settings.theme_experimental,
        );

        self.ensure_tools(catalog, lang);
        self.ensure_model_registry(settings);
        self.ensure_project_history(project);
        self.ensure_runtime_config(settings);

        let provider_config = settings
            .ai_providers
            .iter()
            .find(|c| c.provider == settings.default_ai_provider)
            .cloned()
            .unwrap_or_default();
        let effective_model_id = self
            .model_registry
            .resolve_model_id(&self.selected_model, provider_config.provider)
            .unwrap_or_else(|| provider_config.model.clone());
        let readiness = agent_readiness(&provider_config, &effective_model_id);

        // Poll the runtime each frame to advance non-blocking state machine.
        let _ = self.runtime.poll(Some(executor));
        if self.flush_runtime_events() {
            self.save_history();
        }

        // Header row: title + selectors + sidebar toggle.
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.sidebar_open, if self.sidebar_open { "<" } else { ">" })
                .clicked()
            {
                self.sidebar_open = !self.sidebar_open;
            }
            ui.add_space(6.0);

            ui.label(
                egui::RichText::new(t("app.agent_title", lang))
                    .size(15.0)
                    .strong()
                    .color(palette.text),
            );

            self.model_selector(ui, settings, lang);
            self.mode_selector(ui, settings, lang);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(egui::RichText::new(t("app.agent_settings", lang)).size(11.0))
                    .clicked()
                {
                    self.open_settings_requested = true;
                }
            });
        });
        ui.separator();

        // Main area: sidebar + chat or full chat.
        if self.sidebar_open {
            let sidebar_width = 170.0_f32.min((ui.available_width() - 260.0).max(140.0));
            let content_height = ui.available_height();
            ui.horizontal(|ui| {
                // Left: sessions sidebar.
                ui.allocate_ui_with_layout(
                    egui::vec2(sidebar_width, content_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.sidebar_panel(ui, sidebar_width, palette, lang, project),
                );
                ui.separator();
                // Right: chat.
                ui.vertical(|ui| {
                    self.main_chat_area(ui, palette, lang, settings, project, executor, readiness);
                });
            });
        } else {
            self.main_chat_area(ui, palette, lang, settings, project, executor, readiness);
        }
    }

    // Sidebar

    fn sidebar_panel(
        &mut self,
        ui: &mut Ui,
        width: f32,
        palette: crate::theme::ThemePalette,
        lang: Language,
        project: Option<&Project>,
    ) {
        ui.vertical(|ui| {
            ui.set_width(width);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t("app.agent_sessions", lang))
                        .size(12.0)
                        .strong()
                        .color(palette.text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(egui::RichText::new("X").size(12.0).color(palette.text_dim))
                        .clicked()
                    {
                        self.sidebar_open = false;
                    }
                });
            });

            ui.add_space(6.0);

            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(t("app.agent_new_chat", lang))
                            .size(11.0)
                            .color(egui::Color32::WHITE),
                    )
                    .fill(theme::ACCENT)
                    .rounding(5.0)
                    .min_size(egui::vec2(120.0, 26.0)),
                )
                .clicked()
            {
                self.start_new_chat(project);
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            let session_count = self.history.sessions.len();
            let session_indices: Vec<usize> = (0..session_count).rev().collect();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for &idx in &session_indices {
                        let session = &self.history.sessions[idx];
                        let is_active = self.history.active_index == Some(idx);
                        let title = if session.title.len() > 22 {
                            format!("{}...", &session.title[..22])
                        } else {
                            session.title.clone()
                        };

                        let bg = if is_active {
                            palette.widget_hover
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        let response = ui.add_sized(
                            [ui.available_width(), 28.0],
                            egui::Button::new(egui::RichText::new(title).size(11.0).color(
                                if is_active {
                                    theme::ACCENT
                                } else {
                                    palette.text
                                },
                            ))
                            .fill(bg)
                            .rounding(4.0),
                        );

                        if response.clicked() {
                            if let Some(session) = self.history.sessions.get(idx) {
                                self.runtime.clear();
                                self.runtime.messages = session.messages.clone();
                                self.runtime
                                    .set_system_prompt(build_agent_prompt(&self.tools));
                                self.history.active_index = Some(idx);
                                self.chat = welcome_chat_panel();
                                for msg in &self.runtime.messages {
                                    self.chat.messages.push(msg.clone());
                                }
                            }
                        }
                        if response.secondary_clicked() {
                            self.pending_delete_session = Some(idx);
                        }
                    }
                });

            if let Some(del_idx) = self.pending_delete_session {
                if del_idx < self.history.sessions.len() {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button(t("app.agent_delete", lang)).clicked() {
                            self.history.sessions.remove(del_idx);
                            self.pending_delete_session = None;
                            if let Some(path) = self.loaded_project_path.as_ref() {
                                let _ = self.history.save(path);
                            }
                        }
                        if ui.button(t("app.agent_cancel", lang)).clicked() {
                            self.pending_delete_session = None;
                        }
                    });
                } else {
                    self.pending_delete_session = None;
                }
            }
        });
    }

    // Main Chat Area

    fn main_chat_area(
        &mut self,
        ui: &mut Ui,
        palette: crate::theme::ThemePalette,
        lang: Language,
        settings: &mut EngineSettings,
        project: Option<&Project>,
        executor: &mut AgentToolExecutor<'_>,
        readiness: AgentReadiness,
    ) {
        // Agent activity indicator.
        if self.runtime.status == AgentStatus::Thinking
            || self.runtime.status == AgentStatus::ExecutingTools
        {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("|").size(10.0).color(theme::ACCENT));
                ui.label(
                    egui::RichText::new(if self.runtime.status == AgentStatus::ExecutingTools {
                        t("app.agent_executing_tools", lang)
                    } else {
                        t("app.agent_thinking", lang)
                    })
                    .size(11.0)
                    .color(palette.text_dim),
                );
            });
            ui.add_space(4.0);
        }

        // Warning banner.
        if readiness != AgentReadiness::Ready {
            ui.add_space(4.0);
            let warning = match readiness {
                AgentReadiness::Ready => String::new(),
                AgentReadiness::ProviderDisabled => t("app.agent_provider_disabled", lang),
                AgentReadiness::ModelMissing => t("app.agent_model_missing", lang),
                AgentReadiness::AdapterRequired => t("app.agent_provider_adapter_required", lang),
            };
            ui.label(
                egui::RichText::new(warning)
                    .size(11.0)
                    .color(egui::Color32::from_rgb(230, 160, 60)),
            );
            ui.add_space(4.0);
        } else if settings.agent_mode == AgentMode::Active {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(t("app.agent_active_mode_warning", lang))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(230, 160, 60)),
            );
            ui.add_space(4.0);
        }

        // Pending tool-call approval UI.
        if !self.runtime.pending_calls.is_empty()
            && self.runtime.status == AgentStatus::AwaitingApproval
        {
            self.pending_calls_ui(ui, palette, lang, executor, settings);
        }

        // Messages.
        let bottom_bar_height = if self.runtime.status.blocks_input() {
            130.0
        } else {
            110.0
        };
        let available_height = (ui.available_height() - bottom_bar_height).max(80.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .max_height(available_height)
            .show(ui, |ui| {
                let visible_messages = self
                    .runtime
                    .messages
                    .iter()
                    .filter(|message| message.role != MessageRole::System)
                    .collect::<Vec<_>>();
                if visible_messages.is_empty() {
                    agent_empty_state(ui, palette, lang, project);
                } else {
                    for msg in visible_messages {
                        message_bubble(ui, palette, lang, msg);
                        ui.add_space(6.0);
                    }
                }
            });

        ui.separator();

        // Suggestion chips (not while thinking/awaiting).
        if self.runtime.status == AgentStatus::Done || self.runtime.status == AgentStatus::Error {
            suggestion_chips(ui, palette, lang, project, &mut self.chat.input_text);
            ui.add_space(6.0);
        }

        // Input.
        ui.horizontal(|ui| {
            let available_width = (ui.available_width() - 70.0).max(0.0);
            let send_enabled = readiness == AgentReadiness::Ready
                && !self.chat.input_text.trim().is_empty()
                && !self.runtime.status.blocks_input();

            let response = ui.add_sized(
                [available_width, 48.0],
                egui::TextEdit::multiline(&mut self.chat.input_text)
                    .hint_text(t("app.agent_input_placeholder", lang))
                    .desired_rows(2),
            );

            let send_label = if self.runtime.status == AgentStatus::AwaitingApproval {
                t("app.agent_awaiting_approval", lang)
            } else if self.runtime.status == AgentStatus::Thinking {
                t("app.agent_thinking_dots", lang)
            } else if self.runtime.status == AgentStatus::ExecutingTools {
                t("app.agent_executing_tools_dots", lang)
            } else {
                t("app.agent_send", lang)
            };

            let send_button = ui.add_sized(
                [64.0, 34.0],
                egui::Button::new(egui::RichText::new(send_label).size(12.0).color(
                    if send_enabled {
                        egui::Color32::WHITE
                    } else {
                        palette.text_dim
                    },
                ))
                .fill(if send_enabled {
                    theme::ACCENT
                } else {
                    palette.widget
                })
                .rounding(6.0),
            );

            let ctrl_enter = ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter));

            if (send_button.clicked() || ctrl_enter) && send_enabled {
                let input = self.chat.input_text.trim().to_string();
                self.chat.input_text.clear();
                self.submit(input, settings.agent_mode == AgentMode::Active);
            }

            if response.changed() {
                response.request_focus();
            }
        });
    }

    // Core Methods

    fn submit(&mut self, input: String, active_mode: bool) {
        self.runtime.start_run(input, &self.tools, active_mode);
        let _ = self.flush_runtime_events();
        self.save_history();
    }

    fn approve_all(&mut self, executor: &mut AgentToolExecutor<'_>) {
        self.runtime.approve_pending(&self.tools, executor);
        let _ = self.flush_runtime_events();
        self.save_history();
    }

    fn deny_all(&mut self, reason: String, executor: &mut AgentToolExecutor<'_>) {
        self.runtime.deny_pending(reason, &self.tools, executor);
        let _ = self.flush_runtime_events();
        self.save_history();
    }

    fn flush_runtime_events(&mut self) -> bool {
        let mut messages_changed = false;
        for event in self.runtime.events.drain(..) {
            match event {
                AgentEvent::MessageAdded(message) => {
                    self.chat.messages.push(message);
                    messages_changed = true;
                }
                AgentEvent::ToolCallsRequested(_)
                | AgentEvent::ToolResult { .. }
                | AgentEvent::StatusChanged(_, _) => {}
            }
        }
        messages_changed
    }

    fn start_new_chat(&mut self, project: Option<&Project>) {
        self.runtime.clear();
        self.runtime
            .set_system_prompt(build_agent_prompt(&self.tools));
        self.chat = welcome_chat_panel();
        let title = t("app.agent_new_chat_default", self.lang);
        self.history.start_session(title);
        if let Some(project) = project {
            let _ = self.history.save(&project.path);
        }
    }

    fn save_history(&mut self) {
        if let Some(project) = self.loaded_project_path.as_ref() {
            if let Some(session) = self.history.active_session_mut() {
                session.messages = self
                    .runtime
                    .messages
                    .iter()
                    .filter(|message| message.role != MessageRole::System)
                    .cloned()
                    .collect();
                session.touch();
            }
            let _ = self.history.save(project);
        }
    }

    fn ensure_tools(&mut self, catalog: &CommandCatalog, language: Language) {
        if self.tools_language != Some(language) || self.tools.len() != catalog.commands.len() {
            self.tools = AgentToolExecutor::build_tools(catalog, language);
            self.tool_name_map = build_tool_name_map(catalog);
            self.tools_language = Some(language);
        }
    }

    fn ensure_runtime_config(&mut self, settings: &EngineSettings) {
        let provider_config = settings
            .ai_providers
            .iter()
            .find(|c| c.provider == settings.default_ai_provider)
            .cloned()
            .unwrap_or_default();

        let model_id = self
            .model_registry
            .resolve_model_id(&self.selected_model, provider_config.provider)
            .unwrap_or_else(|| provider_config.model.clone());

        let config = OpenAiConfig {
            base_url: provider_config.base_url.clone(),
            model: model_id,
            api_key: provider_config.api_key.clone(),
            max_tokens: 4096,
            temperature: 0.7,
        };
        if self.last_runtime_config.as_ref() != Some(&config) {
            self.runtime.client = raf_ai::openai_client::OpenAiClient::new(config.clone());
            self.last_runtime_config = Some(config);
        }
        self.runtime
            .set_system_prompt(build_agent_prompt(&self.tools));
    }

    fn ensure_model_registry(&mut self, settings: &mut EngineSettings) {
        self.model_registry = AgentModelRegistry::new(settings.agent_model_shortcuts.clone());
    }

    fn ensure_project_history(&mut self, project: Option<&Project>) {
        let new_path = project.map(|p| p.path.clone());
        if new_path != self.loaded_project_path {
            self.loaded_project_path = new_path.clone();
            if let Some(path) = new_path {
                self.history = AgentHistory::load(&path);
                let session_index = self
                    .history
                    .ensure_active_session(t("app.agent_new_chat_default", self.lang));
                if let Some(session) = self.history.sessions.get(session_index) {
                    self.runtime.clear();
                    self.runtime.messages = session.messages.clone();
                }
            } else {
                self.history = AgentHistory::default();
                self.runtime.clear();
            }
            self.chat = welcome_chat_panel();
            for msg in &self.runtime.messages {
                self.chat.messages.push(msg.clone());
            }
        }
    }

    // Selectors

    fn model_selector(&mut self, ui: &mut Ui, settings: &mut EngineSettings, lang: Language) {
        let labels = self.model_registry.selector_labels();
        let selected = if self.selected_model.is_empty() {
            AgentModelRegistry::PROVIDER_DEFAULT
        } else {
            &self.selected_model
        };

        egui::ComboBox::from_id_salt("agent_model_selector")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for label in &labels {
                    ui.selectable_value(&mut self.selected_model, label.clone(), label);
                }
            });

        ui.add_space(8.0);

        ui.menu_button(t("app.agent_add_model", lang), |ui| {
            ui.horizontal(|ui| {
                ui.label(t("app.agent_model_label", lang));
                ui.text_edit_singleline(&mut self.new_model_label);
            });
            ui.horizontal(|ui| {
                ui.label(t("app.agent_model_id", lang));
                ui.text_edit_singleline(&mut self.new_model_id);
            });
            if ui.button(t("app.agent_add_model_confirm", lang)).clicked() {
                if self.model_registry.add(
                    self.new_model_label.clone(),
                    settings.default_ai_provider,
                    self.new_model_id.clone(),
                ) {
                    settings.agent_model_shortcuts = self.model_registry.shortcuts.clone();
                    self.settings_changed = true;
                    self.new_model_label.clear();
                    self.new_model_id.clear();
                }
            }
        });
    }

    fn mode_selector(&mut self, ui: &mut Ui, settings: &mut EngineSettings, lang: Language) {
        let previous = settings.agent_mode;
        let label = match settings.agent_mode {
            AgentMode::Passive => t("app.agent_mode_passive", lang),
            AgentMode::Active => t("app.agent_mode_active", lang),
        };
        egui::ComboBox::from_id_salt("agent_mode_selector")
            .selected_text(label)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut settings.agent_mode,
                    AgentMode::Passive,
                    t("app.agent_mode_passive", lang),
                );
                ui.selectable_value(
                    &mut settings.agent_mode,
                    AgentMode::Active,
                    t("app.agent_mode_active", lang),
                );
            });
        if settings.agent_mode != previous {
            self.settings_changed = true;
        }
    }

    fn pending_calls_ui(
        &mut self,
        ui: &mut Ui,
        palette: crate::theme::ThemePalette,
        lang: Language,
        executor: &mut AgentToolExecutor<'_>,
        _settings: &EngineSettings,
    ) {
        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(t("app.agent_pending_tools", lang))
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(6.0);

            for call in &self.runtime.pending_calls {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{}({:?})", call.name, call.arguments))
                            .size(11.0)
                            .color(palette.text_dim),
                    );
                });
                if let Some(error) = call.argument_error.as_deref() {
                    ui.label(
                        egui::RichText::new(error)
                            .size(10.0)
                            .color(egui::Color32::from_rgb(210, 120, 70)),
                    );
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let approve_btn = egui::Button::new(
                    egui::RichText::new(t("app.agent_approve", lang)).color(egui::Color32::WHITE),
                )
                .fill(theme::ACCENT);
                if ui.add(approve_btn).clicked() {
                    self.approve_all(executor);
                }
                let deny_btn = egui::Button::new(
                    egui::RichText::new(t("app.agent_deny", lang)).color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(58, 58, 62));
                if ui.add(deny_btn).clicked() {
                    self.deny_all(t("app.agent_denied_by_user", lang).to_string(), executor);
                }
            });
        });
    }
}

// Helpers

fn build_agent_prompt(tools: &[OpenAiTool]) -> String {
    let base = include_str!("../../../../crates/raf_ai/AGENT.md");
    let tools_json = serde_json::to_string_pretty(tools).unwrap_or_default();
    format!(
        "{}\n\n## Electronics inspection policy\n\nWhen the user asks whether a circuit is connected correctly, what is wrong, or why it does not work, call `electronics_diagnose` before proposing edits. Read its DRC issues, pin-to-net topology, and simulation messages. Name the exact components and nets implicated. Do not infer correctness solely from component, wire, or net counts. Use `electronics_drc` or `electronics_simulate` for a narrower follow-up only after the diagnosis.\n\n## Tool definitions (OpenAI format)\n\n```json\n{}\n```\n",
        base, tools_json
    )
}

fn welcome_chat_panel() -> ChatPanel {
    let mut chat = ChatPanel::default();
    chat.is_available = true;
    chat
}

fn agent_readiness(provider: &AiProviderConfig, model_id: &str) -> AgentReadiness {
    if !provider.enabled {
        return AgentReadiness::ProviderDisabled;
    }
    if !provider.provider.is_editor_supported() {
        return AgentReadiness::AdapterRequired;
    }
    if model_id.trim().is_empty() {
        return AgentReadiness::ModelMissing;
    }
    if direct_endpoint_requires_adapter(provider) {
        return AgentReadiness::AdapterRequired;
    }
    AgentReadiness::Ready
}

fn direct_endpoint_requires_adapter(provider: &AiProviderConfig) -> bool {
    if !matches!(
        provider.provider,
        AiProvider::Puerto | AiProvider::GenAI | AiProvider::Claude
    ) {
        return false;
    }

    let default = AiProviderConfig::for_provider(provider.provider);
    provider.base_url.trim_end_matches('/') == default.base_url.trim_end_matches('/')
}

fn agent_empty_state(
    ui: &mut Ui,
    palette: crate::theme::ThemePalette,
    lang: Language,
    project: Option<&Project>,
) {
    let available = ui.available_rect_before_wrap();
    let center = available.center();

    ui.painter().text(
        egui::pos2(center.x, center.y - 24.0),
        egui::Align2::CENTER_CENTER,
        t("app.agent_empty_title", lang),
        egui::FontId::proportional(15.0),
        palette.text,
    );

    let subtitle = match project.map(|p| p.project_type) {
        Some(ProjectType::Game) => t("app.agent_empty_subtitle_game", lang),
        Some(ProjectType::Electronics) => t("app.agent_empty_subtitle_electronics", lang),
        None => t("app.agent_empty_subtitle", lang),
    };

    ui.painter().text(
        egui::pos2(center.x, center.y + 6.0),
        egui::Align2::CENTER_CENTER,
        subtitle,
        egui::FontId::proportional(11.0),
        palette.text_dim,
    );
}

fn message_bubble(
    ui: &mut Ui,
    palette: crate::theme::ThemePalette,
    lang: Language,
    msg: &ChatMessage,
) {
    let is_user = msg.role == MessageRole::User;
    let is_tool = msg.role == MessageRole::Tool;
    let is_system = msg.role == MessageRole::System;
    let is_error = msg.role == MessageRole::Assistant
        && (msg.content.starts_with("Error:") || msg.content.starts_with("I encountered"));

    let (bg, text_color, label, label_color) = if is_user {
        (
            theme::ACCENT,
            egui::Color32::WHITE,
            t("app.you", lang),
            egui::Color32::WHITE,
        )
    } else if is_tool {
        (
            egui::Color32::from_rgb(40, 40, 50),
            palette.text_dim,
            String::new(),
            palette.text_dim,
        )
    } else if is_system {
        (
            palette.panel,
            palette.text_dim,
            t("app.agent_system_label", lang),
            palette.text_dim,
        )
    } else if is_error {
        (
            egui::Color32::from_rgb(60, 30, 30),
            egui::Color32::from_rgb(255, 120, 120),
            t("app.agent_error_label", lang),
            egui::Color32::from_rgb(255, 80, 80),
        )
    } else {
        (
            palette.widget,
            palette.text,
            t("app.agent_label", lang),
            theme::ACCENT,
        )
    };

    let available = ui.available_rect_before_wrap();
    let max_width = available.width() * 0.82;
    let text = egui::RichText::new(&msg.content)
        .size(12.0)
        .color(text_color);
    let galley = ui.fonts(|fonts| {
        fonts.layout(
            text.text().to_string(),
            egui::FontId::proportional(12.0),
            text_color,
            max_width,
        )
    });

    let padding = egui::vec2(12.0, 9.0);
    let label_height = if is_user || is_tool { 0.0 } else { 14.0 };
    let bubble_size = galley.size() + padding * 2.0 + egui::vec2(0.0, label_height);

    let x = if is_user {
        available.right() - bubble_size.x - 4.0
    } else if is_system {
        available.left() + (available.width() - bubble_size.x) / 2.0
    } else {
        available.left() + 4.0
    };
    let y = ui.cursor().top();
    let rect = egui::Rect::from_min_size(egui::pos2(x, y), bubble_size);

    ui.painter().rect_filled(rect, 8.0, bg);

    if !is_user && !is_tool {
        ui.painter().text(
            rect.left_top() + egui::vec2(padding.x, 5.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(10.0),
            label_color,
        );
    }

    if is_tool {
        ui.painter().text(
            rect.left_top() + egui::vec2(padding.x, 5.0),
            egui::Align2::LEFT_TOP,
            t("app.agent_tool_result", lang),
            egui::FontId::proportional(9.0),
            palette.text_dim,
        );
    }

    let content_offset = if is_user || is_tool {
        padding.y
    } else {
        padding.y + 14.0
    };
    ui.painter().galley(
        rect.left_top() + egui::vec2(padding.x, content_offset),
        galley,
        text_color,
    );

    ui.allocate_rect(rect, egui::Sense::hover());
}

fn suggestion_chips(
    ui: &mut Ui,
    palette: crate::theme::ThemePalette,
    lang: Language,
    project: Option<&Project>,
    input: &mut String,
) {
    let suggestions: Vec<String> = match project.map(|p| p.project_type) {
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
    };

    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(t("app.agent_suggestions_title", lang))
                .size(11.0)
                .color(palette.text_dim),
        );
        ui.add_space(8.0);
        for suggestion in suggestions {
            if suggestion_chip(ui, palette, &suggestion) {
                *input = suggestion;
            }
            ui.add_space(6.0);
        }
    });
}

fn suggestion_chip(ui: &mut Ui, palette: crate::theme::ThemePalette, label: &str) -> bool {
    let padding = egui::vec2(10.0, 5.0);
    let text_width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(
                label.to_string(),
                egui::FontId::proportional(11.0),
                palette.text,
            )
            .size()
            .x
    });
    let size = egui::vec2(text_width + padding.x * 2.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    let fill = if response.hovered() {
        palette.widget_hover
    } else {
        palette.widget
    };

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter()
        .rect_stroke(rect, 6.0, egui::Stroke::new(1.0, palette.border));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        palette.text,
    );

    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_blocks_disabled_or_unadapted_provider_defaults() {
        let mut puerto = AiProviderConfig::for_provider(AiProvider::Puerto);
        puerto.enabled = true;
        puerto.model = "assistant".to_string();
        assert_eq!(
            agent_readiness(&puerto, &puerto.model),
            AgentReadiness::AdapterRequired
        );

        puerto.base_url = "http://localhost:11434/v1".to_string();
        assert_eq!(
            agent_readiness(&puerto, &puerto.model),
            AgentReadiness::AdapterRequired
        );

        puerto.enabled = false;
        assert_eq!(
            agent_readiness(&puerto, &puerto.model),
            AgentReadiness::ProviderDisabled
        );

        let mut openrouter = AiProviderConfig::for_provider(AiProvider::OpenRouter);
        openrouter.enabled = true;
        openrouter.model = "assistant".to_string();
        assert_eq!(
            agent_readiness(&openrouter, &openrouter.model),
            AgentReadiness::Ready
        );
    }
}
