use std::{ptr, rc::Rc};

use ash::vk;

use crate::Result;

use super::{context::Gpu, vk_error};

pub(super) struct Buffer {
    gpu: Rc<Gpu>,
    pub raw: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: *mut u8,
    size: u64,
}

impl Buffer {
    pub fn host(gpu: &Rc<Gpu>, size: u64, usage: vk::BufferUsageFlags) -> Result<Self> {
        let flags = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let mut buffer = Self::new(gpu, size, usage, flags)?;
        buffer.mapped = unsafe {
            gpu.device
                .map_memory(buffer.memory, 0, size, vk::MemoryMapFlags::empty())
        }
        .map_err(vk_error)?
        .cast();
        Ok(buffer)
    }

    pub fn local(gpu: &Rc<Gpu>, size: u64, usage: vk::BufferUsageFlags) -> Result<Self> {
        Self::new(gpu, size, usage, vk::MemoryPropertyFlags::DEVICE_LOCAL)
    }

    fn new(
        gpu: &Rc<Gpu>,
        size: u64,
        usage: vk::BufferUsageFlags,
        flags: vk::MemoryPropertyFlags,
    ) -> Result<Self> {
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let raw = unsafe { gpu.device.create_buffer(&info, None) }.map_err(vk_error)?;
        let mut buffer = Self {
            gpu: Rc::clone(gpu),
            raw,
            memory: vk::DeviceMemory::null(),
            mapped: ptr::null_mut(),
            size,
        };
        let requirements = unsafe { gpu.device.get_buffer_memory_requirements(raw) };
        buffer.memory = allocate(gpu, requirements, flags)?;
        unsafe { gpu.device.bind_buffer_memory(raw, buffer.memory, 0) }.map_err(vk_error)?;
        Ok(buffer)
    }

    pub fn write(&mut self, offset: u64, bytes: &[u8]) {
        assert!(
            !self.mapped.is_null()
                && offset <= self.size
                && bytes.len() as u64 <= self.size - offset
        );
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.mapped.add(offset as usize),
                bytes.len(),
            )
        };
    }

    pub fn read(&self, bytes: usize) -> Vec<u8> {
        assert!(!self.mapped.is_null() && bytes as u64 <= self.size);
        unsafe { std::slice::from_raw_parts(self.mapped, bytes) }.to_vec()
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let device = &self.gpu.device;
        unsafe {
            if !self.mapped.is_null() {
                device.unmap_memory(self.memory);
            }
            device.destroy_buffer(self.raw, None);
            if self.memory != vk::DeviceMemory::null() {
                device.free_memory(self.memory, None);
            }
        }
    }
}

pub(super) struct Image {
    gpu: Rc<Gpu>,
    pub raw: vk::Image,
    pub view: vk::ImageView,
    memory: vk::DeviceMemory,
}

impl Image {
    pub fn new(
        gpu: &Rc<Gpu>,
        [width, height]: [u32; 2],
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
    ) -> Result<Self> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage);
        let raw = unsafe { gpu.device.create_image(&info, None) }.map_err(vk_error)?;
        let mut image = Self {
            gpu: Rc::clone(gpu),
            raw,
            view: vk::ImageView::null(),
            memory: vk::DeviceMemory::null(),
        };
        let requirements = unsafe { gpu.device.get_image_memory_requirements(raw) };
        image.memory = allocate(gpu, requirements, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        unsafe { gpu.device.bind_image_memory(raw, image.memory, 0) }.map_err(vk_error)?;
        image.view = image_view(&gpu.device, raw, format, aspect)?;
        Ok(image)
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        let device = &self.gpu.device;
        unsafe {
            if self.view != vk::ImageView::null() {
                device.destroy_image_view(self.view, None);
            }
            device.destroy_image(self.raw, None);
            if self.memory != vk::DeviceMemory::null() {
                device.free_memory(self.memory, None);
            }
        }
    }
}

