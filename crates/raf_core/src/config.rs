//! Engine configuration / settings module.
//!
//! Covers theme (dark/light), language (EN/ES), render quality,
//! editor preferences, and project defaults. Serialized to RON
//! for human-readable config files.

use crate::ai::{AgentMode, AiModelShortcut, AiProvider, AiProviderConfig};
use crate::units::DisplayUnit;
use serde::{Deserialize, Serialize};

/// Minimum and maximum number of Agent messages rendered in one retained page.
/// The page is intentionally large enough to keep a normal conversation in
/// one wheel-scrollable viewport before manual pagination is needed.
pub const AGENT_MESSAGE_PAGE_SIZE_MIN: u32 = 20;
pub const AGENT_MESSAGE_PAGE_SIZE_MAX: u32 = 64;
pub const AGENT_MESSAGE_PAGE_SIZE_DEFAULT: u32 = 24;

/// Bounds for the maximum number of tokens requested for one Agent response.
/// The selected provider/model may still enforce a lower effective limit.
pub const AGENT_MAX_RESPONSE_TOKENS_MIN: u32 = 1_024;
pub const AGENT_MAX_RESPONSE_TOKENS_MAX: u32 = 32_768;
pub const AGENT_MAX_RESPONSE_TOKENS_DEFAULT: u32 = 4_096;

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

fn default_gizmo_growth() -> f32 {
    0.0
}

fn default_wasd_speed() -> f32 {
    1.0
}

fn default_rotate_sensitivity() -> f32 {
    3.5
}

fn default_grid_load_distance() -> f32 {
    15.0
}

fn default_hierarchy_row_height() -> f32 {
    26.0
}

fn default_hierarchy_indent_width() -> f32 {
    14.0
}

fn default_electronics_grid_step_mm() -> f32 {
    20.0
}

fn default_electronics_grid_opacity() -> f32 {
    0.55
}

fn default_display_unit() -> DisplayUnit {
    DisplayUnit::Metric
}

fn default_agent_message_page_size() -> u32 {
    AGENT_MESSAGE_PAGE_SIZE_DEFAULT
}

fn default_agent_streaming_enabled() -> bool {
    true
}

fn default_ai_persist_credentials() -> bool {
    true
}

