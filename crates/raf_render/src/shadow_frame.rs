//! One bounded, camera-relative directional map. No rendering API leaks through this contract.
use crate::api_graphic_basic::mesh::BasicMesh;
use glam::{Mat4, Vec3};
use raf_core::environment::ShadowSettings;
use std::sync::{Arc, OnceLock};

pub const MAX_SHADOW_CASTERS: usize = 4096;
pub const MAX_SHADOW_TRIANGLES: usize = 100_000;
pub const MAX_SHADOW_MESH_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CPU_SHADOW_PIXEL_TESTS: usize = 4 * 1024 * 1024;

#[derive(Debug)]
struct CpuShadowDepth {
    values: Vec<f32>,
    budget_exceeded: bool,
}

#[derive(Debug, Clone)]
pub struct ShadowCaster {
    pub mesh: Arc<BasicMesh>,
    pub model: Mat4,
}

#[derive(Debug)]
pub struct ShadowGeometry {
    pub key: u64,
    pub view_projection: Mat4,
    pub resolution: u32,
    pub casters: Vec<ShadowCaster>,
    cpu_depth: OnceLock<CpuShadowDepth>,
}

#[derive(Debug, Clone)]
pub struct ShadowFrame {
    pub geometry: Arc<ShadowGeometry>,
    pub settings: ShadowSettings,
    pub camera: Vec3,
}

/// Snap all light-space axes, keeping both XY coverage and the depth interval stable.
pub fn directional_projection(sun: Vec3, camera: Vec3, distance: f32, resolution: u32) -> Mat4 {
    let up = if sun.y.abs() > 0.95 { Vec3::Z } else { Vec3::Y };
    let rotation = Mat4::look_to_rh(Vec3::ZERO, -sun, up);
    let texel = 2.0 * distance / resolution as f32;
    let center = (rotation.transform_point3(camera) / texel).round() * texel;
    Mat4::orthographic_rh(
        -distance,
        distance,
        -distance,
        distance,
        -4.0 * distance,
        4.0 * distance,
    ) * Mat4::from_translation(-center)
        * rotation
}

impl ShadowGeometry {
    pub fn new(view_projection: Mat4, resolution: u32, casters: Vec<ShadowCaster>) -> Self {
        let mut key = 0xcbf29ce484222325u64;
        let mut mix = |v: u64| {
            key = (key ^ v).wrapping_mul(0x100000001b3);
        };
        mix(resolution as u64);
        for n in view_projection.to_cols_array() {
            mix(n.to_bits() as u64);
        }
        for caster in &casters {
            mix(Arc::as_ptr(&caster.mesh) as usize as u64);
            for n in caster.model.to_cols_array() {
                mix(n.to_bits() as u64);
            }
        }
        Self {
            key,
            view_projection,
            resolution,
            casters,
            cpu_depth: OnceLock::new(),
        }
    }
    pub fn cpu_ready(&self) -> bool {
        self.cpu_depth.get().is_some()
    }
    pub fn cpu_resolution(&self) -> u32 {
        self.resolution.min(256)
    }
    pub fn cpu_budget_exceeded(&self) -> bool {
        self.cpu_depth.get().is_some_and(|d| d.budget_exceeded)
    }
    /// CPU recovery is deliberately capped at 256 squared and constructed only on demand.
    pub fn cpu_depth(&self) -> &[f32] {
        self.cpu_depth
            .get_or_init(|| {
                let size = self.cpu_resolution() as usize;
                let mut depth = vec![1.0; size * size];
                let mut pixel_tests = 0;
                for caster in &self.casters {
                    let mvp = self.view_projection * caster.model;
                    for tri in caster.mesh.indices.chunks_exact(3) {
                        let p = [tri[0], tri[1], tri[2]].map(|i| {
                            mvp.transform_point3(caster.mesh.vertices[i as usize].position)
                        });
                        let screen = p.map(|v| {
                            Vec3::new(
                                (v.x * 0.5 + 0.5) * size as f32,
                                (0.5 - v.y * 0.5) * size as f32,
                                v.z,
                            )
                        });
                        let [a, b, c] = screen;
                        let edge = |a: Vec3, b: Vec3, x: f32, y: f32| {
                            (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
                        };
                        let area = edge(a, b, c.x, c.y);
                        if area.abs() < 1e-8 {
                            continue;
                        }
                        let min = a.min(b).min(c).floor().max(Vec3::ZERO);
                        let max = a.max(b).max(c).ceil().min(Vec3::splat(size as f32));
                        pixel_tests += ((max.x - min.x).max(0.0) as usize)
                            * ((max.y - min.y).max(0.0) as usize);
                        if pixel_tests > MAX_CPU_SHADOW_PIXEL_TESTS {
                            depth.fill(1.0);
                            return CpuShadowDepth {
                                values: depth,
                                budget_exceeded: true,
                            };
                        }
                        for y in min.y as usize..max.y as usize {
                            for x in min.x as usize..max.x as usize {
                                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                                let w0 = edge(b, c, px, py) / area;
                                let w1 = edge(c, a, px, py) / area;
                                let w2 = 1.0 - w0 - w1;
                                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                                    continue;
                                }
                                let z = w0 * a.z + w1 * b.z + w2 * c.z;
                                if (0.0..=1.0).contains(&z) {
                                    depth[y * size + x] = depth[y * size + x].min(z);
                                }
                            }
                        }
                    }
                }
                CpuShadowDepth {
                    values: depth,
                    budget_exceeded: false,
                }
            })
            .values
            .as_slice()
    }
}

