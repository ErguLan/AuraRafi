//! Data-driven particle system assets.
//!
//! All effect definitions live in JSON. Rust only knows how to interpret the
//! schema; it never hardcodes fire, smoke, rain or any other effect.

use std::path::{Component, Path};

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Schema version understood by this crate.
pub const PARTICLE_ASSET_VERSION: u32 = 1;

const DEFAULT_MAX_PARTICLES: u32 = 4096;
const DEFAULT_MAX_SPRITES: u32 = 4096;
const MAX_BUDGET: u32 = 1_000_000;
const MAX_EMITTER_PARTICLES: u32 = 1_000_000;
const MAX_BURST: u32 = 1_000_000;
const MAX_RATE: f32 = 100_000.0;
const MIN_LIFETIME: f32 = 0.0001;
const MAX_LIFETIME: f32 = 3600.0;
const MAX_SPEED: f32 = 10_000.0;
const MAX_SIZE: f32 = 100_000.0;
const MAX_SHAPE_EXTENT: f32 = 100_000.0;
const MAX_ACCELERATION: f32 = 100_000.0;
const MAX_FLIPBOOK_DIMENSION: u32 = 1024;

/// Top-level particle system definition. One asset can contain several
/// emitters that share a particle and sprite budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleSystemAsset {
    #[serde(default = "default_version")]
    pub version: u32,
    pub name: String,
    pub budget: ParticleBudget,
    pub emitters: Vec<ParticleEmitter>,
}

/// Hard limits for the whole system and for renderer consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleBudget {
    pub max_particles: u32,
    pub max_sprites: u32,
}

/// A single emitter. Emission direction and shape are expressed in the
/// emitter's local space and transformed by the caller-supplied matrix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleEmitter {
    pub name: String,
    /// Project-relative texture path. It is data only; the simulator never
    /// touches the filesystem.
    pub texture: Option<String>,
    pub enabled: bool,
    pub max_particles: u32,
    /// Spawns per second.
    pub rate: f32,
    /// Particles emitted once at start (and after reset/stop).
    pub burst: u32,
    pub lifetime: [f32; 2],
    pub speed: [f32; 2],
    /// Emission spread around `direction`, in degrees on the local X/Y axes.
    pub spread: [f32; 2],
    pub direction: [f32; 3],
    /// World-space gravity applied to every particle of this emitter.
    pub gravity: [f32; 3],
    pub drag: f32,
    /// Width/height in world units, evaluated over the normalized lifetime.
    pub size: NumberCurve,
    /// Alpha multiplier in `0..=1`, evaluated over the normalized lifetime.
    pub opacity: NumberCurve,
    pub color: ColorGradient,
    pub shape: EmissionShape,
    pub flipbook: Option<Flipbook>,
    pub collision: Option<ParticleCollision>,
    /// When true, drag pulls particles toward the wind velocity instead of
    /// damping absolute velocity while wind is added as acceleration.
    pub wind_affects_drag: bool,
}

/// Local-space spawn volumes. All shapes are centered at the emitter origin.
/// Cones grow along local +Y and discs lie on the local XZ plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmissionShape {
    Point,
    Sphere { radius: f32 },
    Box { half_extents: [f32; 3] },
    Cone { radius: f32, height: f32 },
    Disc { radius: f32, inner: f32 },
}

/// Atlas animation settings. Frame 0 is the top-left cell.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flipbook {
    pub columns: u32,
    pub rows: u32,
    pub fps: f32,
    pub start_random: bool,
}

/// Simple world-space ground plane collision.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleCollision {
    pub ground_y: f32,
    pub bounce: f32,
    pub kill: bool,
}

/// Piecewise-linear scalar curve. An empty curve evaluates to `1.0`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NumberCurve {
    pub keys: Vec<NumberKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NumberKey {
    pub t: f32,
    pub value: f32,
}

/// Piecewise-linear RGBA gradient. Colors use 0..=255 channels. An empty
/// gradient evaluates to opaque white.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorGradient {
    pub keys: Vec<ColorKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorKey {
    pub t: f32,
    pub color: [u8; 4],
}

