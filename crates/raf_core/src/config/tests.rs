use super::*;

use crate::ai::{AiProvider, AiProviderConfig};

#[test]
fn default_settings() {
    let s = EngineSettings::default();
    assert_eq!(s.theme, Theme::Dark);
    assert_eq!(s.language, Language::English);
    assert_eq!(s.render_execution_policy, RenderExecutionPolicy::Auto);
    assert_eq!(s.fps_limit, 60);
    assert!(s.scene_packet_reuse);
    assert_eq!(
        s.texture_quality,
        crate::project::TextureQualityMode::Adaptive
    );
    assert_eq!(s.agent_message_page_size, AGENT_MESSAGE_PAGE_SIZE_DEFAULT);
    assert!(s.agent_streaming_enabled);
    assert!(s.ai_persist_credentials);
    assert_eq!(
        s.agent_max_response_tokens,
        AGENT_MAX_RESPONSE_TOKENS_DEFAULT
    );
    assert!(!s.agent_tool_call_limit_enabled);
    assert_eq!(s.agent_max_tool_calls, AGENT_MAX_TOOL_CALLS_DEFAULT);
    assert!(!s.electronics_hardware_enabled);
}

#[test]
fn round_trip_ron() {
    let mut settings = EngineSettings::default();
    settings.electronics_hardware_enabled = true;
    let serialized =
        ron::ser::to_string_pretty(&settings, ron::ser::PrettyConfig::default()).unwrap();
    let deserialized: EngineSettings = ron::from_str(&serialized).unwrap();
    assert_eq!(deserialized.theme, settings.theme);
    assert_eq!(deserialized.language, settings.language);
    assert!(deserialized.electronics_hardware_enabled);
}

#[test]
fn external_studio_defaults_are_opt_in_and_configuration_round_trips() {
    let mut settings = EngineSettings::default();
    assert!(!settings.external_asset_studio.enabled);
    assert!(settings.external_asset_studio.repository_url.is_empty());
    settings.external_asset_studio.enabled = true;
    settings.external_asset_studio.directory = "D:/Tools/Studio".into();
    settings.external_asset_studio.entry_point = "cli/asset-worker.js".into();
    let raw = ron::to_string(&settings).unwrap();
    let restored: EngineSettings = ron::from_str(&raw).unwrap();
    assert_eq!(
        restored.external_asset_studio,
        settings.external_asset_studio
    );
}

#[test]
fn save_and_load_preserve_provider_credentials_when_enabled() {
    let directory =
        std::env::temp_dir().join(format!("aurarafi-settings-test-{}", uuid::Uuid::new_v4()));
    let mut settings = EngineSettings::default();
    let provider = settings
        .ai_providers
        .iter_mut()
        .find(|provider| provider.provider == AiProvider::OpenRouter)
        .expect("OpenRouter provider");
    provider.model = "test-model".to_string();
    provider.api_key = "test-secret".to_string();

    settings.save(&directory).expect("save settings");
    let loaded = EngineSettings::load(&directory);
    let loaded_provider = loaded
        .ai_providers
        .iter()
        .find(|provider| provider.provider == AiProvider::OpenRouter)
        .expect("loaded OpenRouter provider");
    assert_eq!(loaded_provider.model, "test-model");
    assert_eq!(loaded_provider.api_key, "test-secret");

    settings.ai_persist_credentials = false;
    settings
        .save(&directory)
        .expect("save settings without key");
    let redacted = EngineSettings::load(&directory);
    let redacted_provider = redacted
        .ai_providers
        .iter()
        .find(|provider| provider.provider == AiProvider::OpenRouter)
        .expect("redacted OpenRouter provider");
    assert_eq!(redacted_provider.model, "test-model");
    assert!(redacted_provider.api_key.is_empty());
    assert!(!redacted.ai_persist_credentials);

    std::fs::remove_dir_all(&directory).expect("remove temporary settings");
}

#[test]
fn load_normalizes_legacy_ai_provider_configuration() {
    let mut settings = EngineSettings::default();
    settings.default_ai_provider = AiProvider::Claude;
    settings
        .ai_providers
        .push(AiProviderConfig::for_provider(AiProvider::Claude));
    settings.normalize_ai_providers();

    assert_eq!(settings.default_ai_provider, AiProvider::OpenRouter);
    assert!(settings
        .ai_providers
        .iter()
        .all(|config| config.provider.is_editor_supported()));
}

#[test]
fn load_normalizes_removed_viewport_render_modes() {
    for legacy_mode in [ViewportRenderMode::Wireframe, ViewportRenderMode::Preview] {
        let directory = std::env::temp_dir().join(format!(
            "aurarafi-viewport-mode-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut settings = EngineSettings::default();
        settings.viewport_render_mode = legacy_mode;

        settings.save(&directory).expect("save legacy settings");
        let loaded = EngineSettings::load(&directory);

        assert_eq!(loaded.viewport_render_mode, ViewportRenderMode::Solid);
        std::fs::remove_dir_all(&directory).expect("remove temporary settings");
    }
}
