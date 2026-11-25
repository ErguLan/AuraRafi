//! Engine configuration / settings module.
//!
//! Covers theme (dark/light), language (EN/ES), render quality,
//! editor preferences, and project defaults. Serialized to RON
//! for human-readable config files.

use crate::ai::{AgentMode, AiProvider, AiProviderConfig};
use crate::units::DisplayUnit;
use serde::{Deserialize, Serialize};

mod ai;
mod defaults;
mod editor;
mod graphics;
mod runtime;

#[cfg(test)]
mod tests;

pub use editor::{Language, ScriptExecutionMode, ScriptLanguage, ScriptLanguageFlags, Theme};
pub use graphics::{RenderExecutionPolicy, RenderPreset, RenderQuality, ViewportRenderMode};
pub use runtime::TargetPlatform;

/// Persisted compatibility bounds for the retired manual Agent page control.
/// The native transcript now chooses its rendered window from scroll position.
pub const AGENT_MESSAGE_PAGE_SIZE_MIN: u32 = 20;
pub const AGENT_MESSAGE_PAGE_SIZE_MAX: u32 = 64;
pub const AGENT_MESSAGE_PAGE_SIZE_DEFAULT: u32 = 24;

/// Bounds for the maximum number of tokens requested for one Agent response.
/// The selected provider/model may still enforce a lower effective limit.
pub const AGENT_MAX_RESPONSE_TOKENS_MIN: u32 = 1_024;
pub const AGENT_MAX_RESPONSE_TOKENS_MAX: u32 = 32_768;
pub const AGENT_MAX_RESPONSE_TOKENS_DEFAULT: u32 = 4_096;

/// Optional per-run tool-call budget for the native Agent. It is disabled by
/// default so large scene builds are not stopped by an arbitrary small cap.
pub const AGENT_MAX_TOOL_CALLS_MIN: u32 = 1;
pub const AGENT_MAX_TOOL_CALLS_MAX: u32 = 4_096;
pub const AGENT_MAX_TOOL_CALLS_DEFAULT: u32 = 256;
/// Informational UI warning threshold; it does not stop the Agent.
pub const AGENT_TOOL_CALL_WARNING_THRESHOLD: u32 = 45;

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

fn default_agent_show_reasoning() -> bool {
    true
}

fn default_ai_persist_credentials() -> bool {
    true
}

fn default_agent_max_response_tokens() -> u32 {
    AGENT_MAX_RESPONSE_TOKENS_DEFAULT
}

fn default_agent_tool_call_limit_enabled() -> bool {
    false
}

fn default_agent_max_tool_calls() -> u32 {
    AGENT_MAX_TOOL_CALLS_DEFAULT
}

// ---------------------------------------------------------------------------
// Engine settings
// ---------------------------------------------------------------------------

/// Complete engine settings, persisted to disk as RON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSettings {
    /// Local Play preferences; independent of background editor script tooling.
    #[serde(default)]
    pub runtime: crate::runtime_config::RuntimePreferences,
    /// Explicit local folder grants, keyed by project UUID. Never part of a project file.
    #[serde(default)]
    pub runtime_save_roots: std::collections::BTreeMap<uuid::Uuid, std::path::PathBuf>,
    // -- Appearance --
    pub theme: Theme,
    #[serde(default)]
    pub theme_experimental: f32,
    /// Disables retained UI transitions for users who prefer reduced motion.
    #[serde(default)]
    pub prefers_reduced_motion: bool,
    /// Raises foreground and border contrast across retained UI surfaces.
    #[serde(default)]
    pub high_contrast: bool,
    /// Publishes native controls to local accessibility clients on demand.
    #[serde(default = "default_true")]
    pub native_accessibility_enabled: bool,
    /// Replaces translucent editor chrome with opaque semantic surfaces.
    #[serde(default)]
    pub reduce_transparency: bool,
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
    /// Editor-only particle preview controls. The game runtime uses the
    /// project's `particle_runtime` policy instead.
    #[serde(default)]
    pub particle_preview: crate::scene::ParticlePreviewSettings,

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
    /// Opt-in static script links, presented as virtual rows, never scene children.
    #[serde(default)]
    pub hierarchy_show_script_links: bool,
    /// Opt-in compact script indicator on rows that carry attached scripts.
    /// Derived from `SceneNode.scripts` with zero I/O, so it is O(1) per row.
    #[serde(default)]
    pub hierarchy_show_script_indicators: bool,
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
    /// Allows device enumeration and serial workers. Hardware access stays
    /// opt-in and the application only scans or connects after a user action.
    #[serde(default)]
    pub electronics_hardware_enabled: bool,
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
    /// Explicitly use an authorized account plan for the OpenAI provider.
    #[serde(default)]
    pub agent_openai_use_plan: bool,

    /// Optional external image studio, installed independently of the engine.
    #[serde(default)]
    pub external_asset_studio: crate::ai::ExternalAssetStudio,

    /// Provider selected by default when starting a new conversation.
    #[serde(default)]
    pub default_ai_provider: AiProvider,

    /// Agent permission mode: Passive asks before destructive commands,
    /// Active executes them immediately after warning the user.
    #[serde(default)]
    pub agent_mode: AgentMode,

    /// Retained compatibility preference from the former manual page control.
    /// Native Agent rendering is now automatic and scroll-driven.
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
    /// False delegates output length to the provider instead of a local ceiling.
    #[serde(default)]
    pub agent_response_limit_enabled: bool,
    /// Explicit provider override; empty preserves the provider/model default.
    #[serde(default)]
    pub agent_reasoning_effort: String,
    /// Display the reasoning text returned by the provider in the transcript.
    #[serde(default = "default_agent_show_reasoning")]
    pub agent_show_reasoning: bool,
    /// User confirmation that the selected model supports image input.
    #[serde(default)]
    pub agent_vision_enabled: bool,

    /// Whether the native Agent should enforce `agent_max_tool_calls` for a
    /// single run. Disabled by default for large, iterative workflows.
    #[serde(default = "default_agent_tool_call_limit_enabled")]
    pub agent_tool_call_limit_enabled: bool,

    /// Maximum native tool calls per Agent run when the optional limit is on.
    #[serde(default = "default_agent_max_tool_calls")]
    pub agent_max_tool_calls: u32,

    // -- Window state (persisted) --
    pub window_width: u32,
    pub window_height: u32,
    pub window_maximized: bool,
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
