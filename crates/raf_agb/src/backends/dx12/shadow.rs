//! Directional shadow map for the Direct3D 12 executor.
//!
//! The pass mirrors the Vulkan executor: scene batches with the caster mode are
//! rendered depth-only into a 1024 x 1024 map, alpha-tested with the material
//! texture, and the receiver pipeline compares that depth with the consumer's
//! light matrix and bias. The map and its descriptors exist only while a frame
//! requests shadows.

use std::{mem::size_of, ptr};

use bytemuck::Zeroable;
use windows::Win32::{
    Foundation::RECT,
    Graphics::{
        Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Direct3D12::*, Dxgi::Common::*,
    },
};

use super::{
    memory::{depth_image, transition},
    pipeline::{self, RasterFormats, RasterState, ShaderSource},
    EffectSupport, Resident, COLOR, DEPTH,
};
use crate::{
    device::{SHADOW_MAP_BYTES, SHADOW_MAP_SIDE},
    DrawMode, Result, ShadowFrame,
};

const SHADOW_SOURCE: &str = concat!(
    include_str!("../../shaders/draw.wgsl"),
    include_str!("../../shaders/shadow.wgsl"),
);
const MAP_RESOURCE_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R32_TYPELESS;
const MAP_DEPTH_FORMAT: DXGI_FORMAT = DXGI_FORMAT_D32_FLOAT;
const MAP_SAMPLE_FORMAT: DXGI_FORMAT = DXGI_FORMAT_R32_FLOAT;
const MAP_STATE: D3D12_RESOURCE_STATES = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
const MAP_CLEAR_DEPTH: f32 = 1.;

pub(super) fn supported(
    sample: D3D12_FORMAT_SUPPORT1,
    depth: D3D12_FORMAT_SUPPORT1,
) -> bool {
    sample.contains(D3D12_FORMAT_SUPPORT1_TEXTURE2D | D3D12_FORMAT_SUPPORT1_SHADER_LOAD)
        && depth.contains(D3D12_FORMAT_SUPPORT1_DEPTH_STENCIL)
}

pub(super) fn capability(device: &ID3D12Device) -> EffectSupport {
    let sample = super::format_support(device, MAP_SAMPLE_FORMAT);
    let depth = super::format_support(device, MAP_DEPTH_FORMAT);
    if supported(sample, depth) {
        EffectSupport::yes()
    } else {
        EffectSupport::no("the adapter cannot read the shadow depth format")
    }
}

pub(super) fn bytes(resident: bool) -> u64 {
    if resident {
        SHADOW_MAP_BYTES
    } else {
        0
    }
}

pub(super) struct ShadowPass {
    map: ID3D12Resource,
    _dsv_heap: ID3D12DescriptorHeap,
    dsv: D3D12_CPU_DESCRIPTOR_HANDLE,
    caster: ID3D12PipelineState,
    scene: ID3D12PipelineState,
    parameters: ShadowFrame,
    last: Option<ShadowFrame>,
}

impl ShadowPass {
    pub fn new(device: &ID3D12Device, root: &ID3D12RootSignature) -> Result<Self> {
        let map = depth_image(
            device,
            [SHADOW_MAP_SIDE; 2],
            MAP_RESOURCE_FORMAT,
            MAP_DEPTH_FORMAT,
            MAP_STATE,
        )?;
        let dsv_heap = super::heap(
            device,
            D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
            1,
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        )?;
        let dsv = unsafe { dsv_heap.GetCPUDescriptorHandleForHeapStart() };
        let dsv_desc = D3D12_DEPTH_STENCIL_VIEW_DESC {
            Format: MAP_DEPTH_FORMAT,
            ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D,
            Flags: D3D12_DSV_FLAG_NONE,
            Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 {
                Texture2D: D3D12_TEX2D_DSV { MipSlice: 0 },
            },
        };
        unsafe { device.CreateDepthStencilView(&map, Some(&dsv_desc), dsv) };
        let source =
            ShaderSource::new(SHADOW_SOURCE, naga::back::hlsl::ShaderModel::V5_1)?;
        let formats = RasterFormats {
            color: COLOR,
            depth: DEPTH,
        };
        let caster = pipeline::raster_pipeline(
            device,
            root,
            &source.blob("shadow_depth", c"vs_5_1")?,
            &source.blob("shadow_caster", c"ps_5_1")?,
            formats,
            RasterState {
                color: false,
                depth_test: true,
                depth_write: true,
                vertex_stream: true,
                blend: false,
            },
        )?;
        let scene = pipeline::raster_pipeline(
            device,
            root,
            &source.blob("shadow_scene", c"vs_5_1")?,
            &source.blob("shadow_paint", c"ps_5_1")?,
            formats,
            RasterState {
                color: true,
                depth_test: true,
                depth_write: true,
                vertex_stream: true,
                blend: true,
            },
        )?;
        Ok(Self {
            map,
            dsv,
            _dsv_heap: dsv_heap,
            caster,
            scene,
            parameters: ShadowFrame::zeroed(),
            last: None,
        })
    }

