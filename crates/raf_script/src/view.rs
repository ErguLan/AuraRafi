//! Local presentation state. Never persisted or shared between players.
use raf_core::project::{
    GeometryDetailMode, ProjectSettings, TextureQualityMode, ViewportResolutionMode,
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScriptRole {
    #[default]
    Local,
    Client,
    Server,
}

/// Graphics quality effective for one runtime instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeQuality {
    pub viewport_resolution: ViewportResolutionMode,
    pub geometry_detail: GeometryDetailMode,
    pub texture_quality: TextureQualityMode,
}

impl Default for RuntimeQuality {
    fn default() -> Self {
        Self {
            viewport_resolution: ViewportResolutionMode::Adaptive,
            geometry_detail: GeometryDetailMode::Adaptive,
            texture_quality: TextureQualityMode::Adaptive,
        }
    }
}

impl RuntimeQuality {
    pub fn from_settings(settings: &ProjectSettings) -> Self {
        Self {
            viewport_resolution: settings.viewport_resolution,
            geometry_detail: settings.geometry_detail,
            texture_quality: settings.texture_quality,
        }
    }
}

/// Local quality choices authored by scripts and layered over the project base.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuntimeQualityOverrides {
    pub viewport_resolution: Option<ViewportResolutionMode>,
    pub geometry_detail: Option<GeometryDetailMode>,
    pub texture_quality: Option<TextureQualityMode>,
}

impl RuntimeQualityOverrides {
    pub fn apply_to(self, base: RuntimeQuality) -> RuntimeQuality {
        RuntimeQuality {
            viewport_resolution: self.viewport_resolution.unwrap_or(base.viewport_resolution),
            geometry_detail: self.geometry_detail.unwrap_or(base.geometry_detail),
            texture_quality: self.texture_quality.unwrap_or(base.texture_quality),
        }
    }
}

pub(crate) const fn viewport_resolution_slug(mode: ViewportResolutionMode) -> &'static str {
    match mode {
        ViewportResolutionMode::Efficient => "efficient",
        ViewportResolutionMode::Adaptive => "adaptive",
        ViewportResolutionMode::FullResolution => "native",
    }
}

pub(crate) const fn geometry_detail_slug(mode: GeometryDetailMode) -> &'static str {
    match mode {
        GeometryDetailMode::Efficient => "efficient",
        GeometryDetailMode::Adaptive => "adaptive",
        GeometryDetailMode::Detailed => "detailed",
    }
}

pub(crate) const fn texture_quality_slug(mode: TextureQualityMode) -> &'static str {
    match mode {
        TextureQualityMode::Efficient => "efficient",
        TextureQualityMode::Adaptive => "adaptive",
        TextureQualityMode::Detailed => "detailed",
    }
}

fn invalid_value(path: &str, value: &str, expected: &str) -> String {
    format!("Invalid quality value '{value}' for '{path}'; expected {expected}")
}

fn apply_override(
    path: &str,
    value: &str,
    overrides: &mut RuntimeQualityOverrides,
) -> Result<(), String> {
    match path {
        "viewport_resolution" => {
            let mode = match value {
                "efficient" => ViewportResolutionMode::Efficient,
                "adaptive" => ViewportResolutionMode::Adaptive,
                "native" => ViewportResolutionMode::FullResolution,
                _ => return Err(invalid_value(path, value, "efficient, adaptive, or native")),
            };
            overrides.viewport_resolution = Some(mode);
        }
        "geometry_detail" => {
            let mode = match value {
                "efficient" => GeometryDetailMode::Efficient,
                "adaptive" => GeometryDetailMode::Adaptive,
                "detailed" => GeometryDetailMode::Detailed,
                _ => return Err(invalid_value(path, value, "efficient, adaptive, or detailed")),
            };
            overrides.geometry_detail = Some(mode);
        }
        "texture_quality" => {
            let mode = match value {
                "efficient" => TextureQualityMode::Efficient,
                "adaptive" => TextureQualityMode::Adaptive,
                "detailed" => TextureQualityMode::Detailed,
                _ => return Err(invalid_value(path, value, "efficient, adaptive, or detailed")),
            };
            overrides.texture_quality = Some(mode);
        }
        _ => {
            return Err(format!(
                "Unknown quality path '{path}'; expected viewport_resolution, geometry_detail, or texture_quality"
            ))
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct RuntimeViewState {
    /// Optional per-session gravity override, consumed at a physics boundary.
    pub world_gravity: Option<glam::Vec3>,
    pub advanced_physics: bool,
    pub character_defaults: Option<[f32; 3]>,
    pub game_ui: raf_ui::game_ui::UiGameRuntime,
    pub active_camera: Option<Uuid>,
    pub role: ScriptRole,
    pub services: crate::services::RuntimeServices,
    pub quality_base: RuntimeQuality,
    pub quality_overrides: RuntimeQualityOverrides,
    quality_revision: u64,
    pub(crate) environment_overrides: serde_json::Map<String, serde_json::Value>,
    pub(crate) environment_revision: u64,
    pub(crate) environment_cache: std::cell::RefCell<
        Option<(
            raf_core::environment::EnvironmentSettings,
            u64,
            raf_core::environment::EnvironmentSettings,
        )>,
    >,
}
impl RuntimeViewState {
    pub fn environment_revision(&self) -> u64 {
        self.environment_revision
    }
    pub fn environment(
        &self,
        world: raf_core::environment::EnvironmentSettings,
    ) -> raf_core::environment::EnvironmentSettings {
        if self.environment_overrides.is_empty() {
            return world;
        }
        if let Some((source, revision, resolved)) = *self.environment_cache.borrow() {
            if source == world && revision == self.environment_revision {
                return resolved;
            }
        }
        // Bounded to the core property catalog; unchanged views reuse the cached value.
        let resolved = world
            .with_view_overrides(&self.environment_overrides)
            .expect("Validated bounded view overrides");
        *self.environment_cache.borrow_mut() = Some((world, self.environment_revision, resolved));
        resolved
    }

    /// Effective quality after local script overrides are layered over the project base.
    pub fn quality(&self) -> RuntimeQuality {
        self.quality_overrides.apply_to(self.quality_base)
    }

    pub fn quality_revision(&self) -> u64 {
        self.quality_revision
    }

    /// Sets one local quality override. Returns whether the effective quality changed.
    pub fn set_quality_override(&mut self, path: &str, value: &str) -> Result<bool, String> {
        let before = self.quality();
        let mut overrides = self.quality_overrides;
        apply_override(path, value, &mut overrides)?;
        self.quality_overrides = overrides;
        let changed = self.quality() != before;
        if changed {
            self.quality_revision = self.quality_revision.wrapping_add(1);
        }
        Ok(changed)
    }

    /// Removes every local quality override. Returns whether an override was present.
    pub fn clear_quality(&mut self) -> bool {
        if self.quality_overrides == RuntimeQualityOverrides::default() {
            return false;
        }
        let before = self.quality();
        self.quality_overrides = RuntimeQualityOverrides::default();
        if self.quality() != before {
            self.quality_revision = self.quality_revision.wrapping_add(1);
        }
        true
    }
}
