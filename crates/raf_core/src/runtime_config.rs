//! Persisted local runtime policy. Neither server transport nor editor state lives here.
use glam::Vec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeLaunchMode {
    #[default]
    SeparateWindow,
    SameWindow,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeErrorPolicy {
    #[default]
    Pause,
    DisableScript,
    Stop,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CursorCapture {
    #[default]
    Free,
    Locked,
    Confined,
}
impl CursorCapture {
    pub fn name(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Locked => "locked",
            Self::Confined => "confined",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimePreferences {
    pub launch_mode: RuntimeLaunchMode,
    pub max_instances: u32,
    pub window_size: [u32; 2],
    pub fps_limit: u32,
    pub script_budget_ms: u32,
    pub script_operation_limit: u64,
    pub hot_reload: bool,
    pub diagnostics: bool,
    pub error_policy: RuntimeErrorPolicy,
}
impl Default for RuntimePreferences {
    fn default() -> Self {
        Self {
            launch_mode: RuntimeLaunchMode::SeparateWindow,
            max_instances: 1,
            window_size: [960, 540],
            fps_limit: 60,
            script_budget_ms: 10,
            script_operation_limit: 100_000,
            hot_reload: true,
            diagnostics: true,
            error_policy: RuntimeErrorPolicy::Pause,
        }
    }
}
impl RuntimePreferences {
    pub fn normalized(&self) -> Self {
        let mut settings = self.clone();
        settings.max_instances = settings.max_instances.clamp(1, 4);
        settings.window_size = [
            settings.window_size[0].clamp(640, 1920),
            settings.window_size[1].clamp(360, 1080),
        ];
        settings.fps_limit = settings.fps_limit.clamp(15, 120);
        settings.script_budget_ms = settings.script_budget_ms.clamp(1, 100);
        settings.script_operation_limit = settings.script_operation_limit.clamp(1000, 1_000_000);
        settings
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInputAction {
    pub name: String,
    pub keys: Vec<String>,
}
/// Bindings are data, not a built-in movement or camera controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInputAxis {
    pub name: String,
    pub negative: String,
    pub positive: String,
    #[serde(default = "unit_scale")]
    pub scale: f32,
}
fn unit_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressLocation {
    #[default]
    AppData,
    Portable,
    Custom,
}
/// Shared policy contains no machine-specific absolute paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressSettings {
    pub enabled: bool,
    pub location: ProgressLocation,
    pub schema_version: u32,
    pub max_slot_bytes: u32,
    pub max_slots: u32,
    pub autosave: bool,
    pub autosave_seconds: u32,
    pub autosave_slot: String,
}
impl Default for ProgressSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            location: ProgressLocation::AppData,
            schema_version: 1,
            max_slot_bytes: 256 * 1024,
            max_slots: 16,
            autosave: false,
            autosave_seconds: 120,
            autosave_slot: "autosave".into(),
        }
    }
}
impl ProgressSettings {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version == 0
            || !(1024..=4 * 1024 * 1024).contains(&self.max_slot_bytes)
            || !(1..=128).contains(&self.max_slots)
            || !(10..=86400).contains(&self.autosave_seconds)
            || !valid_storage_name(&self.autosave_slot)
        {
            return Err("Invalid progress policy: schema, size, slot count, interval or slot name");
        }
        Ok(())
    }
}
/// Portable identifiers only. They are never interpreted as filesystem paths.
pub fn valid_storage_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'));
    !reserved
        && !name.is_empty()
        && name.len() <= 48
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectRuntimeSettings {
    /// Explicit world/session selection. None means the current unsaved world.
    pub startup_session: Option<Uuid>,
    /// Only this configured camera is used. Missing references never select another node.
    pub active_camera: Option<Uuid>,
    pub fixed_hz: u32,
    pub max_entities: usize,
    pub input_actions: Vec<RuntimeInputAction>,
    pub input_axes: Vec<RuntimeInputAxis>,
    pub progress: ProgressSettings,
}
impl Default for ProjectRuntimeSettings {
    fn default() -> Self {
        Self {
            startup_session: None,
            active_camera: None,
            fixed_hz: 60,
            max_entities: 10_000,
            input_actions: Vec::new(),
            input_axes: Vec::new(),
            progress: ProgressSettings::default(),
        }
    }
}
impl ProjectRuntimeSettings {
    pub fn validate_input(&self) -> Result<(), String> {
        let mut actions = std::collections::HashSet::new();
        let mut axes = std::collections::HashSet::new();
        if self.input_actions.len() > 128
            || self.input_axes.len() > 128
            || self.input_actions.iter().any(|a| {
                a.name.is_empty()
                    || a.name.len() > 128
                    || !actions.insert(a.name.to_ascii_lowercase())
                    || a.keys.len() > 16
                    || a.keys.iter().any(|k| k.is_empty() || k.len() > 64)
            })
            || self.input_axes.iter().any(|a| {
                a.name.is_empty()
                    || a.name.len() > 128
                    || !axes.insert(a.name.to_ascii_lowercase())
                    || a.negative.len() > 64
                    || a.positive.len() > 64
                    || !a.scale.is_finite()
                    || a.scale.abs() > 100.0
            })
        {
            return Err("runtime.invalid_input_bindings".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameCamera {
    pub orthographic: bool,
    pub fov_degrees: f32,
    pub near: f32,
    pub far: f32,
    pub ortho_scale: f32,
}
impl Default for GameCamera {
    fn default() -> Self {
        Self {
            orthographic: false,
            fov_degrees: 60.0,
            near: 0.1,
            far: 1000.0,
            ortho_scale: 10.0,
        }
    }
}
impl GameCamera {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.fov_degrees.is_finite()
            || !(1.0..179.0).contains(&self.fov_degrees)
            || !self.near.is_finite()
            || !self.far.is_finite()
            || self.near <= 0.0
            || self.far <= self.near
            || self.far > 100_000.0
            || !self.ortho_scale.is_finite()
            || self.ortho_scale <= 0.0
        {
            return Err("Invalid camera lens: FOV 1..179, 0 < near < far <= 100000, scale > 0");
        }
        Ok(())
    }
}
/// Resolved pose in SI units. Render hosts translate it to their private camera type.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeCameraPose {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub projection: GameCamera,
}
