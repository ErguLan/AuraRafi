use std::{
    ffi::{CStr, CString},
    mem::{size_of, ManuallyDrop},
    ptr,
};

use windows::{
    core::PCSTR,
    Win32::Graphics::{
        Direct3D::{
            Fxc::{D3DCompile, D3DCOMPILE_OPTIMIZATION_LEVEL3},
            ID3DBlob, ID3DInclude,
        },
        Direct3D12::*,
        Dxgi::Common::*,
    },
};

use super::{dx_error, Resident, COLOR, DEPTH};
use crate::{DrawMode, Error, Instance, Result, ShadowFrame, Vertex};

pub(super) const ROOT_FRAME_CBV: u32 = 0;
pub(super) const ROOT_MATERIAL_TABLE: u32 = 1;
pub(super) const ROOT_SHADOW_CONSTANTS: u32 = 2;
pub(super) const ROOT_SHADOW_TABLE: u32 = 3;
pub(super) const SHADOW_UNIFORM_WORDS: u32 = (size_of::<ShadowFrame>() / size_of::<u32>()) as u32;

/// Pipeline family selected for one batch. Only scene batches are shadow receivers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Stage {
    Scene,
    ShadowScene,
    ReadDepth,
    Overlay,
}

impl Stage {
    pub(super) fn select(mode: DrawMode, shadow: bool) -> Self {
        match mode {
            DrawMode::Scene if shadow => Self::ShadowScene,
            DrawMode::Scene => Self::Scene,
            DrawMode::DepthReadOnly => Self::ReadDepth,
            DrawMode::Overlay | DrawMode::SceneNoDepth => Self::Overlay,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RasterFormats {
    pub color: DXGI_FORMAT,
    pub depth: DXGI_FORMAT,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RasterState {
    pub color: bool,
    pub depth_test: bool,
    pub depth_write: bool,
    pub vertex_stream: bool,
    pub blend: bool,
}

pub(super) struct Pipelines {
    root: ID3D12RootSignature,
    scene: ID3D12PipelineState,
    read_depth: ID3D12PipelineState,
    overlay: ID3D12PipelineState,
}

impl Pipelines {
    pub fn new(device: &ID3D12Device) -> Result<Self> {
        let root = root_signature(device)?;
        let draw = ShaderSource::new(crate::SHADER, naga::back::hlsl::ShaderModel::V5_0)?;
        let scene = draw.blob("scene", c"vs_5_0")?;
        let overlay = draw.blob("overlay", c"vs_5_0")?;
        let paint = draw.blob("paint", c"ps_5_0")?;
        let formats = RasterFormats {
            color: COLOR,
            depth: DEPTH,
        };
        let scene_state = RasterState {
            color: true,
            depth_test: true,
            depth_write: true,
            vertex_stream: true,
            blend: true,
        };
        Ok(Self {
            scene: raster_pipeline(device, &root, &scene, &paint, formats, scene_state)?,
            read_depth: raster_pipeline(
                device,
                &root,
                &scene,
                &paint,
                formats,
                RasterState {
                    depth_write: false,
                    ..scene_state
                },
            )?,
            overlay: raster_pipeline(
                device,
                &root,
                &overlay,
                &paint,
                formats,
                RasterState {
                    depth_test: false,
                    depth_write: false,
                    ..scene_state
                },
            )?,
            root,
        })
    }

    pub fn root(&self) -> &ID3D12RootSignature {
        &self.root
    }

    pub fn scene(&self) -> &ID3D12PipelineState {
        &self.scene
    }

    pub fn read_depth(&self) -> &ID3D12PipelineState {
        &self.read_depth
    }

    pub fn overlay(&self) -> &ID3D12PipelineState {
        &self.overlay
    }
}

fn root_signature(device: &ID3D12Device) -> Result<ID3D12RootSignature> {
    let material = descriptor_range(1, 0);
    let shadow = descriptor_range(0, 1);
    let parameters = [
        root_cbv(0, 0),
        descriptor_table(&material),
        constants(1, 1, SHADOW_UNIFORM_WORDS, D3D12_SHADER_VISIBILITY_ALL),
        descriptor_table(&shadow),
    ];
    let sampler = D3D12_STATIC_SAMPLER_DESC {
        Filter: D3D12_FILTER_MIN_MAG_LINEAR_MIP_POINT,
        AddressU: D3D12_TEXTURE_ADDRESS_MODE_WRAP,
        AddressV: D3D12_TEXTURE_ADDRESS_MODE_WRAP,
        AddressW: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
        MaxAnisotropy: 1,
        ComparisonFunc: D3D12_COMPARISON_FUNC_ALWAYS,
        ShaderRegister: 2,
        ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        ..Default::default()
    };
    create_root_signature(
        device,
        &D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: parameters.len() as u32,
            pParameters: parameters.as_ptr(),
            NumStaticSamplers: 1,
            pStaticSamplers: &sampler,
            Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
        },
    )
}

fn descriptor_range(register: u32, space: u32) -> D3D12_DESCRIPTOR_RANGE {
    D3D12_DESCRIPTOR_RANGE {
        RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
        NumDescriptors: 1,
        BaseShaderRegister: register,
        RegisterSpace: space,
        OffsetInDescriptorsFromTableStart: 0,
    }
}

fn root_cbv(register: u32, space: u32) -> D3D12_ROOT_PARAMETER {
    D3D12_ROOT_PARAMETER {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
        ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        Anonymous: D3D12_ROOT_PARAMETER_0 {
            Descriptor: D3D12_ROOT_DESCRIPTOR {
                ShaderRegister: register,
                RegisterSpace: space,
            },
        },
    }
}

fn constants(
    register: u32,
    space: u32,
    words: u32,
    visibility: D3D12_SHADER_VISIBILITY,
) -> D3D12_ROOT_PARAMETER {
    D3D12_ROOT_PARAMETER {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
        ShaderVisibility: visibility,
        Anonymous: D3D12_ROOT_PARAMETER_0 {
            Constants: D3D12_ROOT_CONSTANTS {
                ShaderRegister: register,
                RegisterSpace: space,
                Num32BitValues: words,
            },
        },
    }
}

fn descriptor_table(range: &D3D12_DESCRIPTOR_RANGE) -> D3D12_ROOT_PARAMETER {
    D3D12_ROOT_PARAMETER {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
        ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
        Anonymous: D3D12_ROOT_PARAMETER_0 {
            DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                NumDescriptorRanges: 1,
                pDescriptorRanges: range,
            },
        },
    }
}

pub(super) fn create_root_signature(
    device: &ID3D12Device,
    desc: &D3D12_ROOT_SIGNATURE_DESC,
) -> Result<ID3D12RootSignature> {
    let mut blob = None;
    let mut errors = None;
    unsafe {
        D3D12SerializeRootSignature(
            desc,
            D3D_ROOT_SIGNATURE_VERSION_1,
            &mut blob,
            Some(&mut errors),
        )
    }
    .map_err(|error| shader_error(error, errors.as_ref()))?;
    let blob = blob.ok_or_else(|| Error::Backend("empty Direct3D 12 root signature".into()))?;
    unsafe { device.CreateRootSignature(0, bytes(&blob)) }.map_err(dx_error)
}

/// Compiled WGSL module plus the HLSL source and entry point names that
/// Direct3D compiles per stage.
pub(super) struct ShaderSource {
    module: naga::Module,
    reflection: naga::back::hlsl::ReflectionInfo,
    hlsl: String,
}

impl ShaderSource {
    pub fn new(source: &str, shader_model: naga::back::hlsl::ShaderModel) -> Result<Self> {
        let (module, info) = super::super::shaders::parse(source)?;
        let options = naga::back::hlsl::Options {
            shader_model,
            ..Default::default()
        };
        let mut hlsl = String::new();
        let reflection = naga::back::hlsl::Writer::new(&mut hlsl, &options)
            .write(&module, &info, None)
            .map_err(|error| Error::Backend(error.to_string()))?;
        Ok(Self {
            module,
            reflection,
            hlsl,
        })
    }

    pub fn blob(&self, entry: &str, profile: &CStr) -> Result<ID3DBlob> {
        let index = self
            .module
            .entry_points
            .iter()
            .position(|candidate| candidate.name == entry)
            .ok_or(Error::Invalid("missing Direct3D shader entry"))?;
        let reflected = self.reflection.entry_point_names[index]
            .as_ref()
            .map_err(|error| Error::Backend(error.to_string()))?;
        let name =
            CString::new(reflected.as_str()).map_err(|error| Error::Backend(error.to_string()))?;
        let mut bytecode = None;
        let mut errors = None;
        unsafe {
            D3DCompile(
                self.hlsl.as_ptr().cast(),
                self.hlsl.len(),
                PCSTR::null(),
                None,
                None::<&ID3DInclude>,
                PCSTR(name.as_ptr().cast()),
                PCSTR(profile.as_ptr().cast()),
                D3DCOMPILE_OPTIMIZATION_LEVEL3,
                0,
                &mut bytecode,
                Some(&mut errors),
            )
        }
        .map_err(|error| shader_error(error, errors.as_ref()))?;
        bytecode.ok_or(Error::Invalid("missing compiled Direct3D shader"))
    }
}

pub(super) fn raster_pipeline(
    device: &ID3D12Device,
    root: &ID3D12RootSignature,
    vertex: &ID3DBlob,
    pixel: &ID3DBlob,
    formats: RasterFormats,
    state: RasterState,
) -> Result<ID3D12PipelineState> {
    let vertex_formats = [
        DXGI_FORMAT_R32G32B32_FLOAT,
        DXGI_FORMAT_R32G32_FLOAT,
        DXGI_FORMAT_R32G32B32A32_FLOAT,
    ];
    let input: [D3D12_INPUT_ELEMENT_DESC; 10] = std::array::from_fn(|index| {
        let (slot, offset, format, class, rate) = if index < 3 {
            (
                0,
                [0, 12, 20][index],
                vertex_formats[index],
                D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                0,
            )
        } else if index == 8 {
            (
                0,
                std::mem::offset_of!(Vertex, normal) as u32,
                DXGI_FORMAT_R32G32B32_FLOAT,
                D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                0,
            )
        } else if index == 9 {
            (
                1,
                std::mem::offset_of!(Instance, raster_kind) as u32,
                DXGI_FORMAT_R32_UINT,
                D3D12_INPUT_CLASSIFICATION_PER_INSTANCE_DATA,
                1,
            )
        } else {
            (
                1,
                (index as u32 - 3) * 16,
                DXGI_FORMAT_R32G32B32A32_FLOAT,
                D3D12_INPUT_CLASSIFICATION_PER_INSTANCE_DATA,
                1,
            )
        };
        D3D12_INPUT_ELEMENT_DESC {
            SemanticName: windows::core::s!("LOC"),
            SemanticIndex: index as u32,
            InputSlot: slot,
            AlignedByteOffset: offset,
            Format: format,
            InputSlotClass: class,
            InstanceDataStepRate: rate,
        }
    });
    let input = if state.vertex_stream {
        D3D12_INPUT_LAYOUT_DESC {
            pInputElementDescs: input.as_ptr(),
            NumElements: input.len() as u32,
        }
    } else {
        D3D12_INPUT_LAYOUT_DESC::default()
    };
    let blend = D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable: state.blend.into(),
        SrcBlend: D3D12_BLEND_SRC_ALPHA,
        DestBlend: D3D12_BLEND_INV_SRC_ALPHA,
        BlendOp: D3D12_BLEND_OP_ADD,
        SrcBlendAlpha: D3D12_BLEND_ONE,
        DestBlendAlpha: D3D12_BLEND_INV_SRC_ALPHA,
        BlendOpAlpha: D3D12_BLEND_OP_ADD,
        LogicOp: D3D12_LOGIC_OP_NOOP,
        RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
        ..Default::default()
    };
    let stencil = D3D12_DEPTH_STENCILOP_DESC {
        StencilFailOp: D3D12_STENCIL_OP_KEEP,
        StencilDepthFailOp: D3D12_STENCIL_OP_KEEP,
        StencilPassOp: D3D12_STENCIL_OP_KEEP,
        StencilFunc: D3D12_COMPARISON_FUNC_ALWAYS,
    };
    let blend_state = if state.color {
        D3D12_BLEND_DESC {
            RenderTarget: [blend; 8],
            ..Default::default()
        }
    } else {
        D3D12_BLEND_DESC::default()
    };
    let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
        // The root signature is borrowed for pipeline creation.
        pRootSignature: ManuallyDrop::new(Some(unsafe { ptr::read(root) })),
        VS: bytecode(vertex),
        PS: bytecode(pixel),
        BlendState: blend_state,
        SampleMask: u32::MAX,
        RasterizerState: D3D12_RASTERIZER_DESC {
            FillMode: D3D12_FILL_MODE_SOLID,
            CullMode: D3D12_CULL_MODE_NONE,
            DepthClipEnable: true.into(),
            ..Default::default()
        },
        DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
            DepthEnable: state.depth_test.into(),
            DepthWriteMask: if state.depth_write {
                D3D12_DEPTH_WRITE_MASK_ALL
            } else {
                D3D12_DEPTH_WRITE_MASK_ZERO
            },
            DepthFunc: if state.depth_test {
                D3D12_COMPARISON_FUNC_LESS_EQUAL
            } else {
                D3D12_COMPARISON_FUNC_ALWAYS
            },
            FrontFace: stencil,
            BackFace: stencil,
            ..Default::default()
        },
        InputLayout: input,
        PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
        NumRenderTargets: if state.color { 1 } else { 0 },
        RTVFormats: [
            if state.color {
                formats.color
            } else {
                DXGI_FORMAT_UNKNOWN
            },
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
            DXGI_FORMAT_UNKNOWN,
        ],
        DSVFormat: formats.depth,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        ..Default::default()
    };
    unsafe { device.CreateGraphicsPipelineState(&desc) }.map_err(dx_error)
}

