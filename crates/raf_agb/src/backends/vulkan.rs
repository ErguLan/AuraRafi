mod context;
mod filter;
mod memory;
mod patch;
mod pipeline;
mod shadow;
mod target;

use std::rc::Rc;

use ash::vk;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::{
    product::Layer, AdapterInfo, Backend, Batch, Error, Frame, FrameMetrics, FramePlan, Instance,
    Output, RasterUniforms, ResourcePatch, Result, Vertex, Workload,
};

use context::{Gpu, Surface};
use memory::{Buffer, Image};
use pipeline::{Descriptors, Pipelines};
use target::Target;

const CAMERA_BYTES: usize = std::mem::size_of::<RasterUniforms>();

pub struct Vulkan {
    filter: Option<filter::FilterPass>,
    target: Option<Target>,
    shadow: Option<shadow::ShadowPass>,
    pipelines: Option<Pipelines>,
    surface: Option<Surface>,
    camera: Buffer,
    last_camera: Option<RasterUniforms>,
    submit: Submission,
    gpu: Rc<Gpu>,
    info: AdapterInfo,
    completed_gpu_ms: Option<f64>,
    has_timestamp_sample: bool,
}

pub struct Resident {
    descriptors: Descriptors,
    textures: Vec<Image>,
    geometry: Buffer,
    instance_offset: u64,
    index_offset: Option<u64>,
    batches: Vec<Batch>,
    shadow_batches: Vec<Batch>,
}

impl Vulkan {
    pub fn headless() -> Result<Self> {
        Self::open(None)
    }

    /// # Safety
    /// The handles must remain valid until this executor and its surface drop.
    pub unsafe fn window(display: RawDisplayHandle, window: RawWindowHandle) -> Result<Self> {
        Self::open(Some((display, window)))
    }

    fn open(window: Option<(RawDisplayHandle, RawWindowHandle)>) -> Result<Self> {
        let (gpu, surface, info) = context::open(window)?;
        let camera = Buffer::host(
            &gpu,
            CAMERA_BYTES as u64,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        )?;
        let submit = Submission::new(&gpu, surface.is_some())?;
        Ok(Self {
            gpu,
            info,
            surface,
            camera,
            submit,
            pipelines: None,
            target: None,
            filter: None,
            shadow: None,
            last_camera: None,
            completed_gpu_ms: None,
            has_timestamp_sample: false,
        })
    }

    fn begin(&mut self) -> Result<vk::CommandBuffer> {
        self.wait_render()?;
        self.submit.begin()
    }

    fn wait_render(&mut self) -> Result<()> {
        self.complete_render(u64::MAX)
    }

    fn complete_render(&mut self, timeout: u64) -> Result<()> {
        self.submit.wait(timeout)?;
        if self.has_timestamp_sample {
            let mut ticks = [0u64; 2];
            unsafe {
                self.gpu.device.get_query_pool_results(
                    self.submit.timestamps,
                    0,
                    &mut ticks,
                    vk::QueryResultFlags::TYPE_64,
                )
            }
            .map_err(vk_error)?;
            let mask = u64::MAX >> (64 - self.gpu.timestamp_bits);
            let elapsed = ticks[1].wrapping_sub(ticks[0]) & mask;
            self.completed_gpu_ms =
                Some(elapsed as f64 * self.gpu.timestamp_period as f64 / 1_000_000.0);
            self.has_timestamp_sample = false;
        }
        Ok(())
    }
}

impl Backend for Vulkan {
    type Resident = Resident;

    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn supports_shadows(&self) -> bool {
        self.info.max_texture >= crate::device::SHADOW_MAP_SIDE
            && self.gpu.supports_sampled_depth(pipeline::DEPTH_FORMAT)
    }

    fn shadow_bytes(&self) -> u64 {
        self.shadow
            .as_ref()
            .map_or(0, |shadow| crate::device::shadow_map_bytes(shadow.side))
    }

    fn supports_scene_filter(&self) -> bool {
        self.pipelines
            .as_ref()
            .is_some_and(|raster| self.gpu.supports_sampled_color(raster.format))
    }

    fn filter_bytes(&self) -> u64 {
        self.filter.as_ref().map_or(0, filter::FilterPass::bytes)
    }

    fn configure(&mut self, plan: &FramePlan) -> Result<()> {
        if self.surface.is_some() != (plan.output == Output::Present) {
            return Err(Error::Invalid(
                "target output does not match Vulkan surface",
            ));
        }
        self.wait()?;
        let format = match &self.surface {
            Some(surface) => surface.choose_format(self.gpu.physical)?.format,
            None => vk::Format::R8G8B8A8_UNORM,
        };
        if self
            .pipelines
            .as_ref()
            .is_some_and(|pipeline| pipeline.format != format)
        {
            return Err(Error::Backend(
                "Vulkan surface color format changed, recreate the device".into(),
            ));
        }
        if self.pipelines.is_none() {
            self.pipelines = Some(Pipelines::new(&self.gpu, format)?);
        }
        let pipelines = self.pipelines.as_ref().unwrap();
        self.filter = None;
        self.target = None;
        self.target = Some(Target::new(
            &self.gpu,
            self.surface.as_ref(),
            pipelines,
            plan.extent,
        )?);
        Ok(())
    }

