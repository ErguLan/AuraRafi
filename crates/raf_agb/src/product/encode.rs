//! Encode an owned retained revision into the shared native raster streams.

mod paint;
mod scene_primitives;
mod sky;
mod styled_paint;

use std::{collections::HashMap, ops::Range};

use glam::Mat4;

use crate::{Batch, DrawMode, Error, Instance, Result, Texture, Vertex, Workload};

use super::{Command, Feature, Material, Mesh, MeshSpan, Pipeline, ProductPacket};

pub(super) struct Encoded {
    pub workload: Workload,
    pub clear: [f32; 4],
    pub meshes: HashMap<(u32, bool), MeshSpan>,
    pub instances: HashMap<usize, Range<u32>>,
}

pub(super) fn encode(packet: ProductPacket, limit: u64) -> Result<Encoded> {
    let ProductPacket {
        meshes,
        materials,
        mut textures,
        commands,
    } = packet;
    let white = u32::try_from(textures.len()).map_err(|_| Error::Invalid("too many textures"))?;
    textures.push(Texture {
        size: [1, 1],
        rgba: vec![255; 4],
    });
    let overlay = Instance::new(Mat4::IDENTITY, [1.; 4]);
    let mut encoder = Encoder {
        workload: Workload {
            textures,
            instances: vec![overlay],
            ..Workload::default()
        },
        meshes: &meshes,
        materials: &materials,
        mesh_cache: HashMap::new(),
        instance_ranges: HashMap::new(),
        remaining: limit,
        limit,
        white,
        pipeline: Pipeline::SCENE,
        overlay: false,
        line_instance: None,
        shadow_receiver: false,
        clear: [0., 0., 0., 1.],
        paint_scratch: styled_paint::PaintScratch::new(),
    };
    encoder.reserve(encoder.workload.bytes())?;
    for (index, command) in commands.into_iter().enumerate() {
        encoder.command(index, command)?;
    }
    if encoder.workload.vertices.is_empty() {
        // A clear-only frame still needs valid vertex bindings on every API.
        encoder.add_vertices(&[vertex([0., 0.], [0., 0.], [0.; 4])])?;
    }
    Ok(Encoded {
        workload: encoder.workload,
        clear: encoder.clear,
        meshes: encoder.mesh_cache,
        instances: encoder.instance_ranges,
    })
}

struct Encoder<'a> {
    workload: Workload,
    meshes: &'a [Mesh],
    materials: &'a [Material],
    mesh_cache: HashMap<(u32, bool), MeshSpan>,
    instance_ranges: HashMap<usize, Range<u32>>,
    remaining: u64,
    limit: u64,
    white: u32,
    pipeline: Pipeline,
    overlay: bool,
    line_instance: Option<u32>,
    shadow_receiver: bool,
    clear: [f32; 4],
    paint_scratch: styled_paint::PaintScratch,
}

