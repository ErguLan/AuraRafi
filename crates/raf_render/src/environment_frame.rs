//! Resolved, backend-neutral lighting. No scene mutation or background jobs.
use crate::{
    render_pipeline::framebuffer::Framebuffer,
    texture::{linear_to_srgb, srgb_to_linear},
};
use glam::{Mat4, Vec3};
use raf_core::environment::{EnvironmentMode, EnvironmentSettings, SkyMode};

#[derive(Debug, Clone, Copy)]
pub struct EnvironmentFrame {
    pub settings: EnvironmentSettings,
    pub inverse_view_projection: Mat4,
    pub sun_direction: Vec3,
    pub sun: Vec3,
    pub ambient: Vec3,
    pub sky_top: Vec3,
    pub sky_horizon: Vec3,
    pub sky_ground: Vec3,
    /// Sky-only glow around the solar disk; additive, never lights bodies.
    pub halo_strength: f32,
    pub halo_cos: f32,
}
pub fn linear_color(color: [u8; 3]) -> Vec3 {
    Vec3::from_array(color.map(srgb_to_linear))
}
impl EnvironmentFrame {
    pub fn improved(&self) -> bool {
        self.settings.technology == raf_core::environment::LightingTechnology::OptimizedImproved
    }
    pub fn exposure(&self) -> f32 {
        if self.improved() {
            self.settings.exposure
        } else {
            0.0
        }
    }
    pub fn softness(&self) -> f32 {
        if self.improved() {
            self.settings.sun.toon_softness
        } else {
            0.0
        }
    }
    pub fn new(settings: EnvironmentSettings, view_projection: Mat4) -> Option<Self> {
        if settings.mode == EnvironmentMode::Neutral {
            return None;
        }
        let sun_direction = settings.sun_direction();
        let direct = if settings.sun.enabled && sun_direction.y > 0.0 {
            settings.sun.intensity
        } else {
            0.0
        };
        let halo_angle = settings.sun.angular_radius.max(0.00465) * 24.0;
        Some(Self {
            settings,
            inverse_view_projection: view_projection.inverse(),
            sun_direction,
            sun: linear_color(settings.sun.color) * direct,
            ambient: linear_color(settings.ambient_color) * settings.ambient_intensity,
            sky_top: linear_color(settings.sky.top_color) * settings.sky.intensity,
            sky_horizon: linear_color(settings.sky.horizon_color) * settings.sky.intensity,
            sky_ground: linear_color(settings.sky.ground_color) * settings.sky.intensity,
            halo_strength: if direct > 0.0 {
                settings.sun.halo.clamp(0.0, 1.0)
            } else {
                0.0
            },
            halo_cos: halo_angle.cos(),
        })
    }
    pub fn sky_color(&self, direction: Vec3) -> Vec3 {
        self.sky_sample(direction, 0.0)
    }
    fn sky_sample(&self, direction: Vec3, footprint: f32) -> Vec3 {
        let sky = self.settings.sky;
        let top = self.sky_top;
        let horizon = self.sky_horizon;
        let ground = self.sky_ground;
        // Haze compresses the gradient toward the horizon color. Current
        // technology keeps the legacy linear ramp exactly.
        let mut color = if sky.mode == SkyMode::Solid {
            top
        } else {
            let vertical = direction.y.abs().clamp(0.0, 1.0);
            let eased = if self.improved() {
                vertical.powf(1.0 + 2.0 * sky.haze.clamp(0.0, 1.0))
            } else {
                vertical
            };
            if direction.y >= 0.0 {
                horizon.lerp(top, eased)
            } else {
                horizon.lerp(ground, eased)
            }
        };
        if sky.stars_enabled && sky.stars_density > 0.0 && sky.stars_intensity > 0.0 {
            let brightness = if self.improved() {
                crate::sky_stars::brightness_filtered(direction, sky.stars_density, footprint)
            } else {
                crate::sky_stars::brightness(direction, sky.stars_density)
            };
            let chroma = if self.improved() {
                crate::sky_stars::star_tint(direction, sky.stars_density)
            } else {
                Vec3::new(0.72, 0.86, 1.0)
            };
            color += chroma * brightness * sky.stars_intensity;
        }
        if self.improved() && self.halo_strength > 0.0 {
            let glow = ((direction.dot(self.sun_direction) - self.halo_cos)
                / (1.0 - self.halo_cos).max(1e-6))
            .clamp(0.0, 1.0);
            color += self.sun * (glow * glow) * self.halo_strength;
        }
        if self.settings.sun.enabled && self.settings.sun.disk_visible && self.sun_direction.y > 0.0
        {
            let edge = self.settings.sun.angular_radius.cos();
            let amount = ((direction.dot(self.sun_direction) - edge) / (1.0 - edge).max(1e-7))
                .clamp(0.0, 1.0);
            let amount = if self.improved() {
                amount * amount * (3.0 - 2.0 * amount)
            } else {
                amount
            };
            color = color.lerp(self.sun, amount);
        }
        if self.improved() {
            crate::optimized_lighting::expose(color, self.settings.exposure)
        } else {
            color
        }
    }
    pub fn rasterize_sky(&self, fb: &mut Framebuffer) {
        let width = fb.width();
        let height = fb.height();
        // One fullscreen background, no intermediate image or depth writes.
        let ray = |x, y| {
            self.inverse_view_projection
                .project_point3(Vec3::new(x, y, 0.9))
                - self
                    .inverse_view_projection
                    .project_point3(Vec3::new(x, y, 0.0))
        };
        let first = ray(-1.0 + 1.0 / width as f32, 1.0 - 1.0 / height as f32);
        let step_x = (ray(1.0, 1.0) - ray(-1.0, 1.0)) / width as f32;
        let step_y = (ray(-1.0, -1.0) - ray(-1.0, 1.0)) / height as f32;
        let footprint = if self.improved() {
            (step_x.length() + step_y.length()) / first.length().max(1e-6) * 10.0
        } else {
            0.0
        };
        for y in 0..height {
            for x in 0..width {
                let direction = (first + step_x * x as f32 + step_y * y as f32).normalize_or_zero();
                let color = self
                    .sky_sample(direction, footprint)
                    .to_array()
                    .map(linear_to_srgb);
                fb.write_pixel_no_depth(x, y, color[0], color[1], color[2], 255);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lighting_cpu_output_changes_and_neutral_override_restores_legacy() {
        use crate::{
            camera::Camera,
            scene_renderer::{
                RenderOptions, SceneRenderer, SCENE_BACKGROUND, SCENE_LIGHT_DIRECTION,
            },
        };
        let mut scene = raf_core::SceneGraph::new();
        scene.add_root_with_primitive("Cube", raf_core::scene::Primitive::Cube);
        let mut renderer = SceneRenderer::new(64, 64);
        let camera = Camera::default();
        let options = RenderOptions {
            show_grid_3d: false,
            selection_outline: false,
            ..Default::default()
        };
        let neutral = renderer
            .render(
                &scene,
                &camera,
                64.0,
                64.0,
                &[],
                SCENE_BACKGROUND,
                SCENE_LIGHT_DIRECTION,
                options,
                None,
            )
            .to_vec();
        let mut env = *scene.environment();
        env.mode = EnvironmentMode::Scene;
        env.sun.color = [255, 20, 20];
        env.ambient_intensity = 0.1;
        scene.set_environment(env);
        let lit = renderer
            .render(
                &scene,
                &camera,
                64.0,
                64.0,
                &[],
                SCENE_BACKGROUND,
                SCENE_LIGHT_DIRECTION,
                options,
                None,
            )
            .to_vec();
        assert_ne!(neutral, lit);
        assert!(lit.chunks_exact(4).any(|p| p[0] > p[1].saturating_add(20)));
        let preview = RenderOptions {
            environment_override: Some(EnvironmentSettings::default()),
            ..options
        };
        assert_eq!(
            neutral,
            renderer.render(
                &scene,
                &camera,
                64.0,
                64.0,
                &[],
                SCENE_BACKGROUND,
                SCENE_LIGHT_DIRECTION,
                preview,
                None
            )
        );
        assert_eq!(*scene.environment(), env);
    }
    #[test]
    fn neutral_has_no_lighting_work_and_sun_off_preserves_ambient() {
        let mut settings = EnvironmentSettings::default();
        assert!(EnvironmentFrame::new(settings, Mat4::IDENTITY).is_none());
        settings.mode = EnvironmentMode::Scene;
        settings.sun.enabled = false;
        let frame = EnvironmentFrame::new(settings, Mat4::IDENTITY).unwrap();
        assert_eq!(frame.sun, Vec3::ZERO);
        assert!(frame.ambient.length() > 0.0);
        assert_ne!(frame.sky_color(Vec3::Y), frame.sky_color(-Vec3::Y));
    }
    #[test]
    fn improved_sky_only_bounded_improvements_gate_haze_halo_and_star_chroma() {
        use raf_core::environment::LightingTechnology;
        let mut settings = EnvironmentSettings::default();
        settings.mode = EnvironmentMode::Scene;
        settings.technology = LightingTechnology::OptimizedCurrent;
        let mut improved = settings;
        improved.technology = LightingTechnology::OptimizedImproved;
        let current = EnvironmentFrame::new(settings, Mat4::IDENTITY).unwrap();
        let refined = EnvironmentFrame::new(improved, Mat4::IDENTITY).unwrap();
        // Legacy stays transparent to haze and halo no matter their values.
        let mut loud = settings;
        loud.sky.haze = 1.0;
        loud.sun.halo = 1.0;
        let loud_current = EnvironmentFrame::new(loud, Mat4::IDENTITY).unwrap();
        let low = Vec3::new(0.3, 0.12, 0.9).normalize();
        assert_eq!(
            current.sky_color(low),
            loud_current.sky_color(low),
            "current technology must ignore bounded improvements"
        );
        // Haze compresses the near-horizon ramp with improved and keeps the zenith.
        assert_ne!(current.sky_color(low), refined.sky_color(low));
        assert_eq!(current.sky_color(Vec3::Y), refined.sky_color(Vec3::Y));
        // Halo adds energy toward the sun only when the sun is up.
        assert!(refined.halo_strength > 0.0 && refined.halo_strength < 1.0);
        let mut halo_off = improved;
        halo_off.sun.halo = 0.0;
        let halo_off = EnvironmentFrame::new(halo_off, Mat4::IDENTITY).unwrap();
        // Sample just outside the solar disk but well inside the halo ring.
        let axis = Vec3::Y.cross(refined.sun_direction).normalize();
        let edge_sample = (glam::Mat3::from_axis_angle(axis, 0.02) * refined.sun_direction)
            .normalize();
        assert!(refined.sky_color(edge_sample) != halo_off.sky_color(edge_sample));
    }
}
