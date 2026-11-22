use std::rc::Rc;

use ash::vk;

use crate::device::SHADOW_MAP_SIDE;
use crate::{DrawMode, Result, ShadowFrame};

use super::context::Gpu;
use super::memory::{Buffer, Image};
use super::pipeline::{bind_geometry, draw_batch, Pipelines, RasterTarget, Shader, DEPTH_FORMAT};
use super::{vk_error, Resident};

const SHADER: &str = concat!(
    include_str!("../../shaders/draw.wgsl"),
    include_str!("../../shaders/shadow.wgsl")
);

pub(super) struct ShadowPass {
    gpu: Rc<Gpu>,
    image: Image,
    uniform: Buffer,
    last: Option<ShadowFrame>,
    pass: vk::RenderPass,
    framebuffer: vk::Framebuffer,
    set_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    pub set: vk::DescriptorSet,
    pub layout: vk::PipelineLayout,
    caster: vk::Pipeline,
    pub scene: vk::Pipeline,
}

impl ShadowPass {
    pub fn new(gpu: &Rc<Gpu>, pipelines: &Pipelines) -> Result<Self> {
        let mut shadow = Self {
            gpu: Rc::clone(gpu),
            image: Image::new(
                gpu,
                [SHADOW_MAP_SIDE; 2],
                DEPTH_FORMAT,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                vk::ImageAspectFlags::DEPTH,
            )?,
            uniform: Buffer::host(
                gpu,
                size_of::<ShadowFrame>() as u64,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            )?,
            last: None,
            pass: vk::RenderPass::null(),
            framebuffer: vk::Framebuffer::null(),
            set_layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            layout: vk::PipelineLayout::null(),
            caster: vk::Pipeline::null(),
            scene: vk::Pipeline::null(),
        };
        let device = &gpu.device;
        shadow.pass = render_pass(device)?;
        shadow.framebuffer = unsafe {
            device.create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(shadow.pass)
                    .attachments(&[shadow.image.view])
                    .width(SHADOW_MAP_SIDE)
                    .height(SHADOW_MAP_SIDE)
                    .layers(1),
                None,
            )
        }
        .map_err(vk_error)?;
        shadow.create_bindings(pipelines)?;
        let (module, info) = super::super::shaders::parse(SHADER)?;
        let shader = |entry, stage| Shader::new(gpu, &module, &info, entry, stage);
        shadow.caster = pipelines.create_for(
            &shader("shadow_depth", naga::ShaderStage::Vertex)?,
            c"shadow_depth",
            Some((
                &shader("shadow_caster", naga::ShaderStage::Fragment)?,
                c"shadow_caster",
            )),
            RasterTarget {
                pass: shadow.pass,
                layout: shadow.layout,
                color: false,
                depth_test: true,
                depth_write: true,
                vertex_stream: true,
                blend: false,
            },
        )?;
        shadow.scene = pipelines.create_for(
            &shader("shadow_scene", naga::ShaderStage::Vertex)?,
            c"shadow_scene",
            Some((
                &shader("shadow_paint", naga::ShaderStage::Fragment)?,
                c"shadow_paint",
            )),
            RasterTarget {
                pass: pipelines.render_pass,
                layout: shadow.layout,
                color: true,
                depth_test: true,
                depth_write: true,
                vertex_stream: true,
                blend: true,
            },
        )?;
        Ok(shadow)
    }

    fn create_bindings(&mut self, pipelines: &Pipelines) -> Result<()> {
        let device = &self.gpu.device;
        let depth = vk::DescriptorType::SAMPLED_IMAGE;
        let uniform = vk::DescriptorType::UNIFORM_BUFFER;
        let entries = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(depth)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(uniform)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];
        unsafe {
            self.set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&entries),
                    None,
                )
                .map_err(vk_error)?;
            self.layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&[pipelines.material_layout(), self.set_layout]),
                    None,
                )
                .map_err(vk_error)?;
            self.pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&[
                            vk::DescriptorPoolSize {
                                ty: depth,
                                descriptor_count: 1,
                            },
                            vk::DescriptorPoolSize {
                                ty: uniform,
                                descriptor_count: 1,
                            },
                        ]),
                    None,
                )
                .map_err(vk_error)?;
            self.set = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.pool)
                        .set_layouts(&[self.set_layout]),
                )
                .map_err(vk_error)?[0];
            let image = [vk::DescriptorImageInfo::default()
                .image_view(self.image.view)
                .image_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
            let buffer = [vk::DescriptorBufferInfo::default()
                .buffer(self.uniform.raw)
                .range(size_of::<ShadowFrame>() as u64)];
            device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.set)
                        .dst_binding(0)
                        .descriptor_type(depth)
                        .image_info(&image),
                    vk::WriteDescriptorSet::default()
                        .dst_set(self.set)
                        .dst_binding(1)
                        .descriptor_type(uniform)
                        .buffer_info(&buffer),
                ],
                &[],
            );
        }
        Ok(())
    }

    pub fn prepare(&mut self, parameters: ShadowFrame) -> u64 {
        if self.last == Some(parameters) {
            return 0;
        }
        self.uniform.write(0, bytemuck::bytes_of(&parameters));
        self.last = Some(parameters);
        size_of::<ShadowFrame>() as u64
    }

    pub unsafe fn record(&self, cmd: vk::CommandBuffer, resident: &Resident) -> u32 {
        let device = &self.gpu.device;
        let rect = vk::Rect2D::default().extent(vk::Extent2D {
            width: SHADOW_MAP_SIDE,
            height: SHADOW_MAP_SIDE,
        });
        let viewport = vk::Viewport {
            width: SHADOW_MAP_SIDE as f32,
            height: SHADOW_MAP_SIDE as f32,
            max_depth: 1.,
            ..Default::default()
        };
        let clear = [vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.,
                stencil: 0,
            },
        }];
        let bind = vk::PipelineBindPoint::GRAPHICS;
        let mut draws = 0;
        unsafe {
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.pass)
                    .framebuffer(self.framebuffer)
                    .render_area(rect)
                    .clear_values(&clear),
                vk::SubpassContents::INLINE,
            );
            device.cmd_set_viewport(cmd, 0, &[viewport]);
            device.cmd_set_scissor(cmd, 0, &[rect]);
            device.cmd_bind_pipeline(cmd, bind, self.caster);
            device.cmd_bind_descriptor_sets(cmd, bind, self.layout, 1, &[self.set], &[]);
            bind_geometry(device, cmd, resident);
            let mut texture = None;
            for batch in resident
                .batches
                .iter()
                .filter(|batch| batch.mode == DrawMode::Scene)
            {
                if texture != Some(batch.texture) {
                    texture = Some(batch.texture);
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        bind,
                        self.layout,
                        0,
                        &[resident.descriptors.sets[batch.texture as usize]],
                        &[],
                    );
                }
                draw_batch(device, cmd, batch);
                draws += 1;
            }
            device.cmd_end_render_pass(cmd);
        }
        draws
    }
}

