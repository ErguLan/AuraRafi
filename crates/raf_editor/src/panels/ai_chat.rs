//! Controller for the Agent workbench.
//!
//! This module owns runtime/history/provider orchestration only. The retained
//! surface in `agent_surface.rs` receives a read-only reference and emits
//! typed actions, so the UI can evolve without moving the AI backend again.

use std::path::PathBuf;

use raf_ai::agent_history::{AgentHistory, AgentHistoryWriter};
use raf_ai::agent_model_registry::AgentModelRegistry;
use raf_ai::agent_runtime::{AgentRuntime, AgentStatus, ToolExecutor};
use raf_ai::chat::ChatMessage;
use raf_ai::openai_client::{OpenAiConfig, OpenAiTool};
use raf_ai::provider::{AgentMode, AiProvider, AiProviderConfig};
use raf_core::config::EngineSettings;
use raf_core::i18n::t;
use raf_core::project::Project;
use raf_core::Language;

use crate::agent_executor::{build_tool_name_map, AgentToolExecutor};
use crate::commands::catalog::CommandCatalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentReadiness {
    Ready,
    ProviderDisabled,
    ModelMissing,
    AdapterRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeKey {
    provider: AiProvider,
    base_url: String,
    model: String,
    api_key: String,
    streaming: bool,
}

/// The editor-facing Agent state.
pub struct AgentPanel {
    pub runtime: AgentRuntime,
    pub history: AgentHistory,
    pub model_registry: AgentModelRegistry,
    pub tools: Vec<OpenAiTool>,
    pub tool_name_map: std::collections::HashMap<String, String>,
    pub input_text: String,
    pub selected_model: String,
    pub sidebar_open: bool,
    pub new_model_label: String,
    pub new_model_id: String,
    pub open_settings_requested: bool,
    pub settings_changed: bool,
    pub language: Language,
    pub last_status: AgentStatus,
    loaded_project_path: Option<PathBuf>,
    runtime_key: Option<RuntimeKey>,
    tools_language: Option<Language>,
    system_prompt: String,
    last_saved_message_count: usize,
    history_writer: AgentHistoryWriter,
}

impl Default for AgentPanel {
    fn default() -> Self {
        Self {
            runtime: AgentRuntime::new(OpenAiConfig::default()),
            history: AgentHistory::default(),
            model_registry: AgentModelRegistry::default(),
            tools: Vec::new(),
            tool_name_map: std::collections::HashMap::new(),
            input_text: String::new(),
            selected_model: AgentModelRegistry::PROVIDER_DEFAULT.to_string(),
            sidebar_open: true,
            new_model_label: String::new(),
            new_model_id: String::new(),
            open_settings_requested: false,
            settings_changed: false,
            language: Language::English,
            last_status: AgentStatus::Done,
            loaded_project_path: None,
            runtime_key: None,
            tools_language: None,
            system_prompt: String::new(),
            last_saved_message_count: 0,
            history_writer: AgentHistoryWriter::new(),
        }
    }
}

impl AgentPanel {
    /// Synchronise project-local state and return the actionable readiness
    /// state for the current provider/model selection.
    pub(crate) fn prepare(
        &mut self,
        settings: &EngineSettings,
        project: Option<&Project>,
        catalog: &CommandCatalog,
    ) -> AgentReadiness {
        self.language = settings.language;
        if self.tools_language != Some(settings.language) || self.tools.is_empty() {
            self.tools = AgentToolExecutor::build_tools(catalog, settings.language);
            self.tool_name_map = build_tool_name_map(catalog);
            self.tools_language = Some(settings.language);
            self.system_prompt = build_agent_prompt(&self.tools);
            self.runtime.set_system_prompt(self.system_prompt.clone());
        }
        if self.model_registry.shortcuts != settings.agent_model_shortcuts {
            self.model_registry = AgentModelRegistry::new(settings.agent_model_shortcuts.clone());
        }
        if self.runtime_key.is_none()
            && self.selected_model == AgentModelRegistry::PROVIDER_DEFAULT
            && !settings.default_agent_model.trim().is_empty()
            && self
                .model_registry
                .get(settings.default_agent_model.trim())
                .is_some()
        {
            self.selected_model = settings.default_agent_model.trim().to_string();
        }
        self.ensure_project_history(project);
        self.ensure_runtime_config(settings);
        self.readiness(settings)
    }

    /// Poll the non-blocking runtime once per editor frame.
    pub(crate) fn poll(&mut self, executor: &mut dyn ToolExecutor) {
        let status = self.runtime.poll(Some(executor));
        if status != self.last_status {
            self.last_status = status;
            self.persist_history();
        } else if self.runtime.messages.len() != self.last_saved_message_count {
            self.persist_history();
        }
    }

    pub(crate) fn readiness(&self, settings: &EngineSettings) -> AgentReadiness {
        let Some(provider) = settings
            .ai_providers
            .iter()
            .find(|config| config.provider == settings.default_ai_provider)
        else {
            return AgentReadiness::ProviderDisabled;
        };
        if !provider.enabled {
            return AgentReadiness::ProviderDisabled;
        }
        let model = self.effective_model(provider);
        if model.trim().is_empty() {
            return AgentReadiness::ModelMissing;
        }
        if provider.base_url.trim().is_empty() {
            return AgentReadiness::AdapterRequired;
        }
        AgentReadiness::Ready
    }

    pub(crate) fn effective_provider<'a>(
        &self,
        settings: &'a EngineSettings,
    ) -> Option<&'a AiProviderConfig> {
        settings
            .ai_providers
            .iter()
            .find(|config| config.provider == settings.default_ai_provider)
    }

    pub(crate) fn effective_model(&self, provider: &AiProviderConfig) -> String {
        self.model_registry
            .resolve_model_id(&self.selected_model, provider.provider)
            .unwrap_or_else(|| provider.model.clone())
    }

    pub(crate) fn apply_action(&mut self, action: AgentAction, settings: &mut EngineSettings) {
        match action {
            AgentAction::SetInput(value) => self.input_text = value,
            AgentAction::ToggleSidebar => self.sidebar_open = !self.sidebar_open,
            AgentAction::CloseSidebar => self.sidebar_open = false,
            AgentAction::NewChat => self.start_new_chat(),
            AgentAction::SelectSession(index) => self.select_session(index),
            AgentAction::DeleteSession(index) => self.delete_session(index),
            AgentAction::SelectModel(label) => self.selected_model = label,
            AgentAction::SetMode(mode) => {
                if settings.agent_mode != mode {
                    settings.agent_mode = mode;
                    self.settings_changed = true;
                }
            }
            AgentAction::SetNewModelLabel(value) => self.new_model_label = value,
            AgentAction::SetNewModelId(value) => self.new_model_id = value,
            AgentAction::AddModel => {
                if self.model_registry.add(
                    self.new_model_label.trim(),
                    settings.default_ai_provider,
                    self.new_model_id.trim(),
                ) {
                    settings.agent_model_shortcuts = self.model_registry.shortcuts.clone();
                    self.settings_changed = true;
                    self.new_model_label.clear();
                    self.new_model_id.clear();
                }
            }
            AgentAction::OpenSettings => self.open_settings_requested = true,
            AgentAction::Submit => self.submit(settings.agent_mode == AgentMode::Active),
            AgentAction::Stop => {
                self.runtime.cancel();
                self.last_status = AgentStatus::Done;
                self.persist_history();
            }
            AgentAction::Approve => {
                // Approval needs the live executor and is applied in
                // `approve_with_executor` from the application boundary.
            }
            AgentAction::Deny => {
                // Same as approval: the controller keeps the typed action and
                // the host supplies the live editor executor.
            }
            AgentAction::UseSuggestion(value) => self.input_text = value,
        }
    }

    pub(crate) fn approve_with_executor(&mut self, executor: &mut dyn ToolExecutor) {
        self.runtime.approve_pending(&self.tools, executor);
        self.persist_history();
    }

    pub(crate) fn deny_with_executor(&mut self, executor: &mut dyn ToolExecutor) {
        self.runtime.deny_pending(
            t("app.agent_denied_by_user", self.language),
            &self.tools,
            executor,
        );
        self.persist_history();
    }

    pub fn take_open_settings_request(&mut self) -> bool {
        std::mem::take(&mut self.open_settings_requested)
    }

    fn submit(&mut self, active_mode: bool) {
        let content = self.input_text.trim().to_string();
        if content.is_empty() || self.runtime.status.blocks_input() {
            return;
        }
        self.input_text.clear();
        self.runtime.set_system_prompt(self.system_prompt.clone());
        self.runtime.start_run(&content, &self.tools, active_mode);
        self.last_status = AgentStatus::Thinking;
    }

    fn ensure_project_history(&mut self, project: Option<&Project>) {
        let next_path = project.map(|project| project.path.clone());
        if self.loaded_project_path == next_path {
            if self.history.active_index.is_none() && project.is_some() {
                let index = self.history.ensure_active_session("New chat");
                self.load_session_into_runtime(index);
            }
            return;
        }
        self.runtime.cancel();
        self.loaded_project_path = next_path.clone();
        self.history = next_path
            .as_deref()
            .map(AgentHistory::load)
            .unwrap_or_default();
        self.history.ensure_active_session("New chat");
        self.last_saved_message_count = 0;
        if let Some(index) = self.history.active_index {
            self.load_session_into_runtime(index);
        }
    }

    fn ensure_runtime_config(&mut self, settings: &EngineSettings) {
        let Some(provider) = self.effective_provider(settings).cloned() else {
            return;
        };
        let key = RuntimeKey {
            provider: provider.provider,
            base_url: provider.base_url.clone(),
            model: self.effective_model(&provider),
            api_key: provider.api_key.clone(),
            streaming: settings.agent_streaming_enabled,
        };
        if self.runtime_key.as_ref() == Some(&key) {
            return;
        }
        let messages = self.runtime.messages.clone();
        self.runtime.cancel();
        self.runtime = AgentRuntime::new(OpenAiConfig {
            base_url: key.base_url.clone(),
            model: key.model.clone(),
            api_key: key.api_key.clone(),
            max_tokens: 4096,
            temperature: 0.2,
            streaming: key.streaming,
        });
        self.runtime.messages = messages;
        self.runtime.set_system_prompt(self.system_prompt.clone());
        self.runtime_key = Some(key);
    }

    fn start_new_chat(&mut self) {
        self.runtime.clear();
        self.history.start_session("New chat");
        self.runtime.set_system_prompt(self.system_prompt.clone());
        self.input_text.clear();
        self.persist_history();
    }

    fn select_session(&mut self, index: usize) {
        if index >= self.history.sessions.len() {
            return;
        }
        self.history.active_index = Some(index);
        self.load_session_into_runtime(index);
        self.persist_history();
    }

    fn load_session_into_runtime(&mut self, index: usize) {
        let messages = self
            .history
            .sessions
            .get(index)
            .map(|session| session.messages.clone())
            .unwrap_or_default();
        self.runtime.cancel();
        self.runtime.messages = messages;
        self.runtime.set_system_prompt(self.system_prompt.clone());
        self.input_text.clear();
        self.last_status = AgentStatus::Done;
    }

    fn delete_session(&mut self, index: usize) {
        if index >= self.history.sessions.len() {
            return;
        }
        self.history.sessions.remove(index);
        self.history.active_index = self.history.active_index.and_then(|active| {
            if active == index {
                None
            } else if active > index {
                Some(active - 1)
            } else {
                Some(active)
            }
        });
        if self.history.active_index.is_none() && !self.history.sessions.is_empty() {
            self.history.active_index = Some(self.history.sessions.len() - 1);
        }
        if let Some(index) = self.history.active_index {
            self.load_session_into_runtime(index);
        } else {
            self.start_new_chat();
        }
        self.persist_history();
    }

    fn persist_history(&mut self) {
        let Some(index) = self.history.active_index else {
            return;
        };
        let Some(session) = self.history.sessions.get_mut(index) else {
            return;
        };
        if messages_equal(&session.messages, &self.runtime.messages) {
            return;
        }
        session.messages = self.runtime.messages.clone();
        session.touch();
        self.last_saved_message_count = self.runtime.messages.len();
        if let Some(project_path) = self.loaded_project_path.as_deref() {
            self.history_writer.enqueue(project_path, &self.history);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AgentAction {
    SetInput(String),
    ToggleSidebar,
    CloseSidebar,
    NewChat,
    SelectSession(usize),
    DeleteSession(usize),
    SelectModel(String),
    SetMode(AgentMode),
    SetNewModelLabel(String),
    SetNewModelId(String),
    AddModel,
    OpenSettings,
    Submit,
    Stop,
    Approve,
    Deny,
    UseSuggestion(String),
}

fn messages_equal(left: &[ChatMessage], right: &[ChatMessage]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.id == right.id
                && left.role == right.role
                && left.content == right.content
                && left.tool_calls == right.tool_calls
        })
}

fn build_agent_prompt(tools: &[OpenAiTool]) -> String {
    let tool_names = tools
        .iter()
        .map(|tool| tool.function.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "You are the AuraRafi Agent inside the current editor. Be concise and actionable. Use tools when they help the user. Never claim a mutation succeeded unless the tool result confirms it. Available tools: {tool_names}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_ai::chat::MessageRole;

    #[test]
    fn new_panel_starts_with_a_collapsible_chat_rail() {
        let panel = AgentPanel::default();
        assert!(panel.sidebar_open);
        assert_eq!(panel.selected_model, AgentModelRegistry::PROVIDER_DEFAULT);
    }

    #[test]
    fn messages_compare_without_serializing_runtime_state() {
        let messages = vec![ChatMessage::user("hello")];
        assert!(messages_equal(&messages, &messages));
        assert_ne!(messages[0].role, MessageRole::Assistant);
    }
}
