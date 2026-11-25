//! Particle simulation and billboard batching for the scene renderer.
//!
//! Effects are JSON assets interpreted by `raf_particles`. This module owns
//! the per-entity simulator state, project texture residency and the retained
//! command batches handed to ApiGraphicBasic.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec4};
use uuid::Uuid;

use raf_core::scene::{ParticlePlayback, ParticleQuality, ParticleSystemComponent};
use raf_core::SceneGraph;
use raf_particles::{
    ParticleCamera, ParticleFrame, ParticleSimulator, ParticleSprite, ParticleStats,
    ParticleSystemAsset, ParticleUpdate,
};

use crate::api_graphic_basic::command_list::{BasicCommandList, BasicMeshInstance};
use crate::api_graphic_basic::mesh::{BasicMesh, BasicVertex};
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use crate::camera::Camera;
use crate::texture::CpuTexture;

/// Per-frame controls for particle rendering. The editor uses its global
/// preview settings; the player uses the project runtime settings.
#[derive(Debug, Clone, Copy)]
pub struct ParticleRenderOptions {
    pub enabled: bool,
    pub paused: bool,
    pub frozen: bool,
    pub stopped: bool,
    pub time_scale: f32,
    /// Project-wide quality cap. The per-instance tier can only lower it.
    pub quality: ParticleQuality,
    pub max_sprites: u32,
    pub wind: [f32; 3],
}

impl Default for ParticleRenderOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            paused: false,
            frozen: false,
            stopped: false,
            time_scale: 1.0,
            quality: ParticleQuality::Epic,
            max_sprites: 0,
            wind: [0.0; 3],
        }
    }
}

/// Renderer-resident state for all particle systems in one scene host.
#[derive(Default)]
pub struct ParticleRenderer {
    nodes: HashMap<Uuid, ParticleNodeRuntime>,
    assets: HashMap<PathBuf, Option<Arc<ParticleSystemAsset>>>,
    textures: HashMap<PathBuf, Option<Arc<CpuTexture>>>,
    meshes: HashMap<u64, Arc<BasicMesh>>,
    warned: HashMap<PathBuf, ()>,
    stats: ParticleStats,
    last_batches: usize,
}

struct ParticleNodeRuntime {
    asset_path: PathBuf,
    asset: Arc<ParticleSystemAsset>,
    simulator: ParticleSimulator,
    /// Last cumulative emission request observed for this entity.
    consumed_emit_requests: u64,
}

