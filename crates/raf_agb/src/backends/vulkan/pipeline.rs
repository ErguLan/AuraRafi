use std::rc::Rc;

use ash::vk;

use crate::device::ViewportRect;
use crate::product::Layer;
use crate::{DrawMode, Error, Frame, Instance, Result, Vertex};

use super::{
    context::Gpu,
    memory::{Buffer, Image},
    vk_error, Resident,
};

pub(super) const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

pub(super) struct Bindings {
    gpu: Rc<Gpu>,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    sampler: vk::Sampler,
}

impl Bindings {
    fn new(gpu: &Rc<Gpu>) -> Result<Rc<Self>> {
        let mut objects = Self {
            gpu: Rc::clone(gpu),
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            sampler: vk::Sampler::null(),
        };
        let device = &gpu.device;
        let entries = [
            binding(
                0,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                1,
                vk::DescriptorType::SAMPLED_IMAGE,
                vk::ShaderStageFlags::FRAGMENT,
            ),
            binding(
                2,
                vk::DescriptorType::SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            ),
        ];
        unsafe {
            objects.set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&entries),
                    None,
                )
                .map_err(vk_error)?;
            objects.layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&[objects.set_layout]),
                    None,
                )
                .map_err(vk_error)?;
            objects.sampler = device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .min_filter(vk::Filter::LINEAR)
                        .mag_filter(vk::Filter::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::REPEAT)
                        .address_mode_v(vk::SamplerAddressMode::REPEAT),
                    None,
                )
                .map_err(vk_error)?;
        }
        Ok(Rc::new(objects))
    }
}

fn binding(
    index: u32,
    ty: vk::DescriptorType,
    visibility: vk::ShaderStageFlags,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(index)
        .descriptor_type(ty)
        .descriptor_count(1)
        .stage_flags(visibility)
}

impl Drop for Bindings {
    fn drop(&mut self) {
        let device = &self.gpu.device;
        unsafe {
            if self.sampler != vk::Sampler::null() {
                device.destroy_sampler(self.sampler, None);
            }
            if self.layout != vk::PipelineLayout::null() {
                device.destroy_pipeline_layout(self.layout, None);
            }
            if self.set_layout != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(self.set_layout, None);
            }
        }
    }
}

pub(super) struct Descriptors {
    gpu: Rc<Gpu>,
    _bindings: Rc<Bindings>,
    pool: vk::DescriptorPool,
    pub(super) sets: Vec<vk::DescriptorSet>,
}