fn default_agent_max_response_tokens() -> u32 {
    AGENT_MAX_RESPONSE_TOKENS_DEFAULT
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
    /// RafUI Hierarchy presentation preferences. These stay in the shared
    /// Editor section because they affect the editor panel, not the runtime.
    #[serde(default = "default_true")]
    pub hierarchy_show_icons: bool,
    #[serde(default = "default_true")]
    pub hierarchy_show_visibility: bool,
    #[serde(default = "default_true")]
    pub hierarchy_show_locked: bool,
    #[serde(default)]
    pub hierarchy_show_hidden: bool,
    #[serde(default = "default_true")]
    pub hierarchy_auto_reveal_selection: bool,
    #[serde(default = "default_true")]
    pub hierarchy_expand_on_select: bool,
    #[serde(default = "default_true")]
    pub hierarchy_animations: bool,
    /// Refresh numeric Inspector values while a viewport transform is being
    /// dragged. Disable it to update Properties only after mouse release on
    /// lower-end machines.
    #[serde(default = "default_true")]
    pub inspector_live_transform_updates: bool,
    #[serde(default = "default_hierarchy_row_height")]
    pub hierarchy_row_height: f32,
    #[serde(default = "default_hierarchy_indent_width")]
    pub hierarchy_indent_width: f32,
    /// Electronics-only presentation preferences. Grid and snap remain shared
    /// editor settings because the 3D and CAD canvases use the same contract.
    #[serde(default = "default_true")]
    pub electronics_show_status: bool,
    /// Electronics-only CAD grid spacing. The document snap remains in the
    /// same world units; this controls the visible authoring calibration.
    #[serde(default = "default_electronics_grid_step_mm")]
    pub electronics_grid_step_mm: f32,
    /// Multiplier applied to Electronics grid line alpha.
    #[serde(default = "default_electronics_grid_opacity")]
    pub electronics_grid_opacity: f32,

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
    /// Multiplier for WASD camera movement speed.
    #[serde(default = "default_wasd_speed")]
    pub wasd_speed: f32,
    /// Multiplier for rotate gizmo drag response.
    #[serde(default = "default_rotate_sensitivity")]
    pub rotate_gizmo_sensitivity: f32,
    /// Multiplier for scale gizmo drag response.
    #[serde(default = "default_gizmo_sensitivity")]
    pub scale_gizmo_sensitivity: f32,
    /// If true, scale gizmo starts in uniform mode until Shift is held.
    #[serde(default)]
    pub uniform_scale_by_default: bool,
    /// Show the transform gizmo when more than one entity is selected and let
    /// move/rotate/scale operate on the whole group. When false the gizmo
    /// stays a single-selection tool.
    #[serde(default = "default_true")]
    pub multi_select_gizmo_enabled: bool,
    /// Invert WASD forward/backward movement.
    #[serde(default)]
    pub invert_ws: bool,
    /// F key toggles focus lock mode (camera follows selected object).
    #[serde(default = "default_true")]
    pub focus_lock_enabled: bool,
    /// Gizmo handle growth scale (0 = fixed size, 100 = grows with camera distance).
    #[serde(default = "default_gizmo_growth")]
    pub gizmo_growth_scale: f32,

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

    // -- AI providers (v0.9.0) --
    /// Configured AI providers. Defaults include all supported providers with
    /// sensible URLs and models; the user supplies API keys and enables them.
    #[serde(default = "default_ai_providers")]
    pub ai_providers: Vec<AiProviderConfig>,

    /// Persist provider model, endpoint, and API key in the global settings
    /// file. This is enabled by default so provider setup survives restarts.
    #[serde(default = "default_ai_persist_credentials")]
    pub ai_persist_credentials: bool,

    /// Provider selected by default when starting a new conversation.
    #[serde(default)]
    pub default_ai_provider: AiProvider,

    /// Agent permission mode: Passive asks before destructive commands,
    /// Active executes them immediately after warning the user.
    #[serde(default)]
    pub agent_mode: AgentMode,

    /// User-defined model shortcuts shown in the Agent model selector.
    #[serde(default = "default_agent_model_shortcuts")]
    pub agent_model_shortcuts: Vec<AiModelShortcut>,

    /// Legacy persisted page-size preference. The native Agent now renders a
    /// continuous scroll surface, but keeping this field preserves old project
    /// settings files and avoids a migration break.
    #[serde(default = "default_agent_message_page_size")]
    pub agent_message_page_size: u32,

    /// Show assistant text incrementally when the provider supports the
    /// OpenAI-compatible Server-Sent Events response format.
    #[serde(default = "default_agent_streaming_enabled")]
    pub agent_streaming_enabled: bool,

    /// Maximum number of output tokens requested from the configured provider
    /// for each Agent response.
    #[serde(default = "default_agent_max_response_tokens")]
    pub agent_max_response_tokens: u32,

    /// Label of the model shortcut selected by default. "provider_default"
    /// means the raw model configured in the active provider card.
    #[serde(default)]
    pub default_agent_model: String,

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

fn default_ai_providers() -> Vec<AiProviderConfig> {
    AiProvider::editor_supported()
        .iter()
        .map(|provider| AiProviderConfig::for_provider(*provider))
        .collect()
}

fn default_agent_model_shortcuts() -> Vec<AiModelShortcut> {
    Vec::new()
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
            hierarchy_show_icons: true,
            hierarchy_show_visibility: true,
            hierarchy_show_locked: true,
            hierarchy_show_hidden: false,
            hierarchy_auto_reveal_selection: true,
            hierarchy_expand_on_select: true,
            hierarchy_animations: true,
            inspector_live_transform_updates: true,
            hierarchy_row_height: default_hierarchy_row_height(),
            hierarchy_indent_width: default_hierarchy_indent_width(),
            electronics_show_status: true,
            electronics_grid_step_mm: default_electronics_grid_step_mm(),
            electronics_grid_opacity: default_electronics_grid_opacity(),
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
            multi_select_gizmo_enabled: true,
            invert_ws: false,
            focus_lock_enabled: true,
            gizmo_growth_scale: 0.0,
            wasd_speed: 1.0,
            script_runtime_enabled: false,
            default_script_language: ScriptLanguage::Rhai,
            script_hot_reload: true,
            script_timeout_ms: 100,
            script_external_editor_cmd: "code".to_string(),
            ai_providers: default_ai_providers(),
            ai_persist_credentials: true,
            default_ai_provider: AiProvider::OpenRouter,
            agent_mode: AgentMode::Passive,
            agent_model_shortcuts: default_agent_model_shortcuts(),
            agent_message_page_size: AGENT_MESSAGE_PAGE_SIZE_DEFAULT,
            agent_streaming_enabled: true,
            agent_max_response_tokens: AGENT_MAX_RESPONSE_TOKENS_DEFAULT,
            default_agent_model: String::new(),
            window_width: 1280,
            window_height: 720,
            window_maximized: false,
        }
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
                settings.normalize_agent_max_response_tokens();
                settings
            }
            Err(_) => Self::default(),
        }
    }

    fn normalize_ai_providers(&mut self) {
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
        self.agent_model_shortcuts
            .retain(|shortcut| shortcut.provider.is_editor_supported());
        if !self.default_ai_provider.is_editor_supported() {
            self.default_ai_provider = AiProvider::OpenRouter;
        }
    }

    fn normalize_agent_message_page_size(&mut self) {
        self.agent_message_page_size = self
            .agent_message_page_size
            .clamp(AGENT_MESSAGE_PAGE_SIZE_MIN, AGENT_MESSAGE_PAGE_SIZE_MAX);
    }

    fn normalize_agent_max_response_tokens(&mut self) {
        self.agent_max_response_tokens = self
            .agent_max_response_tokens
            .clamp(AGENT_MAX_RESPONSE_TOKENS_MIN, AGENT_MAX_RESPONSE_TOKENS_MAX);
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
        assert_eq!(s.agent_message_page_size, AGENT_MESSAGE_PAGE_SIZE_DEFAULT);
        assert!(s.agent_streaming_enabled);
        assert!(s.ai_persist_credentials);
        assert_eq!(
            s.agent_max_response_tokens,
            AGENT_MAX_RESPONSE_TOKENS_DEFAULT
        );
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
}
