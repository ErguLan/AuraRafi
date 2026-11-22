use std::rc::Rc;

use ash::vk;

use crate::Result;

use super::context::Gpu;
use super::memory::Image;
use super::pipeline::{Pipelines, RasterTarget, Shader};
use super::vk_error;

pub(super) struct FilterPass {
    gpu: Rc<Gpu>,
    pub source: Image,
    pub input: vk::Framebuffer,
    extent: [u32; 2],
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline: vk::Pipeline,
}

impl FilterPass {
    pub fn new(
        gpu: &Rc<Gpu>,
        raster: &Pipelines,
        extent: [u32; 2],
        depth: vk::ImageView,
    ) -> Result<Self> {
        let [width, height] = extent;
        let mut filter = Self {
            gpu: Rc::clone(gpu),
            source: Image::new(
                gpu,
                extent,
                raster.format,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                vk::ImageAspectFlags::COLOR,
            )?,
            input: vk::Framebuffer::null(),
            extent,
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline: vk::Pipeline::null(),
        };
        let device = &gpu.device;
        unsafe {
            filter.input = device
                .create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(raster.render_pass)
                        .attachments(&[filter.source.view, depth])
                        .width(width)
                        .height(height)
                        .layers(1),
                    None,
                )
                .map_err(vk_error)?;
            let ty = vk::DescriptorType::SAMPLED_IMAGE;
            filter.set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[
                        vk::DescriptorSetLayoutBinding::default()
                            .binding(0)
                            .descriptor_type(ty)
                            .descriptor_count(1)
                            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                    ]),
                    None,
                )
                .map_err(vk_error)?;
            filter.layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&[filter.set_layout]),
                    None,
                )
                .map_err(vk_error)?;
            filter.pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .max_sets(1)
                        .pool_sizes(&[vk::DescriptorPoolSize {
                            ty,
                            descriptor_count: 1,
                        }]),
                    None,
                )
                .map_err(vk_error)?;
            filter.set = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(filter.pool)
                        .set_layouts(&[filter.set_layout]),
                )
                .map_err(vk_error)?[0];
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(filter.set)
                    .dst_binding(0)
                    .descriptor_type(ty)
                    .image_info(&[vk::DescriptorImageInfo::default()
                        .image_view(filter.source.view)
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)])],
                &[],
            );
        }
        let (module, info) = super::super::shaders::parse(crate::SCENE_FILTER_SHADER)?;
        let vertex = Shader::new(gpu, &module, &info, "vs_main", naga::ShaderStage::Vertex)?;
        let fragment = Shader::new(gpu, &module, &info, "fs_main", naga::ShaderStage::Fragment)?;
        filter.pipeline = raster.create_for(
            &vertex,
            c"vs_main",
            Some((&fragment, c"fs_main")),
            RasterTarget {
                pass: raster.render_pass,
                layout: filter.layout,
                color: true,
                depth_test: false,
                depth_write: false,
                vertex_stream: false,
                blend: false,
            },
        )?;
        Ok(filter)
    }

    pub fn bytes(&self) -> u64 {
        crate::device::filter_bytes(self.extent)
    }

    /// Invoked before RafUI/CAD overlays in the destination render pass.
    pub unsafe fn draw(&self, cmd: vk::CommandBuffer) {
        let device = &self.gpu.device;
        let bind = vk::PipelineBindPoint::GRAPHICS;
        unsafe {
            device.cmd_bind_pipeline(cmd, bind, self.pipeline);
            device.cmd_bind_descriptor_sets(cmd, bind, self.layout, 0, &[self.set], &[]);
            device.cmd_draw(cmd, 3, 1, 0, 0);
        }
    }
}

impl Drop for FilterPass {
    fn drop(&mut self) {
        let device = &self.gpu.device;
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_framebuffer(self.input, None);
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}