impl Descriptors {
    pub fn new(
        gpu: &Rc<Gpu>,
        bindings: Rc<Bindings>,
        camera: &Buffer,
        textures: &[Image],
    ) -> Result<Self> {
        let device = &gpu.device;
        let count = textures.len() as u32;
        let sizes = [
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::DescriptorType::SAMPLED_IMAGE,
            vk::DescriptorType::SAMPLER,
        ]
        .map(|ty| vk::DescriptorPoolSize {
            ty,
            descriptor_count: count,
        });
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(count)
                    .pool_sizes(&sizes),
                None,
            )
        }
        .map_err(vk_error)?;
        let mut descriptors = Self {
            gpu: Rc::clone(gpu),
            pool,
            _bindings: bindings,
            sets: Vec::new(),
        };
        let layouts = vec![descriptors._bindings.set_layout; textures.len()];
        descriptors.sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
        }
        .map_err(vk_error)?;
        let camera_info = [vk::DescriptorBufferInfo::default()
            .buffer(camera.raw)
            .range(super::CAMERA_BYTES as u64)];
        let sampler_info =
            [vk::DescriptorImageInfo::default().sampler(descriptors._bindings.sampler)];
        for (&set, image) in descriptors.sets.iter().zip(textures) {
            let image_info = [vk::DescriptorImageInfo::default()
                .image_view(image.view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let writes = [
                write(set, 0, vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&camera_info),
                write(set, 1, vk::DescriptorType::SAMPLED_IMAGE).image_info(&image_info),
                write(set, 2, vk::DescriptorType::SAMPLER).image_info(&sampler_info),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
        Ok(descriptors)
    }
}

fn write(
    set: vk::DescriptorSet,
    binding: u32,
    ty: vk::DescriptorType,
) -> vk::WriteDescriptorSet<'static> {
    vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(binding)
        .descriptor_type(ty)
}

impl Drop for Descriptors {
    fn drop(&mut self) {
        unsafe { self.gpu.device.destroy_descriptor_pool(self.pool, None) };
    }
}

pub(super) struct Pipelines {
    gpu: Rc<Gpu>,
    pub format: vk::Format,
    pub render_pass: vk::RenderPass,
    pub bindings: Rc<Bindings>,
    scene: vk::Pipeline,
    scene_two_sided: vk::Pipeline,
    scene_transparent: vk::Pipeline,
    scene_transparent_two_sided: vk::Pipeline,
    read_depth: vk::Pipeline,
    overlay: vk::Pipeline,
    cache: vk::PipelineCache,
}

impl Pipelines {
    pub fn new(gpu: &Rc<Gpu>, format: vk::Format) -> Result<Self> {
        let bindings = Bindings::new(gpu)?;
        let render_pass = create_render_pass(&gpu.device, format)?;
        let mut pipelines = Self {
            gpu: Rc::clone(gpu),
            render_pass,
            bindings,
            format,
            scene: vk::Pipeline::null(),
            scene_two_sided: vk::Pipeline::null(),
            scene_transparent: vk::Pipeline::null(),
            scene_transparent_two_sided: vk::Pipeline::null(),
            read_depth: vk::Pipeline::null(),
            overlay: vk::Pipeline::null(),
            cache: vk::PipelineCache::null(),
        };
        pipelines.cache = unsafe {
            gpu.device
                .create_pipeline_cache(&vk::PipelineCacheCreateInfo::default(), None)
        }
        .map_err(vk_error)?;
        let (module, info) = super::super::shaders::source()?;
        let pixel = Shader::new(gpu, &module, &info, "paint", naga::ShaderStage::Fragment)?;
        let scene = Shader::new(gpu, &module, &info, "scene", naga::ShaderStage::Vertex)?;
        let overlay = Shader::new(gpu, &module, &info, "overlay", naga::ShaderStage::Vertex)?;
        pipelines.scene = pipelines.create(
            &scene,
            c"scene",
            &pixel,
            true,
            true,
            vk::CullModeFlags::BACK,
        )?;
        pipelines.scene_two_sided = pipelines.create(
            &scene,
            c"scene",
            &pixel,
            true,
            true,
            vk::CullModeFlags::NONE,
        )?;
        pipelines.scene_transparent = pipelines.create(
            &scene,
            c"scene",
            &pixel,
            true,
            false,
            vk::CullModeFlags::BACK,
        )?;
        pipelines.scene_transparent_two_sided = pipelines.create(
            &scene,
            c"scene",
            &pixel,
            true,
            false,
            vk::CullModeFlags::NONE,
        )?;
        pipelines.read_depth = pipelines.create(
            &scene,
            c"scene",
            &pixel,
            true,
            false,
            vk::CullModeFlags::NONE,
        )?;
        pipelines.overlay = pipelines.create(
            &overlay,
            c"overlay",
            &pixel,
            false,
            false,
            vk::CullModeFlags::NONE,
        )?;
        Ok(pipelines)
    }

    fn create(
        &self,
        vertex: &Shader,
        entry: &std::ffi::CStr,
        fragment: &Shader,
        depth: bool,
        depth_write: bool,
        cull: vk::CullModeFlags,
    ) -> Result<vk::Pipeline> {
        self.create_for(
            vertex,
            entry,
            Some((fragment, c"paint")),
            RasterTarget {
                pass: self.render_pass,
                layout: self.bindings.layout,
                color: true,
                depth_test: depth,
                depth_write,
                vertex_stream: true,
                blend: true,
                cull,
            },
        )
    }

    pub(super) fn material_layout(&self) -> vk::DescriptorSetLayout {
        self.bindings.set_layout
    }

    pub(super) fn create_for(
        &self,
        vertex: &Shader,
        entry: &std::ffi::CStr,
        fragment: Option<(&Shader, &std::ffi::CStr)>,
        target: RasterTarget,
    ) -> Result<vk::Pipeline> {
        let mut stages = Vec::with_capacity(2);
        stages.push(
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex.raw)
                .name(entry),
        );
        if let Some((fragment, name)) = fragment {
            stages.push(
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment.raw)
                    .name(name),
            );
        }
        let bindings = [
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: size_of::<Vertex>() as u32,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: size_of::<Instance>() as u32,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ];
        let formats = [
            vk::Format::R32G32B32_SFLOAT,
            vk::Format::R32G32_SFLOAT,
            vk::Format::R32G32B32A32_SFLOAT,
        ];
        let mut attributes = [vk::VertexInputAttributeDescription::default(); 10];
        for (index, (&format, offset)) in formats.iter().zip([0, 12, 20]).enumerate() {
            attributes[index] = vk::VertexInputAttributeDescription {
                location: index as u32,
                binding: 0,
                format,
                offset,
            };
        }
        for (index, attribute) in attributes[3..8].iter_mut().enumerate() {
            *attribute = vk::VertexInputAttributeDescription {
                location: index as u32 + 3,
                binding: 1,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: index as u32 * 16,
            };
        }
        attributes[8] = vk::VertexInputAttributeDescription {
            location: 8,
            binding: 0,
            format: vk::Format::R32G32B32_SFLOAT,
            offset: std::mem::offset_of!(Vertex, normal) as u32,
        };
        attributes[9] = vk::VertexInputAttributeDescription {
            location: 9,
            binding: 1,
            format: vk::Format::R32_UINT,
            offset: std::mem::offset_of!(Instance, raster_kind) as u32,
        };
        let mut vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        if target.vertex_stream {
            vertex_input = vertex_input
                .vertex_binding_descriptions(&bindings)
                .vertex_attribute_descriptions(&attributes);
        }
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(target.cull)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.);
        let samples = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(target.depth_test)
            .depth_write_enable(target.depth_write)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
        let attachment = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(target.blend)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(if target.color {
            &attachment
        } else {
            &[]
        });
        let dynamic = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&assembly)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&samples)
            .depth_stencil_state(&depth_state)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(target.layout)
            .render_pass(target.pass);
        let device = &self.gpu.device;
        match unsafe { device.create_graphics_pipelines(self.cache, &[info], None) } {
            Ok(objects) => Ok(objects[0]),
            Err((objects, code)) => {
                for pipeline in objects.into_iter().filter(|p| *p != vk::Pipeline::null()) {
                    unsafe { device.destroy_pipeline(pipeline, None) };
                }
                Err(vk_error(code))
            }
        }
    }

    /// Records ordered residents in one render pass.
    ///
    /// One clear is taken from the frame before the first layer. Scene layers
    /// contribute their scene group on pass 0 and overlays on pass 1; overlay
    /// layers only contribute their overlay group. Each resident binds its own
    /// geometry and descriptors before its batches.
    pub unsafe fn draw_layers(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        framebuffer: vk::Framebuffer,
        extent: vk::Extent2D,
        frame: &Frame,
        layers: &[Layer<'_, Resident>],
        passes: &[u32],
        shadow: Option<&super::shadow::ShadowPass>,
        filter: Option<&super::filter::FilterPass>,
    ) {
        let clear = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: frame.clear,
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.,
                    stencil: 0,
                },
            },
        ];
        let rect = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };
        let size = [extent.width, extent.height];
        let full = ViewportRect::full(size);
        let scene = frame.viewport_rect(size);
        unsafe {
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.render_pass)
                    .framebuffer(framebuffer)
                    .render_area(rect)
                    .clear_values(&clear),
                vk::SubpassContents::INLINE,
            );
            device.cmd_set_viewport(cmd, 0, &[viewport(full)]);
            device.cmd_set_scissor(cmd, 0, &[scissor(full)]);
            if let Some(filter) = filter {
                // The filtered scene resolves only inside the canvas rectangle
                // so it cannot cover editor chrome outside it.
                device.cmd_set_scissor(cmd, 0, &[scissor(scene)]);
                filter.draw(cmd);
            }
            if let Some(shadow) = shadow {
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    shadow.layout,
                    1,
                    &[shadow.set],
                    &[],
                );
            }
            for pass in passes {
                if *pass > 1 {
                    break;
                }
                let mut bound = None;
                for layer in layers {
                    if *pass == 0 && !layer.scene {
                        continue;
                    }
                    let resident = layer.resident;
                    let scene_end = resident
                        .batches
                        .partition_point(|batch| batch.mode.is_scene());
                    let batches = match pass {
                        0 => &resident.batches[..scene_end],
                        _ => &resident.batches[scene_end..],
                    };
                    if batches.is_empty() {
                        continue;
                    }
                    bind_geometry(device, cmd, resident);
                    let mut texture = u32::MAX;
                    for batch in batches {
                        if bound != Some(batch.mode) {
                            bound = Some(batch.mode);
                            let pipeline = shadow
                                .and_then(|shadow| shadow.scene_for(batch.mode))
                                .unwrap_or(match batch.mode {
                                    DrawMode::Scene => self.scene,
                                    DrawMode::SceneTwoSided => self.scene_two_sided,
                                    DrawMode::SceneTransparent => self.scene_transparent,
                                    DrawMode::SceneTransparentTwoSided => {
                                        self.scene_transparent_two_sided
                                    }
                                    DrawMode::DepthReadOnly => self.read_depth,
                                    DrawMode::Overlay | DrawMode::SceneNoDepth => self.overlay,
                                });
                            device.cmd_bind_pipeline(
                                cmd,
                                vk::PipelineBindPoint::GRAPHICS,
                                pipeline,
                            );
                            let target = ViewportRect::for_mode(scene, batch.mode, size);
                            device.cmd_set_viewport(cmd, 0, &[viewport(target)]);
                            device.cmd_set_scissor(cmd, 0, &[scissor(target)]);
                        }
                        if batch.texture != texture {
                            texture = batch.texture;
                            device.cmd_bind_descriptor_sets(
                                cmd,
                                vk::PipelineBindPoint::GRAPHICS,
                                shadow.map_or(self.bindings.layout, |shadow| shadow.layout),
                                0,
                                &[resident.descriptors.sets[texture as usize]],
                                &[],
                            );
                        }
                        draw_batch(device, cmd, batch);
                    }
                }
            }
            device.cmd_end_render_pass(cmd);
        }
    }
}

