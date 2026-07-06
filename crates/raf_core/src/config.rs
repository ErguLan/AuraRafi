//! Engine configuration / settings module.
//!
//! Covers theme (dark/light), language (EN/ES), render quality,
//! editor preferences, and project defaults. Serialized to RON
//! for human-readable config files.

use crate::units::DisplayUnit;
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
// Viewport Render Mode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportRenderMode {
    Solid,
    Wireframe,
    Preview,
}

impl Default for ViewportRenderMode {
    fn default() -> Self {
        Self::Solid
    }
}

fn default_true() -> bool {
    true
}

fn default_invert_mouse_y() -> bool {
    true
}

fn default_gizmo_sensitivity() -> f32 {
    3.5
}

fn default_rotate_sensitivity() -> f32 {
    3.5
}

fn default_grid_load_distance() -> f32 {
    15.0
}

fn default_display_unit() -> DisplayUnit {
    DisplayUnit::Metric
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
// Render execution policy
// ---------------------------------------------------------------------------

/// Global render execution policy for the current machine.
///
/// This selects how the engine prefers to run render work in editor and
/// runtime surfaces. Project settings may still enable heavier GPU-only
/// features, but the base backend preference lives here so the whole app
/// follows one consistent policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderExecutionPolicy {
    /// Prefer GPU when possible and fall back to CPU automatically.
    Auto,
    /// Force CPU rendering for maximum compatibility.
    CpuOnly,
    /// Prefer GPU-backed rendering for interactive surfaces.
    GpuPreferred,
}

impl Default for RenderExecutionPolicy {
    fn default() -> Self {
        Self::Auto
    }
}

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

// ---------------------------------------------------------------------------
// Engine settings
// ---------------------------------------------------------------------------

