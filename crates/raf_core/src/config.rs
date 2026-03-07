//! Engine configuration / settings module.
//!
//! Covers theme (dark/light), language (EN/ES), render quality,
//! editor preferences, and project defaults. Serialized to RON
//! for human-readable config files.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

/// Visual theme selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Dark
    }
}

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

/// Supported UI languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    English,
    Spanish,
}

impl Default for Language {
    fn default() -> Self {
        Self::English
    }
}

impl Language {
    /// Fluent locale identifier.
    pub fn locale_id(&self) -> &str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
        }
    }

    /// Display name in native language.
    pub fn display_name(&self) -> &str {
        match self {
            Self::English => "English",
            Self::Spanish => "Espanol",
        }
    }
}

// ---------------------------------------------------------------------------
// Render quality
// ---------------------------------------------------------------------------

/// Render quality presets (0 = potato, 3 = high-end future RTX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderQuality {
    /// No shadows, no post-processing. Maximum performance.
    Potato = 0,
    /// Basic shadows, simple ambient occlusion.
    Low = 1,
    /// Improved shadows, bloom, anti-aliasing.
    Medium = 2,
    /// Full quality. Future: RTX/ray tracing.
    High = 3,
}

impl Default for RenderQuality {
    fn default() -> Self {
        Self::Low
    }
}

// ---------------------------------------------------------------------------
// Engine settings
// ---------------------------------------------------------------------------

/// Complete engine settings, persisted to disk as RON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSettings {
    // -- Appearance --
    pub theme: Theme,
    pub font_size: f32,
    pub ui_scale: f32,

    // -- Language --
    pub language: Language,

    // -- Performance --
    pub render_quality: RenderQuality,
    pub fps_limit: u32,
    pub vsync: bool,
    pub multithreading: bool,

    // -- Editor --
    pub grid_visible: bool,
    pub snap_to_grid: bool,
    pub grid_size: f32,
    pub auto_save_interval_seconds: u32,
    pub units_metric: bool,

    // -- Window state (persisted) --
    pub window_width: u32,
    pub window_height: u32,
    pub window_maximized: bool,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            font_size: 14.0,
            ui_scale: 1.0,
            language: Language::English,
            render_quality: RenderQuality::Low,
            fps_limit: 60,
            vsync: true,
            multithreading: true,
            grid_visible: true,
            snap_to_grid: true,
            grid_size: 1.0,
            auto_save_interval_seconds: 120,
            units_metric: true,
            window_width: 1280,
            window_height: 720,
            window_maximized: false,
        }
    }
}

impl EngineSettings {
    /// File name for settings on disk.
    pub const FILE_NAME: &'static str = "aura_rafi_settings.ron";

    /// Save settings to a RON file at the given directory path.
    pub fn save(&self, dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let path = dir.join(Self::FILE_NAME);
        let pretty = ron::ser::PrettyConfig::default();
        let data = ron::ser::to_string_pretty(self, pretty)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Load settings from a RON file. Returns defaults if file does not exist.
    pub fn load(dir: &std::path::Path) -> Self {
        let path = dir.join(Self::FILE_NAME);
        match std::fs::read_to_string(&path) {
            Ok(data) => ron::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings() {
        let s = EngineSettings::default();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.language, Language::English);
        assert_eq!(s.fps_limit, 60);
    }

    #[test]
    fn round_trip_ron() {
        let settings = EngineSettings::default();
        let serialized = ron::ser::to_string_pretty(
            &settings,
            ron::ser::PrettyConfig::default(),
        )
        .unwrap();
        let deserialized: EngineSettings = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.theme, settings.theme);
        assert_eq!(deserialized.language, settings.language);
    }
}
