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
        Some(Self {
            settings,
            inverse_view_projection: view_projection.inverse(),
            sun_direction,
            sun: linear_color(settings.sun.color) * direct,
            ambient: linear_color(settings.ambient_color) * settings.ambient_intensity,
            sky_top: linear_color(settings.sky.top_color) * settings.sky.intensity,
            sky_horizon: linear_color(settings.sky.horizon_color) * settings.sky.intensity,
            sky_ground: linear_color(settings.sky.ground_color) * settings.sky.intensity,
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
        let mut color = if sky.mode == SkyMode::Solid {
            top
        } else if direction.y >= 0.0 {
            horizon.lerp(top, direction.y.clamp(0.0, 1.0))
        } else {
            horizon.lerp(ground, (-direction.y).clamp(0.0, 1.0))
        };
        if sky.stars_enabled && sky.stars_density > 0.0 && sky.stars_intensity > 0.0 {
            color += Vec3::new(0.72, 0.86, 1.0)
                * if self.improved() {
                    crate::sky_stars::brightness_filtered(direction, sky.stars_density, footprint)
                } else {
                    crate::sky_stars::brightness(direction, sky.stars_density)
                }
                * sky.stars_intensity;
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
}