impl ParticleRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> ParticleStats {
        self.stats
    }

    pub fn draw_batches(&self) -> usize {
        self.last_batches
    }

    /// Simulate every particle component and append retained billboard batches.
    pub fn update_and_draw(
        &mut self,
        scene: &SceneGraph,
        camera: &Camera,
        asset_root: Option<&Path>,
        options: &ParticleRenderOptions,
        commands: &mut BasicCommandList,
        dt: f32,
    ) {
        self.stats = ParticleStats::default();
        self.last_batches = 0;
        let Some(asset_root) = asset_root else {
            return;
        };
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }

        let inv_view = camera.view_matrix().inverse();
        let right = inv_view.x_axis.truncate().normalize_or_zero();
        let up = inv_view.y_axis.truncate().normalize_or_zero();
        let particle_camera = ParticleCamera {
            position: camera.position.to_array(),
            right: right.to_array(),
            up: up.to_array(),
        };
        let forward = (camera.target - camera.position).normalize_or_zero();

        let mut seen = Vec::new();
        let mut groups: HashMap<(Option<String>, u64), ParticleGroup> = HashMap::new();

        for (id, node) in scene.iter() {
            let Some(component) = node.particle_system.as_ref() else {
                continue;
            };
            if component.asset.trim().is_empty() {
                continue;
            }
            seen.push(node.uuid);
            let Some((asset_path, asset)) = self.load_asset(asset_root, &component.asset) else {
                continue;
            };
            let runtime = self
                .nodes
                .entry(node.uuid)
                .or_insert_with(|| ParticleNodeRuntime {
                    asset_path: asset_path.clone(),
                    asset: asset.clone(),
                    simulator: ParticleSimulator::new(),
                    consumed_emit_requests: 0,
                });
            if runtime.asset_path != asset_path {
                runtime.asset_path = asset_path;
                runtime.asset = asset.clone();
                runtime.simulator.reset();
                runtime.consumed_emit_requests = 0;
            }
            let emit = component
                .emit_requests
                .saturating_sub(runtime.consumed_emit_requests)
                .min(100_000) as u32;
            runtime.consumed_emit_requests = component.emit_requests;

            let playback = component.playback;
            let update = ParticleUpdate {
                enabled: options.enabled
                    && component.visible
                    && playback == ParticlePlayback::Playing,
                paused: options.paused || playback == ParticlePlayback::Paused,
                stopped: options.stopped || playback == ParticlePlayback::Stopped,
                frozen: options.frozen || playback == ParticlePlayback::Frozen,
                time_scale: (component.normalized_time_scale() * options.time_scale)
                    .clamp(0.0, 8.0),
                rate_scale: component.normalized_rate_scale(),
                emit,
                max_particles: component.max_particles,
                max_sprites: particle_sprite_cap(component, options, &asset),
                wind: options.wind,
            };
            let transform = scene.world_matrix(id);
            let asset = Arc::clone(&runtime.asset);
            let frame = runtime
                .simulator
                .update(&asset, transform, &particle_camera, dt, &update);
            self.stats.active = self.stats.active.saturating_add(frame.stats.active);
            self.stats.spawned = self.stats.spawned.saturating_add(frame.stats.spawned);
            self.stats.capped = self.stats.capped.saturating_add(frame.stats.capped);
            self.stats.emitters = self.stats.emitters.saturating_add(frame.stats.emitters);

            if update.stopped || !component.visible {
                continue;
            }
            self.collect_frame(&asset, component, &frame, camera, forward, &mut groups);
        }

        self.nodes.retain(|uuid, _| seen.contains(uuid));
        if groups.is_empty() {
            return;
        }

        commands.set_pipeline(BasicPipelineKind::Particle);
        let mut ordered = groups.into_values().collect::<Vec<_>>();
        ordered.sort_by(|a, b| {
            b.sort_depth
                .partial_cmp(&a.sort_depth)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for group in ordered {
            let Some(texture_id) = self.texture_id(asset_root, group.texture.as_deref(), commands)
            else {
                continue;
            };
            let mesh = self.quad_mesh(group.uv);
            let mesh_id = commands.register_mesh(mesh);
            commands.draw_particle_batch(mesh_id, Some(texture_id), group.instances);
            self.last_batches += 1;
        }
        commands.set_pipeline(BasicPipelineKind::FlatColor);
    }

    fn collect_frame(
        &self,
        asset: &ParticleSystemAsset,
        component: &ParticleSystemComponent,
        frame: &ParticleFrame,
        camera: &Camera,
        forward: Vec3,
        groups: &mut HashMap<(Option<String>, u64), ParticleGroup>,
    ) {
        for sprite in &frame.sprites {
            let Some(emitter) = asset.emitters.get(sprite.emitter as usize) else {
                continue;
            };
            let texture = emitter.texture.clone();
            let uv_key = uv_key(sprite.uv);
            let key = (texture.clone(), uv_key);
            let group = groups.entry(key).or_insert_with(|| ParticleGroup {
                texture,
                uv: sprite.uv,
                instances: Vec::new(),
                sort_depth: 0.0,
            });
            let Some(instance) = billboard_instance(sprite, component, camera, forward) else {
                continue;
            };
            group.sort_depth = group.sort_depth.max(sprite.depth);
            group.instances.push(instance);
        }
    }

    fn load_asset(
        &mut self,
        asset_root: &Path,
        relative: &str,
    ) -> Option<(PathBuf, Arc<ParticleSystemAsset>)> {
        let path = self.resolve_asset_path(asset_root, relative)?;
        if let Some(cached) = self.assets.get(&path) {
            return cached.clone().map(|asset| (path, asset));
        }
        let loaded = raf_particles::load_asset(&path).ok().map(Arc::new);
        if loaded.is_none() && !self.warned.contains_key(&path) {
            self.warned.insert(path.clone(), ());
            tracing::warn!(
                path = %path.display(),
                "particle asset could not be loaded"
            );
        }
        self.assets.insert(path.clone(), loaded.clone());
        loaded.map(|asset| (path, asset))
    }

    fn resolve_asset_path(&self, asset_root: &Path, relative: &str) -> Option<PathBuf> {
        let relative = relative.trim().replace('\\', "/");
        if relative.is_empty() || relative.starts_with('/') {
            return None;
        }
        if relative
            .split('/')
            .any(|part| part == ".." || part.is_empty())
        {
            return None;
        }
        let path = asset_root.join(relative);
        path.is_file().then_some(path)
    }

    fn texture_id(
        &mut self,
        asset_root: &Path,
        texture: Option<&str>,
        commands: &mut BasicCommandList,
    ) -> Option<usize> {
        let texture = texture.unwrap_or("").trim();
        let path = if texture.is_empty() {
            None
        } else {
            self.resolve_asset_path(asset_root, texture)
        };
        let key = path
            .clone()
            .unwrap_or_else(|| PathBuf::from("__particle_fallback__"));
        let cpu = if let Some(cached) = self.textures.get(&key) {
            cached.clone()
        } else {
            let loaded = path.as_deref().and_then(load_texture).map(Arc::new);
            let loaded = loaded.or_else(|| {
                if !texture.is_empty() && !self.warned.contains_key(&key) {
                    self.warned.insert(key.clone(), ());
                    tracing::warn!(
                        path = %key.display(),
                        "particle texture missing; using soft fallback"
                    );
                }
                Some(Arc::new(fallback_texture()))
            });
            self.textures.insert(key.clone(), loaded.clone());
            loaded
        }?;
        Some(commands.register_texture(cpu))
    }

    fn quad_mesh(&mut self, uv: [f32; 4]) -> Arc<BasicMesh> {
        let key = uv_key(uv);
        if let Some(mesh) = self.meshes.get(&key) {
            return Arc::clone(mesh);
        }
        let [u0, v0, u1, v1] = uv;
        let vertices = vec![
            BasicVertex {
                position: Vec3::new(-0.5, -0.5, 0.0),
                normal: Vec3::Z,
                uv: [u0, v1],
            },
            BasicVertex {
                position: Vec3::new(0.5, -0.5, 0.0),
                normal: Vec3::Z,
                uv: [u1, v1],
            },
            BasicVertex {
                position: Vec3::new(0.5, 0.5, 0.0),
                normal: Vec3::Z,
                uv: [u1, v0],
            },
            BasicVertex {
                position: Vec3::new(-0.5, 0.5, 0.0),
                normal: Vec3::Z,
                uv: [u0, v0],
            },
        ];
        let mesh = Arc::new(BasicMesh::new(vertices, vec![0, 1, 2, 0, 2, 3]));
        self.meshes.insert(key, mesh.clone());
        mesh
    }
}