impl ShadowFrame {
    pub fn visibility(&self, world: Vec3, ndotl: f32) -> f32 {
        let p = self.geometry.view_projection.transform_point3(world);
        if p.x.abs() >= 1.0 || p.y.abs() >= 1.0 || p.z <= 0.0 || p.z >= 1.0 {
            return 1.0;
        }
        let size = self.geometry.cpu_resolution() as i32;
        let x = (p.x * 0.5 + 0.5) * size as f32;
        let y = (0.5 - p.y * 0.5) * size as f32;
        let cosine = ndotl.clamp(0.05, 1.0);
        let slope = ((1.0 - cosine * cosine).max(0.0).sqrt() / cosine).min(8.0);
        let footprint = if self.settings.soft_filter { 1.5 } else { 0.75 };
        let z = p.z
            - self.settings.bias / (8.0 * self.settings.distance)
            - footprint * slope / (4.0 * size as f32);
        let depth = self.geometry.cpu_depth();
        let sample = |x: i32, y: i32| {
            if x < 0 || y < 0 || x >= size || y >= size {
                1.0
            } else {
                f32::from(z <= depth[(y * size + x) as usize])
            }
        };
        let visibility = if self.settings.soft_filter {
            let (x0, x1) = ((x - 0.5).floor() as i32, (x + 0.5).floor() as i32);
            let (y0, y1) = ((y - 0.5).floor() as i32, (y + 0.5).floor() as i32);
            (sample(x0, y0) + sample(x1, y0) + sample(x0, y1) + sample(x1, y1)) * 0.25
        } else {
            sample(x.floor() as i32, y.floor() as i32)
        };
        let fade = ((self.settings.distance - world.distance(self.camera))
            / (self.settings.distance * 0.2))
            .clamp(0.0, 1.0);
        1.0 - (1.0 - visibility) * fade * self.settings.strength
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        camera::Camera,
        scene_renderer::{RenderOptions, SceneRenderer, SCENE_BACKGROUND, SCENE_LIGHT_DIRECTION},
    };
    use raf_core::{environment::EnvironmentMode, scene::Primitive, SceneGraph};