fn allocate(
    gpu: &Gpu,
    requirements: vk::MemoryRequirements,
    flags: vk::MemoryPropertyFlags,
) -> Result<vk::DeviceMemory> {
    let index = gpu.memory_type(requirements.memory_type_bits, flags)?;
    unsafe {
        gpu.device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(index),
            None,
        )
    }
    .map_err(vk_error)
}

pub(super) fn image_view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    aspect: vk::ImageAspectFlags,
) -> Result<vk::ImageView> {
    unsafe {
        device.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(range(aspect)),
            None,
        )
    }
    .map_err(vk_error)
}

fn range(aspect: vk::ImageAspectFlags) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(aspect)
        .layer_count(1)
        .level_count(1)
}

pub(super) fn copy_region(offset: u64, [width, height]: [u32; 2]) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .buffer_offset(offset)
        .image_subresource(
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1),
        )
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
}

pub(super) enum LayoutTransition {
    Upload,
    PatchTexture,
    FilterSample,
    Texture,
    Present,
    Readback,
    RestoreColor,
}

pub(super) unsafe fn transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    mode: LayoutTransition,
) {
    use vk::{AccessFlags as A, ImageLayout as L, PipelineStageFlags as S};
    let (from, to, source_layout, target_layout, source, target) = match mode {
        LayoutTransition::Upload => (
            S::TOP_OF_PIPE,
            S::TRANSFER,
            L::UNDEFINED,
            L::TRANSFER_DST_OPTIMAL,
            A::empty(),
            A::TRANSFER_WRITE,
        ),
        LayoutTransition::Texture => (
            S::TRANSFER,
            S::FRAGMENT_SHADER,
            L::TRANSFER_DST_OPTIMAL,
            L::SHADER_READ_ONLY_OPTIMAL,
            A::TRANSFER_WRITE,
            A::SHADER_READ,
        ),
        LayoutTransition::FilterSample => (
            S::COLOR_ATTACHMENT_OUTPUT,
            S::FRAGMENT_SHADER,
            L::COLOR_ATTACHMENT_OPTIMAL,
            L::SHADER_READ_ONLY_OPTIMAL,
            A::COLOR_ATTACHMENT_WRITE,
            A::SHADER_READ,
        ),
        LayoutTransition::PatchTexture => (
            S::FRAGMENT_SHADER,
            S::TRANSFER,
            L::SHADER_READ_ONLY_OPTIMAL,
            L::TRANSFER_DST_OPTIMAL,
            A::SHADER_READ,
            A::TRANSFER_WRITE,
        ),
        LayoutTransition::Present => (
            S::COLOR_ATTACHMENT_OUTPUT,
            S::BOTTOM_OF_PIPE,
            L::COLOR_ATTACHMENT_OPTIMAL,
            L::PRESENT_SRC_KHR,
            A::COLOR_ATTACHMENT_WRITE,
            A::empty(),
        ),
        LayoutTransition::Readback => (
            S::COLOR_ATTACHMENT_OUTPUT,
            S::TRANSFER,
            L::COLOR_ATTACHMENT_OPTIMAL,
            L::TRANSFER_SRC_OPTIMAL,
            A::COLOR_ATTACHMENT_WRITE,
            A::TRANSFER_READ,
        ),
        LayoutTransition::RestoreColor => (
            S::TRANSFER,
            S::COLOR_ATTACHMENT_OUTPUT,
            L::TRANSFER_SRC_OPTIMAL,
            L::COLOR_ATTACHMENT_OPTIMAL,
            A::TRANSFER_READ,
            A::COLOR_ATTACHMENT_WRITE,
        ),
    };
    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(source_layout)
        .new_layout(target_layout)
        .src_access_mask(source)
        .dst_access_mask(target)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(range(vk::ImageAspectFlags::COLOR));
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            from,
            to,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

pub(super) unsafe fn upload_image(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    source: vk::Buffer,
    offset: u64,
    dest: vk::Image,
    size: [u32; 2],
) {
    unsafe {
        transition(device, cmd, dest, LayoutTransition::Upload);
        device.cmd_copy_buffer_to_image(
            cmd,
            source,
            dest,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[copy_region(offset, size)],
        );
        transition(device, cmd, dest, LayoutTransition::Texture);
    }
}
