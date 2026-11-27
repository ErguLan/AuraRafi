use std::rc::Rc;

use ash::{khr, vk};

use crate::{Error, Result};

use super::{
    context::{Gpu, Surface},
    memory::{image_view, Image},
    pipeline::{Pipelines, DEPTH_FORMAT},
    vk_error,
};

pub(super) struct Target {
    pub framebuffers: Vec<vk::Framebuffer>,
    pub extent: vk::Extent2D,
    depth: Image,
    color: Color,
    gpu: Rc<Gpu>,
}

enum Color {
    Offscreen(Image),
    Window(Swapchain),
}

impl Target {
    pub fn depth_view(&self) -> vk::ImageView {
        self.depth.view
    }

    pub fn new(
        gpu: &Rc<Gpu>,
        surface: Option<&Surface>,
        pipelines: &Pipelines,
        size: [u32; 2],
    ) -> Result<Self> {
        let [width, height] = size;
        let extent = vk::Extent2D { width, height };
        let color = match surface {
            Some(surface) => Color::Window(Swapchain::new(gpu, surface, extent)?),
            None => Color::Offscreen(Image::new(
                gpu,
                size,
                pipelines.format,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                vk::ImageAspectFlags::COLOR,
            )?),
        };
        let depth = Image::new(
            gpu,
            size,
            DEPTH_FORMAT,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH,
        )?;
        let mut target = Self {
            gpu: Rc::clone(gpu),
            depth,
            color,
            extent,
            framebuffers: Vec::new(),
        };
        let views = match &target.color {
            Color::Offscreen(image) => std::slice::from_ref(&image.view),
            Color::Window(window) => &window.views,
        };
        for view in views {
            let attachments = [*view, target.depth.view];
            let info = vk::FramebufferCreateInfo::default()
                .render_pass(pipelines.render_pass)
                .attachments(&attachments)
                .width(width)
                .height(height)
                .layers(1);
            target
                .framebuffers
                .push(unsafe { gpu.device.create_framebuffer(&info, None) }.map_err(vk_error)?);
        }
        Ok(target)
    }

    pub fn offscreen(&self) -> Result<&Image> {
        match &self.color {
            Color::Offscreen(image) => Ok(image),
            Color::Window(_) => Err(Error::Invalid("capture requires an offscreen target")),
        }
    }

    pub fn image(&self, index: usize) -> vk::Image {
        match &self.color {
            Color::Offscreen(image) => image.raw,
            Color::Window(window) => window.images[index],
        }
    }

    pub fn acquire(&self, available: vk::Semaphore) -> Result<(usize, Option<vk::Semaphore>)> {
        let Color::Window(window) = &self.color else {
            return Ok((0, None));
        };
        let (index, _) = unsafe {
            window
                .api
                .acquire_next_image(window.raw, 0, available, vk::Fence::null())
        }
        .map_err(vk_error)?;
        Ok((index as usize, Some(window.rendered[index as usize])))
    }

    pub fn present(&self, index: usize, signal: Option<vk::Semaphore>) -> Result<()> {
        let Color::Window(window) = &self.color else {
            return Ok(());
        };
        let semaphores = [signal.expect("acquired swapchain image")];
        let swapchains = [window.raw];
        let indices = [index as u32];
        let info = vk::PresentInfoKHR::default()
            .wait_semaphores(&semaphores)
            .swapchains(&swapchains)
            .image_indices(&indices);
        let _ = unsafe { window.api.queue_present(self.gpu.queue, &info) }.map_err(vk_error)?;
        Ok(())
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        for framebuffer in self.framebuffers.drain(..) {
            unsafe { self.gpu.device.destroy_framebuffer(framebuffer, None) };
        }
    }
}

struct Swapchain {
    gpu: Rc<Gpu>,
    api: khr::swapchain::Device,
    raw: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    rendered: Vec<vk::Semaphore>,
}

impl Swapchain {
    fn new(gpu: &Rc<Gpu>, surface: &Surface, extent: vk::Extent2D) -> Result<Self> {
        let api = gpu.swapchain_api();
        let caps = unsafe {
            surface
                .api
                .get_physical_device_surface_capabilities(gpu.physical, surface.raw)
        }
        .map_err(vk_error)?;
        let format = surface.choose_format(gpu.physical)?;
        let surface_extent = caps.current_extent;
        if (surface_extent.width != u32::MAX && surface_extent != extent)
            || !(caps.min_image_extent.width..=caps.max_image_extent.width).contains(&extent.width)
            || !(caps.min_image_extent.height..=caps.max_image_extent.height)
                .contains(&extent.height)
        {
            return Err(Error::SurfaceOutdated);
        }
        let image_count = caps.min_image_count.max(2);
        let image_count = if caps.max_image_count == 0 {
            image_count
        } else {
            image_count.min(caps.max_image_count)
        };
        let alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::INHERIT,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        ]
        .into_iter()
        .find(|mode| caps.supported_composite_alpha.contains(*mode))
        .ok_or(Error::Invalid(
            "Vulkan surface has no composite alpha format",
        ))?;
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface.raw)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true);
        let raw = unsafe { api.create_swapchain(&info, None) }.map_err(vk_error)?;
        let mut chain = Self {
            api,
            raw,
            gpu: Rc::clone(gpu),
            images: Vec::new(),
            views: Vec::new(),
            rendered: Vec::new(),
        };
        chain.images = unsafe { chain.api.get_swapchain_images(raw) }.map_err(vk_error)?;
        for image in &chain.images {
            chain.views.push(image_view(
                &gpu.device,
                *image,
                format.format,
                vk::ImageAspectFlags::COLOR,
                1,
            )?);
            chain.rendered.push(
                unsafe {
                    gpu.device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                }
                .map_err(vk_error)?,
            );
        }
        Ok(chain)
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            for view in self.views.drain(..) {
                self.gpu.device.destroy_image_view(view, None);
            }
            for semaphore in self.rendered.drain(..) {
                self.gpu.device.destroy_semaphore(semaphore, None);
            }
            self.api.destroy_swapchain(self.raw, None);
        }
    }
}
