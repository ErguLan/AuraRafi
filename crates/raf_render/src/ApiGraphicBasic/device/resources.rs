use std::collections::HashSet;
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::api_graphic_basic::command_list::GraphicCommand;
use crate::api_graphic_basic::memory_ledger::GraphicsResourceFamily;
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::api_graphic_basic::resource_registry::ResourceAdmission;
use crate::scene_renderer::SceneRenderFrame;
use crate::texture::CpuTexture;

use super::{
    rgba8_to_f32, write_uniform_if_changed, GpuLineSlot, GpuLineVertex, GpuMeshBuffers,
    GpuMeshInstance, GpuMeshInstanceSlot, GpuMeshVertex, GpuOverlaySlot, GpuOverlayTriangle,
    GpuSceneState, GpuSceneTarget, GpuSceneTexture, GpuTextureView, GpuTransientMeshSlot,
    GpuUniformSlot, LineUniforms, MeshUniforms, OverlayUniforms, SceneFrameMetrics,
};

impl GpuSceneState {
    pub(super) fn prepare_scene_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &SceneRenderFrame,
    ) -> u64 {
        let has_textured_draw = frame.commands.commands().iter().any(|command| {
            let GraphicCommand::DrawMesh {
                texture_id: Some(texture_id),
                ..
            } = command
            else {
                return false;
            };
            frame.commands.texture(*texture_id).is_some()
        });
        if !has_textured_draw {
            // CAD overlays carry a zero budget; preserve the scene cache for
            // them, while still applying changed quality caps on scene frames.
            if frame.texture_cache_budget_bytes > 0 {
                self.texture_cache_budget_bytes = frame
                    .texture_cache_budget_bytes
                    .min(self.texture_registry.budget_bytes());
                self.texture_registry
                    .set_budget_bytes(self.texture_cache_budget_bytes);
                self.ledger.set_family_budget(
                    GraphicsResourceFamily::Texture,
                    self.texture_cache_budget_bytes,
                );
                self.texture_handles
                    .retain(|_, handle| self.texture_registry.contains(*handle));
            }
            return 0;
        }

        self.texture_cache_budget_bytes = frame
            .texture_cache_budget_bytes
            .min(self.texture_registry.budget_bytes());
        self.texture_registry
            .set_budget_bytes(self.texture_cache_budget_bytes);
        self.ledger.set_family_budget(
            GraphicsResourceFamily::Texture,
            self.texture_cache_budget_bytes,
        );
        self.texture_handles
            .retain(|_, handle| self.texture_registry.contains(*handle));

        let mut seen = HashSet::new();
        let mut textures = Vec::new();
        for command in frame.commands.commands() {
            let GraphicCommand::DrawMesh {
                texture_id: Some(texture_id),
                ..
            } = command
            else {
                continue;
            };
            let Some(texture) = frame.commands.texture(*texture_id) else {
                continue;
            };
            if seen.insert(texture.source.clone()) {
                textures.push(Arc::clone(texture));
            }
        }

        let mut uploaded_bytes = 0_u64;
        for texture in textures {
            let key = texture.source.clone();
            if let Some(handle) = self.texture_handles.get(&key).copied() {
                if self
                    .texture_registry
                    .get(handle, self.frame_index)
                    .is_some()
                {
                    self.texture_registry.pin(handle, true);
                    continue;
                }
                self.texture_handles.remove(&key);
            }
            let bytes = texture.memory_bytes() as u64;
            if bytes == 0 || bytes > self.texture_cache_budget_bytes {
                continue;
            }
            if uploaded_bytes.saturating_add(bytes) > self.frame_upload_budget {
                self.frame_upload_rejected = self.frame_upload_rejected.saturating_add(1);
                continue;
            }
            self.texture_handles
                .retain(|_, handle| self.texture_registry.contains(*handle));
            while self.texture_handles.len() >= self.texture_cache_limit
                && self.texture_cache_limit > 0
            {
                let Some(evicted) = self.texture_registry.evict_least_recently_used_unpinned()
                else {
                    break;
                };
                self.texture_handles.retain(|_, handle| *handle != evicted);
            }
            if self.texture_cache_limit == 0
                || self.texture_handles.len() >= self.texture_cache_limit
            {
                continue;
            }
            let gpu_texture = create_gpu_scene_texture(
                device,
                queue,
                &self.scene_texture_bind_group_layout,
                &self.scene_texture_sampler,
                texture.as_ref(),
                "ApiGraphicBasic.SceneAssetTexture",
            );
            let Ok(handle) = self.texture_registry.insert(
                gpu_texture,
                ResourceAdmission::pinned(bytes, self.frame_index),
            ) else {
                continue;
            };
            self.texture_handles.insert(key, handle);
            uploaded_bytes = uploaded_bytes.saturating_add(bytes);
            self.texture_handles
                .retain(|_, handle| self.texture_registry.contains(*handle));
        }
        uploaded_bytes
    }

    pub(super) fn scene_texture_bind_group(&mut self, texture: &CpuTexture) -> Arc<wgpu::BindGroup> {
        let handle = self.texture_handles.get(&texture.source).copied();
        handle
            .and_then(|handle| self.texture_registry.get(handle, self.frame_index))
            .map(|texture| Arc::clone(&texture.bind_group))
            .unwrap_or_else(|| Arc::clone(&self.fallback_texture_bind_group))
    }

    pub(super) fn external_texture_bind_group(
        &mut self,
        device: &wgpu::Device,
        texture: &GpuTextureView,
    ) -> Arc<wgpu::BindGroup> {
        let key = Arc::as_ptr(&texture.view) as usize;
        self.external_texture_bindings
            .entry(key)
            .or_insert_with(|| {
                let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ApiGraphicBasic.GameUiSceneBinding"),
                    layout: &self.scene_texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(texture.as_wgpu()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.ui_texture_sampler),
                        },
                    ],
                });
                (texture.clone(), Arc::new(binding))
            })
            .1
            .clone()
    }

    pub(super) fn ensure_target(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let needs_rebuild = self
            .target
            .as_ref()
            .map(|target| target.width != width || target.height != height)
            .unwrap_or(true);

        if !needs_rebuild {
            return false;
        }

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ApiGraphicBasic.SceneColor"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view =
            Arc::new(color_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ApiGraphicBasic.SceneDepth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view =
            Arc::new(depth_texture.create_view(&wgpu::TextureViewDescriptor::default()));

        self.target = Some(GpuSceneTarget {
            width,
            height,
            _color_texture: color_texture,
            color_view,
            _depth_texture: depth_texture,
            depth_view,
        });
        self.target_generation = self.target_generation.wrapping_add(1).max(1);

        true
    }

    pub(super) fn draw_line_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        lines: &[crate::api_graphic_basic::command_list::BasicLine],
        frame: &SceneRenderFrame,
        no_depth_test: bool,
        batch_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) {
        if lines.is_empty() {
            return;
        }

        self.line_vertex_scratch.clear();
        self.line_vertex_scratch.extend(
            lines
                .iter()
                .filter_map(|line| {
                    crate::api_graphic_basic::line_projection::prepare_line(
                        line,
                        &frame.view_proj,
                        [frame.width as f32, frame.height as f32],
                    )
                })
                .map(|line| GpuLineVertex {
                    start: line.start.to_array(),
                    _start_padding: 0.0,
                    end: line.end.to_array(),
                    _end_padding: 0.0,
                    color: rgba8_to_f32(line.color),
                    width: line.width.max(1.0),
                    depth_bias: line.depth_bias,
                    _padding: [0.0, 0.0],
                }),
        );
        if self.line_vertex_scratch.is_empty() {
            return;
        }

        let line_count = self.line_vertex_scratch.len();
        let line_slot = self.ensure_line_slot(device, batch_index, line_count, metrics);
        let uniforms = LineUniforms {
            mvp: frame.view_proj.to_cols_array_2d(),
            viewport: [frame.width as f32, frame.height as f32],
            _padding: [0.0, 0.0],
        };
        queue.write_buffer(
            &line_slot.vertex_buffer,
            0,
            bytemuck::cast_slice(self.line_vertex_scratch.as_slice()),
        );
        write_uniform_if_changed(
            queue,
            &line_slot.uniform,
            &mut self.line_uniform_reuse,
            batch_index,
            uniforms,
            metrics,
        );
        metrics.line_upload_bytes +=
            std::mem::size_of_val(self.line_vertex_scratch.as_slice()) as u64;
        metrics.line_draw_calls += 1;

        pass.set_pipeline(if no_depth_test {
            &self.line_pipeline_xray
        } else {
            &self.line_pipeline_depth
        });
        pass.set_bind_group(0, line_slot.uniform.bind_group.as_ref(), &[]);
        pass.set_vertex_buffer(0, line_slot.vertex_buffer.slice(..));
        pass.draw(0..6, 0..line_count as u32);
    }

    pub(super) fn draw_overlay_triangle_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        triangles: &[crate::api_graphic_basic::command_list::BasicScreenTriangle],
        frame: &SceneRenderFrame,
        batch_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) {
        if triangles.is_empty() {
            return;
        }

        self.overlay_vertex_scratch.clear();
        self.overlay_vertex_scratch
            .extend(triangles.iter().map(|triangle| GpuOverlayTriangle {
                point_0: [triangle.points[0][0], triangle.points[0][1], 0.0, 0.0],
                point_1: [triangle.points[1][0], triangle.points[1][1], 0.0, 0.0],
                point_2: [triangle.points[2][0], triangle.points[2][1], 0.0, 0.0],
                color: rgba8_to_f32(triangle.color),
            }));

        let triangle_count = self.overlay_vertex_scratch.len();
        let slot = self.ensure_overlay_slot(device, batch_index, triangle_count, metrics);
        let uniforms = OverlayUniforms {
            viewport: [frame.width as f32, frame.height as f32],
            _padding: [0.0, 0.0],
        };
        queue.write_buffer(
            &slot.vertex_buffer,
            0,
            bytemuck::cast_slice(self.overlay_vertex_scratch.as_slice()),
        );
        write_uniform_if_changed(
            queue,
            &slot.uniform,
            &mut self.overlay_uniform_reuse,
            batch_index,
            uniforms,
            metrics,
        );
        metrics.overlay_upload_bytes +=
            std::mem::size_of_val(self.overlay_vertex_scratch.as_slice()) as u64;
        metrics.overlay_draw_calls += 1;

        pass.set_pipeline(&self.overlay_pipeline);
        pass.set_bind_group(0, slot.uniform.bind_group.as_ref(), &[]);
        pass.set_vertex_buffer(0, slot.vertex_buffer.slice(..));
        pass.draw(0..3, 0..triangle_count as u32);
    }

    pub(super) fn ensure_transient_mesh_slot(
        &mut self,
        device: &wgpu::Device,
        draw_index: usize,
        vertex_bytes: u64,
        index_bytes: u64,
        metrics: &mut SceneFrameMetrics,
    ) -> GpuTransientMeshSlot {
        while self.transient_mesh_slots.len() <= draw_index {
            self.transient_mesh_slots.push(create_transient_mesh_slot(
                device,
                vertex_bytes,
                index_bytes,
            ));
            metrics.transient_mesh_slot_creations += 1;
        }
        let must_grow = {
            let slot = &self.transient_mesh_slots[draw_index];
            slot.vertex_capacity < vertex_bytes || slot.index_capacity < index_bytes
        };
        if must_grow {
            self.transient_mesh_slots[draw_index] =
                create_transient_mesh_slot(device, vertex_bytes, index_bytes);
            metrics.transient_mesh_slot_creations += 1;
        }
        self.transient_mesh_slots[draw_index].clone()
    }

    pub(super) fn ensure_mesh_uniform_slot(
        &mut self,
        device: &wgpu::Device,
        draw_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) -> GpuUniformSlot {
        while self.mesh_uniform_slots.len() <= draw_index {
            self.mesh_uniform_slots.push(create_uniform_slot(
                device,
                &self.mesh_bind_group_layout,
                std::mem::size_of::<MeshUniforms>() as u64,
                "ApiGraphicBasic.MeshUniformBuffer",
                "ApiGraphicBasic.MeshBindGroup",
            ));
            metrics.mesh_uniform_slot_creations += 1;
        }

        self.mesh_uniform_slots[draw_index].clone()
    }

    pub(super) fn ensure_mesh_instance_slot(
        &mut self,
        device: &wgpu::Device,
        batch_index: usize,
        required_capacity: usize,
        metrics: &mut SceneFrameMetrics,
    ) -> GpuMeshInstanceSlot {
        let required_capacity = required_capacity.max(1);
        while self.mesh_instance_slots.len() <= batch_index {
            self.mesh_instance_slots
                .push(create_mesh_instance_slot(device, required_capacity));
            metrics.mesh_instance_slot_creations += 1;
        }
        if self.mesh_instance_slots[batch_index].capacity < required_capacity {
            self.mesh_instance_slots[batch_index] =
                create_mesh_instance_slot(device, required_capacity);
            metrics.mesh_instance_slot_creations += 1;
        }
        self.mesh_instance_slots[batch_index].clone()
    }

    pub(super) fn ensure_line_slot(
        &mut self,
        device: &wgpu::Device,
        draw_index: usize,
        required_capacity: usize,
        metrics: &mut SceneFrameMetrics,
    ) -> GpuLineSlot {
        let required_capacity = slot_capacity(required_capacity);
        while self.line_slots.len() <= draw_index {
            self.line_slots.push(GpuLineSlot {
                vertex_buffer: Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ApiGraphicBasic.LineInstanceBuffer"),
                    size: (std::mem::size_of::<GpuLineVertex>() * required_capacity.max(1)) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })),
                uniform: create_uniform_slot(
                    device,
                    &self.line_bind_group_layout,
                    std::mem::size_of::<LineUniforms>() as u64,
                    "ApiGraphicBasic.LineUniformBuffer",
                    "ApiGraphicBasic.LineBindGroup",
                ),
                capacity: required_capacity.max(1),
            });
            metrics.line_slot_creations += 1;
        }

        if self.line_slots[draw_index].capacity < required_capacity {
            self.line_slots[draw_index].vertex_buffer =
                Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ApiGraphicBasic.LineInstanceBufferGrow"),
                    size: (std::mem::size_of::<GpuLineVertex>() * required_capacity) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            self.line_slots[draw_index].capacity = required_capacity;
            metrics.line_slot_creations += 1;
        }

        self.line_slots[draw_index].clone()
    }

    pub(super) fn ensure_overlay_slot(
        &mut self,
        device: &wgpu::Device,
        batch_index: usize,
        required_capacity: usize,
        metrics: &mut SceneFrameMetrics,
    ) -> GpuOverlaySlot {
        let required_capacity = slot_capacity(required_capacity);
        while self.overlay_slots.len() <= batch_index {
            self.overlay_slots.push(GpuOverlaySlot {
                vertex_buffer: Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ApiGraphicBasic.OverlayTriangleBuffer"),
                    size: (std::mem::size_of::<GpuOverlayTriangle>() * required_capacity) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })),
                uniform: create_uniform_slot(
                    device,
                    &self.overlay_bind_group_layout,
                    std::mem::size_of::<OverlayUniforms>() as u64,
                    "ApiGraphicBasic.OverlayUniformBuffer",
                    "ApiGraphicBasic.OverlayBindGroup",
                ),
                capacity: required_capacity,
            });
            metrics.overlay_slot_creations += 1;
        }

        if self.overlay_slots[batch_index].capacity < required_capacity {
            self.overlay_slots[batch_index].vertex_buffer =
                Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ApiGraphicBasic.OverlayTriangleBufferGrow"),
                    size: (std::mem::size_of::<GpuOverlayTriangle>() * required_capacity) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            self.overlay_slots[batch_index].capacity = required_capacity;
            metrics.overlay_slot_creations += 1;
        }

        self.overlay_slots[batch_index].clone()
    }

    /// Publish one frame of residency into the cross-family ledger.
    ///
    /// A bounded arena is the authoritative owner of its family, so this runs
    /// once per frame after all admissions and evictions have settled. Direct
    /// families use `try_admit`/`release` instead and must not be synced here.
    pub(super) fn sync_memory_ledger_metrics(&mut self, metrics: &mut SceneFrameMetrics) {
        self.ledger.touch(GraphicsResourceFamily::Mesh);
        self.ledger.touch(GraphicsResourceFamily::Texture);
        self.mesh_registry
            .sync_ledger(&mut self.ledger, GraphicsResourceFamily::Mesh);
        self.texture_registry
            .sync_ledger(&mut self.ledger, GraphicsResourceFamily::Texture);

        let transient_bytes = self.transient_slot_bytes();
        if transient_bytes > 0 {
            self.ledger.touch(GraphicsResourceFamily::Transient);
        }
        self.ledger
            .sync_resident(GraphicsResourceFamily::Transient, transient_bytes);

        let snapshot = self.ledger.snapshot();
        metrics.ledger_resident_bytes = snapshot.resident_bytes;
        metrics.ledger_peak_bytes = snapshot.peak_bytes;
        metrics.ledger_rejected_reservations = snapshot.rejected_total;
        metrics.ledger_pressure = snapshot.pressure;
        metrics.frame_upload_rejected = self.frame_upload_rejected;
    }

    fn transient_slot_bytes(&self) -> u64 {
        self.transient_mesh_slots.iter().fold(0_u64, |total, slot| {
            total
                .saturating_add(slot.vertex_capacity)
                .saturating_add(slot.index_capacity)
        })
    }

    pub(super) fn make_room_for_cached_mesh(&mut self) {
        self.mesh_cache
            .retain(|_, handle| self.mesh_registry.contains(*handle));
        if self.mesh_cache.len() < self.mesh_cache_limit {
            return;
        }
        if let Some(evicted) = self.mesh_registry.evict_least_recently_used_unpinned() {
            self.mesh_cache.retain(|_, handle| *handle != evicted);
        }
    }
}