impl Encoder<'_> {
    fn command(&mut self, index: usize, command: Command) -> Result<()> {
        match command {
            Command::Clear(clear) => {
                if index != 0 {
                    return unsupported(Feature::Clear);
                }
                validate_color(clear)?;
                self.clear = clear;
            }
            Command::SetPipeline(pipeline) => {
                if ![Pipeline::SCENE, Pipeline::LIT, Pipeline::OVERLAY].contains(&pipeline) {
                    return unsupported(Feature::Pipeline);
                }
                self.pipeline = pipeline;
            }
            Command::SetShadowReceiver(receiver) => self.shadow_receiver = receiver,
            Command::DrawMesh {
                mesh,
                material,
                instance,
            } => self.mesh(index, mesh, material, std::slice::from_ref(&instance))?,
            Command::DrawMeshBatch {
                mesh,
                material,
                instances,
            } => self.mesh(index, mesh, material, &instances)?,
            Command::DrawLine(line) => self.line(line)?,
            Command::DrawLineBatch(lines) => {
                for line in lines {
                    self.line(line)?;
                }
            }
            Command::DrawGrid(grid) => self.grid(grid)?,
            Command::DrawSky(sky) => self.sky(sky)?,
            Command::SceneBackground(color) => self.scene_background(color)?,
            Command::DrawScreenTriangleBatch { vertices, material } => {
                if self.pipeline != Pipeline::OVERLAY {
                    return Err(Error::Unsupported(
                        "screen triangles require the overlay pipeline",
                    ));
                }
                self.screen_triangles(vertices, material, DrawMode::SceneNoDepth)?;
            }
            Command::Paint(paint) | Command::Cad { paint, .. } => self.paint(paint)?,
            Command::StyledPaint { paint, style } | Command::CadStyled { paint, style, .. } => {
                self.styled_paint(paint, &style)?;
            }
            Command::CadTriangles {
                vertices, material, ..
            } => self.screen_triangles(vertices, material, DrawMode::Overlay)?,
        }
        Ok(())
    }

    /// A clip-space fullscreen triangle in the scene group. It uses the white
    /// texture and the overlay pipeline, so the compositor can paint a canvas
    /// background that the scene viewport clips while RafUI stays full-target.
    fn scene_background(&mut self, color: [f32; 4]) -> Result<()> {
        validate_color(color)?;
        let vertices = [[-1., -1.], [3., -1.], [-1., 3.]].map(|[x, y]| Vertex {
            position: [x, y, 0.],
            uv: [1.; 2],
            rgba: color,
            normal: [0.; 3],
        });
        self.screen_batch(&vertices, self.white, DrawMode::SceneNoDepth)
    }

    fn screen_triangles(
        &mut self,
        mut vertices: Vec<Vertex>,
        material: u32,
        mode: DrawMode,
    ) -> Result<()> {
        let (texture, tint) = self.material(material)?;
        for vertex in &mut vertices {
            validate_vertex(vertex)?;
            if !(0. ..=1.).contains(&vertex.position[2]) {
                return Err(Error::Invalid("screen triangle depth outside clip volume"));
            }
            multiply(&mut vertex.rgba, tint);
            vertex.normal = [0.; 3];
        }
        self.screen_batch(&vertices, texture, mode)
    }

    fn mesh(
        &mut self,
        command: usize,
        mesh: u32,
        material: u32,
        instances: &[Instance],
    ) -> Result<()> {
        if self.overlay || self.pipeline == Pipeline::OVERLAY {
            return Err(Error::Unsupported(
                "depth-tested meshes must precede overlays",
            ));
        }
        if instances.is_empty() {
            return Err(Error::Invalid("empty instance batch"));
        }
        let (texture, tint) = self.material(material)?;
        let key = (mesh, self.pipeline.lit);
        let ranges = if let Some(range) = self.mesh_cache.get(&key) {
            range.clone()
        } else {
            let data = self
                .meshes
                .get(mesh as usize)
                .ok_or(Error::Invalid("unknown mesh"))?;
            let vertices = &data.vertices;
            let count = if data.indices.is_empty() {
                vertices.len()
            } else {
                data.indices.len()
            };
            if count == 0 || count % 3 != 0 {
                return Err(Error::Invalid("invalid mesh triangle list"));
            }
            let range = self.vertex_range(vertices.len())?;
            for vertex in vertices {
                validate_vertex(vertex)?;
                if self.pipeline.lit && vertex.normal == [0.; 3] {
                    return Err(Error::Invalid("lit mesh vertex has no normal"));
                }
            }
            if data.indices.iter().any(|&i| i as usize >= vertices.len()) {
                return Err(Error::Invalid("mesh index out of range"));
            }
            let strip = |mut vertex: Vertex| {
                if !key.1 {
                    vertex.normal = [0.; 3];
                }
                vertex
            };
            self.workload
                .vertices
                .extend(vertices.iter().copied().map(strip));
            let indices = if data.indices.is_empty() {
                None
            } else {
                let indices = stream_range(self.workload.indices.len(), data.indices.len())?;
                self.reserve(std::mem::size_of_val(data.indices.as_slice()) as u64)?;
                self.workload
                    .indices
                    .extend(data.indices.iter().map(|&i| range.start + i));
                Some(indices)
            };
            let ranges = MeshSpan {
                vertices: range,
                indices,
            };
            self.mesh_cache.insert(key, ranges.clone());
            ranges
        };
        let instance_range = stream_range(self.workload.instances.len(), instances.len())?;
        self.reserve(std::mem::size_of_val(instances) as u64)?;
        for instance in instances {
            if !instance
                .transform
                .as_flattened()
                .iter()
                .all(|v| v.is_finite())
            {
                return Err(Error::Invalid("non-finite instance transform"));
            }
            validate_color(instance.tint)?;
        }
        self.workload
            .instances
            .extend(instances.iter().map(|instance| {
                let mut instance = *instance;
                instance.raster_kind = if self.shadow_receiver { 256 } else { 0 };
                multiply(&mut instance.tint, tint);
                instance
            }));
        self.instance_ranges.insert(command, instance_range.clone());
        self.batch(Batch {
            mode: DrawMode::Scene,
            vertices: ranges.vertices,
            indices: ranges.indices,
            instances: instance_range,
            texture,
        })
    }

    fn overlay_triangles(&mut self, vertices: &[Vertex], texture: u32) -> Result<()> {
        self.screen_batch(vertices, texture, DrawMode::Overlay)
    }

    fn screen_batch(&mut self, vertices: &[Vertex], texture: u32, mode: DrawMode) -> Result<()> {
        if vertices.is_empty() || vertices.len() % 3 != 0 {
            return Err(Error::Invalid("invalid overlay triangle list"));
        }
        if self.overlay && mode.is_scene() {
            return Err(Error::Invalid(
                "scene triangles must precede retained UI composition",
            ));
        }
        let vertices = self.add_vertices(vertices)?;
        self.overlay |= mode == DrawMode::Overlay;
        self.batch(Batch {
            mode,
            vertices,
            indices: None,
            instances: 0..1,
            texture,
        })
    }

    fn vertex_range(&mut self, count: usize) -> Result<Range<u32>> {
        let range = stream_range(self.workload.vertices.len(), count)?;
        self.reserve((count as u64).saturating_mul(size_of::<Vertex>() as u64))?;
        Ok(range)
    }

    fn add_vertices(&mut self, vertices: &[Vertex]) -> Result<Range<u32>> {
        let range = self.vertex_range(vertices.len())?;
        self.workload.vertices.extend_from_slice(vertices);
        Ok(range)
    }

    fn batch(&mut self, batch: Batch) -> Result<()> {
        if let Some(last) = self.workload.batches.last_mut() {
            if last.mode == batch.mode && last.texture == batch.texture {
                if last.indices.is_none()
                    && batch.indices.is_none()
                    && last.instances == batch.instances
                    && last.vertices.end == batch.vertices.start
                {
                    last.vertices.end = batch.vertices.end;
                    return Ok(());
                }
                if last.vertices == batch.vertices
                    && last.indices == batch.indices
                    && last.instances.end == batch.instances.start
                {
                    last.instances.end = batch.instances.end;
                    return Ok(());
                }
            }
        }
        self.reserve(size_of::<Batch>() as u64)?;
        self.workload.batches.push(batch);
        Ok(())
    }

    fn material(&self, index: u32) -> Result<(u32, [f32; 4])> {
        let material = self
            .materials
            .get(index as usize)
            .ok_or(Error::Invalid("unknown material"))?;
        validate_color(material.tint)?;
        let texture = match material.texture {
            Some(texture) => self.texture(texture)?,
            None => self.white,
        };
        Ok((texture, material.tint))
    }

    fn texture(&self, texture: u32) -> Result<u32> {
        if texture >= self.white {
            return Err(Error::Invalid("unknown texture or atlas"));
        }
        Ok(texture)
    }

    fn reserve(&mut self, bytes: u64) -> Result<()> {
        self.remaining = self.remaining.checked_sub(bytes).ok_or(Error::Budget {
            requested: self
                .limit
                .saturating_sub(self.remaining)
                .saturating_add(bytes),
            available: self.limit,
        })?;
        Ok(())
    }
}