impl Drop for Pipelines {
    fn drop(&mut self) {
        let device = &self.gpu.device;
        unsafe {
            if self.scene != vk::Pipeline::null() {
                device.destroy_pipeline(self.scene, None);
            }
            if self.scene_two_sided != vk::Pipeline::null() {
                device.destroy_pipeline(self.scene_two_sided, None);
            }
            if self.scene_transparent != vk::Pipeline::null() {
                device.destroy_pipeline(self.scene_transparent, None);
            }
            if self.scene_transparent_two_sided != vk::Pipeline::null() {
                device.destroy_pipeline(self.scene_transparent_two_sided, None);
            }
            if self.read_depth != vk::Pipeline::null() {
                device.destroy_pipeline(self.read_depth, None);
            }
            if self.overlay != vk::Pipeline::null() {
                device.destroy_pipeline(self.overlay, None);
            }
            if self.cache != vk::PipelineCache::null() {
                device.destroy_pipeline_cache(self.cache, None);
            }
            device.destroy_render_pass(self.render_pass, None);
        }
    }
}

fn create_render_pass(device: &ash::Device, format: vk::Format) -> Result<vk::RenderPass> {
    let attachments = [
        vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
        vk::AttachmentDescription::default()
            .format(DEPTH_FORMAT)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
    ];
    let color = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let depth = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };
    let passes = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color)
        .depth_stencil_attachment(&depth)];
    let stages = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS;
    let dependencies = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(stages)
        .dst_stage_mask(stages)
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        )];
    unsafe {
        device.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&passes)
                .dependencies(&dependencies),
            None,
        )
    }
    .map_err(vk_error)
}

