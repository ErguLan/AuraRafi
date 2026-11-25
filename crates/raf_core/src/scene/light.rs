//! Entity-scoped point and spot lights.
//!
//! World lighting (sun, ambient, sky and the directional shadow map) remains in
//! `crate::environment`. This component adds bounded local lights that are
//! authored on ordinary scene nodes and rendered by the shared scene pipeline.

use serde::{Deserialize, Serialize};

/// Local light shape. Directional sunlight is authored through the world
/// environment and is intentionally not part of this component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LightKind {
    #[default]
    Point,
    Spot,
}

impl LightKind {
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Point => "app.light_point",
            Self::Spot => "app.light_spot",
        }
    }
}

/// Bounded local light authored on a scene entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneLight {
    pub kind: LightKind,
    /// RGB bytes in sRGB space.
    pub color: [u8; 3],
    pub intensity: f32,
    /// Reach in metres. Intensity reaches zero at this distance.
    pub range: f32,
    /// Inner cone in degrees (spot only).
    pub inner_cone_degrees: f32,
    /// Outer cone in degrees (spot only).
    pub outer_cone_degrees: f32,
    pub enabled: bool,
}

impl Default for SceneLight {
    fn default() -> Self {
        Self {
            kind: LightKind::Point,
            color: [255, 226, 170],
            intensity: 1.0,
            range: 8.0,
            inner_cone_degrees: 28.0,
            outer_cone_degrees: 52.0,
            enabled: true,
        }
    }
}

impl SceneLight {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.intensity.is_finite() || self.intensity < 0.0 || self.intensity > 64.0 {
            return Err("Light intensity must be finite and within 0..64.");
        }
        if !self.range.is_finite() || self.range < 0.05 || self.range > 10_000.0 {
            return Err("Light range must be within 0.05..10000 metres.");
        }
        if !self.inner_cone_degrees.is_finite()
            || !self.outer_cone_degrees.is_finite()
            || self.inner_cone_degrees < 0.0
            || self.outer_cone_degrees <= self.inner_cone_degrees
            || self.outer_cone_degrees > 179.0
        {
            return Err("Spot cones require 0 <= inner < outer <= 179 degrees.");
        }
        Ok(())
    }

    pub fn color_linear(&self) -> [f32; 3] {
        [
            srgb_to_linear(self.color[0]),
            srgb_to_linear(self.color[1]),
            srgb_to_linear(self.color[2]),
        ]
    }
}

fn srgb_to_linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_light_is_valid() {
        assert!(SceneLight::default().validate().is_ok());
    }

    #[test]
    fn spot_requires_ordered_cones() {
        let mut light = SceneLight {
            kind: LightKind::Spot,
            ..SceneLight::default()
        };
        light.inner_cone_degrees = 60.0;
        light.outer_cone_degrees = 30.0;
        assert!(light.validate().is_err());
    }

    #[test]
    fn ranges_are_bounded() {
        let mut light = SceneLight::default();
        light.range = 0.0;
        assert!(light.validate().is_err());
        light.range = 8.0;
        light.intensity = f32::NAN;
        assert!(light.validate().is_err());
    }
}