fn unsupported<T>(feature: Feature) -> Result<T> {
    Err(Error::Unsupported(feature.name()))
}

fn stream_range(start: usize, count: usize) -> Result<Range<u32>> {
    let start = u32::try_from(start).map_err(|_| Error::Invalid("stream exceeds range limit"))?;
    let count = u32::try_from(count).map_err(|_| Error::Invalid("stream exceeds range limit"))?;
    Ok(start
        ..start
            .checked_add(count)
            .ok_or(Error::Invalid("stream exceeds range limit"))?)
}

fn validate_color(color: [f32; 4]) -> Result<()> {
    if color.into_iter().all(|value| (0.0..=1.0).contains(&value)) {
        Ok(())
    } else {
        Err(Error::Invalid("RGBA must be finite and normalized"))
    }
}

fn validate_vertex(vertex: &Vertex) -> Result<()> {
    validate_color(vertex.rgba)?;
    if vertex
        .position
        .iter()
        .chain(&vertex.uv)
        .chain(&vertex.normal)
        .all(|v| v.is_finite())
    {
        Ok(())
    } else {
        Err(Error::Invalid("non-finite vertex data"))
    }
}

fn multiply(color: &mut [f32; 4], tint: [f32; 4]) {
    color
        .iter_mut()
        .zip(tint)
        .for_each(|(v, factor)| *v *= factor);
}

fn vertex([x, y]: [f32; 2], uv: [f32; 2], rgba: [f32; 4]) -> Vertex {
    Vertex {
        position: [2. * x - 1., 1. - 2. * y, 0.],
        uv,
        rgba,
        normal: [0.; 3],
    }
}
