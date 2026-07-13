//! User-defined model shortcuts for the Agent panel.
//!
//! The registry is stored inside `EngineSettings` so the user can add a model
//! once and select it from the Agent panel without remembering provider-specific
//! ids. No default models are shipped; the user populates the list.

use crate::provider::{AiModelShortcut, AiProvider};

/// In-memory helper around a `Vec<AiModelShortcut>`.
#[derive(Debug, Clone, Default)]
pub struct AgentModelRegistry {
    pub shortcuts: Vec<AiModelShortcut>,
}

impl AgentModelRegistry {
    pub fn new(shortcuts: Vec<AiModelShortcut>) -> Self {
        Self {
            shortcuts: shortcuts
                .into_iter()
                .filter(|shortcut| shortcut.provider.is_editor_supported())
                .collect(),
        }
    }

    /// Sentinel value meaning "use the raw model configured in the provider card".
    pub const PROVIDER_DEFAULT: &'static str = "provider_default";

    /// Add a new shortcut if the label is not empty and not already present.
    pub fn add(
        &mut self,
        label: impl Into<String>,
        provider: AiProvider,
        model_id: impl Into<String>,
    ) -> bool {
        let label = label.into();
        let model_id = model_id.into();
        if !provider.is_editor_supported() || label.trim().is_empty() || model_id.trim().is_empty()
        {
            return false;
        }
        if self.shortcuts.iter().any(|s| s.label == label) {
            return false;
        }
        self.shortcuts.push(AiModelShortcut {
            label,
            provider,
            model_id,
        });
        true
    }

    /// Remove a shortcut by label.
    pub fn remove(&mut self, label: &str) {
        self.shortcuts.retain(|s| s.label != label);
    }

    /// Find a shortcut by label.
    pub fn get(&self, label: &str) -> Option<&AiModelShortcut> {
        self.shortcuts.iter().find(|s| s.label == label)
    }

    /// Labels for the UI selector, with the provider-default sentinel first.
    pub fn selector_labels(&self) -> Vec<String> {
        let mut labels = vec![Self::PROVIDER_DEFAULT.to_string()];
        labels.extend(self.shortcuts.iter().map(|s| s.label.clone()));
        labels
    }

    /// Resolve the effective model id for a label and provider config.
    /// Returns `None` if the label points to a shortcut that does not match the
    /// active provider; the caller should fall back to the provider card model.
    pub fn resolve_model_id(&self, label: &str, active_provider: AiProvider) -> Option<String> {
        if label == Self::PROVIDER_DEFAULT || label.trim().is_empty() {
            return None;
        }
        self.get(label).and_then(|shortcut| {
            if shortcut.provider == active_provider {
                Some(shortcut.model_id.clone())
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_adds_and_resolves() {
        let mut registry = AgentModelRegistry::default();
        assert!(registry.add("OpenAI model", AiProvider::OpenAI, "gpt-4.1"));
        assert_eq!(registry.selector_labels().len(), 2);
        assert_eq!(
            registry.resolve_model_id("OpenAI model", AiProvider::OpenAI),
            Some("gpt-4.1".to_string())
        );
        assert_eq!(
            registry.resolve_model_id("OpenAI model", AiProvider::OpenRouter),
            None
        );
    }

    #[test]
    fn registry_rejects_unverified_provider_shortcuts() {
        let mut registry = AgentModelRegistry::default();
        assert!(!registry.add("Legacy", AiProvider::Claude, "legacy-model"));
    }
}
