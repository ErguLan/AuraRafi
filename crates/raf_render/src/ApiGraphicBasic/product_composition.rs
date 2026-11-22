//! Scene half of the shared product composition.
//!
//! `SceneRenderer` records backend-neutral work into `BasicCommandList`. This
//! module converts one [`SceneRenderFrame`] into the `raf_agb::product`
//! command stream that `ProductDevice` prepares, and keeps the packet open for
//! the CAD and RafUI halves to append their paint work.
//!
//! The builder never creates a device, opens a surface, or presents. The native
//! window compositor owns the `ProductDevice` and its output; it only needs the
//! packet produced here and the camera uniforms derived from the same frame.

use std::{collections::HashMap, sync::Arc};

use glam::{Mat4, Vec3};
use raf_agb::{
    product::{
        Camera, Command, FrameUniforms, Grid, Line, Material, Mesh, PacketHandle, PacketKey,
        Pipeline, ProductDevice, ProductPacket, SkyBackdrop,
    },
    Backend, Instance, Texture, Vertex,
};

use crate::api_graphic_basic::command_list::{BasicLine, BasicScreenTriangle, GraphicCommand};
use crate::api_graphic_basic::device::SceneFrameMetrics;
use crate::api_graphic_basic::mesh::BasicVertex;
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use crate::engine_time::Instant;
use crate::scene_renderer::SceneRenderFrame;

