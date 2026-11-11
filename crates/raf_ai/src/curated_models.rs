//! Curated fallback model catalogs.
//!
//! These are offline first-run suggestions, not observations of the provider
//! APIs. A successful `/models` fetch always supersedes this list in the
//! pickers; the ids here only guarantee that a fresh setup can select a model
//! without a network round-trip.

use crate::provider::AiProvider;

pub fn curated(provider: AiProvider) -> &'static [&'static str] {
    match provider {
        AiProvider::OpenRouter => &[
            "openrouter/auto",
            "openai/gpt-5-mini",
            "anthropic/claude-sonnet-4.5",
            "google/gemini-2.5-flash",
            "deepseek/deepseek-chat-v3.1",
            "x-ai/grok-4",
            "meta-llama/llama-3.3-70b-instruct",
            "mistralai/mistral-large",
            "qwen/qwen3-coder",
            "moonshotai/kimi-k2",
        ],
        // Live credential-scoped discovery only; no bundled availability claims.
        AiProvider::OpenAI => &[],
        AiProvider::OpenCode => &[
            "code-supernova",
            "claude-sonnet-4-5-20250929",
            "qwen3-coder",
            "grok-code",
            "kimi-k2",
            "glm-4.6",
        ],
        AiProvider::OpenCodeGo => &[
            "code-supernova",
            "claude-sonnet-4-5-20250929",
            "qwen3-coder",
            "grok-code",
            "kimi-k2",
            "glm-4.6",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_catalogs_cover_the_four_supported_gateways() {
        assert!(curated(AiProvider::OpenRouter).len() >= 8);
        assert!(curated(AiProvider::OpenAI).is_empty());
        assert!(curated(AiProvider::OpenCode).len() >= 4);
        assert!(curated(AiProvider::OpenCodeGo).len() >= 4);
        assert!(curated(AiProvider::Claude).is_empty());
        assert_eq!(curated(AiProvider::Puerto), Vec::<&str>::new().as_slice());
        for id in curated(AiProvider::OpenRouter) {
            assert_eq!(*id, id.to_lowercase().as_str(), "ids are lower-case slugs");
        }
    }
}