struct ParticleGroup {
    texture: Option<String>,
    uv: [f32; 4],
    instances: Vec<BasicMeshInstance>,
    sort_depth: f32,
}

fn particle_sprite_cap(
    component: &ParticleSystemComponent,
    options: &ParticleRenderOptions,
    asset: &ParticleSystemAsset,
) -> u32 {
    let asset_cap = asset.budget.max_sprites.max(1);
    let quality_cap = component
        .quality
        .max_sprites()
        .min(options.quality.max_sprites());
    let component_cap = if component.max_particles == 0 {
        u32::MAX
    } else {
        component.max_particles
    };
    let render_cap = if options.max_sprites == 0 {
        u32::MAX
    } else {
        options.max_sprites
    };
    asset_cap
        .min(quality_cap)
        .min(component_cap)
        .min(render_cap)
}

fn billboard_instance(
    sprite: &ParticleSprite,
    component: &ParticleSystemComponent,
    camera: &Camera,
    forward: Vec3,
) -> Option<BasicMeshInstance> {
    let size = [sprite.size[0].abs(), sprite.size[1].abs()];
    if !size[0].is_finite() || !size[1].is_finite() || size[0] <= 1e-6 || size[1] <= 1e-6 {
        return None;
    }
    let position = Vec3::from_array(sprite.position);
    if !position.is_finite() {
        return None;
    }
    let inv = camera.view_matrix().inverse();
    let right = inv.x_axis.truncate().normalize_or_zero();
    let up = inv.y_axis.truncate().normalize_or_zero();
    if right.length_squared() < 0.5 || up.length_squared() < 0.5 {
        return None;
    }
    let rotation = Quat::from_axis_angle(forward, sprite.rotation.to_radians());
    let right = rotation * right * size[0];
    let up = rotation * up * size[1];
    let transform = Mat4::from_cols(
        right.extend(0.0),
        up.extend(0.0),
        forward.extend(0.0),
        Vec4::new(position.x, position.y, position.z, 1.0),
    );
    let tint = component.color_tint;
    let color = [
        ((sprite.color[0] as u16 * tint[0] as u16) / 255) as u8,
        ((sprite.color[1] as u16 * tint[1] as u16) / 255) as u8,
        ((sprite.color[2] as u16 * tint[2] as u16) / 255) as u8,
        ((sprite.color[3] as u16 * tint[3] as u16) / 255) as u8,
    ];
    Some(BasicMeshInstance { transform, color })
}

