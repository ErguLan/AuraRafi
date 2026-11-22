use windows::Win32::Graphics::Direct3D12::*;

use super::{
    buffer, memory::copy_from_upload_at, transition, write, Dx12, Resident, COLOR, GEOMETRY_STATE,
};
use crate::{Backend, Instance, ResourcePatch, Result, Vertex};

pub(super) fn update(
    backend: &mut Dx12,
    resident: &Resident,
    patch: ResourcePatch<'_>,
) -> Result<()> {
    let (resource, state, upload) = match patch {
        ResourcePatch::Vertices { first, data } => (
            &resident.geometry,
            GEOMETRY_STATE,
            Copy::Buffer {
                offset: u64::from(first) * size_of::<Vertex>() as u64,
                bytes: bytemuck::cast_slice(data),
            },
        ),
        ResourcePatch::Instances { first, data } => (
            &resident.geometry,
            GEOMETRY_STATE,
            Copy::Buffer {
                offset: u64::from(resident.vertices[0].SizeInBytes)
                    + u64::from(first) * size_of::<Instance>() as u64,
                bytes: bytemuck::cast_slice(data),
            },
        ),
        ResourcePatch::Texture {
            index,
            origin,
            size,
            rgba,
        } => (
            &resident.textures[index as usize],
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            Copy::Image { origin, size, rgba },
        ),
    };
    let bytes = patch.staging_bytes(u64::from(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT));
    let staging = buffer(
        &backend.device,
        bytes,
        D3D12_HEAP_TYPE_UPLOAD,
        D3D12_RESOURCE_STATE_GENERIC_READ,
    )?;
    write(&staging, |mapped| upload.fill(mapped), bytes as usize)?;
    backend.begin()?;
    unsafe {
        transition(
            &backend.list,
            resource,
            state,
            D3D12_RESOURCE_STATE_COPY_DEST,
        );
        match upload {
            Copy::Buffer { offset, .. } => {
                backend
                    .list
                    .CopyBufferRegion(resource, offset, &staging, 0, bytes);
            }
            Copy::Image {
                origin,
                size: [width, height],
                ..
            } => {
                let layout = D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                    Offset: 0,
                    Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                        Format: COLOR,
                        Width: width,
                        Height: height,
                        Depth: 1,
                        RowPitch: (bytes / u64::from(height)) as u32,
                    },
                };
                copy_from_upload_at(&backend.list, resource, &staging, layout, origin);
            }
        }
        transition(
            &backend.list,
            resource,
            D3D12_RESOURCE_STATE_COPY_DEST,
            state,
        );
    }
    backend.submit()?;
    backend.wait()
}

enum Copy<'a> {
    Buffer {
        offset: u64,
        bytes: &'a [u8],
    },
    Image {
        origin: [u32; 2],
        size: [u32; 2],
        rgba: &'a [u8],
    },
}

impl Copy<'_> {
    fn fill(&self, mapped: &mut [u8]) {
        match self {
            Self::Buffer { bytes, .. } => mapped.copy_from_slice(bytes),
            Self::Image {
                size: [width, height],
                rgba,
                ..
            } => {
                let row = *width as usize * 4;
                let pitch = mapped.len() / *height as usize;
                for (target, source) in mapped.chunks_exact_mut(pitch).zip(rgba.chunks_exact(row)) {
                    target[..row].copy_from_slice(source);
                }
            }
        }
    }
}
