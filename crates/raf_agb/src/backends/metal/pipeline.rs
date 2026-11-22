use metal::*;
use naga::back::msl;

use super::{CAMERA_SLOT, DEPTH};
use crate::{Error, Instance, Result, Vertex};

pub(super) struct Pipelines {
    pub scene: RenderPipelineState,
    pub overlay: RenderPipelineState,
    pub scene_depth: DepthStencilState,
    pub read_depth: DepthStencilState,
    pub overlay_depth: DepthStencilState,
}

impl Pipelines {
    pub fn new(device: &DeviceRef, format: MTLPixelFormat) -> Result<Self> {
        let (module, info) = super::super::shaders::source()?;
        let bindings = [
            (
                0,
                msl::BindTarget {
                    buffer: Some(CAMERA_SLOT),
                    ..Default::default()
                },
            ),
            (
                1,
                msl::BindTarget {
                    texture: Some(0),
                    ..Default::default()
                },
            ),
            (
                2,
                msl::BindTarget {
                    sampler: Some(msl::BindSamplerTarget::Resource(0)),
                    ..Default::default()
                },
            ),
        ];
        let resources = |range: &[usize]| msl::EntryPointResources {
            resources: range
                .iter()
                .map(|&i| {
                    (
                        naga::ResourceBinding {
                            group: 0,
                            binding: bindings[i].0,
                        },
                        bindings[i].1.clone(),
                    )
                })
                .collect(),
            ..Default::default()
        };
        let options = msl::Options {
            lang_version: (2, 0),
            fake_missing_bindings: false,
            per_entry_point_map: [
                ("scene".into(), resources(&[0])),
                ("overlay".into(), resources(&[0])),
                ("paint".into(), resources(&[1, 2])),
            ]
            .into(),
            ..Default::default()
        };
        let (source, reflection) =
            msl::write_string(&module, &info, &options, &msl::PipelineOptions::default())
                .map_err(|error| Error::Backend(error.to_string()))?;
        let library = device
            .new_library_with_source(&source, &CompileOptions::new())
            .map_err(Error::Backend)?;
        let function = |entry: &str| {
            let index = module
                .entry_points
                .iter()
                .position(|point| point.name == entry)
                .ok_or(Error::Invalid("missing Metal shader entry"))?;
            let name = reflection.entry_point_names[index]
                .as_ref()
                .map_err(|error| Error::Backend(error.to_string()))?;
            library.get_function(name, None).map_err(Error::Backend)
        };
        let pixel = function("paint")?;
        let scene_vertex = function("scene")?;
        let overlay_vertex = function("overlay")?;
        let scene = pipeline(device, &scene_vertex, &pixel, format)?;
        let overlay = pipeline(device, &overlay_vertex, &pixel, format)?;
        let scene_depth = depth(device, true, true);
        let overlay_depth = depth(device, false, false);
        Ok(Self {
            scene,
            overlay,
            scene_depth,
            read_depth: depth(device, true, false),
            overlay_depth,
        })
    }
}

fn depth(device: &DeviceRef, test: bool, write: bool) -> DepthStencilState {
    let desc = DepthStencilDescriptor::new();
    desc.set_depth_write_enabled(write);
    desc.set_depth_compare_function(if test {
        MTLCompareFunction::LessEqual
    } else {
        MTLCompareFunction::Always
    });
    device.new_depth_stencil_state(&desc)
}

fn pipeline(
    device: &DeviceRef,
    vertex: &FunctionRef,
    fragment: &FunctionRef,
    format: MTLPixelFormat,
) -> Result<RenderPipelineState> {
    let vertex_layout = VertexDescriptor::new();
    let attributes = vertex_layout.attributes();
    for index in 0..10 {
        let attribute = attributes.object_at(index).unwrap();
        if index < 3 {
            attribute.set_buffer_index(0);
            attribute.set_offset([0, 12, 20][index as usize]);
            attribute.set_format(
                [
                    MTLVertexFormat::Float3,
                    MTLVertexFormat::Float2,
                    MTLVertexFormat::Float4,
                ][index as usize],
            );
        } else if index == 8 {
            attribute.set_buffer_index(0);
            attribute.set_offset(std::mem::offset_of!(Vertex, normal) as u64);
            attribute.set_format(MTLVertexFormat::Float3);
        } else if index == 9 {
            attribute.set_buffer_index(1);
            attribute.set_offset(std::mem::offset_of!(Instance, raster_kind) as u64);
            attribute.set_format(MTLVertexFormat::UInt);
        } else {
            attribute.set_buffer_index(1);
            attribute.set_offset((index - 3) * 16);
            attribute.set_format(MTLVertexFormat::Float4);
        }
    }
    let vertices = vertex_layout.layouts().object_at(0).unwrap();
    vertices.set_stride(size_of::<Vertex>() as u64);
    vertices.set_step_function(MTLVertexStepFunction::PerVertex);
    vertices.set_step_rate(1);
    let instances = vertex_layout.layouts().object_at(1).unwrap();
    instances.set_stride(size_of::<Instance>() as u64);
    instances.set_step_function(MTLVertexStepFunction::PerInstance);
    instances.set_step_rate(1);
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_descriptor(Some(vertex_layout));
    desc.set_vertex_function(Some(vertex));
    desc.set_fragment_function(Some(fragment));
    desc.set_depth_attachment_pixel_format(DEPTH);
    let color = desc.color_attachments().object_at(0).unwrap();
    color.set_pixel_format(format);
    color.set_blending_enabled(true);
    color.set_source_rgb_blend_factor(MTLBlendFactor::SourceAlpha);
    color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    color.set_source_alpha_blend_factor(MTLBlendFactor::One);
    color.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
    device
        .new_render_pipeline_state(&desc)
        .map_err(Error::Backend)
}
