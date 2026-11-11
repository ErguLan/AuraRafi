//! Controller for the Agent workbench.
//!
//! This module owns runtime/history/provider orchestration only. A future UI
//! may consume its state and typed actions without moving the AI backend.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use raf_ai::agent_history::{AgentHistory, AgentHistoryWriter};
use raf_ai::agent_runtime::{
    AgentRuntime, AgentStatus, AgentToolResult, ToolExecutionMode, ToolExecutor,
};
use raf_ai::chat::ChatMessage;
use raf_ai::model_capabilities::{vision_support, VisionSupport};
use raf_ai::openai_client::{OpenAiConfig, OpenAiTool};
use raf_ai::provider::{AgentMode, AiProvider, AiProviderConfig};
use raf_core::config::EngineSettings;
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType};
use raf_core::Language;

use crate::agent_context::{build_agent_system_prompt, AgentProjectSnapshot};
use crate::agent_executor::{AgentToolExecutor, AgentToolRoute};
use crate::commands::catalog::CommandCatalog;

const MIN_AGENT_STREAM_PRESENTATION: Duration = Duration::from_millis(33);

/// Transient sentinel: the panel always shows the provider card model.
/// Unifying the model system removed the second label-based selection path.
pub(crate) const MODEL_SENTINEL: &str = "provider_default";

pub(crate) type SharedAgentPanel = Rc<RefCell<AgentPanel>>;

struct ConversationRun {
    runtime: AgentRuntime,
    tools: Vec<OpenAiTool>,
    routes: HashMap<String, AgentToolRoute>,
    prompt: String,
    draft: String,
    submission: Option<String>,
    runtime_key: Option<RuntimeKey>,
    selected_model: String,
    decision: Option<AgentDecision>,
    status: AgentStatus,
    review_writes: bool,
    opening: bool,
    context_requested: bool,
    document_name: String,
    saved_revision: Option<u64>,
    save_error: Option<String>,
    saving: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum AgentDecision {
    ApproveAll,
    DenyAll,
    Approve(String),
    Deny(String),
}

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
    max_tokens: u32,
    reasoning_effort: String,
    chatgpt_plan: bool,
    chatgpt_registration: Option<String>,
    tool_call_limit_enabled: bool,
    max_tool_calls: u32,
}

/// The editor-facing Agent state.
pub struct AgentPanel {
    media_job: Option<(String, crate::agent_media::MediaReceiver)>,
    pub(crate) media_error: Option<String>,
    pub runtime: AgentRuntime,
    pub history: AgentHistory,
    pub tools: Vec<OpenAiTool>,
    pub tool_routes: std::collections::HashMap<String, AgentToolRoute>,
    pub input_text: String,
    pub(crate) authoring_parent: Option<String>,
    pub selected_model: String,
    pub sidebar_open: bool,
    pub new_model_id: String,
    pub new_model_error: Option<String>,
    pub open_settings_requested: bool,
    pub settings_changed: bool,
    pub(crate) model_menu_open: bool,
    pub(crate) mode_menu_open: bool,
    pub(crate) add_model_open: bool,
    tool_call_warning_dismissed: bool,
    pub language: Language,
    pub(crate) show_reasoning: bool,
    catalog_pending: bool,
    catalog_fingerprint: String,
    pub last_status: AgentStatus,
    visual_revision: u64,
    last_visual_message_revision: u64,
    last_visual_present: Instant,
    last_visual_elapsed_second: Option<u64>,
    loaded_project_path: Option<PathBuf>,
    runtime_key: Option<RuntimeKey>,
    external_studio_enabled: bool,
    system_prompt: String,
    pending_submission: Option<String>,
    last_saved_message_count: usize,
    history_writer: AgentHistoryWriter,
    parked_runs: HashMap<String, ConversationRun>,
    loaded_conversation: Option<String>,
    last_history_write: Instant,
    last_history_message_revision: u64,
    visiting_selected: Option<usize>,
    pub(crate) decision: Option<AgentDecision>,
    pub(crate) opening: bool,
    pub(crate) review_writes: bool,
    pub(crate) saved_revision: Option<u64>,
    pub(crate) save_error: Option<String>,
    pub(crate) saving: bool,
    pub(crate) document_name: String,
    pub(crate) expanded_tools: std::collections::HashSet<String>,
    pub(crate) spinner_step: u8,
    context_requested: bool,
}

impl Default for AgentPanel {
    fn default() -> Self {
        Self {
            media_job: None,
            media_error: None,
            runtime: AgentRuntime::new(OpenAiConfig::default()),
            history: AgentHistory::default(),
            tools: Vec::new(),
            tool_routes: std::collections::HashMap::new(),
            input_text: String::new(),
            authoring_parent: None,
            selected_model: MODEL_SENTINEL.to_string(),
            sidebar_open: true,
            new_model_id: String::new(),
            new_model_error: None,
            open_settings_requested: false,
            settings_changed: false,
            model_menu_open: false,
            mode_menu_open: false,
            add_model_open: false,
            tool_call_warning_dismissed: false,
            language: Language::English,
            show_reasoning: true,
            catalog_pending: false,
            catalog_fingerprint: String::new(),
            last_status: AgentStatus::Done,
            visual_revision: 0,
            last_visual_message_revision: 0,
            last_visual_present: Instant::now(),
            last_visual_elapsed_second: None,
            loaded_project_path: None,
            runtime_key: None,
            external_studio_enabled: false,
            system_prompt: String::new(),
            pending_submission: None,
            last_saved_message_count: 0,
            history_writer: AgentHistoryWriter::new(),
            parked_runs: HashMap::new(),
            loaded_conversation: None,
            last_history_write: Instant::now(),
            last_history_message_revision: 0,
            visiting_selected: None,
            decision: None,
            opening: false,
            review_writes: true,
            saved_revision: None,
            save_error: None,
            saving: false,
            document_name: String::new(),
            expanded_tools: std::collections::HashSet::new(),
            spinner_step: 0,
            context_requested: false,
        }
    }
}

#[allow(dead_code)]
impl AgentPanel {
    pub(crate) fn image_provider(
        &self,
        settings: &EngineSettings,
    ) -> Option<(raf_ai::AssetImageProvider, String)> {
        let provider = self.effective_provider(settings)?;
        if provider.provider == AiProvider::OpenAI && settings.agent_openai_use_plan {
            return None;
        }
        Some((
            raf_ai::AssetImageProvider {
                provider: match provider.provider {
                    AiProvider::OpenRouter => "openrouter",
                    AiProvider::OpenAI => "openai",
                    _ => return None,
                }
                .into(),
                base_url: provider.base_url.clone(),
                api_key: provider.api_key.clone(),
            },
            self.effective_model(provider),
        ))
    }

