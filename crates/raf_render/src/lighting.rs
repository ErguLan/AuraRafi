//! Enhanced lighting system for the CPU painter.
//!
//! Adds point lights, specular highlights, and fog to the existing
//! directional-light-only shading. ALL features respect RenderConfig toggles.
//! When disabled (default), this module is zero-cost: not called at all.

use glam::Vec3;

/// A light source in the scene.
#[derive(Debug, Clone)]
pub enum Light {
    /// Infinite directional light (sun). Always active.
    Directional {
        direction: Vec3,
        color: [f32; 3],
        intensity: f32,
    },
    /// Point light with attenuation. Only evaluated if max_point_lights > 0.
    Point {
        position: Vec3,
        color: [f32; 3],
        intensity: f32,
        /// Attenuation radius. Light drops to zero at this distance.
        radius: f32,
    },
    /// Spot light with cone angle. Only evaluated if max_point_lights > 0.
    Spot {
        position: Vec3,
        direction: Vec3,
        color: [f32; 3],
        intensity: f32,
        radius: f32,
        /// Half-angle of the cone in radians.
        cone_angle: f32,
    },
}

/// Lighting environment for a frame.
pub struct LightingEnv {
    /// Ambient light intensity (0.0 - 1.0).
    pub ambient: f32,
    /// Directional light (always present).
    pub sun: Light,
    /// Dynamic lights (only first N evaluated based on config).
    pub point_lights: Vec<Light>,
}

impl Default for LightingEnv {
    fn default() -> Self {
        Self {
            ambient: 0.3,
            sun: Light::Directional {
                direction: Vec3::new(0.5, 0.8, 0.3).normalize(),
                color: [1.0, 0.98, 0.95],
                intensity: 1.0,
            },
            point_lights: Vec::new(),
        }
    }
}

/// Calculate the brightness of a face given its normal, position, and lighting.
///
/// - `normal`: face normal (world space, normalized).
/// - `face_center`: face center position (world space).
/// - `camera_pos`: camera position (for specular).
/// - `env`: lighting environment.
/// - `specular`: enable specular highlights.
/// - `max_points`: max point lights to evaluate.
///
/// Returns brightness in 0.0..=1.0+ (can exceed 1.0 with specular).
pub fn compute_lighting(
    normal: Vec3,
    face_center: Vec3,
    camera_pos: Vec3,
    env: &LightingEnv,
    specular: bool,
    max_points: u32,
) -> f32 {
    let n = normal.normalize();

    // Start with ambient.
    let mut brightness = env.ambient;

    // Directional light.
    if let Light::Directional { direction, intensity, .. } = &env.sun {
        let ndl = n.dot(*direction).max(0.0);
        brightness += ndl * intensity;

        // Specular (Blinn-Phong, cheap).
        if specular && ndl > 0.0 {
            let view_dir = (camera_pos - face_center).normalize();
            let half_dir = (*direction + view_dir).normalize();
            let spec = n.dot(half_dir).max(0.0).powf(32.0);
            brightness += spec * 0.3 * intensity;
        }
    }

    // Point lights (only if configured).
    let count = (max_points as usize).min(env.point_lights.len());
    for light in env.point_lights.iter().take(count) {
        match light {
            Light::Point { position, intensity, radius, .. } => {
                let to_light = *position - face_center;
                let dist = to_light.length();
                if dist < *radius && dist > 0.001 {
                    let dir = to_light / dist;
                    let ndl = n.dot(dir).max(0.0);
                    let attenuation = 1.0 - (dist / radius).powi(2);
                    brightness += ndl * intensity * attenuation;

                    if specular && ndl > 0.0 {
                        let view_dir = (camera_pos - face_center).normalize();
                        let half_dir = (dir + view_dir).normalize();
                        let spec = n.dot(half_dir).max(0.0).powf(32.0);
                        brightness += spec * 0.2 * intensity * attenuation;
                    }
                }
            }
            Light::Spot { position, direction, intensity, radius, cone_angle, .. } => {
                let to_light = *position - face_center;
                let dist = to_light.length();
                if dist < *radius && dist > 0.001 {
                    let dir = to_light / dist;
                    let spot_dot = (-dir).dot(*direction);
                    if spot_dot > cone_angle.cos() {
                        let ndl = n.dot(dir).max(0.0);
                        let attenuation = 1.0 - (dist / radius).powi(2);
                        let spot_factor = ((spot_dot - cone_angle.cos()) / (1.0 - cone_angle.cos())).min(1.0);
                        brightness += ndl * intensity * attenuation * spot_factor;
                    }
                }
            }
            _ => {}
        }
    }

    brightness.clamp(0.0, 2.0)
}

/// Apply fog to a color based on distance from camera.
///
/// Returns the fogged color as [R, G, B] in 0..255.
/// When fog is disabled, this function is never called (zero cost).
pub fn apply_fog(
    color: [u8; 3],
    depth: f32,
    fog_color: [f32; 3],
    fog_start: f32,
    fog_end: f32,
) -> [u8; 3] {
    if depth <= fog_start {
        return color;
    }
    let factor = ((depth - fog_start) / (fog_end - fog_start)).clamp(0.0, 1.0);
    let r = color[0] as f32 / 255.0;
    let g = color[1] as f32 / 255.0;
    let b = color[2] as f32 / 255.0;
    [
        ((r * (1.0 - factor) + fog_color[0] * factor) * 255.0) as u8,
        ((g * (1.0 - factor) + fog_color[1] * factor) * 255.0) as u8,
        ((b * (1.0 - factor) + fog_color[2] * factor) * 255.0) as u8,
    ]
}

/// Simple bloom approximation for cpu painter.
///
/// Given a brightness value, returns an additive glow factor.
/// Only applied to faces brighter than the threshold.
/// Zero cost when bloom_enabled = false (never called).
pub fn bloom_factor(brightness: f32, intensity: f32) -> f32 {
    let threshold = 0.8;
    if brightness > threshold {
        (brightness - threshold) * intensity
    } else {
        0.0
    }
}