impl ParticleSystemAsset {
    /// Clamps malformed values in place. Unknown JSON fields are already
    /// ignored by serde, so future schema additions remain loadable.
    ///
    /// Returns human-readable warnings for values that had to be repaired.
    pub fn validate(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();

        if self.version == 0 {
            self.version = PARTICLE_ASSET_VERSION;
            warnings.push("version 0 was replaced with 1".to_string());
        } else if self.version > PARTICLE_ASSET_VERSION {
            warnings.push(format!(
                "asset version {} is newer than supported version {}",
                self.version, PARTICLE_ASSET_VERSION
            ));
        }

        if self.name.trim().is_empty() {
            self.name = "Particles".to_string();
            warnings.push("empty asset name was replaced".to_string());
        }

        if self.budget.max_particles > MAX_BUDGET {
            self.budget.max_particles = MAX_BUDGET;
            warnings.push("budget.max_particles was clamped".to_string());
        }
        if self.budget.max_sprites > MAX_BUDGET {
            self.budget.max_sprites = MAX_BUDGET;
            warnings.push("budget.max_sprites was clamped".to_string());
        }

        for (index, emitter) in self.emitters.iter_mut().enumerate() {
            if emitter.name.trim().is_empty() {
                emitter.name = format!("emitter_{index}");
                warnings.push(format!("emitter {index} had an empty name"));
            }

            let raw_texture = emitter.texture.take();
            let texture_was_present = raw_texture.is_some();
            emitter.texture = sanitize_texture_path(raw_texture);
            if texture_was_present && emitter.texture.is_none() {
                warnings.push(format!(
                    "emitter '{}' had an invalid texture path and it was removed",
                    emitter.name
                ));
            }

            if emitter.max_particles > MAX_EMITTER_PARTICLES {
                emitter.max_particles = MAX_EMITTER_PARTICLES;
                warnings.push(format!(
                    "emitter '{}' max_particles was clamped",
                    emitter.name
                ));
            }
            if emitter.burst > MAX_BURST {
                emitter.burst = MAX_BURST;
                warnings.push(format!("emitter '{}' burst was clamped", emitter.name));
            }

            emitter.rate = clamp_with_warning(
                emitter.rate,
                0.0,
                0.0,
                MAX_RATE,
                &format!("emitter '{}' rate", emitter.name),
                &mut warnings,
            );

            emitter.lifetime[0] = clamp_with_warning(
                emitter.lifetime[0],
                0.5,
                MIN_LIFETIME,
                MAX_LIFETIME,
                &format!("emitter '{}' lifetime[0]", emitter.name),
                &mut warnings,
            );
            emitter.lifetime[1] = clamp_with_warning(
                emitter.lifetime[1],
                1.5,
                MIN_LIFETIME,
                MAX_LIFETIME,
                &format!("emitter '{}' lifetime[1]", emitter.name),
                &mut warnings,
            );
            if emitter.lifetime[1] < emitter.lifetime[0] {
                emitter.lifetime.swap(0, 1);
                warnings.push(format!(
                    "emitter '{}' lifetime range was reordered",
                    emitter.name
                ));
            }

            emitter.speed[0] = clamp_with_warning(
                emitter.speed[0],
                0.0,
                0.0,
                MAX_SPEED,
                &format!("emitter '{}' speed[0]", emitter.name),
                &mut warnings,
            );
            emitter.speed[1] = clamp_with_warning(
                emitter.speed[1],
                0.0,
                0.0,
                MAX_SPEED,
                &format!("emitter '{}' speed[1]", emitter.name),
                &mut warnings,
            );
            if emitter.speed[1] < emitter.speed[0] {
                emitter.speed.swap(0, 1);
                warnings.push(format!(
                    "emitter '{}' speed range was reordered",
                    emitter.name
                ));
            }

            emitter.spread[0] = clamp_with_warning(
                emitter.spread[0],
                0.0,
                0.0,
                180.0,
                &format!("emitter '{}' spread[0]", emitter.name),
                &mut warnings,
            );
            emitter.spread[1] = clamp_with_warning(
                emitter.spread[1],
                0.0,
                0.0,
                180.0,
                &format!("emitter '{}' spread[1]", emitter.name),
                &mut warnings,
            );

            let mut direction = Vec3::from_array(emitter.direction);
            if !direction.is_finite() || direction.length_squared() <= 1.0e-12 {
                direction = Vec3::Y;
                warnings.push(format!(
                    "emitter '{}' direction was replaced with +Y",
                    emitter.name
                ));
            }
            emitter.direction = direction.normalize().to_array();

            for (axis, value) in emitter.gravity.iter_mut().enumerate() {
                *value = clamp_with_warning(
                    *value,
                    0.0,
                    -MAX_ACCELERATION,
                    MAX_ACCELERATION,
                    &format!("emitter '{}' gravity[{axis}]", emitter.name),
                    &mut warnings,
                );
            }

            emitter.drag = clamp_with_warning(
                emitter.drag,
                0.0,
                0.0,
                1000.0,
                &format!("emitter '{}' drag", emitter.name),
                &mut warnings,
            );

            sanitize_number_curve(&mut emitter.size, 0.0, MAX_SIZE);
            sanitize_number_curve(&mut emitter.opacity, 0.0, 1.0);
            sanitize_color_gradient(&mut emitter.color);

            match &mut emitter.shape {
                EmissionShape::Point => {}
                EmissionShape::Sphere { radius } => {
                    *radius = clamp_with_warning(
                        *radius,
                        0.0,
                        0.0,
                        MAX_SHAPE_EXTENT,
                        &format!("emitter '{}' sphere radius", emitter.name),
                        &mut warnings,
                    );
                }
                EmissionShape::Box { half_extents } => {
                    for (axis, value) in half_extents.iter_mut().enumerate() {
                        *value = clamp_with_warning(
                            value.abs(),
                            0.0,
                            0.0,
                            MAX_SHAPE_EXTENT,
                            &format!("emitter '{}' box half_extents[{axis}]", emitter.name),
                            &mut warnings,
                        );
                    }
                }
                EmissionShape::Cone { radius, height } => {
                    *radius = clamp_with_warning(
                        *radius,
                        0.0,
                        0.0,
                        MAX_SHAPE_EXTENT,
                        &format!("emitter '{}' cone radius", emitter.name),
                        &mut warnings,
                    );
                    *height = clamp_with_warning(
                        *height,
                        0.0,
                        0.0,
                        MAX_SHAPE_EXTENT,
                        &format!("emitter '{}' cone height", emitter.name),
                        &mut warnings,
                    );
                }
                EmissionShape::Disc { radius, inner } => {
                    *radius = clamp_with_warning(
                        *radius,
                        0.0,
                        0.0,
                        MAX_SHAPE_EXTENT,
                        &format!("emitter '{}' disc radius", emitter.name),
                        &mut warnings,
                    );
                    *inner = clamp_with_warning(
                        inner.abs(),
                        0.0,
                        0.0,
                        MAX_SHAPE_EXTENT,
                        &format!("emitter '{}' disc inner", emitter.name),
                        &mut warnings,
                    );
                    *inner = (*inner).min(*radius);
                }
            }

            if let Some(flipbook) = &mut emitter.flipbook {
                if flipbook.columns == 0 {
                    flipbook.columns = 1;
                    warnings.push(format!(
                        "emitter '{}' flipbook columns was replaced with 1",
                        emitter.name
                    ));
                }
                if flipbook.rows == 0 {
                    flipbook.rows = 1;
                    warnings.push(format!(
                        "emitter '{}' flipbook rows was replaced with 1",
                        emitter.name
                    ));
                }
                flipbook.columns = flipbook.columns.min(MAX_FLIPBOOK_DIMENSION);
                flipbook.rows = flipbook.rows.min(MAX_FLIPBOOK_DIMENSION);
                flipbook.fps = clamp_with_warning(
                    flipbook.fps,
                    12.0,
                    0.001,
                    1000.0,
                    &format!("emitter '{}' flipbook fps", emitter.name),
                    &mut warnings,
                );
            }

            if let Some(collision) = &mut emitter.collision {
                collision.ground_y = clamp_with_warning(
                    collision.ground_y,
                    0.0,
                    -MAX_ACCELERATION,
                    MAX_ACCELERATION,
                    &format!("emitter '{}' collision ground_y", emitter.name),
                    &mut warnings,
                );
                collision.bounce = clamp_with_warning(
                    collision.bounce,
                    0.0,
                    0.0,
                    1.0,
                    &format!("emitter '{}' collision bounce", emitter.name),
                    &mut warnings,
                );
            }
        }

        warnings
    }
}