pub(super) unsafe fn bind_geometry(list: &ID3D12GraphicsCommandList, resident: &Resident) {
    unsafe {
        list.IASetVertexBuffers(0, Some(&resident.vertices));
        if let Some(indices) = &resident.indices {
            list.IASetIndexBuffer(Some(ptr::from_ref(indices)));
        }
    }
}

pub(super) unsafe fn draw_batch(list: &ID3D12GraphicsCommandList, batch: &crate::Batch) {
    unsafe {
        if let Some(indices) = &batch.indices {
            list.DrawIndexedInstanced(
                indices.len() as u32,
                batch.instances.len() as u32,
                indices.start,
                0,
                batch.instances.start,
            );
        } else {
            list.DrawInstanced(
                batch.vertices.len() as u32,
                batch.instances.len() as u32,
                batch.vertices.start,
                batch.instances.start,
            );
        }
    }
}

fn bytes(blob: &ID3DBlob) -> &[u8] {
    unsafe { std::slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize()) }
}

fn bytecode(blob: &ID3DBlob) -> D3D12_SHADER_BYTECODE {
    let data = bytes(blob);
    D3D12_SHADER_BYTECODE {
        pShaderBytecode: data.as_ptr().cast(),
        BytecodeLength: data.len(),
    }
}