    /// Synchronise project-local state and return the actionable readiness
    /// state for the current provider/model selection.
    pub(crate) fn prepare(
        &mut self,
        settings: &EngineSettings,
        project: Option<&Project>,
        _catalog: &CommandCatalog,
    ) -> AgentReadiness {
        self.language = settings.language;
        self.show_reasoning = settings.agent_show_reasoning;
        if settings.agent_openai_use_plan && settings.default_ai_provider == AiProvider::OpenAI {
            raf_ai::chatgpt_auth::ensure_models_requested();
        }
        if let Some(provider) =
            settings
                .ai_providers
                .iter()
                .find(|p| p.provider == settings.default_ai_provider && p.enabled)
        {
            if !(provider.provider == AiProvider::OpenAI && settings.agent_openai_use_plan) && matches!(
                provider.provider,
                AiProvider::OpenRouter
                    | AiProvider::OpenAI
                    | AiProvider::OpenCode
                    | AiProvider::OpenCodeGo
            ) {
                // OpenCode services keep their dedicated catalog endpoints; the
                // generic gateways fetch from the configured base URL.
                let base_url = match provider.provider {
                    AiProvider::OpenRouter | AiProvider::OpenAI => provider.base_url.clone(),
                    _ => String::new(),
                };
                raf_ai::provider_catalog::ensure_requested(
                    provider.provider,
                    &base_url,
                    &provider.api_key,
                );
            }
        }
        let catalog = match settings.default_ai_provider {
            AiProvider::OpenAI
                if settings.agent_openai_use_plan && !raf_ai::chatgpt_auth::active_connected() =>
            {
                None
            }
            AiProvider::OpenAI if settings.agent_openai_use_plan => raf_ai::chatgpt_auth::snapshot()
                .map(|view| (view.busy, format!("{:?}{:?}{}", view.models, view.message, view.busy))),
            AiProvider::OpenRouter
            | AiProvider::OpenAI
            | AiProvider::OpenCode
            | AiProvider::OpenCodeGo => {
                let view = self.effective_provider(settings).map(|p| raf_ai::provider_catalog::snapshot_for(p.provider, &p.base_url, &p.api_key)).unwrap_or_default();
                Some((view.loading, format!("{:?}{:?}{}{}", view.models, view.error, view.loading, view.fetched)))
            }
            _ => Some((false, String::new())),
        };
        if let Some((pending, fingerprint)) = catalog {
            self.catalog_pending = pending;
            if self.catalog_fingerprint != fingerprint {
                self.catalog_fingerprint = fingerprint;
                self.visual_revision = self.visual_revision.wrapping_add(1);
            }
        } else if self.catalog_pending {
            self.catalog_pending = false;
            self.catalog_fingerprint.clear();
            self.visual_revision = self.visual_revision.wrapping_add(1);
        }
        self.external_studio_enabled = settings.external_asset_studio.enabled;
        self.ensure_project_history(project);
        self.poll_media();
        self.ensure_runtime_config(settings);
        if let Some(session) = self.history.active_session() {
            self.runtime.client.set_session_id(&session.id);
        }
        self.sync_execution_permissions(settings.agent_mode);
        self.readiness(settings)
    }

    /// Active is the user's authorization for native tool execution. Review
    /// preferences cannot override it, including in parked conversations.
    pub(crate) fn sync_execution_permissions(&mut self, mode: AgentMode) {
        let before = self.runtime.status.clone();
        self.runtime
            .set_review_mutations(self.review_writes && mode != AgentMode::Active);
        let mut changed = before != self.runtime.status;
        for run in self.parked_runs.values_mut() {
            let before = run.runtime.status.clone();
            run.runtime
                .set_review_mutations(run.review_writes && mode != AgentMode::Active);
            changed |= before != run.runtime.status;
        }
        if changed {
            self.visual_revision = self.visual_revision.wrapping_add(1);
        }
    }

    /// Poll the non-blocking runtime once per editor frame.
    pub(crate) fn poll(&mut self, executor: &mut dyn ToolExecutor) {
        let previous_status = self.runtime.status.clone();
        let previous_message_count = self.runtime.messages.len();
        let previous_task = self.runtime.task_snapshot();
        let status = self.runtime.poll(Some(executor));
        let message_revision = self.runtime.message_revision();
        let elapsed_second = self
            .runtime
            .activity_snapshot()
            .map(|activity| activity.elapsed_seconds);
        let elapsed_tick = elapsed_second != self.last_visual_elapsed_second;
        let structural_change = previous_status != self.runtime.status
            || previous_message_count != self.runtime.messages.len()
            || previous_task != self.runtime.task_snapshot();
        let stream_update_ready = message_revision != self.last_visual_message_revision
            && self.last_visual_present.elapsed() >= MIN_AGENT_STREAM_PRESENTATION;
        if structural_change || stream_update_ready || elapsed_tick {
            self.visual_revision = self.visual_revision.wrapping_add(1);
            self.last_visual_message_revision = message_revision;
            self.last_visual_present = Instant::now();
        }
        self.last_visual_elapsed_second = elapsed_second;
        if status != self.last_status {
            self.last_status = status;
            self.persist_history();
        } else if self.runtime.messages.len() != self.last_saved_message_count
            || (message_revision != self.last_history_message_revision
                && self.last_history_write.elapsed() >= Duration::from_secs(1))
        {
            self.persist_history();
        }
    }

    pub(crate) fn visual_revision(&self) -> u64 {
        self.visual_revision
    }

    pub(crate) fn has_live_output(&self) -> bool {
        self.media_job.is_some()
            || self.catalog_pending
            || self.opening
            || self.pending_submission.is_some()
            || self.decision.is_some()
            || (self.save_error.is_none() && self.runtime.status.needs_continuous_frame())
            || self.parked_runs.values().any(|run| {
                run.opening
                    || run.submission.is_some()
                    || run.decision.is_some()
                    || (run.save_error.is_none() && run.runtime.status.needs_continuous_frame())
            })
    }

    pub(crate) fn has_pending_submission(&self) -> bool {
        self.pending_submission.is_some()
    }

    pub(crate) fn current_needs_poll(&self) -> bool {
        self.media_job.is_some()
            || self.opening
            || self.pending_submission.is_some()
            || self.decision.is_some()
            || (self.save_error.is_none() && self.runtime.status.needs_continuous_frame())
    }

    pub(crate) fn should_show_tool_call_warning(&self) -> bool {
        if self.tool_call_warning_dismissed
            || !matches!(
                self.runtime.status,
                AgentStatus::Thinking | AgentStatus::ExecutingTools
            )
        {
            return false;
        }
        self.runtime.activity_snapshot().is_some_and(|activity| {
            activity.completed_tools >= raf_core::config::AGENT_TOOL_CALL_WARNING_THRESHOLD as usize
        })
    }

    pub(crate) fn dismiss_tool_call_warning(&mut self) {
        self.tool_call_warning_dismissed = true;
        self.visual_revision = self.visual_revision.wrapping_add(1);
    }

