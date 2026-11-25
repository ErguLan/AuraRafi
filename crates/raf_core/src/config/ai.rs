//! AI provider catalog normalization for persisted engine settings.

use crate::ai::{AiProvider, AiProviderConfig};

use super::{
    EngineSettings, AGENT_MAX_RESPONSE_TOKENS_MAX, AGENT_MAX_RESPONSE_TOKENS_MIN,
    AGENT_MAX_TOOL_CALLS_MAX, AGENT_MAX_TOOL_CALLS_MIN, AGENT_MESSAGE_PAGE_SIZE_MAX,
    AGENT_MESSAGE_PAGE_SIZE_MIN,
};

/// Keeps the persisted sanitized catalog bounded and free of control
/// characters so a corrupted provider list can never bloat settings files.
fn catalog_sanitize(models: &mut Vec<String>) {
    models
        .retain(|id| !id.trim().is_empty() && id.len() <= 160 && !id.chars().any(char::is_control));
    models.sort();
    models.dedup();
}

impl EngineSettings {
    pub(super) fn normalize_ai_providers(&mut self) {
        self.ai_providers
            .retain(|config| config.provider.is_editor_supported());
        for provider in AiProvider::editor_supported() {
            if !self
                .ai_providers
                .iter()
                .any(|config| config.provider == *provider)
            {
                self.ai_providers
                    .push(AiProviderConfig::for_provider(*provider));
            }
        }
        // Pinned catalogs written by older builds lose their list; the model
        // itself is the truth that survives the migration.
        for provider in &mut self.ai_providers {
            provider
                .pinned_models
                .retain(|id| !id.trim().is_empty() && id.len() <= 160);
            catalog_sanitize(&mut provider.catalog_models);
        }
        if !self.default_ai_provider.is_editor_supported() {
            self.default_ai_provider = AiProvider::OpenRouter;
        }
    }

    pub(super) fn normalize_agent_message_page_size(&mut self) {
        self.agent_message_page_size = self
            .agent_message_page_size
            .clamp(AGENT_MESSAGE_PAGE_SIZE_MIN, AGENT_MESSAGE_PAGE_SIZE_MAX);
    }

    pub(super) fn normalize_agent_response_settings(&mut self) {
        if !matches!(
            self.agent_reasoning_effort.as_str(),
            "" | "none" | "low" | "medium" | "high" | "max"
        ) {
            self.agent_reasoning_effort.clear();
        }
        self.agent_max_response_tokens = self
            .agent_max_response_tokens
            .clamp(AGENT_MAX_RESPONSE_TOKENS_MIN, AGENT_MAX_RESPONSE_TOKENS_MAX);
    }

    pub(super) fn normalize_agent_max_tool_calls(&mut self) {
        self.agent_max_tool_calls = self
            .agent_max_tool_calls
            .clamp(AGENT_MAX_TOOL_CALLS_MIN, AGENT_MAX_TOOL_CALLS_MAX);
    }
}