impl NumberCurve {
    /// Evaluates the curve at a normalized lifetime in `0..=1`. Values outside
    /// that range clamp to the nearest endpoint. Empty curves return `1.0`.
    pub fn eval(&self, t: f32) -> f32 {
        let Some((left, right, weight)) = neighbor_keys(&self.keys, t, |key| key.t) else {
            return 1.0;
        };

        let left_value = finite_or(left.value, 1.0);
        let right_value = finite_or(right.value, 1.0);
        let value = left_value + (right_value - left_value) * weight;
        finite_or(value, 1.0)
    }
}

impl ColorGradient {
    /// Evaluates the gradient at a normalized lifetime. Channels interpolate
    /// independently and round to the nearest byte. Empty gradients return
    /// opaque white.
    pub fn eval(&self, t: f32) -> [u8; 4] {
        let Some((left, right, weight)) = neighbor_keys(&self.keys, t, |key| key.t) else {
            return [255, 255, 255, 255];
        };

        let mut color = [0u8; 4];
        for (channel, value) in color.iter_mut().enumerate() {
            let a = left.color[channel] as f32;
            let b = right.color[channel] as f32;
            *value = (a + (b - a) * weight).round().clamp(0.0, 255.0) as u8;
        }
        color
    }
}

impl Default for ParticleSystemAsset {
    fn default() -> Self {
        Self {
            version: PARTICLE_ASSET_VERSION,
            name: "Particles".to_string(),
            budget: ParticleBudget::default(),
            emitters: Vec::new(),
        }
    }
}

