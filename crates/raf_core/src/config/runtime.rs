//! Target platform selection and persisted engine settings lifecycle.

use serde::{Deserialize, Serialize};

use super::EngineSettings;

// ---------------------------------------------------------------------------
// Target platform
// ---------------------------------------------------------------------------

/// Target platform for build/export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetPlatform {
    /// Standard desktop (Windows, macOS, Linux).
    Desktop,
    /// Mobile devices (Android, iOS) - responsive layout, touch input.
    Mobile,
    /// WebAssembly (browser) - shareable, universal access.
    Web,
    /// Cloud/streaming server (headless rendering, low latency input).
    Cloud,
    /// Console (Xbox, PlayStation, Switch) - future, requires SDK.
    Console,
}

impl Default for TargetPlatform {
    fn default() -> Self {
        Self::Desktop
    }
}

impl TargetPlatform {
    /// Human-readable display name.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Desktop => "Desktop",
            Self::Mobile => "Mobile",
            Self::Web => "Web (WASM)",
            Self::Cloud => "Cloud/Streaming",
            Self::Console => "Console",
        }
    }

    /// All supported platforms.
    pub fn all() -> &'static [TargetPlatform] {
        &[
            TargetPlatform::Desktop,
            TargetPlatform::Mobile,
            TargetPlatform::Web,
            TargetPlatform::Cloud,
            TargetPlatform::Console,
        ]
    }
}

impl EngineSettings {
    /// File name for settings on disk.
    pub const FILE_NAME: &'static str = "aura_rafi_settings.ron";

    /// Returns the per-user directory used for global engine settings.
    pub fn user_config_dir() -> std::path::PathBuf {
        #[cfg(windows)]
        let base = std::env::var_os("APPDATA")
            .or_else(|| std::env::var_os("LOCALAPPDATA"))
            .map(std::path::PathBuf::from);

        #[cfg(not(windows))]
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .map(|home| home.join(".config"))
            });

        base.map(|path| path.join("AuraRafi"))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    }

    /// Save settings to a RON file at the given directory path.
    pub fn save(&self, dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(dir)?;
        let mut persisted = self.clone();
        if !persisted.ai_persist_credentials {
            for provider in &mut persisted.ai_providers {
                provider.api_key.clear();
            }
        }
        let path = dir.join(Self::FILE_NAME);
        let pretty = ron::ser::PrettyConfig::default();
        let data = ron::ser::to_string_pretty(&persisted, pretty)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Load settings from a RON file. Returns defaults if file does not exist.
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join(Self::FILE_NAME);
        match std::fs::read_to_string(&path) {
            Ok(data) => {
                let mut settings: EngineSettings = ron::from_str(&data).unwrap_or_default();
                if (settings.move_gizmo_sensitivity - 1.0).abs() < f32::EPSILON
                    && (settings.rotate_gizmo_sensitivity - 2.0).abs() < f32::EPSILON
                    && (settings.scale_gizmo_sensitivity - 1.0).abs() < f32::EPSILON
                {
                    settings.move_gizmo_sensitivity = 3.5;
                    settings.rotate_gizmo_sensitivity = 3.5;
                    settings.scale_gizmo_sensitivity = 3.5;
                }
                settings.normalize_ai_providers();
                settings.normalize_agent_message_page_size();
                settings.normalize_agent_response_settings();
                settings.normalize_agent_max_tool_calls();
                settings.viewport_render_mode = settings.viewport_render_mode.normalized();
                settings
            }
            Err(_) => Self::default(),
        }
    }
}
