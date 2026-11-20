use std::borrow::Cow;
use std::sync::Arc;

use crate::api_graphic_basic::command_list::{BasicMeshInstance, GraphicCommand};
use crate::api_graphic_basic::handles::TextureHandle;
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use crate::api_graphic_basic::resource_registry::ResourceAdmission;
use crate::engine_time::Instant;
use crate::scene_renderer::SceneRenderFrame;
use crate::shaders::basic_scene_shader;

use super::backend_wgpu::create_scene_mesh_pipeline;
use super::resources::create_gpu_mesh_buffers;
use super::timestamps::GPU_TIMESTAMP_QUERY_BYTES;
use super::{
    rgba8_to_f32, write_uniform_if_changed, BasicDevice, GpuMeshInstance, GpuMeshVertex,
    GpuSceneState, GpuTextureView, MeshUniforms, SceneFrameCapture, SceneFrameMetrics,
    SceneFrameOutput,
};

impl BasicDevice {
    pub(super) fn capture_gpu_scene_rgba(&self) -> Result<SceneFrameCapture, String> {
        let gpu_scene = self
            .gpu_scene
            .as_ref()
            .ok_or_else(|| "The GPU scene renderer is not initialized.".to_string())?;
        let target = gpu_scene
            .target
            .as_ref()
            .ok_or_else(|| "The Game viewport has not rendered a frame yet.".to_string())?;
        let device = self
            .wgpu_device
            .as_ref()
            .ok_or_else(|| "The GPU device is not available for readback.".to_string())?;
        let queue = self
            .wgpu_queue
            .as_ref()
            .ok_or_else(|| "The GPU queue is not available for readback.".to_string())?;

        let width = target.width;
        let height = target.height;
        if width == 0 || height == 0 {
            return Err("The GPU scene target has no rendered pixels.".to_string());
        }
        let unpadded_row = width
            .checked_mul(4)
            .ok_or_else(|| "The GPU scene row size overflowed.".to_string())?;
        let padded_row = unpadded_row
            .checked_add(255)
            .map(|row| row / 256 * 256)
            .ok_or_else(|| "The GPU scene row alignment overflowed.".to_string())?;
        let buffer_size = (padded_row as u64)
            .checked_mul(height as u64)
            .ok_or_else(|| "The GPU scene readback size overflowed.".to_string())?;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ApiGraphicBasic.AgentViewportReadback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.AgentViewportReadbackEncoder"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: gpu_scene
                    .optimized_filter
                    .as_ref()
                    .map(|f| &f.texture)
                    .unwrap_or(&target._color_texture),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));

        let mapped = Arc::new(std::sync::Mutex::new(None));
        let mapped_result = Arc::clone(&mapped);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                *mapped_result.lock().expect("scene readback callback lock") = Some(result);
            });
        let _ = device.poll(wgpu::Maintain::Wait);
        let map_result = mapped
            .lock()
            .expect("scene readback result lock")
            .take()
            .ok_or_else(|| "The GPU scene readback callback did not complete.".to_string())?;
        map_result.map_err(|error| format!("The GPU scene readback failed: {error}"))?;

        let row_bytes = unpadded_row as usize;
        let padded_row_bytes = padded_row as usize;
        let output_len = row_bytes
            .checked_mul(height as usize)
            .ok_or_else(|| "The GPU scene output size overflowed.".to_string())?;
        let mut rgba8 = Vec::with_capacity(output_len);
        {
            let mapped_range = readback.slice(..).get_mapped_range();
            for row in mapped_range
                .chunks_exact(padded_row_bytes)
                .take(height as usize)
            {
                rgba8.extend_from_slice(&row[..row_bytes]);
            }
        }
        readback.unmap();
        if rgba8.len() != output_len {
            return Err("The GPU scene readback returned an invalid RGBA8 length.".to_string());
        }
        Ok(SceneFrameCapture {
            width,
            height,
            rgba8,
        })
    }

    pub(super) fn execute_gpu_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        let Some(device) = self.wgpu_device.as_ref() else {
            return self.execute_cpu_scene_frame(frame);
        };
        let Some(queue) = self.wgpu_queue.as_ref() else {
            return self.execute_cpu_scene_frame(frame);
        };
        let Some(gpu_scene) = self.gpu_scene.as_mut() else {
            return self.execute_cpu_scene_frame(frame);
        };

        match gpu_scene.render(device, queue, frame) {
            Some((output, metrics)) => {
                self.last_frame_metrics = metrics;
                output
            }
            None => {
                tracing::warn!(
                    "ApiGraphicBasic GPU scene execution failed, falling back to CPU raster path."
                );
                self.execute_cpu_scene_frame(frame)
            }
        }
    }
}

