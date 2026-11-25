//! Bounded entity-light collection shared by the CPU and GPU scene paths.

use glam::Vec3;
use raf_core::scene::{LightKind, SceneGraph};

pub const MAX_ENTITY_LIGHTS: usize = 8;

/// Frame-ready local lights. Four vec4 words per light:
/// `[position.xyz, range]`, `[color.rgb, intensity]`,
/// `[direction.xyz, kind]` and `[inner_cos, outer_cos, falloff, 0]`.
#[derive(Debug, Clone, Copy)]
pub struct EntityLightFrame {
    pub words: [[f32; 4]; MAX_ENTITY_LIGHTS * 4],
    pub count: u32,
}

impl Default for EntityLightFrame {
    fn default() -> Self {
        Self {
            words: [[0.0; 4]; MAX_ENTITY_LIGHTS * 4],
            count: 0,
        }
    }
}

impl EntityLightFrame {
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Additional diffuse contribution from local lights at a world position.
    pub fn shade(&self, normal: Vec3, position: Vec3) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        let n = normal.normalize_or_zero();
        let mut result = 0.0;
        for index in 0..(self.count as usize).min(MAX_ENTITY_LIGHTS) {
            let base = index * 4;
            let [px, py, pz, range] = self.words[base];
            let [r, g, b, intensity] = self.words[base + 1];
            let [dx, dy, dz, kind] = self.words[base + 2];
            let [inner_cos, outer_cos, falloff, _] = self.words[base + 3];
            let to_light = Vec3::new(px, py, pz) - position;
            let distance_sq = to_light.length_squared();
            if distance_sq <= 1e-8 || range <= 0.0 {
                continue;
            }
            let distance = distance_sq.sqrt();
            if distance > range {
                continue;
            }
            let attenuation = (1.0 - distance / range).clamp(0.0, 1.0).powf(falloff.max(1.0));
            let direction = to_light / distance;
            let mut diffuse = n.dot(direction).max(0.0);
            if kind > 0.5 {
                let spot_cos = (-direction).dot(Vec3::new(dx, dy, dz).normalize_or_zero());
                diffuse *= smoothstep(outer_cos, inner_cos, spot_cos);
            }
            let luminance = r * 0.2126 + g * 0.7152 + b * 0.0722;
            result += diffuse * attenuation * intensity * luminance;
        }
        result
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return f32::from(value >= edge1);
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Choose at most [`MAX_ENTITY_LIGHTS`] enabled local lights, prioritizing
/// lights that actually reach the camera focus region.
pub fn collect_entity_lights(scene: &SceneGraph, focus: Vec3) -> EntityLightFrame {
    let mut candidates = Vec::new();
    for (id, node) in scene.iter() {
        let Some(light) = node.light.as_ref() else {
            continue;
        };
        if !light.enabled || light.intensity <= 0.0 || light.range <= 0.0 {
            continue;
        }
        let world = scene.world_matrix(id);
        let position = world.transform_point3(Vec3::ZERO);
        if !position.is_finite() {
            continue;
        }
        let distance = position.distance(focus);
        let influence = light.intensity * light.range / (1.0 + distance * distance);
        if !influence.is_finite() {
            continue;
        }
        candidates.push((influence, position, world, light.clone()));
    }
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut frame = EntityLightFrame::default();
    for (_, position, world, light) in candidates.into_iter().take(MAX_ENTITY_LIGHTS) {
        let slot = frame.count as usize * 4;
        let color = light.color_linear();
        let intensity = light.intensity;
        let direction = -world.z_axis.truncate().normalize_or_zero();
        let (kind, inner_cos, outer_cos) = if light.kind == LightKind::Spot {
            (
                1.0,
                light.inner_cone_degrees.to_radians().cos(),
                light.outer_cone_degrees.to_radians().cos(),
            )
        } else {
            // Point lights ignore the cone words; keep them valid for the
            // shared uniform validator.
            (0.0, 1.0, 1.0)
        };
        frame.words[slot] = [position.x, position.y, position.z, light.range];
        frame.words[slot + 1] = [color[0], color[1], color[2], intensity];
        frame.words[slot + 2] = [direction.x, direction.y, direction.z, kind];
        frame.words[slot + 3] = [inner_cos, outer_cos, 2.0, 0.0];
        frame.count += 1;
    }
    frame
}
