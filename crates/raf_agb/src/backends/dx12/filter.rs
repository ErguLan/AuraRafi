//! Scene-only spatial filter for the Direct3D 12 executor.
//!
//! The pass renders scene batches into an offscreen color target that reuses the
//! main depth attachment, then composites that target over the cleared output
//! with one fullscreen triangle before overlays and UI. The presentation target
//! and its descriptors are transient: they exist only while a frame requests the
//! filter and are released as soon as it is disabled.

use std::ptr;

use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};

use super::{
    heap,
    memory::image,
    pipeline::{self, RasterFormats, RasterState, ShaderSource},
    EffectSupport, COLOR, DEPTH,
};
use crate::Result;

pub(super) const SOURCE_STATE: D3D12_RESOURCE_STATES =
    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;

pub(super) fn supported(support: D3D12_FORMAT_SUPPORT1) -> bool {
    support.contains(
        D3D12_FORMAT_SUPPORT1_RENDER_TARGET
            | D3D12_FORMAT_SUPPORT1_TEXTURE2D
            | D3D12_FORMAT_SUPPORT1_SHADER_LOAD,
    )
}

pub(super) fn capability(device: &ID3D12Device) -> EffectSupport {
    if supported(super::format_support(device, COLOR)) {
        EffectSupport::yes()
    } else {
        EffectSupport::no("the adapter cannot read the scene color format")
    }
}

pub(super) fn bytes(extent: [u32; 2]) -> u64 {
    crate::device::filter_bytes(extent)
}

pub(super) struct FilterPass {
    source: ID3D12Resource,
    _rtv_heap: ID3D12DescriptorHeap,
    rtv: D3D12_CPU_DESCRIPTOR_HANDLE,
    heaps: [Option<ID3D12DescriptorHeap>; 1],
    srv: D3D12_GPU_DESCRIPTOR_HANDLE,
    root: ID3D12RootSignature,
    pipeline: ID3D12PipelineState,
    extent: [u32; 2],
}

impl FilterPass {
    pub fn new(device: &ID3D12Device, extent: [u32; 2]) -> Result<Self> {
        let source = image(
            device,
            extent,
            COLOR,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
            SOURCE_STATE,
        )?;
        let rtv_heap = heap(
            device,
            D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            1,
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        )?;
        let srv_heap = heap(
            device,
            D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            1,
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        )?;
        let root = root_signature(device)?;
        let shader = ShaderSource::new(
            crate::SCENE_FILTER_SHADER,
            naga::back::hlsl::ShaderModel::V5_0,
        )?;
        let pipeline = pipeline::raster_pipeline(
            device,
            &root,
            &shader.blob("vs_main", c"vs_5_0")?,
            &shader.blob("fs_main", c"ps_5_0")?,
            RasterFormats {
                color: COLOR,
                depth: DEPTH,
            },
            RasterState {
                color: true,
                depth_test: false,
                depth_write: false,
                vertex_stream: false,
                blend: false,
            },
        )?;
        let rtv = unsafe { rtv_heap.GetCPUDescriptorHandleForHeapStart() };
        let srv = unsafe { srv_heap.GetGPUDescriptorHandleForHeapStart() };
        unsafe {
            device.CreateRenderTargetView(&source, None, rtv);
            device.CreateShaderResourceView(
                &source,
                Some(&view(COLOR)),
                srv_heap.GetCPUDescriptorHandleForHeapStart(),
            );
        }
        Ok(Self {
            source,
            _rtv_heap: rtv_heap,
            rtv,
            heaps: [Some(srv_heap)],
            srv,
            root,
            pipeline,
            extent,
        })
    }

    pub fn extent(&self) -> [u32; 2] {
        self.extent
    }

    pub fn bytes(&self) -> u64 {
        bytes(self.extent)
    }

    pub fn source(&self) -> &ID3D12Resource {
        &self.source
    }

    pub fn rtv(&self) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        self.rtv
    }

    /// Invoked after the filtered scene and before overlays or UI.
    pub(super) unsafe fn draw(&self, list: &ID3D12GraphicsCommandList) {
        unsafe {
            list.SetGraphicsRootSignature(&self.root);
            list.SetDescriptorHeaps(&self.heaps);
            list.SetGraphicsRootDescriptorTable(0, self.srv);
            list.SetPipelineState(&self.pipeline);
            list.DrawInstanced(3, 1, 0, 0);
        }
    }
}

fn root_signature(device: &ID3D12Device) -> Result<ID3D12RootSignature> {
    let range = D3D12_DESCRIPTOR_RANGE {
        RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
        NumDescriptors: 1,
        BaseShaderRegister: 0,
        RegisterSpace: 0,
        OffsetInDescriptorsFromTableStart: 0,
    };
    let parameters = [D3D12_ROOT_PARAMETER {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
        ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        Anonymous: D3D12_ROOT_PARAMETER_0 {
            DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                NumDescriptorRanges: 1,
                pDescriptorRanges: &range,
            },
        },
    }];
    pipeline::create_root_signature(
        device,
        &D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: parameters.len() as u32,
            pParameters: parameters.as_ptr(),
            NumStaticSamplers: 0,
            pStaticSamplers: ptr::null(),
            Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
        },
    )
}

fn view(format: DXGI_FORMAT) -> D3D12_SHADER_RESOURCE_VIEW_DESC {
    D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: format,
        ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
        Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_SRV {
                MostDetailedMip: 0,
                MipLevels: 1,
                PlaneSlice: 0,
                ResourceMinLODClamp: 0.,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gating_requires_a_render_target_that_can_be_read() {
        assert!(!supported(D3D12_FORMAT_SUPPORT1(0)));
        assert!(!supported(D3D12_FORMAT_SUPPORT1_RENDER_TARGET));
        assert!(supported(
            D3D12_FORMAT_SUPPORT1_RENDER_TARGET
                | D3D12_FORMAT_SUPPORT1_TEXTURE2D
                | D3D12_FORMAT_SUPPORT1_SHADER_LOAD
        ));
    }

    #[test]
    fn budget_matches_the_transient_scene_target() {
        assert_eq!(bytes([1280, 720]), 1280 * 720 * 4);
        assert_eq!(bytes([640, 360]), crate::device::filter_bytes([640, 360]));
    }
}