    fn upload(&mut self, workload: &Workload) -> Result<Resident> {
        let layouts = self
            .pipelines
            .as_ref()
            .ok_or(Error::Invalid("configure a target before uploading"))?
            .bindings
            .clone();
        let bytes = workload.bytes();
        let vertex = bytemuck::cast_slice(&workload.vertices);
        let instances = bytemuck::cast_slice(&workload.instances);
        let indices: &[u8] = bytemuck::cast_slice(&workload.indices);
        let instance_offset = vertex.len() as u64;
        let index_offset = instance_offset + instances.len() as u64;
        let geometry_size = index_offset + indices.len() as u64;
        let mut staging = Buffer::host(&self.gpu, bytes, vk::BufferUsageFlags::TRANSFER_SRC)?;
        staging.write(0, vertex);
        staging.write(instance_offset, instances);
        if !indices.is_empty() {
            staging.write(index_offset, indices);
        }
        let geometry = Buffer::local(
            &self.gpu,
            geometry_size,
            vk::BufferUsageFlags::VERTEX_BUFFER
                | vk::BufferUsageFlags::INDEX_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST,
        )?;
        let mut textures = Vec::with_capacity(workload.textures.len());
        let mut offset = geometry_size;
        for texture in &workload.textures {
            staging.write(offset, &texture.rgba);
            offset += texture.rgba.len() as u64;
            textures.push(Image::new(
                &self.gpu,
                texture.size,
                vk::Format::R8G8B8A8_UNORM,
                vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                vk::ImageAspectFlags::COLOR,
            )?);
        }
        let descriptors = Descriptors::new(&self.gpu, layouts, &self.camera, &textures)?;
        let cmd = self.begin()?;
        let device = &self.gpu.device;
        // Upload immutable data once in one submit. The transfer barrier makes
        // device-local vertices visible to the next frame on the same queue.
        unsafe {
            device.cmd_copy_buffer(
                cmd,
                staging.raw,
                geometry.raw,
                &[vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: geometry_size,
                }],
            );
            let buffer_barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(
                    vk::AccessFlags::VERTEX_ATTRIBUTE_READ | vk::AccessFlags::INDEX_READ,
                )
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(geometry.raw)
                .size(vk::WHOLE_SIZE);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::VERTEX_INPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[buffer_barrier],
                &[],
            );
            let mut offset = geometry_size;
            for (texture, source) in textures.iter().zip(&workload.textures) {
                memory::upload_image(device, cmd, staging.raw, offset, texture.raw, source.size);
                offset += source.rgba.len() as u64;
            }
        }
        self.submit.submit(None)?;
        self.wait_render()?;
        Ok(Resident {
            descriptors,
            textures,
            geometry,
            instance_offset,
            index_offset: (!indices.is_empty()).then_some(index_offset),
            batches: workload.batches.clone(),
            shadow_batches: workload.shadow_batches.clone(),
        })
    }

    unsafe fn patch(&mut self, resident: &mut Resident, patch: ResourcePatch<'_>) -> Result<()> {
        patch::update(self, resident, patch)
    }

    fn supports_layers(&self) -> bool {
        // Layered composition is a windowed editor path. Headless capture keeps
        // the single combined packet so its output stays byte-for-byte on the
        // previous path.
        self.surface.is_some()
    }

    fn draw(
        &mut self,
        resident: &Resident,
        plan: &FramePlan,
        frame: &Frame,
    ) -> Result<FrameMetrics> {
        self.draw_layers(
            &[Layer {
                resident,
                scene: true,
            }],
            plan,
            frame,
        )
    }

    fn draw_layers(
        &mut self,
        layers: &[Layer<'_, Resident>],
        plan: &FramePlan,
        frame: &Frame,
    ) -> Result<FrameMetrics> {
        if layers.is_empty() {
            return Err(Error::Invalid("layered draw needs at least one layer"));
        }
        self.complete_render(if plan.output == Output::Present {
            0
        } else {
            u64::MAX
        })?;
        let target = self.target.as_ref().ok_or(Error::SurfaceOutdated)?;
        let pipelines = self.pipelines.as_ref().ok_or(Error::SurfaceOutdated)?;
        // Only a layer with real scene batches owns the depth pass, the shadow
        // casters, and the spatial filter. A UI-only frame never clears depth.
        let scene_resident = layers
            .iter()
            .find(|layer| layer.scene)
            .map(|layer| layer.resident);
        let scene_filter = frame.scene_filter && scene_resident.is_some();
        if scene_filter {
            if self.filter.is_none() {
                self.filter = Some(filter::FilterPass::new(
                    &self.gpu,
                    pipelines,
                    plan.extent,
                    target.depth_view(),
                )?);
            }
        } else {
            self.filter = None;
        }
        let (side, depth_key) = frame
            .shadow_map
            .map_or((crate::device::SHADOW_MAP_SIDE, None), |(side, key)| {
                (side, Some(key))
            });
        let shadow_upload = if let (Some(parameters), Some(_)) = (frame.shadow, scene_resident) {
            if self
                .shadow
                .as_ref()
                .is_none_or(|shadow| shadow.side != side)
            {
                self.shadow = None;
                self.shadow = Some(shadow::ShadowPass::new(&self.gpu, pipelines, side)?);
            }
            self.shadow.as_mut().unwrap().prepare(parameters)
        } else {
            self.shadow = None;
            0
        };
        let (index, signal) = target.acquire(self.submit.available)?;
        let cmd = self.submit.begin()?;
        let gpu = &self.gpu.device;
        let uniforms = frame.uniforms(plan.extent);
        let changed = self.last_camera != Some(uniforms);
        if changed {
            self.camera.write(0, bytemuck::bytes_of(&uniforms));
            self.last_camera = Some(uniforms);
        }
        let mut shadow_draws = 0;
        unsafe {
            if self.submit.timestamps != vk::QueryPool::null() {
                gpu.cmd_reset_query_pool(cmd, self.submit.timestamps, 0, 2);
                gpu.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    self.submit.timestamps,
                    0,
                );
            }
            if let (Some(shadow), Some(resident)) = (&self.shadow, scene_resident) {
                if depth_key.is_none() || shadow.depth_key != depth_key {
                    shadow_draws = shadow.record(cmd, resident, depth_key.is_some());
                }
            }
            if let Some(filter) = &self.filter {
                // The filtered scene resolves into its own target first. Only
                // the scene layers participate; UI overlays stay sharp.
                pipelines.draw_layers(
                    gpu,
                    cmd,
                    filter.input,
                    target.extent,
                    frame,
                    layers,
                    &[0],
                    self.shadow.as_ref(),
                    None,
                );
                memory::transition(
                    gpu,
                    cmd,
                    filter.source.raw,
                    memory::LayoutTransition::FilterSample,
                );
            }
            pipelines.draw_layers(
                gpu,
                cmd,
                target.framebuffers[index],
                target.extent,
                frame,
                layers,
                if self.filter.is_some() {
                    // Scene content already resolved through the filter; the
                    // target pass records only overlay groups.
                    &[1]
                } else {
                    &plan.graph.order
                },
                if self.filter.is_some() {
                    None
                } else {
                    self.shadow.as_ref()
                },
                self.filter.as_ref(),
            );
            if plan.output == Output::Present {
                memory::transition(
                    gpu,
                    cmd,
                    target.image(index),
                    memory::LayoutTransition::Present,
                );
            }
            if self.submit.timestamps != vk::QueryPool::null() {
                gpu.cmd_write_timestamp(
                    cmd,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    self.submit.timestamps,
                    1,
                );
            }
        }
        self.submit.submit(signal)?;
        if let Some(shadow) = self.shadow.as_mut() {
            shadow.depth_key = depth_key;
        }
        self.has_timestamp_sample = self.submit.timestamps != vk::QueryPool::null();
        target.present(index, signal)?;
        let draws = layers.iter().fold(0u32, |draws, layer| {
            draws.saturating_add(layer.resident.batches.len().min(u32::MAX as usize) as u32)
        });
        Ok(FrameMetrics {
            draw_calls: draws
                .saturating_add(shadow_draws)
                .saturating_add(u32::from(self.filter.is_some())),
            submits: 1,
            upload_bytes: (if changed { CAMERA_BYTES as u64 } else { 0 }) + shadow_upload,
            gpu_ms: self.completed_gpu_ms,
            shadow_draw_calls: shadow_draws,
            ..Default::default()
        })
    }

    fn wait(&mut self) -> Result<()> {
        self.wait_render()?;
        if self.surface.is_some() {
            unsafe { self.gpu.device.queue_wait_idle(self.gpu.queue) }.map_err(vk_error)?;
        }
        Ok(())
    }

    fn capture(&mut self) -> Result<Vec<u8>> {
        let target = self.target.as_ref().ok_or(Error::SurfaceOutdated)?;
        let (image, size) = (target.offscreen()?.raw, target.extent);
        let bytes = u64::from(size.width) * u64::from(size.height) * 4;
        let buffer = Buffer::host(&self.gpu, bytes, vk::BufferUsageFlags::TRANSFER_DST)?;
        let cmd = self.begin()?;
        unsafe {
            memory::transition(
                &self.gpu.device,
                cmd,
                image,
                memory::LayoutTransition::Readback,
            );
            self.gpu.device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer.raw,
                &[memory::copy_region(0, [size.width, size.height])],
            );
            let host_access = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(buffer.raw)
                .size(vk::WHOLE_SIZE);
            self.gpu.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                &[host_access],
                &[],
            );
            memory::transition(
                &self.gpu.device,
                cmd,
                image,
                memory::LayoutTransition::RestoreColor,
            );
        }
        self.submit.submit(None)?;
        self.wait_render()?;
        Ok(buffer.read(bytes as usize))
    }
}

