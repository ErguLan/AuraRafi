use ash::vk;

use super::*;
use memory::{copy_region, transition, LayoutTransition};

pub(super) fn update(
    backend: &mut Vulkan,
    resident: &Resident,
    patch: ResourcePatch<'_>,
) -> Result<()> {
    let bytes = match patch {
        ResourcePatch::Vertices { data, .. } => bytemuck::cast_slice(data),
        ResourcePatch::Instances { data, .. } => bytemuck::cast_slice(data),
        ResourcePatch::Texture { rgba, .. } => rgba,
    };
    let mut staging = Buffer::host(
        &backend.gpu,
        bytes.len() as u64,
        vk::BufferUsageFlags::TRANSFER_SRC,
    )?;
    staging.write(0, bytes);
    let cmd = backend.begin()?;
    let device = &backend.gpu.device;
    // begin waits for the previous draw. The staging range remains alive
    // until the fence for this transfer is signalled.
    unsafe {
        match patch {
            ResourcePatch::Vertices { first, .. } => copy_geometry(
                device,
                cmd,
                staging.raw,
                resident.geometry.raw,
                u64::from(first) * size_of::<Vertex>() as u64,
                bytes.len() as u64,
            ),
            ResourcePatch::Instances { first, .. } => copy_geometry(
                device,
                cmd,
                staging.raw,
                resident.geometry.raw,
                resident.instance_offset + u64::from(first) * size_of::<Instance>() as u64,
                bytes.len() as u64,
            ),
            ResourcePatch::Texture {
                index,
                origin,
                size,
                ..
            } => {
                let image = resident.textures[index as usize].raw;
                transition(device, cmd, image, LayoutTransition::PatchTexture);
                device.cmd_copy_buffer_to_image(
                    cmd,
                    staging.raw,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy_region(0, size).image_offset(vk::Offset3D {
                        x: origin[0] as i32,
                        y: origin[1] as i32,
                        z: 0,
                    })],
                );
                transition(device, cmd, image, LayoutTransition::Texture);
            }
        }
    }
    backend.submit.submit(None)?;
    backend.wait_render()
}

unsafe fn copy_geometry(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    staging: vk::Buffer,
    geometry: vk::Buffer,
    offset: u64,
    size: u64,
) {
    unsafe {
        device.cmd_copy_buffer(
            cmd,
            staging,
            geometry,
            &[vk::BufferCopy {
                src_offset: 0,
                dst_offset: offset,
                size,
            }],
        );
        let barrier = vk::BufferMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::VERTEX_ATTRIBUTE_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(geometry)
            .offset(offset)
            .size(size);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::VERTEX_INPUT,
            vk::DependencyFlags::empty(),
            &[],
            &[barrier],
            &[],
        );
    }
}
