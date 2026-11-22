use std::{mem::ManuallyDrop, ptr};

use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};

use super::dx_error;
use crate::{Error, Result};

pub(super) fn buffer(
    device: &ID3D12Device,
    bytes: u64,
    heap: D3D12_HEAP_TYPE,
    state: D3D12_RESOURCE_STATES,
) -> Result<ID3D12Resource> {
    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: bytes,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        ..Default::default()
    };
    create(device, heap, desc, state, ptr::null())
}

pub(super) fn image(
    device: &ID3D12Device,
    [w, h]: [u32; 2],
    format: DXGI_FORMAT,
    flags: D3D12_RESOURCE_FLAGS,
    state: D3D12_RESOURCE_STATES,
) -> Result<ID3D12Resource> {
    let desc = texture([w, h], format, flags);
    let value = desc
        .Flags
        .contains(D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL)
        .then(|| D3D12_CLEAR_VALUE {
            Format: desc.Format,
            Anonymous: D3D12_CLEAR_VALUE_0 {
                DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                    Depth: 1.,
                    Stencil: 0,
                },
            },
        });
    let clear = value
        .as_ref()
        .map_or(ptr::null(), |value| value as *const D3D12_CLEAR_VALUE);
    create(device, D3D12_HEAP_TYPE_DEFAULT, desc, state, clear)
}

/// A depth attachment whose resource format is typeless so a shader-readable
/// view can use the paired color format.
pub(super) fn depth_image(
    device: &ID3D12Device,
    [w, h]: [u32; 2],
    format: DXGI_FORMAT,
    clear: DXGI_FORMAT,
    state: D3D12_RESOURCE_STATES,
) -> Result<ID3D12Resource> {
    let desc = texture([w, h], format, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL);
    let value = D3D12_CLEAR_VALUE {
        Format: clear,
        Anonymous: D3D12_CLEAR_VALUE_0 {
            DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                Depth: 1.,
                Stencil: 0,
            },
        },
    };
    create(
        device,
        D3D12_HEAP_TYPE_DEFAULT,
        desc,
        state,
        &value as *const D3D12_CLEAR_VALUE,
    )
}

fn texture(
    [w, h]: [u32; 2],
    format: DXGI_FORMAT,
    flags: D3D12_RESOURCE_FLAGS,
) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Width: u64::from(w),
        Height: h,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Flags: flags,
        ..Default::default()
    }
}

fn create(
    device: &ID3D12Device,
    heap: D3D12_HEAP_TYPE,
    desc: D3D12_RESOURCE_DESC,
    state: D3D12_RESOURCE_STATES,
    clear: *const D3D12_CLEAR_VALUE,
) -> Result<ID3D12Resource> {
    let mut result = None;
    let props = D3D12_HEAP_PROPERTIES {
        Type: heap,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
        ..Default::default()
    };
    let clear = (!clear.is_null()).then_some(clear);
    unsafe {
        device.CreateCommittedResource(
            &props,
            D3D12_HEAP_FLAG_NONE,
            &desc,
            state,
            clear,
            &mut result,
        )
    }
    .map_err(dx_error)?;
    result.ok_or_else(|| Error::Backend("Direct3D 12 returned no resource".into()))
}

pub(super) fn write(
    buffer: &ID3D12Resource,
    copy: impl FnOnce(&mut [u8]),
    bytes: usize,
) -> Result<()> {
    let mut address = ptr::null_mut();
    let empty = D3D12_RANGE { Begin: 0, End: 0 };
    unsafe {
        buffer
            .Map(0, Some(&empty), Some(&mut address))
            .map_err(dx_error)?;
        // Upload buffer memory is initialized before creating the Rust slice.
        ptr::write_bytes(address.cast::<u8>(), 0, bytes);
        copy(std::slice::from_raw_parts_mut(address.cast(), bytes));
        buffer.Unmap(0, None);
    }
    Ok(())
}

pub(super) fn read_rows(
    buffer: &ID3D12Resource,
    bytes: usize,
    row: usize,
    pitch: usize,
) -> Result<Vec<u8>> {
    let mut address = ptr::null_mut();
    unsafe {
        buffer
            .Map(
                0,
                Some(&D3D12_RANGE {
                    Begin: 0,
                    End: bytes,
                }),
                Some(&mut address),
            )
            .map_err(dx_error)?;
        let mut pixels = Vec::with_capacity((bytes.div_ceil(pitch)) * row);
        for offset in (0..bytes).step_by(pitch) {
            pixels.extend_from_slice(std::slice::from_raw_parts(
                address.cast::<u8>().add(offset),
                row,
            ));
        }
        buffer.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
        Ok(pixels)
    }
}

pub(super) fn footprint(
    device: &ID3D12Device,
    texture: &ID3D12Resource,
) -> (D3D12_PLACED_SUBRESOURCE_FOOTPRINT, u64) {
    let mut layout = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
    let mut bytes = 0;
    unsafe {
        device.GetCopyableFootprints(
            &texture.GetDesc(),
            0,
            1,
            0,
            Some(&mut layout),
            None,
            None,
            Some(&mut bytes),
        );
    }
    (layout, bytes)
}

pub(super) unsafe fn transition(
    list: &ID3D12GraphicsCommandList,
    resource: &ID3D12Resource,
    from: D3D12_RESOURCE_STATES,
    to: D3D12_RESOURCE_STATES,
) {
    let transition = D3D12_RESOURCE_TRANSITION_BARRIER {
        // The caller retains the COM object for the lifetime of the recording and execution.
        pResource: ManuallyDrop::new(Some(unsafe { ptr::read(resource) })),
        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
        StateBefore: from,
        StateAfter: to,
    };
    unsafe {
        list.ResourceBarrier(&[D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: ManuallyDrop::new(transition),
            },
        }]);
    }
}

pub(super) unsafe fn copy_from_upload(
    list: &ID3D12GraphicsCommandList,
    image: &ID3D12Resource,
    staging: &ID3D12Resource,
    layout: D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
) {
    unsafe { copy_from_upload_at(list, image, staging, layout, [0, 0]) };
}

pub(super) unsafe fn copy_from_upload_at(
    list: &ID3D12GraphicsCommandList,
    image: &ID3D12Resource,
    staging: &ID3D12Resource,
    layout: D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
    origin: [u32; 2],
) {
    unsafe {
        list.CopyTextureRegion(
            &image_location(image),
            origin[0],
            origin[1],
            0,
            &buffer_location(staging, layout),
            None,
        );
    }
}

pub(super) unsafe fn copy_to_readback(
    list: &ID3D12GraphicsCommandList,
    staging: &ID3D12Resource,
    image: &ID3D12Resource,
    layout: D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
) {
    unsafe {
        list.CopyTextureRegion(
            &buffer_location(staging, layout),
            0,
            0,
            0,
            &image_location(image),
            None,
        );
    }
}

unsafe fn image_location(resource: &ID3D12Resource) -> D3D12_TEXTURE_COPY_LOCATION {
    D3D12_TEXTURE_COPY_LOCATION {
        pResource: ManuallyDrop::new(Some(unsafe { ptr::read(resource) })),
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            SubresourceIndex: 0,
        },
    }
}

unsafe fn buffer_location(
    resource: &ID3D12Resource,
    layout: D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
) -> D3D12_TEXTURE_COPY_LOCATION {
    D3D12_TEXTURE_COPY_LOCATION {
        pResource: ManuallyDrop::new(Some(unsafe { ptr::read(resource) })),
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            PlacedFootprint: layout,
        },
    }
}
