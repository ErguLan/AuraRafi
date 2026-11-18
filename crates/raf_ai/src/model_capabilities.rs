//! Automatic model capability policy for native Agent inputs.
//!
//! AuraRafi must not ask users to confirm capabilities that the provider can
//! answer itself. Known model families are classified locally, unknown models
//! are attempted optimistically, and a provider rejection is reported as
//! evidence instead of silently dropping the user's image.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn observed_text_only() -> &'static Mutex<HashSet<String>> {
    static OBSERVED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    OBSERVED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn capability_file(root: &Path) -> PathBuf {
    root.join(".aura_rafi").join("model_capabilities.json")
}

/// Load provider observations persisted for this project. The in-memory set is
/// merged so a model learned in one project is not forgotten when another
/// project is opened in the same process.
pub fn load_project_observations(root: &Path) {
    let Ok(bytes) = std::fs::read(capability_file(root)) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    let Some(models) = value.get("text_only").and_then(serde_json::Value::as_array) else {
        return;
    };
    if let Ok(mut observed) = observed_text_only().lock() {
        for model in models.iter().filter_map(serde_json::Value::as_str) {
            let id = model.trim().to_ascii_lowercase();
            if !id.is_empty() {
                observed.insert(id);
            }
        }
    }
}

fn persist_observations(root: &Path) -> Result<(), String> {
    let path = capability_file(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create capability directory: {error}"))?;
    }
    let models = observed_text_only()
        .lock()
        .map(|observed| {
            let mut models = observed.iter().cloned().collect::<Vec<_>>();
            models.sort();
            models
        })
        .map_err(|_| "Model capability state is poisoned.".to_string())?;
    let body = serde_json::json!({"version": 1, "text_only": models});
    let encoded = serde_json::to_vec_pretty(&body)
        .map_err(|error| format!("Could not encode model capabilities: {error}"))?;
    std::fs::write(&path, encoded)
        .map_err(|error| format!("Could not write model capabilities: {error}"))
}

/// Record a provider rejection for this project and persist it so the same
/// model is not attempted again after an editor restart.
pub fn mark_project_observed_text_only(root: &Path, model_id: &str) {
    mark_observed_text_only(model_id);
    let _ = persist_observations(root);
}

/// Record a provider rejection so the same model is not attempted again in
/// this process. The model id is normalized; no credential or prompt is stored.
pub fn mark_observed_text_only(model_id: &str) {
    let id = model_id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return;
    }
    if let Ok(mut observed) = observed_text_only().lock() {
        observed.insert(id);
    }
}

/// Whether a model is known to accept image input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionSupport {
    /// The model family is known to accept image content.
    Supported,
    /// The model family is known to reject image content.
    Unsupported,
    /// The engine cannot prove either way; image input is attempted.
    Unknown,
}

impl VisionSupport {
    pub fn allows_attempt(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

/// Classify a provider model id without a network request.
///
/// This is intentionally conservative: only families that are unambiguously
/// text-only are marked `Unsupported`. Everything else, including local and
/// future model ids, remains attemptable so the provider stays authoritative.
pub fn vision_support(model_id: &str) -> VisionSupport {
    let id = model_id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return VisionSupport::Unknown;
    }
    if observed_text_only()
        .lock()
        .map(|observed| observed.contains(&id))
        .unwrap_or(false)
    {
        return VisionSupport::Unsupported;
    }
    const SUPPORTED: &[&str] = &[
        "gpt-4o",
        "gpt-4.1",
        "gpt-4.5",
        "gpt-5",
        "gpt-6",
        "chatgpt-4o",
        "o3",
        "o4",
        "claude",
        "gemini",
        "grok-3",
        "grok-4",
        "grok-2-vision",
        "vision",
        "llava",
        "pixtral",
        "internvl",
        "qwen2-vl",
        "qwen2.5-vl",
        "qwen3-vl",
        "qwen-vl",
        "llama-3.2-vision",
        "llama-4",
        "phi-3-vision",
        "phi-4-multimodal",
    ];
    if SUPPORTED.iter().any(|prefix| id.contains(prefix)) {
        return VisionSupport::Supported;
    }
    const UNSUPPORTED: &[&str] = &[
        "deepseek-chat",
        "deepseek-coder",
        "deepseek-reasoner",
        "deepseek-r1",
        "codestral",
        "mistral-large",
        "mistral-medium",
        "mistral-small",
        "mixtral",
        "qwen-max",
        "qwen-plus",
        "qwen-turbo",
        "qwen2.5-coder",
        "qwen3-coder",
        "grok-code",
        "glm-4.5",
        "glm-4.6",
        "command-r",
        "command-a",
    ];
    if UNSUPPORTED.iter().any(|prefix| id.contains(prefix)) {
        return VisionSupport::Unsupported;
    }
    VisionSupport::Unknown
}

/// True when a provider error means the request content was rejected because
/// the selected model does not accept images.
pub fn is_image_rejection(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    let image_term = [
        "image",
        "vision",
        "multimodal",
        "modality",
        "image_url",
        "image input",
    ]
    .iter()
    .any(|term| message.contains(term));
    let rejection_term = [
        "not support",
        "unsupported",
        "does not accept",
        "cannot process",
        "invalid content",
        "invalid message",
        "modality",
        "content type",
    ]
    .iter()
    .any(|term| message.contains(term));
    image_term && rejection_term
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vision_families_are_supported() {
        for model in [
            "gpt-5.4",
            "gpt-4o-mini",
            "claude-sonnet-4.6",
            "google/gemini-3-flash",
            "x-ai/grok-4.7",
            "qwen/qwen2.5-vl-72b",
        ] {
            assert_eq!(vision_support(model), VisionSupport::Supported, "{model}");
        }
    }

    #[test]
    fn known_text_models_are_not_attempted() {
        for model in [
            "deepseek/deepseek-chat",
            "deepseek-reasoner",
            "qwen/qwen3-coder",
            "mistral/codestral-2508",
        ] {
            assert_eq!(
                vision_support(model),
                VisionSupport::Unsupported,
                "{model}"
            );
        }
    }

    #[test]
    fn unknown_models_remain_attemptable() {
        assert_eq!(vision_support("local/my-new-model"), VisionSupport::Unknown);
        assert!(vision_support("local/my-new-model").allows_attempt());
    }

    #[test]
    fn observed_provider_rejection_overrides_family_hints() {
        let model = "test/vision-family-lookalike";
        assert_eq!(vision_support(model), VisionSupport::Supported);
        mark_observed_text_only(model);
        assert_eq!(vision_support(model), VisionSupport::Unsupported);
        assert!(!vision_support(model).allows_attempt());
    }

    #[test]
    fn project_observations_are_persisted_and_reloaded() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "raf-model-capabilities-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let model = format!("test/persisted-{unique}");
        mark_project_observed_text_only(&root, &model);
        let file = capability_file(&root);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert!(value["text_only"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry.as_str() == Some(model.as_str())));
        load_project_observations(&root);
        assert_eq!(vision_support(&model), VisionSupport::Unsupported);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn provider_image_rejections_are_recognized() {
        assert!(is_image_rejection(
            "This model does not support image input content parts."
        ));
        assert!(is_image_rejection("unsupported modality: image_url"));
        assert!(!is_image_rejection("rate limit exceeded"));
        assert!(!is_image_rejection("invalid API key"));
    }
}