#[inline]
pub(super) fn slot_capacity(required_capacity: usize) -> usize {
    required_capacity.max(1).next_power_of_two()
}

pub(super) fn create_gpu_mesh_buffers(device: &wgpu::Device, mesh: &BasicMesh) -> GpuMeshBuffers {
    let vertices: Vec<GpuMeshVertex> = mesh
        .vertices
        .iter()
        .map(|vertex| GpuMeshVertex {
            position: vertex.position.to_array(),
            normal: vertex.normal.to_array(),
            uv: vertex.uv,
        })
        .collect();
    let vertex_bytes = std::mem::size_of_val(vertices.as_slice()) as u64;
    let index_bytes = std::mem::size_of_val(mesh.indices.as_slice()) as u64;
    let vertex_buffer = Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshVertexBuffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    );
    let index_buffer = Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ApiGraphicBasic.MeshIndexBuffer"),
            contents: bytemuck::cast_slice(mesh.indices.as_slice()),
            usage: wgpu::BufferUsages::INDEX,
        }),
    );

    GpuMeshBuffers {
        vertex_buffer,
        index_buffer,
        index_count: mesh.indices.len() as u32,
        vertex_bytes,
        index_bytes,
    }
}

pub(super) fn create_gpu_scene_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    image: &CpuTexture,
    label: &str,
) -> GpuSceneTexture {
    let texture = Arc::new(device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: image.width.max(1),
            height: image.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }));
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: texture.as_ref(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &image.data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(image.width.max(1) * 4),
            rows_per_image: Some(image.height.max(1)),
        },
        wgpu::Extent3d {
            width: image.width.max(1),
            height: image.height.max(1),
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    }));
    GpuSceneTexture {
        _texture: texture,
        bind_group,
    }
}

fn create_transient_mesh_slot(
    device: &wgpu::Device,
    vertex_bytes: u64,
    index_bytes: u64,
) -> GpuTransientMeshSlot {
    let vertex_capacity = vertex_bytes.max(256).next_power_of_two();
    let index_capacity = index_bytes.max(256).next_power_of_two();
    GpuTransientMeshSlot {
        vertex_buffer: Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.TransientMeshVertexSlot"),
            size: vertex_capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })),
        index_buffer: Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.TransientMeshIndexSlot"),
            size: index_capacity,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })),
        vertex_capacity,
        index_capacity,
    }
}

fn create_mesh_instance_slot(device: &wgpu::Device, capacity: usize) -> GpuMeshInstanceSlot {
    let capacity = capacity.max(1).next_power_of_two();
    GpuMeshInstanceSlot {
        buffer: Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.MeshInstanceBuffer"),
            size: (std::mem::size_of::<GpuMeshInstance>() * capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })),
        capacity,
    }
}

fn create_uniform_slot(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    size: u64,
    buffer_label: &'static str,
    bind_group_label: &'static str,
) -> GpuUniformSlot {
    let buffer = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(buffer_label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    }));
    let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(bind_group_label),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    }));

    GpuUniformSlot { buffer, bind_group }
}