/// Packet key reserved for the scene-led composition. CAD and RafUI append
/// their commands to the same packet so a device prepares one retained
/// revision per frame.
pub const PRODUCT_SCENE_PACKET_KEY: PacketKey = PacketKey(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MeshKey {
    identity: usize,
    vertices: usize,
    indices: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TextureKey {
    identity: usize,
    width: u32,
    height: u32,
    bytes: usize,
}

/// Bounded, reusable conversion of scene frames into product commands.
///
/// A revision covers geometry, textures, materials, environment, and any
/// pixel-space command. The camera is not part of the packet: it travels in
/// `FrameUniforms`, so a camera-only frame at the same revision reuses the
/// prepared packet without rebuilding geometry or textures.
pub struct ProductFrameBuilder {
    key: PacketKey,
    revision: u64,
    packet: ProductPacket,
    mesh_ids: HashMap<MeshKey, u32>,
    texture_ids: HashMap<TextureKey, u32>,
    material_ids: HashMap<Option<u32>, u32>,
    extent: [u32; 2],
    pixel_extent_used: bool,
    composed_clear: Option<[f32; 4]>,
    placement: Option<[f32; 4]>,
    materialized: bool,
    /// Retained source for reconstruction. The frame shares its mesh and
    /// texture handles, so keeping it does not copy pixels.
    source: Option<SceneRenderFrame>,
}

impl Default for ProductFrameBuilder {
    fn default() -> Self {
        Self::new(PRODUCT_SCENE_PACKET_KEY)
    }
}

impl ProductFrameBuilder {
    pub fn new(key: PacketKey) -> Self {
        Self {
            key,
            revision: 0,
            packet: ProductPacket::default(),
            mesh_ids: HashMap::new(),
            texture_ids: HashMap::new(),
            material_ids: HashMap::new(),
            extent: [0, 0],
            pixel_extent_used: false,
            composed_clear: None,
            placement: None,
            materialized: false,
            source: None,
        }
    }

    /// Key of the retained packet on the product device.
    pub const fn key(&self) -> PacketKey {
        self.key
    }

    /// Latest materialized composition revision. Hand it to
    /// `ProductDevice::prepare`; a camera-only frame keeps it unchanged.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Current packet for inspection. After a handoff it is empty until the
    /// next revision change; use `handoff_packet` for reconstruction.
    pub fn packet(&self) -> &ProductPacket {
        &self.packet
    }

    /// Extension seam for the CAD and RafUI halves. They append their paint
    /// commands directly; the scene commands stay at the head of the packet.
    /// Append only while `has_packet` is true.
    pub fn packet_mut(&mut self) -> &mut ProductPacket {
        &mut self.packet
    }

    /// True while a complete packet is available for handoff.
    pub const fn has_packet(&self) -> bool {
        self.materialized
    }

    /// Produces the packet for a device handoff.
    ///
    /// Takes the retained packet when present. Otherwise it rebuilds the scene
    /// half from the retained source frame, which covers device loss and a
    /// repeated handoff at the same revision. CAD and RafUI content must be
    /// appended again after a reconstruction.
    pub fn handoff_packet(&mut self) -> Result<ProductPacket, String> {
        if self.packet.commands.is_empty() {
            match (self.source.clone(), self.composed_clear) {
                (Some(source), _) => {
                    let revision = self.revision;
                    let clear = self.composed_clear;
                    let placement = self.placement;
                    self.build(&source, revision, clear, placement)?;
                }
                (None, Some(clear)) => {
                    // UI-only composition: preserve the target clear so a
                    // device replacement does not flash a different background.
                    self.packet.commands.push(Command::Clear(clear));
                    self.materialized = true;
                }
                (None, None) => {
                    return Err(
                        "product scene packet is empty and has no retained source".to_string()
                    );
                }
            }
        }
        self.materialized = false;
        Ok(std::mem::take(&mut self.packet))
    }

    /// Drops the packet, its resources, its source, and its revision. The key
    /// is kept.
    pub fn clear(&mut self) {
        self.revision = 0;
        self.reset_packet();
        self.mesh_ids.clear();
        self.texture_ids.clear();
        self.material_ids.clear();
        self.extent = [0, 0];
        self.pixel_extent_used = false;
        self.composed_clear = None;
        self.placement = None;
        self.materialized = false;
        self.source = None;
    }

    /// Replace the packet with a target clear and no scene geometry.
    ///
    /// Used by editor screens that only compose retained UI. The clear is the
    /// same value the scene path would use, so switching between them does not
    /// flash a different background.
    pub fn append_empty(&mut self, clear: [f32; 4], revision: u64) -> Result<(), String> {
        if revision < self.revision {
            return Err(format!(
                "empty composition revision {revision} is older than the prepared revision {}",
                self.revision
            ));
        }
        if revision == self.revision && self.materialized {
            return Ok(());
        }
        self.reset_packet();
        self.mesh_ids.clear();
        self.texture_ids.clear();
        self.material_ids.clear();
        self.revision = revision;
        self.composed_clear = Some(clear);
        self.placement = None;
        self.extent = [0, 0];
        self.pixel_extent_used = false;
        self.source = None;
        self.packet.commands.push(Command::Clear(clear));
        self.materialized = true;
        Ok(())
    }

    /// Keeps the packet's vector capacity so a rebuild reuses its allocations.
    fn reset_packet(&mut self) {
        self.packet.commands.clear();
        self.packet.meshes.clear();
        self.packet.materials.clear();
        self.packet.textures.clear();
    }

    /// Replace the scene half of the packet with this frame.
    ///
    /// `revision` is monotonic per source content. A repeated revision reuses
    /// the retained composition and never rebuilds geometry or textures; an
    /// older revision is rejected instead of silently reverting the
    /// composition. A handoff does not force a rebuild; `handoff_packet`
    /// reconstructs from the retained source only when a new packet is needed.
    ///
    /// When the returned metrics report `product_packet_rebuilds == 1`, the
    /// packet holds only the scene half, so CAD and RafUI must append their
    /// commands again. Zero means the retained composition is unchanged.
    pub fn append_scene(
        &mut self,
        frame: &SceneRenderFrame,
        revision: u64,
    ) -> Result<SceneFrameMetrics, String> {
        self.append_scene_with_clear(frame, revision, None)
    }

    /// Compose a canvas into an editor target whose background is already
    /// chosen by the host.
    pub fn append_scene_with_clear(
        &mut self,
        frame: &SceneRenderFrame,
        revision: u64,
        clear: Option<[f32; 4]>,
    ) -> Result<SceneFrameMetrics, String> {
        self.append_scene_placed(frame, revision, clear, None)
    }

    /// Compose a canvas into a sub-rectangle of the target.
    ///
    /// `clear` is the full-target color chosen by the host. `placement` is the
    /// normalized canvas rectangle; the frame's own clear becomes a scene
    /// background clipped to that rectangle, and screen-space triangles are
    /// remapped from canvas clip space into the rectangle.
    pub fn append_scene_placed(
        &mut self,
        frame: &SceneRenderFrame,
        revision: u64,
        clear: Option<[f32; 4]>,
        placement: Option<[f32; 4]>,
    ) -> Result<SceneFrameMetrics, String> {
        let started = Instant::now();
        if revision < self.revision {
            return Err(format!(
                "scene frame revision {revision} is older than the prepared revision {}",
                self.revision
            ));
        }
        let extent = [frame.width, frame.height];
        if revision == self.revision
            && self.source.is_some()
            && clear == self.composed_clear
            && placement == self.placement
        {
            if self.pixel_extent_used && extent != self.extent {
                return Err(format!(
                    "scene frame extent changed from {}x{} to {}x{} at revision {revision}; \
                     bump the revision because pixel-space commands are baked into the packet",
                    self.extent[0], self.extent[1], extent[0], extent[1]
                ));
            }
            self.extent = extent;
            return Ok(self.metrics(frame, started, 0));
        }
        self.rebuild(frame, revision, clear, placement)?;
        Ok(self.metrics(frame, started, 1))
    }

    fn rebuild(
        &mut self,
        frame: &SceneRenderFrame,
        revision: u64,
        clear: Option<[f32; 4]>,
        placement: Option<[f32; 4]>,
    ) -> Result<(), String> {
        let previous_revision = self.revision;
        let result = self.build(frame, revision, clear, placement);
        if result.is_err() {
            self.reset_packet();
            self.mesh_ids.clear();
            self.texture_ids.clear();
            self.material_ids.clear();
            self.revision = previous_revision;
            self.materialized = false;
        }
        result
    }

    fn build(
        &mut self,
        frame: &SceneRenderFrame,
        revision: u64,
        clear: Option<[f32; 4]>,
        placement: Option<[f32; 4]>,
    ) -> Result<(), String> {
        self.reset_packet();
        self.mesh_ids.clear();
        self.texture_ids.clear();
        self.material_ids.clear();
        self.revision = revision;
        self.composed_clear = clear;
        self.placement = placement;
        self.extent = [frame.width, frame.height];
        self.pixel_extent_used = false;
        self.materialized = false;

        let mut recorded = frame.commands.commands().iter();
        let Some(first) = recorded.next() else {
            return Err("scene frame has no commands".to_string());
        };
        let GraphicCommand::Clear { r, g, b, a } = first else {
            return Err("scene frame must begin with Clear".to_string());
        };
        let target_clear = clear.unwrap_or_else(|| linear_rgba([*r, *g, *b, *a]));
        self.packet.commands.push(Command::Clear(target_clear));
        // The sky pass owns the canvas background when an environment exists.
        // Emitting both would put a screen batch before the sky, which the
        // encoder rejects on purpose.
        if placement.is_some() && frame.commands.environment.is_none() {
            self.packet
                .commands
                .push(Command::SceneBackground(linear_rgba([*r, *g, *b, *a])));
        }
        self.append_environment(frame);
        // The basic command list starts with shadow receivers enabled. The
        // product encoder defaults to disabled, so seed the same state before
        // the first mesh instead of letting one draw disagree.
        self.packet.commands.push(Command::SetShadowReceiver(true));

        let mut pipeline = BasicPipelineKind::FlatColor;
        for (offset, command) in recorded.enumerate() {
            let index = offset + 1;
            match command {
                GraphicCommand::Clear { .. } => {
                    return Err(format!(
                        "scene frame command {index}: Clear is only valid as the first command"
                    ));
                }
                GraphicCommand::SetPipeline(kind) => {
                    self.packet
                        .commands
                        .push(Command::SetPipeline(product_pipeline(*kind)?));
                    pipeline = *kind;
                }
                GraphicCommand::SetShadowReceiver(enabled) => {
                    self.packet
                        .commands
                        .push(Command::SetShadowReceiver(*enabled));
                }
                GraphicCommand::DrawMesh {
                    mesh_id,
                    transform,
                    color,
                    texture_id,
                } => {
                    let instance = product_instance(*transform, *color, *mesh_id)?;
                    let mesh = self.mesh_index(frame, *mesh_id)?;
                    let texture = self.texture_index(frame, *texture_id, *mesh_id)?;
                    let material = self.material_index(texture)?;
                    self.packet.commands.push(Command::DrawMesh {
                        mesh,
                        material,
                        instance,
                    });
                }
                GraphicCommand::DrawMeshBatch { mesh_id, instances } => {
                    if instances.is_empty() {
                        return Err(format!(
                            "scene frame command {index}: mesh batch {mesh_id} is empty"
                        ));
                    }
                    let mesh = self.mesh_index(frame, *mesh_id)?;
                    let material = self.material_index(None)?;
                    let instances = instances
                        .iter()
                        .map(|instance| {
                            product_instance(instance.transform, instance.color, *mesh_id)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    self.packet.commands.push(Command::DrawMeshBatch {
                        mesh,
                        material,
                        instances,
                    });
                }
                GraphicCommand::DrawLine {
                    start,
                    end,
                    color,
                    width,
                    no_depth_test,
                    depth_bias,
                } => {
                    let line = product_line(
                        &BasicLine {
                            start: *start,
                            end: *end,
                            color: *color,
                            width: *width,
                            depth_bias: *depth_bias,
                        },
                        *no_depth_test,
                        index,
                    )?;
                    self.packet.commands.push(Command::DrawLine(line));
                }
                GraphicCommand::DrawLineBatch {
                    lines,
                    no_depth_test,
                } => {
                    let lines = lines
                        .iter()
                        .map(|line| product_line(line, *no_depth_test, index))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.packet.commands.push(Command::DrawLineBatch(lines));
                }
                GraphicCommand::DrawScreenTriangleBatch { triangles } => {
                    // Screen triangles always execute with the overlay state in
                    // the basic path, regardless of the current scene pipeline.
                    // Product encoding requires the same explicit state.
                    let current = product_pipeline(pipeline)?;
                    if current != Pipeline::OVERLAY {
                        self.packet
                            .commands
                            .push(Command::SetPipeline(Pipeline::OVERLAY));
                    }
                    self.push_screen_triangles(
                        triangles,
                        [frame.width, frame.height],
                        placement,
                        index,
                    )?;
                    if current != Pipeline::OVERLAY {
                        self.packet.commands.push(Command::SetPipeline(current));
                    }
                }
                GraphicCommand::DrawGrid {
                    grid_y,
                    spacing,
                    no_depth_test,
                } => {
                    self.packet.commands.push(Command::DrawGrid(product_grid(
                        *grid_y,
                        *spacing,
                        *no_depth_test,
                        index,
                    )?));
                }
            }
        }

        self.append_shadow_casters(frame)?;
        self.source = Some(frame.clone());
        self.materialized = true;
        Ok(())
    }

    fn metrics(
        &self,
        frame: &SceneRenderFrame,
        started: Instant,
        rebuilds: u32,
    ) -> SceneFrameMetrics {
        let stats = frame.commands.stats();
        SceneFrameMetrics {
            frame_cpu_ms: started.elapsed().as_secs_f32() * 1000.0,
            command_count: stats.command_count,
            mesh_instances: stats.mesh_instances,
            line_instances: stats.line_instances,
            overlay_triangles: stats.overlay_triangles,
            product_packet_revision: self.revision,
            product_packet_rebuilds: rebuilds,
            product_command_count: count(self.packet.commands.len()),
            product_mesh_count: count(self.packet.meshes.len()),
            product_texture_count: count(self.packet.textures.len()),
            product_material_count: count(self.packet.materials.len()),
            ..SceneFrameMetrics::default()
        }
    }

    /// The environment frame is scene state, not a recorded command. The
    /// product contract draws the camera-relative sky before scene geometry,
    /// so it lands right after the clear.
    fn append_environment(&mut self, frame: &SceneRenderFrame) {
        let Some(environment) = frame.commands.environment else {
            return;
        };
        let improved = environment.improved();
        let map = |color: Vec3| -> [f32; 4] {
            let [red, green, blue] = color.to_array();
            [red, green, blue, 1.0]
        };
        let solid = environment.settings.sky.mode == raf_core::environment::SkyMode::Solid;
        let zenith = map(environment.sky_top);
        let (horizon, ground) = if solid {
            (zenith, zenith)
        } else {
            (map(environment.sky_horizon), map(environment.sky_ground))
        };
        self.packet.commands.push(Command::DrawSky(SkyBackdrop {
            zenith,
            horizon,
            ground,
            haze: if improved {
                environment.settings.sky.haze.clamp(0.0, 1.0)
            } else {
                0.0
            },
            texture: None,
            sun_disk_cos: if environment.settings.sun.disk_visible
                && environment.sun_direction.y > 0.0
                && environment.settings.sun.enabled
            {
                environment.settings.sun.angular_radius.cos()
            } else {
                1.0
            },
            halo: if improved {
                environment.halo_strength
            } else {
                0.0
            },
            halo_cos: environment.halo_cos,
            stars_density: if environment.settings.sky.stars_enabled {
                environment.settings.sky.stars_density
            } else {
                0.0
            },
            stars_intensity: environment.settings.sky.stars_intensity,
        }));
    }

    fn mesh_index(&mut self, frame: &SceneRenderFrame, mesh_id: usize) -> Result<u32, String> {
        let mesh = frame
            .commands
            .mesh_arc(mesh_id)
            .ok_or_else(|| format!("scene mesh {mesh_id} is not registered"))?;
        self.register_mesh(mesh, mesh_id)
    }

    fn register_mesh(
        &mut self,
        mesh: &Arc<crate::api_graphic_basic::mesh::BasicMesh>,
        mesh_id: usize,
    ) -> Result<u32, String> {
        let key = MeshKey {
            identity: Arc::as_ptr(mesh) as usize,
            vertices: mesh.vertices.len(),
            indices: mesh.indices.len(),
        };
        if let Some(index) = self.mesh_ids.get(&key) {
            return Ok(*index);
        }
        let triangle_vertices = if mesh.indices.is_empty() {
            mesh.vertices.len()
        } else {
            mesh.indices.len()
        };
        if triangle_vertices == 0 || triangle_vertices % 3 != 0 {
            return Err(format!("scene mesh {mesh_id} is not a triangle list"));
        }
        if mesh
            .indices
            .iter()
            .any(|index| *index as usize >= mesh.vertices.len())
        {
            return Err(format!(
                "scene mesh {mesh_id} has an index outside its vertex list"
            ));
        }
        let vertices = mesh
            .vertices
            .iter()
            .map(|vertex| product_vertex(vertex, mesh_id))
            .collect::<Result<Vec<_>, _>>()?;
        let index = count_u32(self.packet.meshes.len(), "mesh")?;
        self.packet.meshes.push(Mesh {
            vertices,
            indices: mesh.indices.clone(),
        });
        self.mesh_ids.insert(key, index);
        Ok(index)
    }

    fn append_shadow_casters(&mut self, frame: &SceneRenderFrame) -> Result<(), String> {
        let Some(shadow) = &frame.commands.shadows else {
            return Ok(());
        };
        let material = self.material_index(None)?;
        let mut batches = Vec::<(u32, Vec<Instance>)>::new();
        let mut batch_by_mesh = HashMap::new();
        for caster in &shadow.geometry.casters {
            let mesh = self.register_mesh(&caster.mesh, self.packet.meshes.len())?;
            let batch = *batch_by_mesh.entry(mesh).or_insert_with(|| {
                let index = batches.len();
                batches.push((mesh, Vec::new()));
                index
            });
            batches[batch].1.push(Instance::new(caster.model, [1.; 4]));
        }
        for (mesh, instances) in batches {
            self.packet.commands.push(Command::DrawShadowBatch {
                mesh,
                material,
                instances,
            });
        }
        Ok(())
    }

    fn texture_index(
        &mut self,
        frame: &SceneRenderFrame,
        texture_id: Option<usize>,
        mesh_id: usize,
    ) -> Result<Option<u32>, String> {
        let Some(texture_id) = texture_id else {
            return Ok(None);
        };
        let Some(texture) = frame.commands.texture(texture_id) else {
            return Err(format!(
                "scene mesh {mesh_id} references unknown texture {texture_id}"
            ));
        };
        let key = TextureKey {
            identity: Arc::as_ptr(texture) as usize,
            width: texture.width,
            height: texture.height,
            bytes: texture.data.len(),
        };
        if let Some(index) = self.texture_ids.get(&key) {
            return Ok(Some(*index));
        }
        let expected = (texture.width as usize)
            .checked_mul(texture.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                format!(
                    "scene texture {} has overflowing dimensions",
                    texture.source.display()
                )
            })?;
        if texture.width == 0 || texture.height == 0 || texture.data.len() != expected {
            return Err(format!(
                "scene texture {} is not a valid RGBA8 image",
                texture.source.display()
            ));
        }
        let index = count_u32(self.packet.textures.len(), "texture")?;
        self.packet.textures.push(Texture {
            size: [texture.width, texture.height],
            rgba: texture.data.clone(),
        });
        self.texture_ids.insert(key, index);
        Ok(Some(index))
    }

    fn material_index(&mut self, texture: Option<u32>) -> Result<u32, String> {
        if let Some(index) = self.material_ids.get(&texture) {
            return Ok(*index);
        }
        let index = count_u32(self.packet.materials.len(), "material")?;
        self.packet.materials.push(Material {
            texture,
            // The per-instance tint already carries the scene color. The
            // encoder multiplies both, so a white material avoids tinting twice.
            tint: [1.0; 4],
        });
        self.material_ids.insert(texture, index);
        Ok(index)
    }

    fn push_screen_triangles(
        &mut self,
        triangles: &[BasicScreenTriangle],
        extent: [u32; 2],
        _placement: Option<[f32; 4]>,
        command: usize,
    ) -> Result<(), String> {
        let [width, height] = extent;
        if width == 0 || height == 0 {
            return Err(format!(
                "scene frame command {command}: screen triangles need a non-zero target extent"
            ));
        }
        if triangles.is_empty() {
            return Err(format!(
                "scene frame command {command}: screen triangle batch is empty"
            ));
        }
        let material = self.material_index(None)?;
        let mut vertices = Vec::with_capacity(triangles.len() * 3);
        for triangle in triangles {
            let rgba = linear_rgba(triangle.color);
            for [x, y] in triangle.points {
                if !x.is_finite() || !y.is_finite() {
                    return Err(format!(
                        "scene frame command {command}: screen triangle points are not finite"
                    ));
                }
                vertices.push(Vertex {
                    // Screen triangles are a scene-group batch, so the scene
                    // viewport maps their clip space into the canvas rectangle.
                    position: [
                        2.0 * x / width as f32 - 1.0,
                        1.0 - 2.0 * y / height as f32,
                        0.0,
                    ],
                    uv: [0.0; 2],
                    rgba,
                    normal: [0.0; 3],
                });
            }
        }
        self.pixel_extent_used = true;
        self.packet
            .commands
            .push(Command::DrawScreenTriangleBatch { vertices, material });
        Ok(())
    }
}

/// Appends an RGBA CPU canvas as one image quad inside `rect`.
///
/// The CPU rasterizer is the recovery path, so its output must compose through
/// the same packet as a native scene. `rect` is the normalized target rect and
/// `size` is the pixel size of `pixels`.
pub fn append_cpu_canvas(
    packet: &mut ProductPacket,
    rect: [f32; 4],
    pixels: &[u8],
    size: [u32; 2],
) -> Result<u32, String> {
    if size[0] == 0 || size[1] == 0 {
        return Err("CPU canvas has a zero dimension".to_string());
    }
    let expected = (size[0] as usize)
        .checked_mul(size[1] as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "CPU canvas dimensions overflow".to_string())?;
    if pixels.len() != expected {
        return Err(format!(
            "CPU canvas has {} bytes, expected {expected} for {}x{} RGBA",
            pixels.len(),
            size[0],
            size[1]
        ));
    }
    if !rect.iter().all(|value| value.is_finite()) || rect[2] <= 0.0 || rect[3] <= 0.0 {
        return Err("CPU canvas target rect is invalid".to_string());
    }
    let texture = count_u32(packet.textures.len(), "texture")?;
    packet.textures.push(Texture {
        size,
        rgba: pixels.to_vec(),
    });
    packet
        .commands
        .push(Command::Paint(raf_agb::product::Paint::Image {
            quad: raf_agb::product::PaintQuad {
                rect,
                clip: [0.0, 0.0, 1.0, 1.0],
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                color: [1.0; 4],
            },
            texture,
        }));
    Ok(texture)
}

/// Camera and light direction for the scene half of the frame.
///
/// Environment color, exposure, and shadows stay with the composing host
/// because they depend on passes this producer does not own.
pub fn scene_frame_uniforms(frame: &SceneRenderFrame) -> FrameUniforms {
    let mut uniforms = FrameUniforms::default();
    uniforms.camera = Camera {
        view_projection: frame.view_proj,
        world_position: frame.commands.camera_eye,
    };
    uniforms.light.toward_light = frame.light_dir.try_normalize().unwrap_or(Vec3::Y);
    if let Some(environment) = frame.commands.environment {
        uniforms.light.toward_light = environment
            .sun_direction
            .try_normalize()
            .unwrap_or(uniforms.light.toward_light);
        let sun = environment.sun;
        if sun.is_finite() && sun.min_element() >= 0.0 {
            uniforms.light.color = sun.to_array();
        }
        uniforms.environment.ambient_tint = environment.ambient.to_array();
        uniforms.environment.authored = true;
        uniforms.environment.toon_steps = environment.settings.sun.shading_steps();
        if environment.improved() {
            uniforms.environment.improved = true;
            uniforms.environment.toon_softness = environment.softness();
            let exposure = environment.exposure();
            if exposure.is_finite() && exposure > 0.0 {
                uniforms.environment.exposure_ev = exposure.log2().clamp(-16.0, 16.0);
            }
        }
    }
    if let Some(shadow) = frame.commands.shadows.as_ref() {
        if shadow.settings.enabled {
            uniforms.shadows = Some(raf_agb::product::ShadowUniforms {
                light_view_projection: shadow.geometry.view_projection,
                bias: shadow.settings.bias / (8.0 * shadow.settings.distance),
                resolution: shadow.geometry.resolution,
                depth_key: shadow.geometry.key,
                distance: shadow.settings.distance,
                strength: shadow.settings.strength,
                slope_bias_per_texel: shadow
                    .geometry
                    .normalized_texel_bias(shadow.geometry.resolution)
                    * if shadow.settings.soft_filter {
                        1.5
                    } else {
                        0.85
                    },
                filter_taps: if !shadow.settings.soft_filter {
                    1
                } else if shadow.improved {
                    9
                } else {
                    4
                },
            });
        }
    }
    uniforms
}

/// Hand the scene-led packet to the product device. The caller owns the
/// device, output, and surface. A repeated revision costs no upload and no
/// rebuild; a new revision or a reconstructed device materializes the packet
/// from the builder's retained source.
pub fn prepare_product_scene<B: Backend>(
    device: &mut ProductDevice<B>,
    builder: &mut ProductFrameBuilder,
) -> Result<PacketHandle, String> {
    let key = builder.key();
    let revision = builder.revision();
    let mut failure = None;
    let prepared = device.prepare(key, revision, || match builder.handoff_packet() {
        Ok(packet) => packet,
        Err(cause) => {
            failure = Some(cause);
            ProductPacket::default()
        }
    });
    match prepared {
        Ok(handle) => {
            if let Some(cause) = failure {
                let _ = device.release(handle);
                Err(cause)
            } else {
                Ok(handle)
            }
        }
        Err(error) => {
            Err(failure
                .unwrap_or_else(|| format!("product device rejected the scene packet: {error}")))
        }
    }
}

/// Shared sRGB-byte to linear-float conversion. The basic GPU executors and
/// the product encoder must agree on color so a backend change is invisible.
pub(crate) fn linear_rgba(color: [u8; 4]) -> [f32; 4] {
    [
        crate::post_process::srgb_to_linear(color[0] as f32 / 255.0),
        crate::post_process::srgb_to_linear(color[1] as f32 / 255.0),
        crate::post_process::srgb_to_linear(color[2] as f32 / 255.0),
        color[3] as f32 / 255.0,
    ]
}

fn product_pipeline(kind: BasicPipelineKind) -> Result<Pipeline, String> {
    Ok(match kind {
        BasicPipelineKind::FlatColor | BasicPipelineKind::FlatColorTwoSided => Pipeline::SCENE,
        BasicPipelineKind::PbrLit | BasicPipelineKind::PbrLitTwoSided => Pipeline::LIT,
        BasicPipelineKind::XRay => Pipeline::OVERLAY,
        BasicPipelineKind::UnlitWireframe => {
            return Err(
                "scene pipeline UnlitWireframe has no product topology: wireframes are recorded \
                 as line batches"
                    .to_string(),
            );
        }
        // World-space retained UI is a textured quad. It keeps straight alpha
        // and disables depth, which is exactly the product overlay pipeline.
        BasicPipelineKind::UiSurface | BasicPipelineKind::UiSurfaceOverlay => Pipeline::OVERLAY,
    })
}

fn product_instance(transform: Mat4, color: [u8; 4], mesh_id: usize) -> Result<Instance, String> {
    if !transform.is_finite() {
        return Err(format!(
            "scene mesh {mesh_id} has a non-finite instance transform"
        ));
    }
    Ok(Instance::new(transform, linear_rgba(color)))
}

fn product_vertex(vertex: &BasicVertex, mesh_id: usize) -> Result<Vertex, String> {
    if !vertex.position.is_finite()
        || !vertex.normal.is_finite()
        || !vertex.uv.iter().all(|value| value.is_finite())
    {
        return Err(format!("scene mesh {mesh_id} has non-finite vertex data"));
    }
    Ok(Vertex {
        position: vertex.position.to_array(),
        uv: vertex.uv,
        rgba: [1.0; 4],
        normal: vertex.normal.to_array(),
    })
}

fn product_line(line: &BasicLine, no_depth_test: bool, command: usize) -> Result<Line, String> {
    if !line.start.is_finite() || !line.end.is_finite() {
        return Err(format!(
            "scene frame command {command}: line endpoints are not finite"
        ));
    }
    if !line.width.is_finite() || line.width <= 0.0 {
        return Err(format!(
            "scene frame command {command}: line width {} is invalid",
            line.width
        ));
    }
    if !line.depth_bias.is_finite() {
        return Err(format!(
            "scene frame command {command}: line depth bias is not finite"
        ));
    }
    Ok(Line {
        start: line.start.to_array(),
        end: line.end.to_array(),
        rgba: linear_rgba(line.color),
        width_px: line.width,
        no_depth_test,
        depth_bias: line.depth_bias,
    })
}

fn product_grid(y: f32, spacing: f32, no_depth_test: bool, command: usize) -> Result<Grid, String> {
    if !y.is_finite() || !spacing.is_finite() || spacing <= 0.0 {
        return Err(format!(
            "scene frame command {command}: grid position or spacing is invalid"
        ));
    }
    Ok(Grid {
        y,
        spacing,
        no_depth_test,
    })
}

fn count(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn count_u32(value: usize, table: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("scene packet {table} table exceeds u32"))
}

#[cfg(test)]
mod tests;