pub(super) struct RasterTarget {
    pub pass: vk::RenderPass,
    pub layout: vk::PipelineLayout,
    pub color: bool,
    pub depth_test: bool,
    pub depth_write: bool,
    pub vertex_stream: bool,
    pub blend: bool,
    pub cull: vk::CullModeFlags,
}

pub(super) unsafe fn bind_geometry(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    resident: &Resident,
) {
    unsafe {
        device.cmd_bind_vertex_buffers(
            cmd,
            0,
            &[resident.geometry.raw; 2],
            &[0, resident.instance_offset],
        );
        if let Some(offset) = resident.index_offset {
            device.cmd_bind_index_buffer(cmd, resident.geometry.raw, offset, vk::IndexType::UINT32);
        }
    }
}

pub(super) unsafe fn draw_batch(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    batch: &crate::Batch,
) {
    unsafe {
        if let Some(indices) = &batch.indices {
            device.cmd_draw_indexed(
                cmd,
                indices.len() as u32,
                batch.instances.len() as u32,
                indices.start,
                0,
                batch.instances.start,
            );
        } else {
            device.cmd_draw(
                cmd,
                batch.vertices.len() as u32,
                batch.instances.len() as u32,
                batch.vertices.start,
                batch.instances.start,
            );
        }
    }
}

/// Framebuffer-space transform for a resolved viewport. Vulkan keeps its
/// existing top-left origin convention, so no Y flip is applied.
fn viewport(rect: ViewportRect) -> vk::Viewport {
    vk::Viewport {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        min_depth: 0.,
        max_depth: 1.,
    }
}

fn scissor(rect: ViewportRect) -> vk::Rect2D {
    let [x, y, width, height] = rect.pixels;
    vk::Rect2D {
        offset: vk::Offset2D {
            x: x as i32,
            y: y as i32,
        },
        extent: vk::Extent2D { width, height },
    }
}

pub(super) struct Shader {
    gpu: Rc<Gpu>,
    raw: vk::ShaderModule,
}

impl Shader {
    pub(super) fn new(
        gpu: &Rc<Gpu>,
        module: &naga::Module,
        info: &naga::valid::ModuleInfo,
        entry: &str,
        stage: naga::ShaderStage,
    ) -> Result<Self> {
        let options = naga::back::spv::Options::default();
        let select = naga::back::spv::PipelineOptions {
            shader_stage: stage,
            entry_point: entry.to_owned(),
        };
        let code = naga::back::spv::write_vec(module, info, &options, Some(&select))
            .map_err(|error| Error::Backend(error.to_string()))?;
        let raw = unsafe {
            gpu.device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)
        }
        .map_err(vk_error)?;
        Ok(Self {
            gpu: Rc::clone(gpu),
            raw,
        })
    }
}

impl Drop for Shader {
    fn drop(&mut self) {
        unsafe { self.gpu.device.destroy_shader_module(self.raw, None) };
    }
}