impl Drop for Vulkan {
    fn drop(&mut self) {
        // Present may outlive the graphics fence. Retire it before field drops.
        let _ = unsafe { self.gpu.device.device_wait_idle() };
    }
}

struct Submission {
    gpu: Rc<Gpu>,
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    available: vk::Semaphore,
    timestamps: vk::QueryPool,
    pending: bool,
}

impl Submission {
    fn new(gpu: &Rc<Gpu>, window: bool) -> Result<Self> {
        let mut submission = Self {
            gpu: Rc::clone(gpu),
            pool: vk::CommandPool::null(),
            cmd: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            available: vk::Semaphore::null(),
            timestamps: vk::QueryPool::null(),
            pending: false,
        };
        let device = &gpu.device;
        unsafe {
            submission.pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(gpu.family)
                        .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                    None,
                )
                .map_err(vk_error)?;
            submission.cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(submission.pool)
                        .command_buffer_count(1),
                )
                .map_err(vk_error)?[0];
            submission.fence = device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .map_err(vk_error)?;
            if window {
                submission.available = device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .map_err(vk_error)?;
            }
            if gpu.timestamp_bits > 0 {
                submission.timestamps = device
                    .create_query_pool(
                        &vk::QueryPoolCreateInfo::default()
                            .query_count(2)
                            .query_type(vk::QueryType::TIMESTAMP),
                        None,
                    )
                    .map_err(vk_error)?;
            }
        }
        Ok(submission)
    }

    fn wait(&mut self, timeout: u64) -> Result<()> {
        if self.pending {
            unsafe {
                self.gpu
                    .device
                    .wait_for_fences(&[self.fence], true, timeout)
            }
            .map_err(vk_error)?;
            self.pending = false;
        }
        Ok(())
    }

    fn begin(&self) -> Result<vk::CommandBuffer> {
        unsafe {
            self.gpu
                .device
                .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())
                .map_err(vk_error)?;
            self.gpu
                .device
                .begin_command_buffer(
                    self.cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(vk_error)?;
        }
        Ok(self.cmd)
    }

    fn submit(&mut self, signal: Option<vk::Semaphore>) -> Result<()> {
        let commands = [self.cmd];
        let available = [self.available];
        let stage = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let complete = [signal.unwrap_or_default()];
        let mut info = vk::SubmitInfo::default().command_buffers(&commands);
        if signal.is_some() {
            info = info
                .wait_semaphores(&available)
                .wait_dst_stage_mask(&stage)
                .signal_semaphores(&complete);
        }
        unsafe {
            self.gpu
                .device
                .end_command_buffer(self.cmd)
                .map_err(vk_error)?;
            self.gpu
                .device
                .reset_fences(&[self.fence])
                .map_err(vk_error)?;
            self.gpu
                .device
                .queue_submit(self.gpu.queue, &[info], self.fence)
                .map_err(vk_error)?;
        }
        self.pending = true;
        Ok(())
    }
}

impl Drop for Submission {
    fn drop(&mut self) {
        let gpu = &self.gpu.device;
        unsafe {
            if self.timestamps != vk::QueryPool::null() {
                gpu.destroy_query_pool(self.timestamps, None);
            }
            if self.available != vk::Semaphore::null() {
                gpu.destroy_semaphore(self.available, None);
            }
            if self.fence != vk::Fence::null() {
                gpu.destroy_fence(self.fence, None);
            }
            if self.pool != vk::CommandPool::null() {
                gpu.destroy_command_pool(self.pool, None);
            }
        }
    }
}

fn vk_error(code: vk::Result) -> Error {
    match code {
        vk::Result::ERROR_DEVICE_LOST => Error::DeviceLost,
        vk::Result::ERROR_OUT_OF_DATE_KHR => Error::SurfaceOutdated,
        vk::Result::ERROR_SURFACE_LOST_KHR => Error::SurfaceLost,
        vk::Result::TIMEOUT | vk::Result::NOT_READY => Error::Busy,
        code => Error::Backend(format!("Vulkan error ({})", code.as_raw())),
    }
}