fn uv_key(uv: [f32; 4]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for value in uv {
        hash = (hash ^ u64::from(value.to_bits())).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn load_texture(path: &Path) -> Option<CpuTexture> {
    let image = image::open(path).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some(CpuTexture {
        data: image.into_raw(),
        width,
        height,
        source: path.to_path_buf(),
    })
}

fn fallback_texture() -> CpuTexture {
    const SIZE: u32 = 64;
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = (x as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
            let radius = (dx * dx + dy * dy).sqrt();
            let alpha = ((1.0 - radius).clamp(0.0, 1.0).powf(2.0) * 255.0) as u8;
            data.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    CpuTexture {
        data,
        width: SIZE,
        height: SIZE,
        source: PathBuf::from("__particle_fallback__"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::command_list::GraphicCommand;
    use raf_core::SceneNodeId;

    const TEST_ASSET: &str = r#"{
      "version": 1,
      "name": "Test",
      "budget": { "max_particles": 32, "max_sprites": 32 },
      "emitters": [
        {
          "name": "burst",
          "texture": null,
          "enabled": true,
          "max_particles": 32,
          "rate": 0.0,
          "burst": 5,
          "lifetime": [1.0, 1.0],
          "speed": [0.0, 0.0],
          "spread": [0.0, 0.0],
          "direction": [0.0, 1.0, 0.0],
          "gravity": [0.0, 0.0, 0.0],
          "drag": 0.0,
          "size": { "keys": [{ "t": 0.0, "value": 0.5 }] },
          "opacity": { "keys": [{ "t": 0.0, "value": 1.0 }] },
          "color": { "keys": [{ "t": 0.0, "color": [255, 255, 255, 255] }] },
          "shape": { "type": "point" },
          "flipbook": null,
          "collision": null,
          "wind_affects_drag": false
        }
      ]
    }"#;

    fn test_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "raf-particle-render-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let particles = root.join("assets/particles");
        std::fs::create_dir_all(&particles).unwrap();
        std::fs::write(particles.join("test.rafparticle"), TEST_ASSET).unwrap();
        root
    }

    fn camera() -> Camera {
        Camera {
            position: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            ..Camera::default()
        }
    }

    fn particle_scene(asset: &str) -> (SceneGraph, SceneNodeId) {
        let mut scene = SceneGraph::new();
        let id = scene.add_root("Effect");
        if let Some(node) = scene.get_mut(id) {
            node.particle_system = Some(ParticleSystemComponent::new(asset));
        }
        (scene, id)
    }

    fn batch_instances(commands: &BasicCommandList) -> usize {
        commands
            .commands()
            .iter()
            .map(|command| match command {
                GraphicCommand::DrawMeshBatch { instances, .. } => instances.len(),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn burst_renders_one_textured_batch_with_the_fallback_texture() {
        let root = test_root();
        let (scene, _) = particle_scene("particles/test.rafparticle");
        let mut renderer = ParticleRenderer::new();
        let mut commands = BasicCommandList::new();
        renderer.update_and_draw(
            &scene,
            &camera(),
            Some(&root.join("assets")),
            &ParticleRenderOptions::default(),
            &mut commands,
            1.0 / 60.0,
        );
        assert_eq!(renderer.stats().active, 5);
        assert_eq!(renderer.draw_batches(), 1);
        assert_eq!(batch_instances(&commands), 5);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn emit_requests_are_consumed_once() {
        let root = test_root();
        let (mut scene, id) = particle_scene("particles/test.rafparticle");
        scene
            .get_mut(id)
            .unwrap()
            .particle_system
            .as_mut()
            .unwrap()
            .request_emit(3);
        let mut renderer = ParticleRenderer::new();
        let mut commands = BasicCommandList::new();
        let options = ParticleRenderOptions::default();
        renderer.update_and_draw(
            &scene,
            &camera(),
            Some(&root.join("assets")),
            &options,
            &mut commands,
            1.0 / 60.0,
        );
        assert_eq!(renderer.stats().spawned, 8);
        let mut commands = BasicCommandList::new();
        renderer.update_and_draw(
            &scene,
            &camera(),
            Some(&root.join("assets")),
            &options,
            &mut commands,
            1.0 / 60.0,
        );
        assert_eq!(renderer.stats().spawned, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stopped_systems_clear_and_draw_nothing() {
        let root = test_root();
        let (mut scene, id) = particle_scene("particles/test.rafparticle");
        scene
            .get_mut(id)
            .unwrap()
            .particle_system
            .as_mut()
            .unwrap()
            .playback = ParticlePlayback::Stopped;
        let mut renderer = ParticleRenderer::new();
        let mut commands = BasicCommandList::new();
        renderer.update_and_draw(
            &scene,
            &camera(),
            Some(&root.join("assets")),
            &ParticleRenderOptions::default(),
            &mut commands,
            1.0 / 60.0,
        );
        assert_eq!(renderer.stats().active, 0);
        assert_eq!(renderer.draw_batches(), 0);
        let _ = std::fs::remove_dir_all(root);
    }
}