/// Complete engine settings, persisted to disk as RON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSettings {
    // -- Appearance --
    pub theme: Theme,
    #[serde(default)]
    pub theme_experimental: f32,
    pub font_size: f32,
    pub ui_scale: f32,
    /// When true, the editor detects the system DPI at startup and applies
    /// it as the UI scale factor. The manual slider is disabled while active.
    #[serde(default = "default_true")]
    pub auto_ui_scale: bool,

    // -- Language --
    pub language: Language,

    // -- Performance --
    pub render_quality: RenderQuality,
    #[serde(default)]
    pub render_execution_policy: RenderExecutionPolicy,
    pub fps_limit: u32,
    pub vsync: bool,
    pub multithreading: bool,

    // -- Editor --
    pub grid_visible: bool,
    pub snap_to_grid: bool,
    pub grid_size: f32,
    #[serde(default = "default_grid_load_distance")]
    pub grid_load_distance: f32,
    pub auto_save_interval_seconds: u32,
    /// Display unit system for the UI. Calculation is always in SI (meters).
    #[serde(default = "default_display_unit")]
    pub display_unit: DisplayUnit,

    // -- Solid Mode Rendering --
    /// Show surface edge lines in solid render mode.
    #[serde(default)]
    pub solid_show_surface_edges: bool,
    /// X-ray mode: see-through solid surfaces.
    #[serde(default)]
    pub solid_xray_mode: bool,
    /// Face tonality: when enabled, applies directional light shading to faces.
    #[serde(default = "default_true")]
    pub solid_face_tonality: bool,

    // -- Simple Mode --
    /// When true, hides advanced parameters (parasitics, timing,
    /// advanced simulation). Shows only basic controls for beginners.
    pub simple_mode: bool,

    // -- Platform --
    /// Target build platform. Affects layout, input handling,
    /// and export options.
    pub target_platform: TargetPlatform,

    /// Headless mode: no window, for cloud/server rendering.
    pub headless: bool,

    /// Responsive layout: adapts UI to small screens (mobile/tablet).
    pub responsive_layout: bool,

    // -- Rendering (v0.7.0) --
    /// Render quality preset (Potato/Low/Medium/High).
    /// Controls which advanced features are enabled.
    /// Default: Potato (everything off, maximum performance).
    /// Individual feature toggles are in the project's render_config.
    pub render_preset: RenderPreset,

    // -- Input (v0.7.0) --
    /// Invert mouse X axis for orbit camera.
    #[serde(default)]
    pub invert_mouse_x: bool,
    /// Invert mouse Y axis for orbit camera.
    #[serde(default = "default_invert_mouse_y")]
    pub invert_mouse_y: bool,
    /// Default viewport presentation mode.
    #[serde(default)]
    pub viewport_render_mode: ViewportRenderMode,
    /// Whether entity labels are shown in the viewport.
    #[serde(default = "default_true")]
    pub show_viewport_labels: bool,
    /// Whether the editor toolbar FPS counter is visible.
    #[serde(default = "default_true")]
    pub show_fps_counter: bool,
    /// Enables the manual slash-command console entrypoint.
    #[serde(default)]
    pub command_console_enabled: bool,
    /// Multiplier for move gizmo drag response.
    #[serde(default = "default_gizmo_sensitivity")]
    pub move_gizmo_sensitivity: f32,
    /// Multiplier for rotate gizmo drag response.
    #[serde(default = "default_rotate_sensitivity")]
    pub rotate_gizmo_sensitivity: f32,
    /// Multiplier for scale gizmo drag response.
    #[serde(default = "default_gizmo_sensitivity")]
    pub scale_gizmo_sensitivity: f32,
    /// If true, scale gizmo starts in uniform mode until Shift is held.
    #[serde(default)]
    pub uniform_scale_by_default: bool,

    // -- Scripting (v0.8.x) --
    /// Master switch for the script runtime. Off until the runtime is built.
    /// See docs/SCRIPTING_SYSTEM.md.
    #[serde(default)]
    pub script_runtime_enabled: bool,
    /// Default language for "New Script" templates.
    #[serde(default = "default_script_language")]
    pub default_script_language: ScriptLanguage,
    /// Reload scripts automatically when files change on disk.
    #[serde(default = "default_true")]
    pub script_hot_reload: bool,
    /// Maximum script execution time per frame, in milliseconds.
    /// Prevents infinite loops from hanging the engine (Rhai only).
    #[serde(default = "default_script_timeout_ms")]
    pub script_timeout_ms: u32,
    /// External editor command for opening .rhai / .cpp files.
    #[serde(default = "default_script_editor_cmd")]
    pub script_external_editor_cmd: String,

    // -- Window state (persisted) --
    pub window_width: u32,
    pub window_height: u32,
    pub window_maximized: bool,
}

/// v0.7.0: Render quality presets that map to RenderConfig defaults.
/// These are stored in EngineSettings. The actual feature toggles
/// live in raf_render::RenderConfig and are per-project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderPreset {
    /// Everything off. Maximum compatibility. Default.
    Potato,
    /// Specular + fog + basic textures.
    Low,
    /// GPU + shadows + bloom + FXAA.
    Medium,
    /// Everything on (except raytracing).
    High,
}

impl Default for RenderPreset {
    fn default() -> Self {
        Self::Potato
    }
}

// ---------------------------------------------------------------------------
// Scripting types
// ---------------------------------------------------------------------------

/// Scripting language for the AuraRafi Host API.
/// See docs/SCRIPTING_SYSTEM.md for the tier system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptLanguage {
    /// Tier 1: sandboxed, pure Rust, beginner-friendly.
    Rhai,
    /// Tier 2: native-performance modules compiled to WASM.
    /// Requires a WASM runtime (Phase D).
    Cpp,
    /// Tier 3: visual node graphs from raf_nodes.
    Nodes,
}

impl Default for ScriptLanguage {
    fn default() -> Self {
        Self::Rhai
    }
}

impl ScriptLanguage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rhai => "Rhai",
            Self::Cpp => "C++ (WASM)",
            Self::Nodes => "Visual Nodes",
        }
    }

    pub fn all() -> [ScriptLanguage; 3] {
        [Self::Rhai, Self::Cpp, Self::Nodes]
    }
}