impl Default for ParticleBudget {
    fn default() -> Self {
        Self {
            max_particles: DEFAULT_MAX_PARTICLES,
            max_sprites: DEFAULT_MAX_SPRITES,
        }
    }
}

impl Default for ParticleEmitter {
    fn default() -> Self {
        Self {
            name: "emitter".to_string(),
            texture: None,
            enabled: true,
            max_particles: 1024,
            rate: 24.0,
            burst: 0,
            lifetime: [0.5, 1.5],
            speed: [0.2, 0.6],
            spread: [15.0, 15.0],
            direction: [0.0, 1.0, 0.0],
            gravity: [0.0, -9.81, 0.0],
            drag: 0.1,
            size: NumberCurve::default(),
            opacity: NumberCurve::default(),
            color: ColorGradient::default(),
            shape: EmissionShape::Point,
            flipbook: None,
            collision: None,
            wind_affects_drag: false,
        }
    }
}

impl Default for EmissionShape {
    fn default() -> Self {
        Self::Point
    }
}

impl Default for Flipbook {
    fn default() -> Self {
        Self {
            columns: 1,
            rows: 1,
            fps: 12.0,
            start_random: false,
        }
    }
}

impl Default for ParticleCollision {
    fn default() -> Self {
        Self {
            ground_y: 0.0,
            bounce: 0.3,
            kill: false,
        }
    }
}

impl Default for NumberKey {
    fn default() -> Self {
        Self { t: 0.0, value: 1.0 }
    }
}

impl Default for ColorKey {
    fn default() -> Self {
        Self {
            t: 0.0,
            color: [255, 255, 255, 255],
        }
    }
}

/// Loads and validates a `.rafparticle` JSON asset.
pub fn load_asset(path: &Path) -> Result<ParticleSystemAsset, String> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read particle asset '{}': {error}",
            path.display()
        )
    })?;

    parse_asset(&source).map_err(|error| format!("{}: {error}", path.display()))
}