fn render_pass(device: &ash::Device) -> Result<vk::RenderPass> {
    use vk::{AccessFlags as A, PipelineStageFlags as S};
    let attachment = [vk::AttachmentDescription::default()
        .format(DEPTH_FORMAT)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)];
    let reference = vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };
    let subpass = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .depth_stencil_attachment(&reference)];
    let tests = S::EARLY_FRAGMENT_TESTS | S::LATE_FRAGMENT_TESTS;
    let dependencies = [
        vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(S::FRAGMENT_SHADER)
            .src_access_mask(A::SHADER_READ)
            .dst_stage_mask(tests)
            .dst_access_mask(A::DEPTH_STENCIL_ATTACHMENT_WRITE),
        vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(tests)
            .src_access_mask(A::DEPTH_STENCIL_ATTACHMENT_WRITE)
            .dst_stage_mask(S::FRAGMENT_SHADER)
            .dst_access_mask(A::SHADER_READ),
    ];
    unsafe {
        device.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachment)
                .subpasses(&subpass)
                .dependencies(&dependencies),
            None,
        )
    }
    .map_err(vk_error)
}

impl Drop for ShadowPass {
    fn drop(&mut self) {
        let device = &self.gpu.device;
        unsafe {
            device.destroy_pipeline(self.caster, None);
            device.destroy_pipeline(self.scene, None);
            device.destroy_framebuffer(self.framebuffer, None);
            device.destroy_render_pass(self.pass, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}