    pub(crate) fn fixture() -> (
        SceneGraph,
        raf_core::scene::SceneNodeId,
        raf_core::scene::SceneNodeId,
    ) {
        let mut scene = SceneGraph::new();
        let floor = scene.add_root_with_primitive("Floor", Primitive::Plane);
        scene.get_mut(floor).unwrap().scale = Vec3::new(20.0, 1.0, 20.0);
        scene.get_mut(floor).unwrap().casts_shadows = false;
        let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
        scene.get_mut(cube).unwrap().position.y = 1.0;
        scene.get_mut(cube).unwrap().scale = Vec3::splat(2.0);
        let mut env = *scene.environment();
        env.mode = EnvironmentMode::Scene;
        env.shadows.enabled = true;
        env.sun.azimuth = std::f32::consts::PI + 0.4;
        env.sun.elevation = 0.9;
        // Neutral light separates gray receiver pixels from the orange caster in image assertions.
        env.sun.color = [255; 3];
        env.ambient_intensity = 0.2;
        scene.set_environment(env);
        (scene, floor, cube)
    }
    pub(crate) fn frame(
        renderer: &mut SceneRenderer,
        scene: &SceneGraph,
    ) -> crate::scene_renderer::SceneRenderFrame {
        renderer.build_frame(
            scene,
            &Camera::default(),
            128.0,
            128.0,
            &[],
            SCENE_BACKGROUND,
            SCENE_LIGHT_DIRECTION,
            RenderOptions {
                show_grid_3d: false,
                selection_outline: false,
                ..Default::default()
            },
            None,
        )
    }
    #[test]
    fn shadows_cache_ignores_receiver_material_and_filter_changes_but_tracks_casters() {
        let (mut scene, floor, cube) = fixture();
        let mut renderer = SceneRenderer::new(128, 128);
        let initial = frame(&mut renderer, &scene)
            .commands
            .shadows
            .unwrap()
            .geometry;
        let mut env = *scene.environment();
        env.sun.color = [255, 100, 50];
        env.sun.intensity = 2.0;
        env.shadows.bias = 0.1;
        env.shadows.soft_filter = true;
        scene.set_environment(env);
        let next = frame(&mut renderer, &scene)
            .commands
            .shadows
            .unwrap()
            .geometry;
        assert!(Arc::ptr_eq(&initial, &next));
        scene.get_mut(floor).unwrap().receives_shadows = false;
        scene.get_mut(cube).unwrap().color.r = 50;
        assert_eq!(
            initial.key,
            frame(&mut renderer, &scene)
                .commands
                .shadows
                .unwrap()
                .geometry
                .key
        );
        scene.get_mut(cube).unwrap().position.x += 1.0;
        assert_ne!(
            initial.key,
            frame(&mut renderer, &scene)
                .commands
                .shadows
                .unwrap()
                .geometry
                .key
        );
        scene.get_mut(cube).unwrap().casts_shadows = false;
        assert!(frame(&mut renderer, &scene).commands.shadows.is_none());
        scene.get_mut(cube).unwrap().casts_shadows = true;
        scene.get_mut(cube).unwrap().color.a = 128;
        assert!(frame(&mut renderer, &scene).commands.shadows.is_none());
    }
    #[test]
    fn shadows_offscreen_casters_are_kept_and_disabled_paths_allocate_nothing() {
        let (mut scene, _, cube) = fixture();
        scene.get_mut(cube).unwrap().position = Vec3::new(-8.0, 8.0, -8.0);
        let mut renderer = SceneRenderer::new(128, 128);
        let f = frame(&mut renderer, &scene);
        assert_eq!(
            f.commands.shadows.as_ref().unwrap().geometry.casters.len(),
            1
        );
        assert!(f.stats.frustum_culled_entities > 0);
        for case in 0..3 {
            let mut env = *scene.environment();
            if case == 0 {
                env.mode = EnvironmentMode::Neutral;
            }
            if case == 1 {
                env.mode = EnvironmentMode::Scene;
                env.sun.enabled = false;
            }
            if case == 2 {
                env.sun.enabled = true;
                env.shadows.enabled = false;
            }
            scene.set_environment(env);
            assert!(frame(&mut renderer, &scene).commands.shadows.is_none());
        }
    }
    #[test]
    fn shadows_depth_projection_and_cpu_filter_are_bounded() {
        let (scene, _, _) = fixture();
        let mut renderer = SceneRenderer::new(128, 128);
        let mut shadow = frame(&mut renderer, &scene).commands.shadows.unwrap();
        assert!(!shadow.geometry.cpu_ready());
        assert!(shadow.visibility(Vec3::ZERO, 1.0) < 0.5);
        assert!(shadow.geometry.cpu_ready());
        assert_eq!(shadow.geometry.cpu_depth().len(), 256 * 256);
        assert_eq!(shadow.visibility(Vec3::splat(1000.0), 1.0), 1.0);
        shadow.camera = Vec3::new(0.0, 0.0, -31.9);
        assert!(shadow.visibility(Vec3::ZERO, 1.0) > 0.95);
        shadow.camera = Vec3::new(0.0, 0.0, -32.0);
        assert_eq!(shadow.visibility(Vec3::ZERO, 1.0), 1.0);
        shadow.camera = Vec3::ZERO;
        shadow.settings.soft_filter = true;
        for x in -10..10 {
            assert!(
                (0.0..=1.0).contains(&shadow.visibility(Vec3::new(x as f32 * 0.1, 0.0, 0.0), 1.0))
            );
        }
        shadow.settings.strength = 0.0;
        assert_eq!(shadow.visibility(Vec3::ZERO, 1.0), 1.0);
        let sun = Vec3::new(0.4, 0.8, 0.3).normalize();
        assert_eq!(
            directional_projection(sun, Vec3::ZERO, 32.0, 512),
            directional_projection(sun, Vec3::splat(0.00001), 32.0, 512)
        );
        assert_ne!(
            directional_projection(sun, Vec3::ZERO, 32.0, 512),
            directional_projection(sun, Vec3::X, 32.0, 512)
        );
    }
    #[test]
    fn shadows_budget_refuses_incomplete_maps() {
        let (mut scene, _, _) = fixture();
        for _ in 0..MAX_SHADOW_CASTERS {
            scene.add_root_with_primitive("Repeated", Primitive::Cube);
        }
        let mut renderer = SceneRenderer::new(16, 16);
        let f = frame(&mut renderer, &scene);
        assert!(f.commands.shadows.is_none());
        assert!(f.commands.shadow_budget_exceeded);
    }
    #[test]
    fn shadows_cpu_overdraw_budget_returns_no_partial_shadow_map() {
        let (scene, _, _) = fixture();
        let mut renderer = SceneRenderer::new(16, 16);
        let original = frame(&mut renderer, &scene).commands.shadows.unwrap();
        let mesh = Arc::clone(&original.geometry.casters[0].mesh);
        let casters = (0..32)
            .map(|_| ShadowCaster {
                mesh: Arc::clone(&mesh),
                model: Mat4::from_scale(Vec3::splat(100.0)),
            })
            .collect();
        let geometry = ShadowGeometry::new(original.geometry.view_projection, 512, casters);
        assert!(geometry.cpu_depth().iter().all(|v| *v == 1.0));
        assert!(geometry.cpu_budget_exceeded());
    }
}