    pub(crate) fn readiness(&self, settings: &EngineSettings) -> AgentReadiness {
        let Some(provider) = self.effective_provider(settings) else {
            return AgentReadiness::ProviderDisabled;
        };
        if !provider.enabled {
            return AgentReadiness::ProviderDisabled;
        }
        if provider.provider == AiProvider::OpenAI && settings.agent_openai_use_plan
            && !raf_ai::chatgpt_auth::active_connected() {
            return AgentReadiness::AdapterRequired;
        }
        let model = self.effective_model(provider);
        if model.trim().is_empty() {
            return AgentReadiness::ModelMissing;
        }
        if provider.provider == AiProvider::OpenAI && settings.agent_openai_use_plan
            && !crate::panels::ai_model_options::selection_allowed(settings, provider.provider, &format!("account:{model}")) {
            return AgentReadiness::ModelMissing;
        }
        if provider.base_url.trim().is_empty() && !(provider.provider == AiProvider::OpenAI && settings.agent_openai_use_plan) {
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
        provider.model.clone()
    }

    /// Automatic image-input policy. Only a model family known to be text-only
    /// blocks the attachment; unknown models are attempted and the provider
    /// remains the authority.
    pub(crate) fn vision_support(&self, settings: &EngineSettings) -> VisionSupport {
        self.effective_provider(settings)
            .map(|provider| vision_support(&self.effective_model(provider)))
            .unwrap_or(VisionSupport::Unknown)
    }

    fn images_allowed(&self, settings: &EngineSettings) -> bool {
        self.vision_support(settings).allows_attempt()
    }

    pub(crate) fn apply_action(&mut self, action: AgentAction, settings: &mut EngineSettings) {
        match action {
            AgentAction::PasteImage => self.paste_image(settings),
            AgentAction::RefreshModelCatalog => {
                match settings.default_ai_provider {
                    AiProvider::OpenAI if settings.agent_openai_use_plan => { let _ = raf_ai::chatgpt_auth::begin_model_refresh(); }
                    AiProvider::OpenRouter | AiProvider::OpenAI | AiProvider::OpenCode | AiProvider::OpenCodeGo => {
                        if let Some(provider) = self.effective_provider(settings) {
                            let base_url = match provider.provider {
                                AiProvider::OpenRouter | AiProvider::OpenAI => provider.base_url.clone(),
                                _ => String::new(),
                            };
                            raf_ai::provider_catalog::refresh(provider.provider, &base_url, &provider.api_key);
                        }
                    }
                    _ => {}
                }
                self.visual_revision = self.visual_revision.wrapping_add(1);
            }
            AgentAction::OpenPlanUsage => { crate::script_support::open_url("https://chatgpt.com/settings/usage"); }
            AgentAction::AttachFiles(files) => self.attach_files(files, settings),
            AgentAction::CycleReasoning => {
                if !self.runtime.status.blocks_input() {
                    settings.agent_reasoning_effort =
                        match settings.agent_reasoning_effort.as_str() {
                            "" => "low",
                            "low" => "medium",
                            "medium" => "high",
                            "high" => "max",
                            _ => "",
                        }
                        .into();
                    self.settings_changed = true;
                    self.visual_revision = self.visual_revision.wrapping_add(1);
                }
            }
            AgentAction::RemoveImage(file) => {
                if let Some(session) = self.history.active_session_mut() {
                    session.draft_images.retain(|image| image.file != file);
                }
                self.enqueue_history_snapshot();
                self.visual_revision = self.visual_revision.wrapping_add(1);
            }
            AgentAction::SetInput(value) => self.input_text = value,
            AgentAction::ToggleSidebar => self.sidebar_open = !self.sidebar_open,
            AgentAction::CloseSidebar => self.sidebar_open = false,
            AgentAction::NewChat => self.start_new_chat(),
            AgentAction::SelectSession(index) => self.select_session(index),
            AgentAction::DeleteSession(index) => self.delete_session(index),
            AgentAction::SelectModel(value) => {
                // One truth: every selection writes the provider card model.
                let (prefix, model_id) = match value
                    .strip_prefix("account:")
                    .map(|slug| ("account", slug.to_string()))
                    .or_else(|| {
                        value
                            .strip_prefix("catalog:")
                            .map(|slug| ("catalog", slug.to_string()))
                    }) {
                    Some(pair) => pair,
                    None => ("card", value.trim().to_string()),
                };
                if model_id.trim().is_empty() {
                    self.model_menu_open = false;
                    return;
                }
                let target_provider = if prefix == "account" {
                    AiProvider::OpenAI
                } else {
                    settings.default_ai_provider
                };
                let validated_model_id = if prefix == "account" {
                    let available = raf_ai::chatgpt_auth::snapshot().is_some_and(|view| {
                        view.models.iter().any(|model| model.slug == model_id)
                    });
                    if available {
                        Some(model_id)
                    } else {
                        None
                    }
                } else if !crate::panels::ai_model_options::selection_allowed(settings, target_provider, &model_id) {
                    None
                } else {
                    Some(model_id)
                };
                if let Some(model_id) = validated_model_id {
                    if prefix == "account" { settings.agent_openai_use_plan = true; }
                    let (mut changed, mut provider_switched) = (false, false);
                    if let Some(config) = settings
                        .ai_providers
                        .iter_mut()
                        .find(|config| config.provider == target_provider)
                    {
                        if config.model.trim() != model_id {
                            config.model = model_id.clone();
                            changed = true;
                        }
                    }
                    if settings.default_ai_provider != target_provider {
                        settings.default_ai_provider = target_provider;
                        provider_switched = true;
                    }
                    if changed || provider_switched {
                        settings.agent_vision_enabled = false;
                        settings.agent_reasoning_effort.clear();
                    }
                    self.selected_model = MODEL_SENTINEL.to_string();
                    self.settings_changed = true;
                    self.visual_revision = self.visual_revision.wrapping_add(1);
                }
                self.model_menu_open = false;
            }
            AgentAction::PinModel(value) => {
                let model_id = value
                    .strip_prefix("account:")
                    .map(str::to_string)
                    .or_else(|| value.strip_prefix("catalog:").map(str::to_string))
                    .unwrap_or_else(|| value.trim().to_string());
                let pin_changed = settings
                    .ai_providers
                    .iter_mut()
                    .find(|config| config.provider == settings.default_ai_provider)
                    .is_some_and(|config| {
                        if !crate::panels::ai_model_options::pin_allowed(config) {
                            return false;
                        }
                        if let Some(position) = config
                            .pinned_models
                            .iter()
                            .position(|pinned| pinned == &model_id)
                        {
                            config.pinned_models.remove(position);
                        } else if !model_id.trim().is_empty()
                            && !config.pinned_models.contains(&model_id)
                        {
                            config.pinned_models.push(model_id.clone());
                        } else {
                            return false;
                        }
                        true
                    });
                if pin_changed {
                    self.settings_changed = true;
                }
                self.visual_revision = self.visual_revision.wrapping_add(1);
            }
            AgentAction::ToggleModelMenu => {
                self.model_menu_open = !self.model_menu_open;
                self.mode_menu_open = false;
                self.add_model_open = false;
            }
            AgentAction::ToggleModeMenu => {
                self.mode_menu_open = !self.mode_menu_open;
                self.model_menu_open = false;
                self.add_model_open = false;
            }
            AgentAction::CloseMenus => {
                self.model_menu_open = false;
                self.mode_menu_open = false;
                self.add_model_open = false;
            }
            AgentAction::SetMode(mode) => {
                if settings.agent_mode != mode {
                    settings.agent_mode = mode;
                    self.settings_changed = true;
                }
                self.mode_menu_open = false;
                self.sync_execution_permissions(mode);
            }
            AgentAction::SetNewModelId(value) => {
                self.new_model_id = value;
                self.new_model_error = None;
            }
            AgentAction::AddModel => {
                // The single-field custom id confirms into the card model and
                // pins it, so "easier adding" is literally one field.
                let model_id = self.new_model_id.trim().to_string();
                if model_id.is_empty() {
                    self.new_model_error =
                        Some(t("app.agent_add_model_missing_id", self.language));
                    self.visual_revision = self.visual_revision.wrapping_add(1);
                    return;
                }
                let provider_changed = settings
                    .ai_providers
                    .iter_mut()
                    .find(|config| config.provider == settings.default_ai_provider)
                    .unwrap()
                    .model
                    .trim()
                    != model_id;
                if let Some(config) = settings
                    .ai_providers
                    .iter_mut()
                    .find(|config| config.provider == settings.default_ai_provider)
                {
                    config.model = model_id.clone();
                    if !config.pinned_models.contains(&model_id) {
                        if config.pinned_models.len() < 32 {
                            config.pinned_models.push(model_id.clone());
                        }
                    }
                }
                if provider_changed {
                    settings.agent_vision_enabled = false;
                    settings.agent_reasoning_effort.clear();
                }
                self.selected_model = MODEL_SENTINEL.to_string();
                self.settings_changed = true;
                self.new_model_id.clear();
                self.new_model_error = None;
                self.model_menu_open = false;
                self.visual_revision = self.visual_revision.wrapping_add(1);
            }
            AgentAction::OpenSettings => self.open_settings_requested = true,
            AgentAction::DismissToolCallWarning => self.dismiss_tool_call_warning(),
            AgentAction::Submit => {
                self.queue_submission(settings);
            }
            AgentAction::Resume => {
                self.input_text = t("agent_workspace.resume_prompt", self.language);
                self.queue_submission(settings);
            }
            AgentAction::Stop => {
                self.runtime.cancel();
                self.last_status = AgentStatus::Done;
                self.persist_history();
            }
            AgentAction::Approve => {
                self.decision = Some(AgentDecision::ApproveAll);
            }
            AgentAction::Deny => {
                self.decision = Some(AgentDecision::DenyAll);
            }
            AgentAction::ApproveCall(id) => self.decision = Some(AgentDecision::Approve(id)),
            AgentAction::DenyCall(id) => self.decision = Some(AgentDecision::Deny(id)),
            AgentAction::ToggleReview => {
                if settings.agent_mode != AgentMode::Active {
                    self.review_writes = !self.review_writes;
                    self.sync_execution_permissions(settings.agent_mode);
                }
            }
            AgentAction::ToggleTool(id) => {
                if !self.expanded_tools.remove(&id) {
                    self.expanded_tools.insert(id);
                }
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

    pub(crate) fn take_settings_changed(&mut self) -> bool {
        std::mem::take(&mut self.settings_changed)
    }

    pub(crate) fn close_menus(&mut self) {
        self.model_menu_open = false;
        self.mode_menu_open = false;
        self.add_model_open = false;
        self.new_model_error = None;
    }

    pub(crate) fn has_open_menu(&self) -> bool {
        self.model_menu_open || self.mode_menu_open || self.add_model_open
    }

    pub(crate) fn media_pending(&self) -> bool {
        self.media_job.is_some()
    }

    fn paste_image(&mut self, settings: &EngineSettings) {
        if self.media_job.is_some()
            || self.pending_submission.is_some()
            || self.runtime.status.blocks_input()
        {
            return;
        }
        if !self.images_allowed(settings) {
            self.media_error = Some(t("agent.media.model_text_only", self.language));
            self.visual_revision = self.visual_revision.wrapping_add(1);
            return;
        }
        let Some(root) = self.loaded_project_path.clone() else {
            return;
        };
        let Some(session) = self.history.active_session() else {
            return;
        };
        if session.draft_images.len() >= raf_ai::chat_media::MAX_IMAGES {
            self.media_error = Some(t("agent.media.too_many", self.language));
            self.visual_revision = self.visual_revision.wrapping_add(1);
            return;
        }
        self.media_error = None;
        self.media_job = Some((session.id.clone(), crate::agent_media::clipboard_job(root)));
        self.visual_revision = self.visual_revision.wrapping_add(1);
    }

    fn attach_files(&mut self, files: Vec<std::path::PathBuf>, settings: &EngineSettings) {
        if files.is_empty()
            || self.media_job.is_some()
            || self.pending_submission.is_some()
            || self.runtime.status.blocks_input()
        {
            return;
        }
        let Some(session) = self.history.active_session() else {
            return;
        };
        let Some(root) = self.loaded_project_path.clone() else {
            return;
        };
        let error = if !self.images_allowed(settings) {
            Some("agent.media.model_text_only")
        } else if files.len() + session.draft_images.len() > raf_ai::chat_media::MAX_IMAGES {
            Some("agent.media.too_many")
        } else {
            None
        };
        if let Some(key) = error {
            self.media_error = Some(t(key, self.language));
        } else {
            self.media_error = None;
            self.media_job = Some((
                session.id.clone(),
                crate::agent_media::files_job(root, files),
            ));
        }
        self.visual_revision = self.visual_revision.wrapping_add(1);
    }

    fn poll_media(&mut self) {
        let result = self
            .media_job
            .as_ref()
            .and_then(|(_, rx)| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(_) => Some(vec![Err("agent.media.save_failed".into())]),
            });
        if let Some(result) = result {
            let (id, _) = self.media_job.take().unwrap();
            for result in result {
                match result {
                    Ok(image) => {
                        if let Some(session) = self.history.sessions.iter_mut().find(|s| s.id == id)
                        {
                            session.draft_images.push(image);
                        }
                        self.enqueue_history_snapshot();
                    }
                    Err(key) => self.media_error = Some(t(&key, self.language)),
                }
            }
            self.visual_revision = self.visual_revision.wrapping_add(1);
        }
    }

    fn queue_submission(&mut self, settings: &EngineSettings) {
        if !self.images_allowed(settings)
            && self
                .history
                .active_session()
                .is_some_and(|s| !s.draft_images.is_empty())
        {
            self.media_error = Some(t("agent.media.model_text_only", self.language));
            self.visual_revision = self.visual_revision.wrapping_add(1);
            return;
        }
        let content = self.input_text.trim().to_string();
        let has_images = self
            .history
            .active_session_mut()
            .is_some_and(|s| !s.draft_images.is_empty());
        if (content.is_empty() && !has_images)
            || self.runtime.status.blocks_input()
            || self.media_job.is_some()
        {
            return;
        }
        self.input_text.clear();
        self.media_error = None;
        self.pending_submission = Some(content);
        self.visual_revision = self.visual_revision.wrapping_add(1);
    }

    pub(crate) fn start_pending_run(
        &mut self,
        snapshot: &AgentProjectSnapshot,
        catalog: &CommandCatalog,
        project_type: Option<ProjectType>,
        mode: AgentMode,
        _viewport_available: bool,
    ) {
        let Some(content) = self.pending_submission.take() else {
            return;
        };
        let continuation = ["continue", "continua", "continúa", "sigue", "termina"]
            .iter()
            .any(|word| content.trim().to_lowercase().starts_with(word));
        let asset_context = continuation
            && self.tools.iter().any(|tool| {
                tool.function.name.starts_with("asset_generate_")
                    || tool.function.name.starts_with("external_studio_")
            });
        let tool_prompt = if asset_context {
            format!("{content} assets image")
        } else {
            content.clone()
        };
        let mut pack = AgentToolExecutor::build_tool_pack(
            catalog,
            self.language,
            project_type,
            mode,
            &tool_prompt,
        );
        if !self.external_studio_enabled {
            pack.tools
                .retain(|tool| !tool.function.name.starts_with("external_studio_"));
            pack.routes
                .retain(|name, _| !name.starts_with("external_studio_"));
        }
        self.tools = pack.tools;
        self.tool_routes = pack.routes;
        self.system_prompt = build_agent_system_prompt(snapshot, mode.label(), self.language);
        if let Some(parent) = &self.authoring_parent {
            self.system_prompt.push_str(&format!("\nAuthoring insertion scope: {}. Use this parent for new objects unless the user specifies another parent.", serde_json::json!({ "path": parent })));
        }
        self.runtime.set_system_prompt(self.system_prompt.clone());
        self.runtime.set_initial_tool_context(
            "project_summary",
            serde_json::json!({}),
            AgentToolResult::success(
                t("app.agent_context_loaded", self.language),
                serde_json::to_value(snapshot).unwrap_or(serde_json::Value::Null),
            ),
        );
        let execution_mode = match mode {
            AgentMode::Inspect => ToolExecutionMode::Inspect,
            AgentMode::Plan => ToolExecutionMode::Preview,
            AgentMode::Active => ToolExecutionMode::Apply,
        };
        let images = self
            .history
            .active_session_mut()
            .map(|s| std::mem::take(&mut s.draft_images))
            .unwrap_or_default();
        self.runtime.media_root = self.loaded_project_path.clone();
        self.runtime
            .start_run_with_images(&content, images, &self.tools, execution_mode);
        self.sync_execution_permissions(mode);
        self.tool_call_warning_dismissed = false;
        self.last_status = AgentStatus::Thinking;
        self.persist_history();
    }

    fn ensure_project_history(&mut self, project: Option<&Project>) {
        let next_path = project.map(|project| project.path.clone());
        if self.loaded_project_path == next_path {
            if self.history.active_index.is_none() && project.is_some() {
                let default_title = t("app.agent_new_chat_default", self.language);
                let index = self.history.ensure_active_session(&default_title);
                self.load_session_into_runtime(index);
                self.enqueue_history_snapshot();
            }
            return;
        }
        self.runtime.cancel();
        self.media_job = None;
        self.media_error = None;
        self.loaded_project_path = next_path.clone();
        self.history = next_path
            .as_deref()
            .map(AgentHistory::load)
            .unwrap_or_default();
        let had_valid_active_session = self
            .history
            .active_index
            .is_some_and(|index| index < self.history.sessions.len());
        let default_title = t("app.agent_new_chat_default", self.language);
        self.history.ensure_active_session(&default_title);
        if let Some(index) = self.history.active_index {
            self.load_session_into_runtime(index);
            if !had_valid_active_session {
                self.enqueue_history_snapshot();
            }
        }
    }

    fn ensure_runtime_config(&mut self, settings: &EngineSettings) {
        // A settings change applies to the next turn, never interrupts work.
        if self.runtime.status.blocks_input() {
            return;
        }
        let Some(provider) = self.effective_provider(settings).cloned() else {
            return;
        };
        let key = RuntimeKey {
            provider: provider.provider,
            base_url: provider.base_url.clone(),
            model: self.effective_model(&provider),
            api_key: provider.api_key.clone(),
            streaming: settings.agent_streaming_enabled,
            max_tokens: if settings.agent_response_limit_enabled {
                settings.agent_max_response_tokens
            } else {
                0
            },
            reasoning_effort: settings.agent_reasoning_effort.clone(),
            chatgpt_plan: provider.provider == AiProvider::OpenAI && settings.agent_openai_use_plan,
            chatgpt_registration: if provider.provider == AiProvider::OpenAI && settings.agent_openai_use_plan {
                raf_ai::chatgpt_auth::active_registration().or_else(|| self.runtime_key.as_ref().and_then(|key| key.chatgpt_registration.clone()))
            } else { None },
            tool_call_limit_enabled: settings.agent_tool_call_limit_enabled,
            max_tool_calls: settings.agent_max_tool_calls,
        };
        if self.runtime_key.as_ref() == Some(&key) {
            return;
        }
        let messages = self.runtime.messages.clone();
        self.runtime.cancel();
        self.runtime = AgentRuntime::new(OpenAiConfig {
            base_url: if key.chatgpt_plan { "https://api.openai.com/v1".into() } else { key.base_url.clone() },
            model: key.model.clone(),
            api_key: if key.chatgpt_plan { String::new() } else { key.api_key.clone() },
            max_tokens: key.max_tokens,
            reasoning_effort: key.reasoning_effort.clone(),
            chatgpt_plan: key.chatgpt_plan,
            chatgpt_registration: key.chatgpt_registration.clone(),
            temperature: None,
            streaming: key.streaming,
        });
        self.runtime.messages = messages;
        self.runtime.set_run_limits(
            128,
            key.tool_call_limit_enabled
                .then_some(key.max_tool_calls as usize),
            16 * 1024,
        );
        self.runtime.set_system_prompt(self.system_prompt.clone());
        self.runtime_key = Some(key);
        self.visual_revision = self.visual_revision.wrapping_add(1);
    }

    fn start_new_chat(&mut self) {
        self.media_error = None;
        self.park_current_run();
        self.tool_call_warning_dismissed = false;
        let default_title = t("app.agent_new_chat_default", self.language);
        let index = self.history.start_session(&default_title);
        self.loaded_conversation = Some(self.history.sessions[index].id.clone());
        self.runtime.set_system_prompt(self.system_prompt.clone());
        self.input_text.clear();
        self.last_saved_message_count = 0;
        self.opening = true;
        self.context_requested = false;
        self.document_name.clear();
        self.saved_revision = None;
        self.save_error = None;
        self.close_menus();
        self.saving = false;
        self.last_status = AgentStatus::Done;
        self.enqueue_history_snapshot();
    }

    fn select_session(&mut self, index: usize) {
        self.media_error = None;
        if index >= self.history.sessions.len() {
            return;
        }
        self.park_current_run();
        self.history.active_index = Some(index);
        self.load_session_into_runtime(index);
        self.enqueue_history_snapshot();
    }

    fn load_session_into_runtime(&mut self, index: usize) {
        let Some(id) = self
            .history
            .sessions
            .get(index)
            .map(|session| session.id.clone())
        else {
            return;
        };
        if let Some(run) = self.parked_runs.remove(&id) {
            self.runtime = run.runtime;
            self.tools = run.tools;
            self.tool_routes = run.routes;
            self.system_prompt = run.prompt;
            self.input_text = run.draft;
            self.pending_submission = run.submission;
            self.runtime_key = run.runtime_key;
            self.selected_model = run.selected_model;
            self.decision = run.decision;
            self.last_status = run.status;
            self.review_writes = run.review_writes;
            self.opening = run.opening;
            self.context_requested = run.context_requested;
            self.document_name = run.document_name;
            self.saved_revision = run.saved_revision;
            self.save_error = run.save_error;
            self.saving = run.saving;
            self.loaded_conversation = Some(id);
            self.visual_revision = self.visual_revision.wrapping_add(1);
            return;
        }
        let messages = self
            .history
            .sessions
            .get(index)
            .map(|session| session.messages.clone())
            .unwrap_or_default();
        self.runtime = AgentRuntime::new(self.runtime.client.config.clone());
        self.tool_call_warning_dismissed = false;
        self.runtime.messages = messages;
        self.runtime.set_system_prompt(self.system_prompt.clone());
        self.input_text = self.history.sessions[index].draft.clone();
        self.last_status = AgentStatus::Done;
        self.last_saved_message_count = self.runtime.messages.len();
        self.visual_revision = self.visual_revision.wrapping_add(1);
        self.loaded_conversation = Some(id);
        self.opening = true;
        self.context_requested = false;
        self.runtime_key = None;
        self.document_name.clear();
        self.saved_revision = self.history.sessions[index].saved_revision;
        self.save_error = None;
        self.close_menus();
        self.saving = false;
    }

    fn delete_session(&mut self, index: usize) {
        if index >= self.history.sessions.len() {
            return;
        }
        if self.session_is_running(index) {
            return;
        }
        self.park_current_run();
        self.parked_runs.remove(&self.history.sessions[index].id);
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
            return;
        }
        self.enqueue_history_snapshot();
    }

    pub(crate) fn persist_history(&mut self) {
        let Some(index) = self.history.active_index else {
            return;
        };
        let Some(session) = self.history.sessions.get_mut(index) else {
            return;
        };
        let title = self
            .runtime
            .messages
            .iter()
            .find(|message| message.role == raf_ai::chat::MessageRole::User)
            .map(|message| {
                if message.content.trim().is_empty() && !message.images.is_empty() {
                    t("agent.media.conversation", self.language)
                } else {
                    fallback_session_title(&message.content)
                }
            });
        let title_changed = title.as_deref().is_some_and(|title| {
            is_default_session_title(&session.title) && session.title != title
        });
        if title_changed {
            if let Some(title) = title {
                session.title = title;
            }
        }
        let messages_changed = !messages_equal(&session.messages, &self.runtime.messages);
        let task = self
            .runtime
            .task_snapshot()
            .or_else(|| session.last_task.clone());
        let task_changed = task != session.last_task;
        let receipt_changed = session.saved_revision != self.saved_revision;
        let draft_changed = session.draft != self.input_text;
        if !messages_changed
            && !title_changed
            && !task_changed
            && !receipt_changed
            && !draft_changed
        {
            return;
        }
        if draft_changed {
            session.draft = self.input_text.clone();
        }
        session.saved_revision = self.saved_revision;
        if task_changed {
            session.last_task = task;
            session.task_events = self.runtime.task_events_since(0);
            session.saved_revision = self.saved_revision;
        }
        if messages_changed {
            session.messages = self.runtime.messages.clone();
            session.touch();
        }
        self.last_saved_message_count = self.runtime.messages.len();
        self.last_history_write = Instant::now();
        self.last_history_message_revision = self.runtime.message_revision();
        self.enqueue_history_snapshot();
    }

    fn enqueue_history_snapshot(&self) {
        if let Some(project_path) = self.loaded_project_path.as_deref() {
            if let Some(index) = self.visiting_selected {
                let mut snapshot = self.history.clone();
                snapshot.active_index = Some(index);
                self.history_writer.enqueue(project_path, &snapshot);
            } else {
                self.history_writer.enqueue(project_path, &self.history);
            }
        }
    }

    fn park_current_run(&mut self) {
        self.persist_history();
        let Some(id) = self.loaded_conversation.take() else {
            return;
        };
        let config = self.runtime.client.config.clone();
        let run = ConversationRun {
            runtime: std::mem::replace(&mut self.runtime, AgentRuntime::new(config)),
            tools: std::mem::take(&mut self.tools),
            routes: std::mem::take(&mut self.tool_routes),
            prompt: self.system_prompt.clone(),
            draft: std::mem::take(&mut self.input_text),
            submission: self.pending_submission.take(),
            runtime_key: self.runtime_key.take(),
            selected_model: self.selected_model.clone(),
            decision: self.decision.take(),
            status: self.last_status.clone(),
            review_writes: self.review_writes,
            opening: self.opening,
            context_requested: self.context_requested,
            document_name: self.document_name.clone(),
            saved_revision: self.saved_revision,
            save_error: self.save_error.clone(),
            saving: self.saving,
        };
        self.parked_runs.insert(id, run);
    }

    /// Visit live conversations without changing the user's selected chat.
    pub(crate) fn visit_runs(&mut self, mut poll: impl FnMut(&mut Self)) {
        let selected = self.history.active_index;
        self.visiting_selected = selected;
        let indices: Vec<_> = self
            .history
            .sessions
            .iter()
            .enumerate()
            .filter_map(|(index, session)| {
                let running = self.parked_runs.get(&session.id).is_some_and(|run| {
                    run.opening
                        || run.runtime.status.blocks_input()
                        || run.submission.is_some()
                        || run.decision.is_some()
                });
                (selected == Some(index) || running).then_some(index)
            })
            .collect();
        for index in indices {
            if self.history.active_index != Some(index) {
                self.park_current_run();
                self.history.active_index = Some(index);
                self.load_session_into_runtime(index);
            }
            poll(self);
        }
        if let Some(index) = selected {
            if self.history.active_index != Some(index) {
                self.park_current_run();
                self.history.active_index = Some(index);
                self.load_session_into_runtime(index);
            }
        }
        self.visiting_selected = None;
    }

    pub(crate) fn pending_calls(&self) -> Vec<raf_ai::agent_runtime::PendingToolCall> {
        self.runtime
            .pending_calls
            .iter()
            .chain(
                self.parked_runs
                    .values()
                    .flat_map(|run| run.runtime.pending_calls.iter()),
            )
            .cloned()
            .collect()
    }

    pub(crate) fn session_is_running(&self, index: usize) -> bool {
        if self.history.active_index == Some(index) {
            return self.runtime.status.blocks_input();
        }
        self.history
            .sessions
            .get(index)
            .and_then(|s| self.parked_runs.get(&s.id))
            .is_some_and(|run| run.runtime.status.blocks_input())
    }

    pub(crate) fn initialize(&mut self, project: &Project, history: AgentHistory) {
        self.loaded_project_path = Some(project.path.clone());
        self.history = history;
        let index = self
            .history
            .ensure_active_session(t("app.agent_new_chat_default", self.language));
        self.load_session_into_runtime(index);
    }

    pub(crate) fn has_pending_review(&self) -> bool {
        self.runtime.status == AgentStatus::AwaitingApproval
            || self
                .parked_runs
                .values()
                .any(|run| run.runtime.status == AgentStatus::AwaitingApproval)
    }

    pub(crate) fn document_id(&self) -> Option<raf_core::session::SessionId> {
        self.history
            .active_session()
            .and_then(|s| s.document_id)
            .map(raf_core::session::SessionId)
    }

    pub(crate) fn bind_document(&mut self, id: raf_core::session::SessionId, name: &str) {
        if let Some(session) = self.history.active_session_mut() {
            if session.document_id.is_none() {
                session.document_id = Some(id.0);
                self.enqueue_history_snapshot();
            }
        }
        if self.opening || self.document_name != name {
            self.visual_revision = self.visual_revision.wrapping_add(1);
        }
        self.document_name = name.to_string();
        self.opening = false;
    }

    pub(crate) fn set_save_state(
        &mut self,
        revision: Option<u64>,
        error: Option<String>,
        saving: bool,
    ) {
        if self.saved_revision != revision || self.save_error != error || self.saving != saving {
            self.visual_revision = self.visual_revision.wrapping_add(1);
            self.saved_revision = revision;
            self.save_error = error;
            self.saving = saving;
        }
    }

    pub(crate) fn defer_context_open(&mut self) -> bool {
        if self.opening && !self.context_requested {
            self.context_requested = true;
            return true;
        }
        false
    }

    pub(crate) fn cancel_all(&mut self) {
        self.visit_runs(|panel| {
            panel.pending_submission = None;
            panel.decision = None;
            panel.opening = false;
            panel.runtime.cancel();
            panel.persist_history();
        });
    }

    pub(crate) fn finish_history_writes(&mut self) -> Result<bool, String> {
        self.history_writer.finish_if_idle()
    }

    pub(crate) fn retry_history_writes(&mut self) {
        self.history_writer = AgentHistoryWriter::new();
        self.enqueue_history_snapshot();
    }
}

fn is_default_session_title(title: &str) -> bool {
    let normalized = title.trim().to_ascii_lowercase();
    normalized.is_empty()
        || normalized == "new chat"
        || normalized == "nueva conversacion"
        || [Language::English, Language::Spanish]
            .iter()
            .any(|language| {
                t("app.agent_new_chat_default", *language).to_ascii_lowercase() == normalized
            })
}

fn fallback_session_title(content: &str) -> String {
    const MAX_TITLE_CHARS: usize = 42;
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title = normalized.chars().take(MAX_TITLE_CHARS).collect::<String>();
    if normalized.chars().count() > MAX_TITLE_CHARS {
        title.push('…');
    }
    if title.is_empty() {
        "New chat".to_string()
    } else {
        title
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub(crate) enum AgentAction {
    AttachFiles(Vec<std::path::PathBuf>),
    RefreshModelCatalog,
    OpenPlanUsage,
    CycleReasoning,
    PasteImage,
    RemoveImage(String),
    SetInput(String),
    ToggleSidebar,
    CloseSidebar,
    NewChat,
    SelectSession(usize),
    DeleteSession(usize),
    SelectModel(String),
    PinModel(String),
    ToggleModelMenu,
    ToggleModeMenu,
    CloseMenus,
    SetMode(AgentMode),
    SetNewModelId(String),
    AddModel,
    OpenSettings,
    DismissToolCallWarning,
    Submit,
    Resume,
    Stop,
    Approve,
    Deny,
    ApproveCall(String),
    DenyCall(String),
    ToggleReview,
    ToggleTool(String),
    UseSuggestion(String),
}

fn messages_equal(left: &[ChatMessage], right: &[ChatMessage]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.id == right.id
                && left.role == right.role
                && left.content == right.content
                && left.images == right.images
                && left.reasoning == right.reasoning
                && left.tool_calls == right.tool_calls
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_ai::chat::MessageRole;

    #[test]
    fn harness_attachment_completion_targets_the_original_conversation() {
        let mut panel = AgentPanel::default();
        let first = panel.history.start_session("First");
        let first_id = panel.history.sessions[first].id.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        panel.media_job = Some((first_id, rx));
        let second = panel.history.start_session("Second");
        tx.send(vec![Ok(raf_ai::chat_media::ChatImage {
            file: "fixture.png".into(),
            width: 16,
            height: 16,
        })])
        .unwrap();
        panel.poll_media();
        assert_eq!(panel.history.sessions[first].draft_images.len(), 1);
        assert!(panel.history.sessions[second].draft_images.is_empty());
        assert!(!panel.media_pending());
    }

    #[test]
    fn harness_image_only_submit_uses_automatic_model_capability() {
        let mut panel = AgentPanel::default();
        panel.history.start_session("Images");
        panel
            .history
            .active_session_mut()
            .unwrap()
            .draft_images
            .push(raf_ai::chat_media::ChatImage {
                file: "fixture.png".into(),
                width: 16,
                height: 16,
            });
        let mut settings = EngineSettings::default();
        settings.ai_providers[0].model = "unknown-local-model".into();
        settings.agent_vision_enabled = false;
        panel.apply_action(AgentAction::Submit, &mut settings);
        assert_eq!(panel.pending_submission.as_deref(), Some(""));
        panel.pending_submission = None;

        settings.ai_providers[0].model = "deepseek-chat".into();
        panel.apply_action(AgentAction::Submit, &mut settings);
        assert!(panel.pending_submission.is_none());
        assert!(panel.media_error.is_some());
    }

    #[test]
    fn agent_workspace_navigation_preserves_runs_drafts_and_document_binding() {
        let mut panel = AgentPanel::default();
        let first = panel.history.start_session("First");
        panel.load_session_into_runtime(first);
        let first_document = raf_core::session::SessionId::new();
        panel.bind_document(first_document, "World A");
        panel
            .runtime
            .messages
            .push(ChatMessage::user("Create a cube"));
        panel.runtime.status = AgentStatus::ExecutingTools;
        panel
            .runtime
            .pending_calls
            .push(raf_ai::agent_runtime::PendingToolCall::new(
                "operation",
                "scene_build",
                serde_json::json!({}),
            ));
        panel.start_new_chat();
        let second = panel.history.active_index.unwrap();
        let second_document = raf_core::session::SessionId::new();
        panel.bind_document(second_document, "World B");
        panel.input_text = "draft in B".into();
        assert!(panel.session_is_running(first));
        panel.select_session(first);
        assert_eq!(panel.document_id(), Some(first_document));
        assert_eq!(panel.runtime.status, AgentStatus::ExecutingTools);
        assert_eq!(panel.runtime.pending_calls[0].id, "operation");
        panel.select_session(second);
        let mut visited = Vec::new();
        panel.visit_runs(|run| visited.push(run.document_id().unwrap()));
        assert!(visited.contains(&first_document));
        assert!(visited.contains(&second_document));
        assert_eq!(panel.history.active_index, Some(second));
        assert_eq!(panel.input_text, "draft in B");
        assert_eq!(panel.document_id(), Some(second_document));
    }

    #[test]
    fn new_panel_starts_with_a_collapsible_chat_rail() {
        let panel = AgentPanel::default();
        assert!(panel.sidebar_open);
        assert_eq!(panel.selected_model, MODEL_SENTINEL);
    }

    #[test]
    fn active_permissions_override_legacy_review_and_resume_current_and_parked_chats() {
        let mut panel = AgentPanel::default();
        panel.start_new_chat();
        let first = panel.history.active_index.unwrap();
        panel.runtime.status = AgentStatus::AwaitingApproval;
        panel
            .runtime
            .pending_calls
            .push(raf_ai::agent_runtime::PendingToolCall::new(
                "first",
                "scene_create",
                serde_json::json!({}),
            ));
        panel.start_new_chat();
        panel.runtime.status = AgentStatus::AwaitingApproval;
        panel
            .runtime
            .pending_calls
            .push(raf_ai::agent_runtime::PendingToolCall::new(
                "second",
                "scene_create",
                serde_json::json!({}),
            ));
        let mut settings = EngineSettings::default();
        panel.apply_action(AgentAction::SetMode(AgentMode::Active), &mut settings);
        assert_eq!(panel.runtime.status, AgentStatus::ExecutingTools);
        assert!(
            panel.review_writes,
            "legacy preference must not control Active"
        );
        panel.apply_action(AgentAction::ToggleReview, &mut settings);
        assert!(panel.review_writes);
        panel.select_session(first);
        assert_eq!(panel.runtime.status, AgentStatus::ExecutingTools);
        assert_eq!(panel.runtime.pending_calls[0].id, "first");
        panel.runtime.cancel();
        panel.visit_runs(|run| run.runtime.cancel());
    }

    #[test]
    fn messages_compare_without_serializing_runtime_state() {
        let messages = vec![ChatMessage::user("hello")];
        assert!(messages_equal(&messages, &messages));
        assert_ne!(messages[0].role, MessageRole::Assistant);
    }

    #[test]
    fn restored_session_is_already_marked_as_persisted() {
        let mut panel = AgentPanel::default();
        let index = panel.history.start_session("Restored");
        panel.history.sessions[index]
            .messages
            .push(ChatMessage::user("saved message"));

        panel.load_session_into_runtime(index);

        assert_eq!(panel.last_saved_message_count, panel.runtime.messages.len());
    }

    #[test]
    fn mode_action_updates_settings_for_the_next_agent_submission() {
        let mut panel = AgentPanel::default();
        let mut settings = EngineSettings::default();
        settings.agent_mode = AgentMode::Plan;

        panel.apply_action(AgentAction::SetMode(AgentMode::Active), &mut settings);

        assert_eq!(settings.agent_mode, AgentMode::Active);
        assert!(panel.settings_changed);
    }

    #[test]
    fn selecting_a_card_model_sets_the_active_provider_model() {
        let mut panel = AgentPanel::default();
        let mut settings = EngineSettings::default();

        panel.apply_action(
            AgentAction::SelectModel("anthropic/claude-sonnet-4.5".to_string()),
            &mut settings,
        );

        assert_eq!(settings.default_ai_provider, AiProvider::OpenRouter);
        let provider = settings
            .ai_providers
            .iter()
            .find(|provider| provider.provider == AiProvider::OpenRouter)
            .expect("OpenRouter provider");
        assert_eq!(panel.effective_model(provider), "anthropic/claude-sonnet-4.5");
        assert!(panel.selected_model == MODEL_SENTINEL);
        assert!(panel.settings_changed);
    }

    #[test]
    fn pinning_toggles_the_provider_card_favorites() {
        let mut panel = AgentPanel::default();
        let mut settings = EngineSettings::default();

        panel.apply_action(AgentAction::PinModel("gpt-5".to_string()), &mut settings);
        let provider = settings
            .ai_providers
            .iter()
            .find(|provider| provider.provider == AiProvider::OpenRouter)
            .expect("OpenRouter provider");
        assert_eq!(provider.pinned_models, vec!["gpt-5".to_string()]);

        panel.apply_action(AgentAction::PinModel("gpt-5".to_string()), &mut settings);
        let provider = settings
            .ai_providers
            .iter()
            .find(|provider| provider.provider == AiProvider::OpenRouter)
            .expect("OpenRouter provider");
        assert!(provider.pinned_models.is_empty());
        assert!(panel.settings_changed);
    }

    #[test]
    fn custom_model_confirmation_writes_and_pins_the_card() {
        let mut panel = AgentPanel::default();
        let mut settings = EngineSettings::default();
        panel.new_model_id = "local-model".to_string();

        panel.apply_action(AgentAction::AddModel, &mut settings);

        let provider = settings
            .ai_providers
            .iter()
            .find(|provider| provider.provider == AiProvider::OpenRouter)
            .expect("OpenRouter provider");
        assert_eq!(provider.model, "local-model");
        assert!(provider.pinned_models.contains(&"local-model".to_string()));
        assert!(panel.selected_model == MODEL_SENTINEL);
        assert!(panel.settings_changed);
    }

    #[test]
    fn response_token_setting_reconfigures_the_native_runtime() {
        let mut panel = AgentPanel::default();
        let mut settings = EngineSettings::default();
        settings.agent_max_response_tokens = 8_192;
        settings.agent_response_limit_enabled = true;
        settings
            .ai_providers
            .iter_mut()
            .find(|provider| provider.provider == settings.default_ai_provider)
            .expect("default AI provider")
            .enabled = true;

        panel.prepare(&settings, None, &CommandCatalog::builtin());

        assert_eq!(panel.runtime.client.config.max_tokens, 8_192);
    }
}