fn shader_error(error: windows::core::Error, diagnostics: Option<&ID3DBlob>) -> Error {
    match diagnostics {
        Some(blob) => Error::Backend(
            String::from_utf8_lossy(bytes(blob))
                .trim_matches('\0')
                .to_owned(),
        ),
        None => dx_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_selection_keeps_only_scene_batches_on_the_shadow_pipeline() {
        assert_eq!(Stage::select(DrawMode::Scene, true), Stage::ShadowScene);
        assert_eq!(Stage::select(DrawMode::Scene, false), Stage::Scene);
        assert_eq!(
            Stage::select(DrawMode::DepthReadOnly, true),
            Stage::ReadDepth
        );
        assert_eq!(Stage::select(DrawMode::SceneNoDepth, true), Stage::Overlay);
        assert_eq!(Stage::select(DrawMode::Overlay, true), Stage::Overlay);
    }

    #[test]
    fn root_parameters_fit_the_direct3d_budget() {
        // Frame CBV (2) + material table (1) + shadow constants + shadow table (1).
        let words = 2 + 1 + SHADOW_UNIFORM_WORDS + 1;
        assert!(words <= D3D12_MAX_ROOT_COST);
        assert_eq!(SHADOW_UNIFORM_WORDS, 24);
        assert_eq!(size_of::<ShadowFrame>() % size_of::<u32>(), 0);
    }
}