impl GpuSceneState {
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &SceneRenderFrame,
    ) -> Option<(SceneFrameOutput, SceneFrameMetrics)> {
        let frame_start = Instant::now();
        self.frame_index = self.frame_index.wrapping_add(1).max(1);
        self.ledger.begin_frame(self.frame_index);
        self.frame_upload_rejected = 0;
        self.upload_pressure = 0.0;
        self.ui_mode = None;
        self.ui_premultiplied = false;
        if self.ui_surface_pipeline.is_none()
            && frame.commands.commands().iter().any(|command| {
                matches!(
                    command,
                    GraphicCommand::SetPipeline(
                        BasicPipelineKind::UiSurface | BasicPipelineKind::UiSurfaceOverlay
                    )
                )
            })
        {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("ApiGraphicBasic.UiSurfaceShader"),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(basic_scene_shader())),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ApiGraphicBasic.UiSurfaceLayout"),
                bind_group_layouts: &[
                    &self.mesh_bind_group_layout,
                    &self.scene_texture_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });
            self.ui_surface_pipeline = Some(create_scene_mesh_pipeline(
                device,
                &shader,
                &layout,
                self.color_format,
                false,
                true,
                Some(false),
            ));
            self.ui_overlay_pipeline = Some(create_scene_mesh_pipeline(
                device,
                &shader,
                &layout,
                self.color_format,
                false,
                true,
                Some(true),
            ));
        }
        self.external_texture_bindings.retain(|key, _| {
            frame
                .commands
                .gpu_textures()
                .any(|view| Arc::as_ptr(&view.view) as usize == *key)
        });
        let target_rebuilt = self.ensure_target(device, frame.width, frame.height);
        let timestamp_plan = self.gpu_timing.as_mut().and_then(|timing| {
            timing.collect_ready(device);
            timing.reserve_readback().map(|(slot, readback)| {
                (
                    slot,
                    Arc::clone(&timing.query_set),
                    Arc::clone(&timing.resolve_buffer),
                    readback,
                )
            })
        });
        let target = self.target.as_ref()?;
        let color_view = Arc::clone(&target.color_view);
        let depth_view = Arc::clone(&target.depth_view);
        let clear_color = extract_clear_color(frame.commands.commands());
        let texture_upload_bytes = self.prepare_scene_textures(device, queue, frame);
        let environment = crate::api_graphic_basic::environment_gpu::prepare(
            &mut self.environment,
            device,
            queue,
            self.color_format,
            frame.commands.environment,
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.SceneEncoder"),
        });
        let command_stats = frame.commands.stats();
        let mut metrics = SceneFrameMetrics {
            gpu_timing_supported: self.gpu_timing.is_some(),
            gpu_frame_ms: self
                .gpu_timing
                .as_ref()
                .map(|timing| timing.last_gpu_ms)
                .unwrap_or(0.0),
            gpu_timing_sampled: self
                .gpu_timing
                .as_ref()
                .map(|timing| timing.has_sample)
                .unwrap_or(false),
            command_count: command_stats.command_count,
            mesh_instances: command_stats.mesh_instances,
            line_instances: command_stats.line_instances,
            overlay_triangles: command_stats.overlay_triangles,
            target_rebuilds: u32::from(target_rebuilt),
            texture_upload_bytes,
            ..SceneFrameMetrics::default()
        };
        let mut mesh_draw_index = 0usize;
        metrics.uniform_upload_bytes += environment.as_ref().map(|(_, bytes)| *bytes).unwrap_or(0);
        metrics.environment_draw_calls = u32::from(environment.is_some());
        let mut line_draw_index = 0usize;
        let mut overlay_draw_index = 0usize;

        if let Some((_, query_set, _, _)) = timestamp_plan.as_ref() {
            encoder.write_timestamp(query_set.as_ref(), 0);
        }
        metrics.shadow_budget_exceeded = frame.commands.shadow_budget_exceeded;
        self.shadow_receiver = true;
        if let Some(shadow) = frame.commands.shadows.as_ref() {
            let already_admitted = self
                .shadows
                .as_ref()
                .is_some_and(|s| s.matches_geometry(shadow.geometry.key));
            let mut unique = std::collections::HashSet::new();
            let mesh_bytes: u64 = shadow
                .geometry
                .casters
                .iter()
                .filter(|c| !already_admitted && unique.insert(Arc::as_ptr(&c.mesh) as usize))
                .map(|c| {
                    (c.mesh.vertices.len() * std::mem::size_of::<GpuMeshVertex>()
                        + c.mesh.indices.len() * 4) as u64
                })
                .sum();
            let required = mesh_bytes
                + (shadow.geometry.resolution as u64).pow(2) * 4
                + shadow.geometry.casters.len().next_power_of_two() as u64
                    * std::mem::size_of::<GpuMeshInstance>() as u64
                + 112;
            if !already_admitted && required > self.shadow_memory_budget {
                self.shadows = None;
                metrics.shadow_budget_exceeded = true;
            } else {
                let start = Instant::now();
                if self
                    .shadows
                    .as_ref()
                    .is_none_or(|s| s.resolution != shadow.geometry.resolution)
                {
                    self.shadows = None;
                    self.shadows = Some(super::shadow_gpu::ShadowGpuState::new(
                        device,
                        shadow,
                        &self.mesh_bind_group_layout,
                        &self.scene_texture_bind_group_layout,
                        self.color_format,
                    ));
                }
                self.shadows.as_mut().unwrap().prepare(
                    device,
                    queue,
                    &mut encoder,
                    shadow,
                    &mut metrics,
                );
                metrics.shadow_prepare_cpu_ms = start.elapsed().as_secs_f32() * 1000.0;
            }
        } else {
            self.shadows = None;
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ApiGraphicBasic.ScenePass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view.as_ref(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view.as_ref(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some((environment, _)) = environment.as_ref() {
                pass.set_pipeline(&environment.pipeline);
                pass.set_bind_group(0, &environment.binding, &[]);
                pass.draw(0..3, 0..1);
            }
            let mut current_pipeline = BasicPipelineKind::FlatColor;
            // Borrow commands for the duration of the pass. This avoids
            // cloning every line batch's Vec on potato machines.
            for command in frame.commands.commands() {
                match command {
                    GraphicCommand::Clear { .. } => {}
                    GraphicCommand::SetPipeline(pipeline) => {
                        current_pipeline = *pipeline;
                        self.ui_mode = match pipeline {
                            BasicPipelineKind::UiSurface => Some(false),
                            BasicPipelineKind::UiSurfaceOverlay => Some(true),
                            _ => None,
                        };
                    }
                    GraphicCommand::SetShadowReceiver(value) => self.shadow_receiver = *value,
                    GraphicCommand::DrawMesh {
                        mesh_id,
                        transform,
                        color,
                        texture_id,
                    } => {
                        let Some(mesh) = frame.commands.mesh_arc(*mesh_id) else {
                            continue;
                        };
                        let cacheable = frame.commands.mesh_cacheable(*mesh_id);
                        self.ui_premultiplied = self.ui_mode.is_some()
                            && texture_id
                                .and_then(|id| frame.commands.gpu_texture(id))
                                .is_some();
                        let texture_bind_group = if let Some(view) =
                            texture_id.and_then(|id| frame.commands.gpu_texture(id))
                        {
                            self.external_texture_bind_group(device, view)
                        } else {
                            texture_id
                                .and_then(|id| frame.commands.texture(id))
                                .map(|texture| self.scene_texture_bind_group(texture))
                                .unwrap_or_else(|| Arc::clone(&self.fallback_texture_bind_group))
                        };
                        self.draw_mesh(
                            device,
                            queue,
                            &mut pass,
                            mesh,
                            *transform,
                            frame,
                            *color,
                            current_pipeline.is_lit(),
                            current_pipeline.is_two_sided(),
                            texture_bind_group.as_ref(),
                            texture_id.is_some(),
                            cacheable,
                            mesh_draw_index,
                            &mut metrics,
                        );
                        mesh_draw_index += 1;
                    }
                    GraphicCommand::DrawMeshBatch { mesh_id, instances } => {
                        let Some(mesh) = frame.commands.mesh_arc(*mesh_id) else {
                            continue;
                        };
                        let cacheable = frame.commands.mesh_cacheable(*mesh_id);
                        let fallback_texture_bind_group =
                            Arc::clone(&self.fallback_texture_bind_group);
                        self.draw_mesh_batch(
                            device,
                            queue,
                            &mut pass,
                            mesh,
                            instances,
                            frame,
                            current_pipeline.is_lit(),
                            current_pipeline.is_two_sided(),
                            fallback_texture_bind_group.as_ref(),
                            cacheable,
                            mesh_draw_index,
                            &mut metrics,
                        );
                        mesh_draw_index += 1;
                    }
                    GraphicCommand::DrawLine {
                        start,
                        end,
                        color,
                        width,
                        no_depth_test,
                        depth_bias,
                    } => {
                        self.draw_line_batch(
                            device,
                            queue,
                            &mut pass,
                            &[crate::api_graphic_basic::command_list::BasicLine {
                                start: *start,
                                end: *end,
                                color: *color,
                                width: *width,
                                depth_bias: *depth_bias,
                            }],
                            frame,
                            *no_depth_test,
                            line_draw_index,
                            &mut metrics,
                        );
                        line_draw_index += 1;
                    }
                    GraphicCommand::DrawLineBatch {
                        lines,
                        no_depth_test,
                    } => {
                        self.draw_line_batch(
                            device,
                            queue,
                            &mut pass,
                            lines,
                            frame,
                            *no_depth_test,
                            line_draw_index,
                            &mut metrics,
                        );
                        line_draw_index += 1;
                    }
                    GraphicCommand::DrawScreenTriangleBatch { triangles } => {
                        self.draw_overlay_triangle_batch(
                            device,
                            queue,
                            &mut pass,
                            triangles,
                            frame,
                            overlay_draw_index,
                            &mut metrics,
                        );
                        overlay_draw_index += 1;
                    }
                    GraphicCommand::DrawGrid { .. } => {}
                }
            }
        }

        let filter_bytes = frame.width as u64 * frame.height as u64 * 4;
        if frame.commands.optimized_improved && filter_bytes <= self.optimized_filter_memory_budget
        {
            if target_rebuilt
                || self
                    .optimized_filter
                    .as_ref()
                    .is_none_or(|f| f.size != [frame.width, frame.height])
            {
                self.optimized_filter = None;
                self.optimized_filter = Some(crate::api_graphic_basic::optimized_filter_gpu::OptimizedFilterGpu::new(
                    device,
                    color_view.as_ref(),
                    [frame.width, frame.height],
                    self.color_format,
                ));
            }
            self.optimized_filter.as_ref().unwrap().encode(&mut encoder);
            metrics.optimized_filter_draw_calls = 1;
            metrics.optimized_filter_resident_bytes = filter_bytes;
        } else {
            self.optimized_filter = None;
            metrics.optimized_filter_budget_exceeded = frame.commands.optimized_improved;
        }
        if let Some((_, query_set, resolve_buffer, readback)) = timestamp_plan.as_ref() {
            encoder.write_timestamp(query_set.as_ref(), 1);
            encoder.resolve_query_set(query_set.as_ref(), 0..2, resolve_buffer.as_ref(), 0);
            encoder.copy_buffer_to_buffer(
                resolve_buffer.as_ref(),
                0,
                readback.as_ref(),
                0,
                GPU_TIMESTAMP_QUERY_BYTES,
            );
        }
        queue.submit(std::iter::once(encoder.finish()));
        for handle in self.texture_handles.values().copied() {
            self.texture_registry.pin(handle, false);
        }
        if let Some((slot, _, _, _)) = timestamp_plan {
            if let Some(timing) = self.gpu_timing.as_ref() {
                timing.map_readback(slot);
            }
        }
        let residency = self.mesh_registry.metrics();
        let texture_residency = self.texture_registry.metrics();
        metrics.mesh_resident_bytes = residency.resident_bytes;
        metrics.mesh_resident_entries = residency.resident_entries;
        metrics.mesh_cache_evictions = residency.evictions;
        metrics.texture_resident_bytes = texture_residency.resident_bytes;
        metrics.texture_resident_entries = texture_residency.resident_entries;
        metrics.texture_cache_evictions = texture_residency.evictions;
        metrics.frame_upload_bytes = metrics.total_upload_bytes();
        metrics.upload_budget_exceeded = metrics.frame_upload_bytes > self.frame_upload_budget;
        self.upload_pressure =
            upload_pressure(metrics.frame_upload_bytes, self.frame_upload_budget);
        metrics.frame_upload_pressure = self.upload_pressure;
        self.sync_memory_ledger_metrics(&mut metrics);
        metrics.frame_cpu_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        Some((
            SceneFrameOutput::GpuTexture {
                view: GpuTextureView::from_wgpu(
                    self.optimized_filter
                        .as_ref()
                        .map(|f| Arc::clone(&f.view))
                        .unwrap_or(color_view),
                    TextureHandle::new(0, self.target_generation),
                ),
                width: frame.width,
                height: frame.height,
            },
            metrics,
        ))
    }

    fn bind_mesh_pipeline(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        instanced: bool,
        two_sided: bool,
    ) {
        if let Some(overlay) = self.ui_mode {
            pass.set_pipeline(if overlay {
                self.ui_overlay_pipeline
                    .as_ref()
                    .expect("prepared UI pipeline")
            } else {
                self.ui_surface_pipeline
                    .as_ref()
                    .expect("prepared UI pipeline")
            });
        } else if let Some(shadows) = self.shadows.as_ref() {
            pass.set_pipeline(
                &shadows.pipelines[usize::from(instanced) * 2 + usize::from(two_sided)],
            );
            pass.set_bind_group(2, &shadows.binding, &[]);
        } else {
            pass.set_pipeline(match (instanced, two_sided) {
                (false, false) => &self.mesh_pipeline,
                (false, true) => &self.mesh_two_sided_pipeline,
                (true, false) => &self.mesh_instanced_pipeline,
                (true, true) => &self.mesh_instanced_two_sided_pipeline,
            });
        }
    }

    fn draw_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        mesh: &Arc<BasicMesh>,
        transform: glam::Mat4,
        frame: &SceneRenderFrame,
        color: [u8; 4],
        lit: bool,
        two_sided: bool,
        texture_bind_group: &wgpu::BindGroup,
        texture_enabled: bool,
        cacheable: bool,
        draw_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) {
        if mesh.indices.is_empty() || mesh.vertices.is_empty() {
            return;
        }

        let mesh_key = Arc::as_ptr(mesh) as usize;
        let allow_cache = cacheable && self.mesh_cache_limit > 0;
        let cached_buffers = if allow_cache {
            let cached = self
                .mesh_cache
                .get(&mesh_key)
                .copied()
                .and_then(|handle| self.mesh_registry.get(handle, self.frame_index).cloned());
            if let Some(buffers) = cached {
                metrics.mesh_cache_hits += 1;
                buffers
            } else {
                self.mesh_cache.remove(&mesh_key);
                self.make_room_for_cached_mesh();

                metrics.mesh_cache_misses += 1;
                let buffers = create_gpu_mesh_buffers(device, mesh.as_ref());
                let bytes = buffers.vertex_bytes.saturating_add(buffers.index_bytes);
                metrics.mesh_upload_bytes += bytes;
                match self.mesh_registry.insert(
                    buffers.clone(),
                    ResourceAdmission::cached(bytes, self.frame_index),
                ) {
                    Ok(handle) => {
                        self.mesh_cache.insert(mesh_key, handle);
                        buffers
                    }
                    Err(_) => {
                        return self.draw_transient_mesh(
                            device,
                            queue,
                            pass,
                            mesh.as_ref(),
                            transform,
                            frame,
                            color,
                            lit,
                            two_sided,
                            texture_bind_group,
                            texture_enabled,
                            draw_index,
                            metrics,
                        );
                    }
                }
            }
        } else {
            metrics.mesh_cache_misses += 1;
            return self.draw_transient_mesh(
                device,
                queue,
                pass,
                mesh.as_ref(),
                transform,
                frame,
                color,
                lit,
                two_sided,
                texture_bind_group,
                texture_enabled,
                draw_index,
                metrics,
            );
        };

        let uniform_slot = self.ensure_mesh_uniform_slot(device, draw_index, metrics);
        let uniforms = MeshUniforms {
            mvp: (frame.view_proj * transform).to_cols_array_2d(),
            model: transform.to_cols_array_2d(),
            normal_matrix: gpu_normal_matrix(transform),
            color: rgba8_to_f32(color),
            light_dir: [
                frame.light_dir.x,
                frame.light_dir.y,
                frame.light_dir.z,
                frame
                    .commands
                    .environment
                    .map(|e| e.softness())
                    .unwrap_or(0.0),
            ],
            sun: frame
                .commands
                .environment
                .map(|env| env.sun.extend(env.settings.sun.shading_steps()).to_array())
                .unwrap_or([0.0; 4]),
            ambient: frame
                .commands
                .environment
                .map(|env| env.ambient.extend(env.exposure()).to_array())
                .unwrap_or([0.0; 4]),
            params: [
                if lit { 1.0 } else { 0.0 },
                if self.ui_premultiplied {
                    2.0
                } else if texture_enabled {
                    1.0
                } else {
                    0.0
                },
                f32::from(frame.commands.environment.is_some()),
                f32::from(self.shadows.is_some() && self.shadow_receiver),
            ],
        };
        write_uniform_if_changed(
            queue,
            &uniform_slot,
            &mut self.mesh_uniform_reuse,
            draw_index,
            uniforms,
            metrics,
        );
        metrics.mesh_draw_calls += 1;

        self.bind_mesh_pipeline(pass, false, two_sided);
        pass.set_bind_group(0, uniform_slot.bind_group.as_ref(), &[]);
        pass.set_bind_group(1, texture_bind_group, &[]);
        pass.set_vertex_buffer(0, cached_buffers.vertex_buffer.slice(..));
        pass.set_index_buffer(
            cached_buffers.index_buffer.slice(..),
            wgpu::IndexFormat::Uint32,
        );
        pass.draw_indexed(0..cached_buffers.index_count, 0, 0..1);
    }

    fn draw_mesh_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        mesh: &Arc<BasicMesh>,
        instances: &[BasicMeshInstance],
        frame: &SceneRenderFrame,
        lit: bool,
        two_sided: bool,
        texture_bind_group: &wgpu::BindGroup,
        cacheable: bool,
        batch_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) {
        if instances.len() < 2 {
            for instance in instances {
                self.draw_mesh(
                    device,
                    queue,
                    pass,
                    mesh,
                    instance.transform,
                    frame,
                    instance.color,
                    lit,
                    two_sided,
                    texture_bind_group,
                    false,
                    cacheable,
                    batch_index,
                    metrics,
                );
            }
            return;
        }
        if !cacheable || self.mesh_cache_limit == 0 {
            for instance in instances {
                self.draw_mesh(
                    device,
                    queue,
                    pass,
                    mesh,
                    instance.transform,
                    frame,
                    instance.color,
                    lit,
                    two_sided,
                    texture_bind_group,
                    false,
                    cacheable,
                    batch_index,
                    metrics,
                );
            }
            return;
        }

        let mesh_key = Arc::as_ptr(mesh) as usize;
        let cached_buffers = if let Some(handle) = self.mesh_cache.get(&mesh_key).copied() {
            if let Some(buffers) = self.mesh_registry.get(handle, self.frame_index).cloned() {
                metrics.mesh_cache_hits += 1;
                buffers
            } else {
                self.mesh_cache.remove(&mesh_key);
                self.make_room_for_cached_mesh();
                metrics.mesh_cache_misses += 1;
                let buffers = create_gpu_mesh_buffers(device, mesh.as_ref());
                let bytes = buffers.vertex_bytes.saturating_add(buffers.index_bytes);
                metrics.mesh_upload_bytes += bytes;
                let Ok(handle) = self.mesh_registry.insert(
                    buffers.clone(),
                    ResourceAdmission::cached(bytes, self.frame_index),
                ) else {
                    for instance in instances {
                        self.draw_mesh(
                            device,
                            queue,
                            pass,
                            mesh,
                            instance.transform,
                            frame,
                            instance.color,
                            lit,
                            two_sided,
                            texture_bind_group,
                            false,
                            cacheable,
                            batch_index,
                            metrics,
                        );
                    }
                    return;
                };
                self.mesh_cache.insert(mesh_key, handle);
                buffers
            }
        } else {
            self.make_room_for_cached_mesh();
            metrics.mesh_cache_misses += 1;
            let buffers = create_gpu_mesh_buffers(device, mesh.as_ref());
            let bytes = buffers.vertex_bytes.saturating_add(buffers.index_bytes);
            metrics.mesh_upload_bytes += bytes;
            let Ok(handle) = self.mesh_registry.insert(
                buffers.clone(),
                ResourceAdmission::cached(bytes, self.frame_index),
            ) else {
                for instance in instances {
                    self.draw_mesh(
                        device,
                        queue,
                        pass,
                        mesh,
                        instance.transform,
                        frame,
                        instance.color,
                        lit,
                        two_sided,
                        texture_bind_group,
                        false,
                        cacheable,
                        batch_index,
                        metrics,
                    );
                }
                return;
            };
            self.mesh_cache.insert(mesh_key, handle);
            buffers
        };

        self.mesh_instance_scratch.clear();
        self.mesh_instance_scratch
            .extend(instances.iter().copied().map(GpuMeshInstance::from_basic));
        let instance_slot = self.ensure_mesh_instance_slot(
            device,
            batch_index,
            self.mesh_instance_scratch.len(),
            metrics,
        );
        queue.write_buffer(
            &instance_slot.buffer,
            0,
            bytemuck::cast_slice(self.mesh_instance_scratch.as_slice()),
        );

        let uniform_slot = self.ensure_mesh_uniform_slot(device, batch_index, metrics);
        let uniforms = MeshUniforms {
            mvp: frame.view_proj.to_cols_array_2d(),
            model: glam::Mat4::IDENTITY.to_cols_array_2d(),
            normal_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
            color: [1.0; 4],
            light_dir: [
                frame.light_dir.x,
                frame.light_dir.y,
                frame.light_dir.z,
                frame
                    .commands
                    .environment
                    .map(|e| e.softness())
                    .unwrap_or(0.0),
            ],
            sun: frame
                .commands
                .environment
                .map(|env| env.sun.extend(env.settings.sun.shading_steps()).to_array())
                .unwrap_or([0.0; 4]),
            ambient: frame
                .commands
                .environment
                .map(|env| env.ambient.extend(env.exposure()).to_array())
                .unwrap_or([0.0; 4]),
            params: [
                if lit { 1.0 } else { 0.0 },
                0.0,
                f32::from(frame.commands.environment.is_some()),
                f32::from(self.shadows.is_some() && self.shadow_receiver),
            ],
        };
        write_uniform_if_changed(
            queue,
            &uniform_slot,
            &mut self.mesh_uniform_reuse,
            batch_index,
            uniforms,
            metrics,
        );
        metrics.mesh_instance_upload_bytes +=
            std::mem::size_of_val(self.mesh_instance_scratch.as_slice()) as u64;
        metrics.mesh_draw_calls += 1;

        self.bind_mesh_pipeline(pass, true, two_sided);
        pass.set_bind_group(0, uniform_slot.bind_group.as_ref(), &[]);
        pass.set_bind_group(1, texture_bind_group, &[]);
        pass.set_vertex_buffer(0, cached_buffers.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_slot.buffer.slice(..));
        pass.set_index_buffer(
            cached_buffers.index_buffer.slice(..),
            wgpu::IndexFormat::Uint32,
        );
        pass.draw_indexed(0..cached_buffers.index_count, 0, 0..instances.len() as u32);
    }

    fn draw_transient_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        mesh: &BasicMesh,
        transform: glam::Mat4,
        frame: &SceneRenderFrame,
        color: [u8; 4],
        lit: bool,
        two_sided: bool,
        texture_bind_group: &wgpu::BindGroup,
        texture_enabled: bool,
        draw_index: usize,
        metrics: &mut SceneFrameMetrics,
    ) {
        self.transient_vertex_scratch.clear();
        self.transient_vertex_scratch
            .extend(mesh.vertices.iter().map(|vertex| GpuMeshVertex {
                position: vertex.position.to_array(),
                normal: vertex.normal.to_array(),
                uv: vertex.uv,
            }));
        let vertex_bytes = std::mem::size_of_val(self.transient_vertex_scratch.as_slice()) as u64;
        let index_bytes = std::mem::size_of_val(mesh.indices.as_slice()) as u64;
        let transient =
            self.ensure_transient_mesh_slot(device, draw_index, vertex_bytes, index_bytes, metrics);
        queue.write_buffer(
            &transient.vertex_buffer,
            0,
            bytemuck::cast_slice(self.transient_vertex_scratch.as_slice()),
        );
        queue.write_buffer(
            &transient.index_buffer,
            0,
            bytemuck::cast_slice(mesh.indices.as_slice()),
        );
        let uniform_slot = self.ensure_mesh_uniform_slot(device, draw_index, metrics);
        let uniforms = MeshUniforms {
            mvp: (frame.view_proj * transform).to_cols_array_2d(),
            model: transform.to_cols_array_2d(),
            normal_matrix: gpu_normal_matrix(transform),
            color: rgba8_to_f32(color),
            light_dir: [
                frame.light_dir.x,
                frame.light_dir.y,
                frame.light_dir.z,
                frame
                    .commands
                    .environment
                    .map(|e| e.softness())
                    .unwrap_or(0.0),
            ],
            sun: frame
                .commands
                .environment
                .map(|env| env.sun.extend(env.settings.sun.shading_steps()).to_array())
                .unwrap_or([0.0; 4]),
            ambient: frame
                .commands
                .environment
                .map(|env| env.ambient.extend(env.exposure()).to_array())
                .unwrap_or([0.0; 4]),
            params: [
                if lit { 1.0 } else { 0.0 },
                if self.ui_premultiplied {
                    2.0
                } else if texture_enabled {
                    1.0
                } else {
                    0.0
                },
                f32::from(frame.commands.environment.is_some()),
                f32::from(self.shadows.is_some() && self.shadow_receiver),
            ],
        };
        write_uniform_if_changed(
            queue,
            &uniform_slot,
            &mut self.mesh_uniform_reuse,
            draw_index,
            uniforms,
            metrics,
        );
        metrics.mesh_upload_bytes += vertex_bytes + index_bytes;
        metrics.mesh_draw_calls += 1;

        self.bind_mesh_pipeline(pass, false, two_sided);
        pass.set_bind_group(0, uniform_slot.bind_group.as_ref(), &[]);
        pass.set_bind_group(1, texture_bind_group, &[]);
        pass.set_vertex_buffer(0, transient.vertex_buffer.slice(..vertex_bytes));
        pass.set_index_buffer(
            transient.index_buffer.slice(..index_bytes),
            wgpu::IndexFormat::Uint32,
        );
        pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
    }
}

/// Frame upload consumption as a ratio of the hard upload budget.
///
/// A zero budget reports `1.0` as soon as anything uploads so callers do not
/// have to special-case an infinite ratio.
fn upload_pressure(used_bytes: u64, budget_bytes: u64) -> f32 {
    if budget_bytes == 0 {
        return if used_bytes == 0 { 0.0 } else { 1.0 };
    }
    used_bytes as f32 / budget_bytes as f32
}

fn gpu_normal_matrix(model: glam::Mat4) -> [[f32; 4]; 4] {
    crate::math::transform::normal_matrix(&model).to_cols_array_2d()
}

pub(super) fn extract_clear_color(commands: &[GraphicCommand]) -> wgpu::Color {
    let mut color = wgpu::Color::BLACK;
    for command in commands {
        if let GraphicCommand::Clear { r, g, b, a } = command {
            color = wgpu::Color {
                r: f64::from(crate::post_process::srgb_to_linear(*r as f32 / 255.0)),
                g: f64::from(crate::post_process::srgb_to_linear(*g as f32 / 255.0)),
                b: f64::from(crate::post_process::srgb_to_linear(*b as f32 / 255.0)),
                a: *a as f64 / 255.0,
            };
        }
    }
    color
}
