//! Automatic model capability policy for native Agent inputs.
//!
//! AuraRafi must not ask users to confirm capabilities that the provider can
//! answer itself. Known model families are classified locally, unknown models
//! are attempted optimistically, and a provider rejection is reported as
//! evidence instead of silently dropping the user's image.

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
    fn provider_image_rejections_are_recognized() {
        assert!(is_image_rejection(
            "This model does not support image input content parts."
        ));
        assert!(is_image_rejection("unsupported modality: image_url"));
        assert!(!is_image_rejection("rate limit exceeded"));
        assert!(!is_image_rejection("invalid API key"));
    }
}