/// When scripts execute during the project lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptExecutionMode {
    /// Scripts do not run at all.
    Disabled,
    /// Scripts run only inside the editor (Play mode).
    EditorOnly,
    /// Scripts run in the editor and in exported runtime builds.
    Runtime,
}

impl Default for ScriptExecutionMode {
    fn default() -> Self {
        Self::EditorOnly
    }
}

impl ScriptExecutionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::EditorOnly => "Editor Only",
            Self::Runtime => "Runtime",
        }
    }

    pub fn all() -> [ScriptExecutionMode; 3] {
        [Self::Disabled, Self::EditorOnly, Self::Runtime]
    }
}

/// Bitflags for which script languages a project allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptLanguageFlags(pub u8);

impl ScriptLanguageFlags {
    pub const RHAI: Self = Self(0x01);
    pub const CPP: Self = Self(0x02);
    pub const NODES: Self = Self(0x04);
    pub const ALL: Self = Self(0x07);
    pub const DEFAULT: Self = Self(0x05); // Rhai + Nodes

    pub fn has(self, lang: ScriptLanguage) -> bool {
        let bit = match lang {
            ScriptLanguage::Rhai => 0x01,
            ScriptLanguage::Cpp => 0x02,
            ScriptLanguage::Nodes => 0x04,
        };
        (self.0 & bit) != 0
    }

    pub fn set(&mut self, lang: ScriptLanguage, enabled: bool) {
        let bit = match lang {
            ScriptLanguage::Rhai => 0x01,
            ScriptLanguage::Cpp => 0x02,
            ScriptLanguage::Nodes => 0x04,
        };
        if enabled {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

impl Default for ScriptLanguageFlags {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn default_script_language() -> ScriptLanguage {
    ScriptLanguage::Rhai
}

fn default_script_timeout_ms() -> u32 {
    100
}

fn default_script_editor_cmd() -> String {
    "code".to_string()
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            theme_experimental: 0.0,
            font_size: 14.0,
            ui_scale: 1.0,
            auto_ui_scale: true,
            language: Language::English,
            render_quality: RenderQuality::Low,
            render_execution_policy: RenderExecutionPolicy::Auto,
            fps_limit: 60,
            vsync: true,
            multithreading: true,
            grid_visible: true,
            snap_to_grid: true,
            grid_size: 1.0,
            grid_load_distance: 15.0,
            auto_save_interval_seconds: 120,
            display_unit: DisplayUnit::Metric,
            solid_show_surface_edges: false,
            solid_xray_mode: false,
            solid_face_tonality: true,
            simple_mode: false,
            target_platform: TargetPlatform::Desktop,
            headless: false,
            responsive_layout: false,
            render_preset: RenderPreset::Potato,
            invert_mouse_x: false,
            invert_mouse_y: true,
            viewport_render_mode: ViewportRenderMode::Solid,
            show_viewport_labels: true,
            show_fps_counter: true,
            command_console_enabled: false,
            move_gizmo_sensitivity: 3.5,
            rotate_gizmo_sensitivity: 3.5,
            scale_gizmo_sensitivity: 3.5,
            uniform_scale_by_default: false,
            script_runtime_enabled: false,
            default_script_language: ScriptLanguage::Rhai,
            script_hot_reload: true,
            script_timeout_ms: 100,
            script_external_editor_cmd: "code".to_string(),
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
                settings
            }
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
        assert_eq!(s.render_execution_policy, RenderExecutionPolicy::Auto);
        assert_eq!(s.fps_limit, 60);
    }

    #[test]
    fn round_trip_ron() {
        let settings = EngineSettings::default();
        let serialized =
            ron::ser::to_string_pretty(&settings, ron::ser::PrettyConfig::default()).unwrap();
        let deserialized: EngineSettings = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.theme, settings.theme);
        assert_eq!(deserialized.language, settings.language);
    }
}