/// Parses and validates a particle asset from a JSON string. Useful for
/// editor previews and tests that do not want to touch the filesystem.
pub fn parse_asset(source: &str) -> Result<ParticleSystemAsset, String> {
    let mut asset: ParticleSystemAsset = serde_json::from_str(source)
        .map_err(|error| format!("invalid particle asset JSON: {error}"))?;
    asset.validate();
    Ok(asset)
}

fn default_version() -> u32 {
    PARTICLE_ASSET_VERSION
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn clamp_with_warning(
    value: f32,
    fallback: f32,
    min: f32,
    max: f32,
    label: &str,
    warnings: &mut Vec<String>,
) -> f32 {
    let clamped = if value.is_finite() {
        value.clamp(min, max)
    } else {
        warnings.push(format!("{label} was not finite and was replaced"));
        fallback
    };

    if clamped != value && value.is_finite() {
        warnings.push(format!("{label} was clamped"));
    }
    clamped
}

fn sanitize_texture_path(raw: Option<String>) -> Option<String> {
    let raw = raw?;
    let mut path = raw.trim().replace('\\', "/");
    while let Some(rest) = path.strip_prefix("./") {
        path = rest.to_string();
    }
    path = path.trim().to_string();

    if path.is_empty() {
        return None;
    }
    if path.starts_with('/') {
        return None;
    }
    if path.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    if path.contains('\0') {
        return None;
    }

    let candidate = Path::new(&path);
    if candidate.is_absolute() {
        return None;
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }

    Some(path)
}

fn sanitize_number_curve(curve: &mut NumberCurve, min: f32, max: f32) {
    curve
        .keys
        .retain(|key| key.t.is_finite() && key.value.is_finite());
    for key in &mut curve.keys {
        key.t = key.t.clamp(0.0, 1.0);
        key.value = key.value.clamp(min, max);
    }
    curve.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
}

fn sanitize_color_gradient(gradient: &mut ColorGradient) {
    gradient.keys.retain(|key| key.t.is_finite());
    for key in &mut gradient.keys {
        key.t = key.t.clamp(0.0, 1.0);
    }
    gradient.keys.sort_by(|a, b| a.t.total_cmp(&b.t));
}

fn neighbor_keys<'a, T>(
    keys: &'a [T],
    t: f32,
    key_t: impl Fn(&T) -> f32,
) -> Option<(&'a T, &'a T, f32)> {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mut before: Option<(&T, f32)> = None;
    let mut after: Option<(&T, f32)> = None;

    for key in keys {
        let key_time = key_t(key);
        if !key_time.is_finite() {
            continue;
        }
        let key_time = key_time.clamp(0.0, 1.0);

        if key_time <= t && before.is_none_or(|(_, best)| key_time >= best) {
            before = Some((key, key_time));
        }
        if key_time >= t && after.is_none_or(|(_, best)| key_time <= best) {
            after = Some((key, key_time));
        }
    }

    match (before, after) {
        (Some((left, left_t)), Some((right, right_t))) => {
            let span = right_t - left_t;
            let weight = if span.abs() <= 1.0e-6 {
                0.0
            } else {
                ((t - left_t) / span).clamp(0.0, 1.0)
            };
            Some((left, right, weight))
        }
        (Some((left, _)), None) => Some((left, left, 0.0)),
        (None, Some((right, _))) => Some((right, right, 0.0)),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template_path(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/particles/templates")
            .join(format!("{name}.rafparticle"))
    }

    #[test]
    fn all_templates_parse_and_are_valid() {
        for name in ["fire", "smoke", "sparks", "rain", "dust", "magic"] {
            let asset = load_asset(&template_path(name))
                .unwrap_or_else(|error| panic!("template '{name}' failed: {error}"));

            assert_eq!(asset.version, PARTICLE_ASSET_VERSION, "{name}");
            assert!(!asset.name.trim().is_empty(), "{name}");
            assert!(asset.budget.max_particles > 0, "{name}");
            assert!(asset.budget.max_sprites > 0, "{name}");
            assert!(!asset.emitters.is_empty(), "{name}");
            assert!(asset.emitters.len() <= 3, "{name}");

            for emitter in &asset.emitters {
                assert!(emitter.max_particles > 0, "{name}/{}", emitter.name);
                assert!(emitter.rate >= 0.0, "{name}/{}", emitter.name);
                assert!(emitter.lifetime[0] > 0.0, "{name}/{}", emitter.name);
                assert!(
                    emitter.lifetime[1] >= emitter.lifetime[0],
                    "{name}/{}",
                    emitter.name
                );
                assert!(!emitter.size.keys.is_empty(), "{name}/{}", emitter.name);
                assert!(!emitter.opacity.keys.is_empty(), "{name}/{}", emitter.name);
                assert!(!emitter.color.keys.is_empty(), "{name}/{}", emitter.name);

                let texture = emitter
                    .texture
                    .as_deref()
                    .unwrap_or_else(|| panic!("{name}/{} has no texture", emitter.name));
                assert!(texture.starts_with("particles/textures/"), "{texture}");
            }
        }

        let fire = load_asset(&template_path("fire")).unwrap();
        assert!(fire.emitters.iter().any(
            |emitter| matches!(emitter.flipbook, Some(fb) if fb.columns == 4 && fb.rows == 4)
        ));
        let smoke = load_asset(&template_path("smoke")).unwrap();
        assert!(smoke.emitters.iter().any(
            |emitter| matches!(emitter.flipbook, Some(fb) if fb.columns == 4 && fb.rows == 4)
        ));
    }

    #[test]
    fn missing_version_defaults_to_one_and_unknown_fields_are_ignored() {
        let source = r#"
        {
            "name": "Future Asset",
            "future_root_field": true,
            "budget": { "max_particles": 10, "max_sprites": 10 },
            "emitters": [{
                "name": "future emitter",
                "texture": "particles/textures/soft.png",
                "future_emitter_field": { "nested": 1 },
                "shape": { "type": "sphere", "radius": 0.5, "future_shape_field": 2 }
            }]
        }
        "#;

        let asset = parse_asset(source).expect("asset should parse");
        assert_eq!(asset.version, 1);
        assert_eq!(asset.emitters.len(), 1);
        assert_eq!(
            asset.emitters[0].texture.as_deref(),
            Some("particles/textures/soft.png")
        );
        assert_eq!(
            asset.emitters[0].shape,
            EmissionShape::Sphere { radius: 0.5 }
        );
    }

    #[test]
    fn texture_paths_are_sanitized() {
        let source = r#"
        {
            "name": "Texture Test",
            "budget": { "max_particles": 10, "max_sprites": 10 },
            "emitters": [
                { "name": "empty", "texture": "   " },
                { "name": "parent", "texture": "../secret.png" },
                { "name": "absolute", "texture": "C:\\secret.png" },
                { "name": "slash", "texture": "/secret.png" },
                { "name": "windows", "texture": ".\\particles\\textures\\soft.png" }
            ]
        }
        "#;

        let asset = parse_asset(source).expect("asset should parse");
        assert_eq!(asset.emitters[0].texture, None);
        assert_eq!(asset.emitters[1].texture, None);
        assert_eq!(asset.emitters[2].texture, None);
        assert_eq!(asset.emitters[3].texture, None);
        assert_eq!(
            asset.emitters[4].texture.as_deref(),
            Some("particles/textures/soft.png")
        );
    }

    #[test]
    fn missing_asset_file_returns_an_error() {
        let error = load_asset(Path::new("definitely/not/a/particle/asset.rafparticle"))
            .expect_err("missing file should fail");
        assert!(error.contains("failed to read particle asset"));
    }

    #[test]
    fn invalid_json_returns_an_error() {
        assert!(parse_asset("{ not json").is_err());
        assert!(parse_asset(r#"{"name":"x","emitters":[{"shape":{"type":"unknown"}}]}"#).is_err());
    }

    #[test]
    fn validation_clamps_meaningful_fields() {
        let source = r#"
        {
            "version": 0,
            "name": "",
            "budget": { "max_particles": 5000000, "max_sprites": 4000000 },
            "emitters": [{
                "name": "",
                "texture": "   ",
                "max_particles": 5000000,
                "rate": -4,
                "burst": 5000000,
                "lifetime": [4.0, 0.2],
                "speed": [-3.0, -1.0],
                "spread": [900.0, -2.0],
                "direction": [0, 0, 0],
                "gravity": [0, -9.81, 0],
                "drag": -1,
                "size": { "keys": [{ "t": -1, "value": -2 }, { "t": 4, "value": 5 }] },
                "opacity": { "keys": [{ "t": 0, "value": -1 }, { "t": 1, "value": 8 }] },
                "shape": { "type": "disc", "radius": 2, "inner": 9 },
                "flipbook": { "columns": 0, "rows": 0, "fps": 0 },
                "collision": { "ground_y": 0, "bounce": 9, "kill": false }
            }]
        }
        "#;

        let asset = parse_asset(source).expect("asset should parse");
        assert_eq!(asset.version, 1);
        assert_eq!(asset.budget.max_particles, MAX_BUDGET);
        assert_eq!(asset.budget.max_sprites, MAX_BUDGET);
        assert_eq!(asset.emitters[0].max_particles, MAX_EMITTER_PARTICLES);
        assert_eq!(asset.emitters[0].rate, 0.0);
        assert_eq!(asset.emitters[0].burst, MAX_BURST);
        assert_eq!(asset.emitters[0].lifetime, [0.2, 4.0]);
        assert_eq!(asset.emitters[0].speed, [0.0, 0.0]);
        assert_eq!(asset.emitters[0].spread, [180.0, 0.0]);
        assert_eq!(asset.emitters[0].direction, [0.0, 1.0, 0.0]);
        assert_eq!(asset.emitters[0].drag, 0.0);
        assert_eq!(asset.emitters[0].size.eval(1.0), 5.0);
        assert_eq!(asset.emitters[0].opacity.eval(0.0), 0.0);
        assert_eq!(asset.emitters[0].opacity.eval(1.0), 1.0);
        assert_eq!(
            asset.emitters[0].shape,
            EmissionShape::Disc {
                radius: 2.0,
                inner: 2.0
            }
        );

        let flipbook = asset.emitters[0].flipbook.unwrap();
        assert_eq!(flipbook.columns, 1);
        assert_eq!(flipbook.rows, 1);
        assert!(flipbook.fps > 0.0);
        assert_eq!(asset.emitters[0].collision.unwrap().bounce, 1.0);
    }

    #[test]
    fn number_curves_clamp_interpolate_and_fallback() {
        let empty = NumberCurve::default();
        assert_eq!(empty.eval(0.0), 1.0);
        assert_eq!(empty.eval(0.7), 1.0);

        let curve = NumberCurve {
            keys: vec![
                NumberKey { t: 1.0, value: 3.0 },
                NumberKey { t: 0.0, value: 1.0 },
                NumberKey { t: 0.5, value: 2.0 },
            ],
        };

        assert_eq!(curve.eval(-2.0), 1.0);
        assert_eq!(curve.eval(0.0), 1.0);
        assert_eq!(curve.eval(0.25), 1.5);
        assert_eq!(curve.eval(0.5), 2.0);
        assert_eq!(curve.eval(0.75), 2.5);
        assert_eq!(curve.eval(2.0), 3.0);

        let single = NumberCurve {
            keys: vec![NumberKey { t: 0.5, value: 7.0 }],
        };
        assert_eq!(single.eval(0.0), 7.0);
        assert_eq!(single.eval(1.0), 7.0);
    }

    #[test]
    fn color_gradients_clamp_interpolate_and_fallback() {
        let empty = ColorGradient::default();
        assert_eq!(empty.eval(0.0), [255, 255, 255, 255]);
        assert_eq!(empty.eval(1.0), [255, 255, 255, 255]);

        let gradient = ColorGradient {
            keys: vec![
                ColorKey {
                    t: 1.0,
                    color: [255, 255, 255, 255],
                },
                ColorKey {
                    t: 0.0,
                    color: [0, 0, 0, 0],
                },
            ],
        };

        assert_eq!(gradient.eval(-1.0), [0, 0, 0, 0]);
        assert_eq!(gradient.eval(0.5), [128, 128, 128, 128]);
        assert_eq!(gradient.eval(2.0), [255, 255, 255, 255]);
    }
}
