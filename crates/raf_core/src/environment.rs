//! World-scoped lighting data and the property contract shared by every adapter.
//! Angles are radians, time is seconds, and authored colors are sRGB bytes.
use glam::Vec3;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::f32::consts::{FRAC_PI_2, TAU};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentMode {
    #[default]
    Neutral,
    Scene,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SunControl {
    #[default]
    Manual,
    Clock,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyMode {
    Solid,
    #[default]
    Gradient,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SunSettings {
    pub enabled: bool,
    pub control: SunControl,
    pub azimuth: f32,
    pub elevation: f32,
    pub color: [u8; 3],
    pub intensity: f32,
    pub disk_visible: bool,
    pub angular_radius: f32,
    pub toon_enabled: bool,
    pub toon_steps: f32,
}
impl Default for SunSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            control: SunControl::Manual,
            azimuth: 0.5880026,
            elevation: 0.837215,
            color: [255, 250, 242],
            intensity: 0.7,
            disk_visible: true,
            angular_radius: 0.00465,
            toon_enabled: false,
            toon_steps: 3.0,
        }
    }
}
impl SunSettings {
    pub fn shading_steps(&self) -> f32 {
        if self.toon_enabled {
            self.toon_steps
        } else {
            0.0
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkySettings {
    pub mode: SkyMode,
    pub top_color: [u8; 3],
    pub horizon_color: [u8; 3],
    pub ground_color: [u8; 3],
    pub intensity: f32,
    pub stars_enabled: bool,
    pub stars_density: f32,
    pub stars_intensity: f32,
}
impl Default for SkySettings {
    fn default() -> Self {
        Self {
            mode: SkyMode::Gradient,
            top_color: [70, 130, 205],
            horizon_color: [200, 221, 240],
            ground_color: [90, 100, 115],
            intensity: 1.0,
            stars_enabled: false,
            stars_density: 0.35,
            stars_intensity: 2.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EnvironmentClock {
    pub running: bool,
    pub time_seconds: f32,
    pub day_duration: f32,
    pub north_offset: f32,
}
impl Default for EnvironmentClock {
    fn default() -> Self {
        Self {
            running: false,
            time_seconds: 43200.0,
            day_duration: 1200.0,
            north_offset: 0.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShadowSettings {
    pub enabled: bool,
    /// One bounded directional map. Resolution must be 256, 512 or 1024.
    pub resolution: f32,
    /// World-space radius around the camera; the outer 20% fades to unshadowed.
    pub distance: f32,
    pub soft_filter: bool,
    /// World-space receiver bias, scaled by surface slope.
    pub bias: f32,
    pub strength: f32,
}
impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            resolution: 512.0,
            distance: 32.0,
            soft_filter: false,
            bias: 0.025,
            strength: 1.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EnvironmentSettings {
    pub version: u32,
    pub mode: EnvironmentMode,
    pub sun: SunSettings,
    pub sky: SkySettings,
    pub ambient_color: [u8; 3],
    pub ambient_intensity: f32,
    pub clock: EnvironmentClock,
    pub shadows: ShadowSettings,
}
impl Default for EnvironmentSettings {
    fn default() -> Self {
        Self {
            version: 1,
            mode: EnvironmentMode::Neutral,
            sun: Default::default(),
            sky: Default::default(),
            ambient_color: [255; 3],
            ambient_intensity: 0.3,
            clock: Default::default(),
            shadows: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EnvironmentPropertyKind {
    Bool,
    Number {
        min: f32,
        max: f32,
        display_scale: f32,
        step: f32,
    },
    Color,
    Choice(&'static [&'static str]),
}
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentProperty {
    pub path: &'static str,
    pub label: &'static str,
    pub kind: EnvironmentPropertyKind,
}
use EnvironmentPropertyKind::{Bool, Choice, Color, Number};
pub const ENVIRONMENT_PROPERTIES: &[EnvironmentProperty] = &[
    EnvironmentProperty {
        path: "sun.toon_enabled",
        label: "lighting.toon_enabled",
        kind: Bool,
    },
    EnvironmentProperty {
        path: "sun.toon_steps",
        label: "lighting.toon_steps",
        kind: Number {
            min: 2.0,
            max: 6.0,
            display_scale: 1.0,
            step: 1.0,
        },
    },
    EnvironmentProperty {
        path: "sky.stars_enabled",
        label: "lighting.stars_enabled",
        kind: Bool,
    },
    EnvironmentProperty {
        path: "sky.stars_density",
        label: "lighting.stars_density",
        kind: Number {
            min: 0.0,
            max: 1.0,
            display_scale: 1.0,
            step: 0.05,
        },
    },
    EnvironmentProperty {
        path: "sky.stars_intensity",
        label: "lighting.stars_intensity",
        kind: Number {
            min: 0.0,
            max: 4.0,
            display_scale: 1.0,
            step: 0.1,
        },
    },
    EnvironmentProperty {
        path: "shadows.enabled",
        label: "lighting.shadow_enabled",
        kind: Bool,
    },
    EnvironmentProperty {
        path: "shadows.resolution",
        label: "lighting.shadow_resolution",
        kind: Number {
            min: 256.0,
            max: 1024.0,
            display_scale: 1.0,
            step: 256.0,
        },
    },
    EnvironmentProperty {
        path: "shadows.distance",
        label: "lighting.shadow_distance",
        kind: Number {
            min: 2.0,
            max: 256.0,
            display_scale: 1.0,
            step: 1.0,
        },
    },
    EnvironmentProperty {
        path: "shadows.soft_filter",
        label: "lighting.shadow_filter",
        kind: Bool,
    },
    EnvironmentProperty {
        path: "shadows.bias",
        label: "lighting.shadow_bias",
        kind: Number {
            min: 0.0,
            max: 1.0,
            display_scale: 1.0,
            step: 0.005,
        },
    },
    EnvironmentProperty {
        path: "shadows.strength",
        label: "lighting.shadow_strength",
        kind: Number {
            min: 0.0,
            max: 1.0,
            display_scale: 1.0,
            step: 0.05,
        },
    },
    EnvironmentProperty {
        path: "mode",
        label: "lighting.mode",
        kind: Choice(&["neutral", "scene"]),
    },
    EnvironmentProperty {
        path: "sun.enabled",
        label: "lighting.sun_enabled",
        kind: Bool,
    },
    EnvironmentProperty {
        path: "sun.control",
        label: "lighting.sun_control",
        kind: Choice(&["manual", "clock"]),
    },
    EnvironmentProperty {
        path: "sun.azimuth",
        label: "lighting.azimuth",
        kind: Number {
            min: 0.0,
            max: TAU,
            display_scale: 57.29578,
            step: 1.0,
        },
    },
    EnvironmentProperty {
        path: "sun.elevation",
        label: "lighting.elevation",
        kind: Number {
            min: -FRAC_PI_2,
            max: FRAC_PI_2,
            display_scale: 57.29578,
            step: 1.0,
        },
    },
    EnvironmentProperty {
        path: "sun.color",
        label: "lighting.sun_color",
        kind: Color,
    },
    EnvironmentProperty {
        path: "sun.intensity",
        label: "lighting.sun_intensity",
        kind: Number {
            min: 0.0,
            max: 8.0,
            display_scale: 1.0,
            step: 0.05,
        },
    },
    EnvironmentProperty {
        path: "sun.disk_visible",
        label: "lighting.disk_visible",
        kind: Bool,
    },
    EnvironmentProperty {
        path: "sun.angular_radius",
        label: "lighting.angular_radius",
        kind: Number {
            min: 0.001,
            max: 0.15,
            display_scale: 57.29578,
            step: 0.05,
        },
    },
    EnvironmentProperty {
        path: "ambient_color",
        label: "lighting.ambient_color",
        kind: Color,
    },
    EnvironmentProperty {
        path: "ambient_intensity",
        label: "lighting.ambient_intensity",
        kind: Number {
            min: 0.0,
            max: 2.0,
            display_scale: 1.0,
            step: 0.05,
        },
    },
    EnvironmentProperty {
        path: "sky.mode",
        label: "lighting.sky_mode",
        kind: Choice(&["solid", "gradient"]),
    },
    EnvironmentProperty {
        path: "sky.top_color",
        label: "lighting.top_color",
        kind: Color,
    },
    EnvironmentProperty {
        path: "sky.horizon_color",
        label: "lighting.horizon_color",
        kind: Color,
    },
    EnvironmentProperty {
        path: "sky.ground_color",
        label: "lighting.ground_color",
        kind: Color,
    },
    EnvironmentProperty {
        path: "sky.intensity",
        label: "lighting.sky_intensity",
        kind: Number {
            min: 0.0,
            max: 4.0,
            display_scale: 1.0,
            step: 0.05,
        },
    },
    EnvironmentProperty {
        path: "clock.running",
        label: "lighting.clock_running",
        kind: Bool,
    },
    EnvironmentProperty {
        path: "clock.time_seconds",
        label: "lighting.time",
        kind: Number {
            min: 0.0,
            max: 86400.0,
            display_scale: 1.0 / 3600.0,
            step: 0.1,
        },
    },
    EnvironmentProperty {
        path: "clock.day_duration",
        label: "lighting.day_duration",
        kind: Number {
            min: 1.0,
            max: 86400.0,
            display_scale: 1.0,
            step: 1.0,
        },
    },
    EnvironmentProperty {
        path: "clock.north_offset",
        label: "lighting.north_offset",
        kind: Number {
            min: 0.0,
            max: TAU,
            display_scale: 57.29578,
            step: 1.0,
        },
    },
];

pub fn environment_property(path: &str) -> Result<&'static EnvironmentProperty, String> {
    ENVIRONMENT_PROPERTIES
        .iter()
        .find(|p| p.path == path)
        .ok_or_else(|| format!("Unknown environment property: {path}"))
}
fn pointer(path: &str) -> String {
    format!("/{}", path.replace('.', "/"))
}
impl EnvironmentSettings {
    /// Stable, allocation-free frame key; independent from document identities.
    pub fn render_fingerprint(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        let mut mix = |v: u32| {
            hash = (hash ^ v as u64).wrapping_mul(0x100000001b3);
        };
        mix(self.mode as u32);
        mix(self.sun.enabled as u32);
        mix(self.sun.control as u32);
        mix(self.sun.toon_enabled as u32);
        mix(self.sun.toon_steps.to_bits());
        mix(self.sky.stars_enabled as u32);
        mix(self.sky.stars_density.to_bits());
        mix(self.sky.stars_intensity.to_bits());
        for n in [
            self.sun.azimuth,
            self.sun.elevation,
            self.sun.intensity,
            self.sun.angular_radius,
            self.sky.intensity,
            self.ambient_intensity,
            self.clock.time_seconds,
            self.clock.north_offset,
        ] {
            mix(n.to_bits());
        }
        mix(self.sun.disk_visible as u32);
        mix(self.shadows.enabled as u32);
        mix(self.shadows.resolution.to_bits());
        mix(self.shadows.distance.to_bits());
        mix(self.shadows.soft_filter as u32);
        mix(self.shadows.bias.to_bits());
        mix(self.shadows.strength.to_bits());
        mix(self.sky.mode as u32);
        for color in [
            self.sun.color,
            self.ambient_color,
            self.sky.top_color,
            self.sky.horizon_color,
            self.sky.ground_color,
        ] {
            for channel in color {
                mix(channel as u32);
            }
        }
        hash
    }
    pub fn get_property(&self, path: &str) -> Result<Value, String> {
        // Script reads and numeric validation must not serialize the entire world environment.
        use serde_json::json;
        Ok(match path {
            "sun.toon_enabled" => json!(self.sun.toon_enabled),
            "sun.toon_steps" => json!(self.sun.toon_steps),
            "sky.stars_enabled" => json!(self.sky.stars_enabled),
            "sky.stars_density" => json!(self.sky.stars_density),
            "sky.stars_intensity" => json!(self.sky.stars_intensity),
            "shadows.enabled" => json!(self.shadows.enabled),
            "shadows.resolution" => json!(self.shadows.resolution),
            "shadows.distance" => json!(self.shadows.distance),
            "shadows.soft_filter" => json!(self.shadows.soft_filter),
            "shadows.bias" => json!(self.shadows.bias),
            "shadows.strength" => json!(self.shadows.strength),
            "mode" => json!(self.mode),
            "sun.enabled" => json!(self.sun.enabled),
            "sun.control" => json!(self.sun.control),
            "sun.azimuth" => json!(self.sun.azimuth),
            "sun.elevation" => json!(self.sun.elevation),
            "sun.color" => json!(self.sun.color),
            "sun.intensity" => json!(self.sun.intensity),
            "sun.disk_visible" => json!(self.sun.disk_visible),
            "sun.angular_radius" => json!(self.sun.angular_radius),
            "ambient_color" => json!(self.ambient_color),
            "ambient_intensity" => json!(self.ambient_intensity),
            "sky.mode" => json!(self.sky.mode),
            "sky.top_color" => json!(self.sky.top_color),
            "sky.horizon_color" => json!(self.sky.horizon_color),
            "sky.ground_color" => json!(self.sky.ground_color),
            "sky.intensity" => json!(self.sky.intensity),
            "clock.running" => json!(self.clock.running),
            "clock.time_seconds" => json!(self.clock.time_seconds),
            "clock.day_duration" => json!(self.clock.day_duration),
            "clock.north_offset" => json!(self.clock.north_offset),
            _ => return Err(format!("Unknown environment property: {path}")),
        })
    }
    /// Validate a bounded batch atomically. Unknown fields and invalid values never partially apply.
    pub fn patched(&self, patch: &serde_json::Map<String, Value>) -> Result<Self, String> {
        self.patch_inner(patch, true)
    }
    /// Resolve already-authored local presentation overrides against a changed world.
    /// Dormant manual angles are retained when the world switches to clock control.
    pub fn with_view_overrides(
        &self,
        patch: &serde_json::Map<String, Value>,
    ) -> Result<Self, String> {
        self.patch_inner(patch, false)
    }
    fn patch_inner(
        &self,
        patch: &serde_json::Map<String, Value>,
        require_manual_driver: bool,
    ) -> Result<Self, String> {
        if patch.len() > ENVIRONMENT_PROPERTIES.len() {
            return Err("Environment patch budget exceeded".into());
        }
        let mut data = serde_json::to_value(self).map_err(|e| e.to_string())?;
        for (path, value) in patch {
            let property = environment_property(path)?;
            let valid = match property.kind {
                Bool => value.is_boolean(),
                Number { min, max, .. } => value
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && n >= min as f64 && n <= max as f64),
                Choice(choices) => value.as_str().is_some_and(|s| choices.contains(&s)),
                Color => value.as_array().is_some_and(|v| {
                    v.len() == 3 && v.iter().all(|c| c.as_u64().is_some_and(|n| n <= 255))
                }),
            };
            if !valid {
                return Err(format!("Invalid value for {path}"));
            }
            *data
                .pointer_mut(&pointer(path))
                .ok_or("Missing environment field")? = value.clone();
        }
        let next: Self = serde_json::from_value(data).map_err(|e| e.to_string())?;
        if require_manual_driver
            && next.sun.control == SunControl::Clock
            && (patch.contains_key("sun.azimuth") || patch.contains_key("sun.elevation"))
        {
            return Err("Switch sun.control to manual before setting its angles".into());
        }
        next.validate()?;
        Ok(next)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.sun.toon_steps.fract() != 0.0 {
            return Err("Sun toon steps must be an integer from 2 to 6".into());
        }
        if ![256.0, 512.0, 1024.0].contains(&self.shadows.resolution) {
            return Err("Shadow resolution must be 256, 512 or 1024".into());
        }
        if self.version != 1 {
            return Err("Unsupported environment version".into());
        }
        for p in ENVIRONMENT_PROPERTIES {
            if let Number { min, max, .. } = p.kind {
                let n = self
                    .get_property(p.path)?
                    .as_f64()
                    .ok_or("Invalid environment number")?;
                if !n.is_finite() || n < min as f64 || n > max as f64 {
                    return Err(format!("{} outside allowed range", p.path));
                }
            }
        }
        Ok(())
    }
    pub fn sun_direction(&self) -> Vec3 {
        if self.sun.control == SunControl::Clock {
            let phase = (self.clock.time_seconds / 86400.0 - 0.25) * TAU;
            let north = self.clock.north_offset;
            Vec3::new(
                phase.cos() * north.cos(),
                phase.sin(),
                phase.cos() * north.sin(),
            )
            .normalize_or_zero()
        } else {
            let (az, el) = (self.sun.azimuth, self.sun.elevation);
            Vec3::new(el.cos() * az.sin(), el.sin(), el.cos() * az.cos()).normalize_or_zero()
        }
    }
    /// No clock job runs in neutral mode or for a stopped/manual clock.
    pub fn advanced(&self, dt: f32) -> Self {
        let mut next = *self;
        if self.mode == EnvironmentMode::Scene
            && self.sun.control == SunControl::Clock
            && self.clock.running
            && dt.is_finite()
            && dt > 0.0
        {
            next.clock.time_seconds = (self.clock.time_seconds
                + dt * 86400.0 / self.clock.day_duration)
                .rem_euclid(86400.0);
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stylized_environment_migrates_bounds_and_invalidates_frame_keys() {
        let env = EnvironmentSettings::default();
        let mut legacy = serde_json::to_value(env).unwrap();
        for key in ["toon_enabled", "toon_steps"] {
            legacy["sun"].as_object_mut().unwrap().remove(key);
        }
        for key in ["stars_enabled", "stars_density", "stars_intensity"] {
            legacy["sky"].as_object_mut().unwrap().remove(key);
        }
        let restored: EnvironmentSettings = serde_json::from_value(legacy).unwrap();
        assert_eq!(env, restored);
        assert_eq!(env.sun.shading_steps(), 0.0);
        for (path, value) in [
            ("sun.toon_enabled", serde_json::json!(true)),
            ("sun.toon_steps", serde_json::json!(4)),
            ("sky.stars_enabled", serde_json::json!(true)),
            ("sky.stars_density", serde_json::json!(0.9)),
            ("sky.stars_intensity", serde_json::json!(3.0)),
        ] {
            let patch = serde_json::Map::from_iter([(path.to_string(), value)]);
            let changed = env.patched(&patch).unwrap();
            assert_ne!(
                env.render_fingerprint(),
                changed.render_fingerprint(),
                "{path}"
            );
            assert_eq!(
                changed,
                ron::from_str::<EnvironmentSettings>(&ron::to_string(&changed).unwrap()).unwrap()
            );
        }
        for patch in [
            serde_json::json!({"sun.toon_steps":2.5}),
            serde_json::json!({"sun.toon_steps":7}),
            serde_json::json!({"sky.stars_density":1.1}),
            serde_json::json!({"sky.stars_intensity":-1}),
        ] {
            assert!(env.patched(patch.as_object().unwrap()).is_err());
        }
    }
    #[test]
    fn shadows_migrate_validate_and_change_frame_keys() {
        let mut old = serde_json::to_value(EnvironmentSettings::default()).unwrap();
        old.as_object_mut().unwrap().remove("shadows");
        let env: EnvironmentSettings = serde_json::from_value(old).unwrap();
        assert!(!env.shadows.enabled);
        let patch = serde_json::json!({"shadows.enabled":true,"shadows.resolution":1024.0,"shadows.bias":0.1});
        let next = env.patched(patch.as_object().unwrap()).unwrap();
        assert_ne!(env.render_fingerprint(), next.render_fingerprint());
        assert!(next.shadows.enabled);
        for invalid in [
            serde_json::json!(768),
            serde_json::json!(512.5),
            serde_json::json!(2048),
        ] {
            assert!(env
                .patched(
                    serde_json::json!({"shadows.resolution":invalid})
                        .as_object()
                        .unwrap()
                )
                .is_err());
        }
        let mut graph = crate::SceneGraph::new();
        let id = graph.add_root_with_primitive("Cube", crate::scene::Primitive::Cube);
        let key = graph.render_fingerprint();
        graph.get_mut(id).unwrap().casts_shadows = false;
        assert_ne!(key, graph.render_fingerprint());
        let old = serde_json::to_value(graph.get(id).unwrap()).unwrap();
        let mut old = old.as_object().unwrap().clone();
        old.remove("casts_shadows");
        old.remove("receives_shadows");
        let node: crate::scene::SceneNode =
            serde_json::from_value(serde_json::Value::Object(old)).unwrap();
        assert!(node.casts_shadows && node.receives_shadows);
    }
    #[test]
    fn lighting_legacy_scene_migrates_to_neutral_and_keys_are_stable() {
        let scene = crate::SceneGraph::new();
        let mut old = serde_json::to_value(&scene).unwrap();
        old.as_object_mut().unwrap().remove("environment");
        let restored: crate::SceneGraph = serde_json::from_value(old).unwrap();
        assert_eq!(restored.environment().mode, EnvironmentMode::Neutral);
        assert_eq!(scene.render_fingerprint(), restored.render_fingerprint());
        let serialized = ron::to_string(&scene).unwrap();
        let restored: crate::SceneGraph = ron::from_str(&serialized).unwrap();
        assert_eq!(scene.render_fingerprint(), restored.render_fingerprint());
    }
    #[test]
    fn lighting_defaults_preserve_legacy_and_patch_is_atomic() {
        let env = EnvironmentSettings::default();
        let serialized = serde_json::to_value(env).unwrap();
        for property in ENVIRONMENT_PROPERTIES {
            assert_eq!(
                env.get_property(property.path).unwrap(),
                *serialized.pointer(&pointer(property.path)).unwrap()
            );
        }
        assert_eq!(env.mode, EnvironmentMode::Neutral);
        let patch = serde_json::json!({"mode":"scene", "sun.intensity":-1.0});
        assert!(env.patched(patch.as_object().unwrap()).is_err());
        assert_eq!(env.mode, EnvironmentMode::Neutral);
        assert!(env
            .patched(
                serde_json::json!({"shadows.resolution":768})
                    .as_object()
                    .unwrap()
            )
            .is_err());
    }
    #[test]
    fn lighting_clock_uses_simulation_time_and_has_an_explicit_driver() {
        let mut env = EnvironmentSettings::default();
        env.mode = EnvironmentMode::Scene;
        env.sun.control = SunControl::Clock;
        env.clock.running = true;
        env.clock.time_seconds = 43200.0;
        assert!(env.sun_direction().y > 0.99);
        assert_ne!(env.advanced(1.0), env);
        env.mode = EnvironmentMode::Neutral;
        assert_eq!(env.advanced(1.0), env);
        env.mode = EnvironmentMode::Scene;
        assert!(env
            .patched(
                serde_json::json!({"sun.elevation":0.1})
                    .as_object()
                    .unwrap()
            )
            .is_err());
    }
}