    pub fn scene(&self) -> &ID3D12PipelineState {
        &self.scene
    }

    /// Writes the map view into the resident material heap. Direct3D 12 allows
    /// one shader-visible view heap per list, so the shadow map shares the
    /// resident heap instead of binding a second one.
    pub(super) unsafe fn write_view(
        &self,
        device: &ID3D12Device,
        resident: &Resident,
    ) {
        let Some(heap) = resident.heaps[0].as_ref() else {
            return;
        };
        let start = unsafe { heap.GetCPUDescriptorHandleForHeapStart() }.ptr;
        let slot = start
            + (resident.textures.len() as u64 * resident.descriptor_step) as usize;
        unsafe {
            device.CreateShaderResourceView(
                &self.map,
                Some(&view()),
                D3D12_CPU_DESCRIPTOR_HANDLE { ptr: slot },
            );
        }
    }

    /// Uploads the light matrix only when the consumer changed it.
    pub fn prepare(&mut self, parameters: ShadowFrame) -> u64 {
        if self.last == Some(parameters) {
            return 0;
        }
        self.parameters = parameters;
        self.last = Some(parameters);
        size_of::<ShadowFrame>() as u64
    }

    pub(super) unsafe fn bind(&self, list: &ID3D12GraphicsCommandList) {
        unsafe {
            list.SetGraphicsRoot32BitConstants(
                pipeline::ROOT_SHADOW_CONSTANTS,
                pipeline::SHADOW_UNIFORM_WORDS,
                ptr::from_ref(&self.parameters).cast(),
                0,
            );
        }
    }

    pub(super) unsafe fn record(
        &self,
        device: &ID3D12Device,
        list: &ID3D12GraphicsCommandList,
        root: &ID3D12RootSignature,
        resident: &Resident,
    ) -> u32 {
        let extent = [SHADOW_MAP_SIDE; 2];
        let viewport = D3D12_VIEWPORT {
            Width: SHADOW_MAP_SIDE as f32,
            Height: SHADOW_MAP_SIDE as f32,
            MaxDepth: 1.,
            ..Default::default()
        };
        let scissor = RECT {
            left: 0,
            top: 0,
            right: extent[0] as i32,
            bottom: extent[1] as i32,
        };
        let mut draws = 0;
        unsafe {
            self.write_view(device, resident);
            transition(
                list,
                &self.map,
                MAP_STATE,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
            );
            list.RSSetViewports(&[viewport]);
            list.RSSetScissorRects(&[scissor]);
            list.OMSetRenderTargets(0, None, false, Some(&self.dsv));
            list.ClearDepthStencilView(
                self.dsv,
                D3D12_CLEAR_FLAG_DEPTH,
                MAP_CLEAR_DEPTH,
                0,
                &[],
            );
            list.SetGraphicsRootSignature(root);
            self.bind(list);
            list.SetDescriptorHeaps(&resident.heaps);
            list.SetPipelineState(&self.caster);
            list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            pipeline::bind_geometry(list, resident);
            let mut texture = u32::MAX;
            for batch in resident
                .batches
                .iter()
                .filter(|batch| batch.mode == DrawMode::Scene)
            {
                if batch.texture != texture {
                    texture = batch.texture;
                    list.SetGraphicsRootDescriptorTable(
                        pipeline::ROOT_MATERIAL_TABLE,
                        D3D12_GPU_DESCRIPTOR_HANDLE {
                            ptr: resident.descriptor_start
                                + u64::from(batch.texture) * resident.descriptor_step,
                        },
                    );
                }
                pipeline::draw_batch(list, batch);
                draws += 1;
            }
            transition(
                list,
                &self.map,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
                MAP_STATE,
            );
        }
        draws
    }
}

fn view() -> D3D12_SHADER_RESOURCE_VIEW_DESC {
    D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: MAP_SAMPLE_FORMAT,
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
    fn gating_requires_a_depth_attachment_and_a_readable_depth_view() {
        let readable =
            D3D12_FORMAT_SUPPORT1_TEXTURE2D | D3D12_FORMAT_SUPPORT1_SHADER_LOAD;
        assert!(!supported(D3D12_FORMAT_SUPPORT1(0), D3D12_FORMAT_SUPPORT1(0)));
        assert!(!supported(readable, D3D12_FORMAT_SUPPORT1(0)));
        assert!(!supported(
            D3D12_FORMAT_SUPPORT1_TEXTURE2D,
            D3D12_FORMAT_SUPPORT1_DEPTH_STENCIL
        ));
        assert!(supported(
            readable,
            D3D12_FORMAT_SUPPORT1_DEPTH_STENCIL
        ));
    }

    #[test]
    fn budget_matches_the_resident_depth_map() {
        assert_eq!(bytes(false), 0);
        assert_eq!(bytes(true), SHADOW_MAP_BYTES);
        assert_eq!(
            SHADOW_MAP_BYTES,
            u64::from(SHADOW_MAP_SIDE) * u64::from(SHADOW_MAP_SIDE) * 4
        );
    }
}
