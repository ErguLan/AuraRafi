mod pipeline;
mod window;

use std::ptr;

use metal::*;
use objc::rc::autoreleasepool;

use crate::{
    device::ViewportRect, AdapterInfo, Backend, Batch, DrawMode, Error, Frame, FrameMetrics,
    FramePlan, Instance, Output, ResourcePatch, Result, Vertex, Workload,
};

use pipeline::Pipelines;

const COLOR: MTLPixelFormat = MTLPixelFormat::RGBA8Unorm;
const DEPTH: MTLPixelFormat = MTLPixelFormat::Depth32Float;
const CAMERA_SLOT: u8 = 2;
const BLIT_ROW_ALIGNMENT: usize = 256;
const ORIGIN: MTLOrigin = MTLOrigin { x: 0, y: 0, z: 0 };

pub struct Metal {
    device: metal::Device,
    queue: CommandQueue,
    pipelines: Pipelines,
    sampler: SamplerState,
    pending: Option<CommandBuffer>,
    target: Option<Target>,
    window: Option<window::WindowLayer>,
    info: AdapterInfo,
}

pub struct Resident {
    vertices: Buffer,
    instances: Buffer,
    indices: Option<Buffer>,
    textures: Vec<Texture>,
    batches: Vec<Batch>,
}
struct Target {
    color: Option<Texture>,
    depth: Texture,
    extent: [u32; 2],
}

impl Metal {
    pub fn headless() -> Result<Self> {
        Self::open(None)
    }

    /// # Safety
    /// The AppKit or UIKit view must remain valid until the device drops. Use
    /// the owning UI thread to create, use and drop a windowed device.
    pub unsafe fn window(handle: raw_window_handle::RawWindowHandle) -> Result<Self> {
        Self::open(Some(handle))
    }

    fn open(handle: Option<raw_window_handle::RawWindowHandle>) -> Result<Self> {
        autoreleasepool(|| {
            #[cfg(target_os = "macos")]
            let device = metal::Device::all()
                .into_iter()
                .min_by_key(|device| !device.is_low_power());
            #[cfg(not(target_os = "macos"))]
            let device = metal::Device::system_default();
            let device = device.ok_or_else(|| Error::Backend("no Metal adapter".into()))?;
            let max_texture = if cfg!(target_os = "macos") {
                16_384
            } else {
                8192
            };
            let info = AdapterInfo {
                backend: "Metal",
                name: device.name().into(),
                driver: String::new(),
                hardware: true,
                max_texture,
                max_buffer: device.max_buffer_length(),
                gpu_timestamps: false,
            };
            let queue = device.new_command_queue();
            let format = if handle.is_some() {
                window::FORMAT
            } else {
                COLOR
            };
            let pipelines = Pipelines::new(&device, format)?;
            let window = handle
                .map(|handle| unsafe { window::WindowLayer::attach(handle, &device) })
                .transpose()?;
            let desc = SamplerDescriptor::new();
            desc.set_address_mode_s(MTLSamplerAddressMode::Repeat);
            desc.set_address_mode_t(MTLSamplerAddressMode::Repeat);
            desc.set_min_filter(MTLSamplerMinMagFilter::Linear);
            desc.set_mag_filter(MTLSamplerMinMagFilter::Linear);
            let sampler = device.new_sampler(&desc);
            Ok(Self {
                device,
                queue,
                pipelines,
                sampler,
                pending: None,
                target: None,
                window,
                info,
            })
        })
    }
}

impl Backend for Metal {
    type Resident = Resident;
    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn configure(&mut self, plan: &FramePlan) -> Result<()> {
        if self.window.is_some() != (plan.output == Output::Present) {
            return Err(Error::Invalid("target output does not match Metal view"));
        }
        self.wait()?;
        if let Some(window) = &self.window {
            window.configure(plan.extent);
        }
        self.target = Some(autoreleasepool(|| Target {
            color: self.window.is_none().then(|| {
                texture(
                    &self.device,
                    plan.extent,
                    COLOR,
                    MTLTextureUsage::RenderTarget,
                )
            }),
            depth: texture(
                &self.device,
                plan.extent,
                DEPTH,
                MTLTextureUsage::RenderTarget,
            ),
            extent: plan.extent,
        }));
        Ok(())
    }

    fn upload(&mut self, workload: &Workload) -> Result<Resident> {
        self.wait()?;
        autoreleasepool(|| {
            let vertex = bytemuck::cast_slice(&workload.vertices);
            let instance = bytemuck::cast_slice(&workload.instances);
            let vertices = buffer(&self.device, vertex);
            let instances = buffer(&self.device, instance);
            let indices = (!workload.indices.is_empty())
                .then(|| buffer(&self.device, bytemuck::cast_slice(&workload.indices)));
            let command = self.queue.new_command_buffer();
            let blit = command.new_blit_command_encoder();
            let mut textures = Vec::with_capacity(workload.textures.len());
            for source in &workload.textures {
                let tex = texture(
                    &self.device,
                    source.size,
                    COLOR,
                    MTLTextureUsage::ShaderRead,
                );
                let row = source.size[0] as usize * 4;
                let pitch = row.div_ceil(BLIT_ROW_ALIGNMENT) * BLIT_ROW_ALIGNMENT;
                let mut data = vec![0; pitch * source.size[1] as usize];
                for (dst, src) in data.chunks_mut(pitch).zip(source.rgba.chunks_exact(row)) {
                    dst[..row].copy_from_slice(src);
                }
                let staging = buffer(&self.device, &data);
                blit.copy_from_buffer_to_texture(
                    &staging,
                    0,
                    pitch as u64,
                    0,
                    extent(source.size),
                    &tex,
                    0,
                    0,
                    ORIGIN,
                    MTLBlitOption::empty(),
                );
                textures.push(tex);
            }
            blit.end_encoding();
            command.commit();
            finish(command)?;
            Ok(Resident {
                vertices,
                instances,
                indices,
                textures,
                batches: workload.batches.clone(),
            })
        })
    }

    fn patch_row_alignment(&self) -> u64 {
        BLIT_ROW_ALIGNMENT as u64
    }

    unsafe fn patch(&mut self, resident: &mut Resident, patch: ResourcePatch<'_>) -> Result<()> {
        self.wait()?;
        match patch {
            ResourcePatch::Vertices { first, data } => write_buffer(
                &resident.vertices,
                first as usize * size_of::<Vertex>(),
                bytemuck::cast_slice(data),
            ),
            ResourcePatch::Instances { first, data } => write_buffer(
                &resident.instances,
                first as usize * size_of::<Instance>(),
                bytemuck::cast_slice(data),
            ),
            ResourcePatch::Texture {
                index,
                origin,
                size,
                rgba,
            } => autoreleasepool(|| {
                let row = size[0] as usize * 4;
                let pitch = row.div_ceil(BLIT_ROW_ALIGNMENT) * BLIT_ROW_ALIGNMENT;
                let staging = self.device.new_buffer(
                    (pitch * size[1] as usize) as u64,
                    MTLResourceOptions::StorageModeShared,
                );
                for (y, source) in rgba.chunks_exact(row).enumerate() {
                    write_buffer(&staging, y * pitch, source)?;
                }
                let command = self.queue.new_command_buffer();
                let blit = command.new_blit_command_encoder();
                blit.copy_from_buffer_to_texture(
                    &staging,
                    0,
                    pitch as u64,
                    0,
                    extent(size),
                    &resident.textures[index as usize],
                    0,
                    0,
                    MTLOrigin {
                        x: origin[0].into(),
                        y: origin[1].into(),
                        z: 0,
                    },
                    MTLBlitOption::empty(),
                );
                blit.end_encoding();
                command.commit();
                finish(command)
            }),
        }
    }

    fn draw(
        &mut self,
        resident: &Resident,
        plan: &FramePlan,
        frame: &Frame,
    ) -> Result<FrameMetrics> {
        if self.window.is_some()
            && self.pending.as_ref().is_some_and(|command| {
                !matches!(
                    command.status(),
                    MTLCommandBufferStatus::Completed | MTLCommandBufferStatus::Error
                )
            })
        {
            return Err(Error::Busy);
        }
        self.wait()?;
        let target = self.target.as_ref().ok_or(Error::SurfaceOutdated)?;
        let command = autoreleasepool(|| -> Result<CommandBuffer> {
            let drawable = self
                .window
                .as_ref()
                .map(|window| window.layer.next_drawable().ok_or(Error::Busy))
                .transpose()?;
            let destination = drawable
                .map(|drawable| drawable.texture())
                .or(target.color.as_deref())
                .ok_or(Error::SurfaceOutdated)?;
            let pass = RenderPassDescriptor::new();
            let color = pass.color_attachments().object_at(0).unwrap();
            color.set_texture(Some(destination));
            color.set_load_action(MTLLoadAction::Clear);
            color.set_store_action(MTLStoreAction::Store);
            let [r, g, b, a] = frame.clear.map(f64::from);
            color.set_clear_color(MTLClearColor::new(r, g, b, a));
            let depth = pass.depth_attachment().unwrap();
            depth.set_texture(Some(&target.depth));
            depth.set_load_action(MTLLoadAction::Clear);
            depth.set_store_action(MTLStoreAction::DontCare);
            depth.set_clear_depth(1.);
            let command = self.queue.new_command_buffer();
            let render = command.new_render_command_encoder(pass);
            render.set_vertex_buffer(0, Some(&resident.vertices), 0);
            render.set_vertex_buffer(1, Some(&resident.instances), 0);
            let uniforms = frame.uniforms(plan.extent);
            render.set_vertex_bytes(
                CAMERA_SLOT.into(),
                size_of_val(&uniforms) as u64,
                ptr::from_ref(&uniforms).cast(),
            );
            render.set_fragment_sampler_state(0, Some(&self.sampler));
            let full = ViewportRect::full(plan.extent);
            let scene = frame.viewport_rect(plan.extent);
            set_viewport(render, full);
            let scene_end = resident
                .batches
                .partition_point(|batch| batch.mode.is_scene());
            let mut last_texture = u32::MAX;
            for index in &plan.graph.order {
                let batches = match index {
                    0 => &resident.batches[..scene_end],
                    1 => &resident.batches[scene_end..],
                    _ => break,
                };
                let mut bound = None;
                for batch in batches {
                    if bound != Some(batch.mode) {
                        bound = Some(batch.mode);
                        let (pipeline, depth) = match batch.mode {
                            DrawMode::Scene => (&self.pipelines.scene, &self.pipelines.scene_depth),
                            DrawMode::DepthReadOnly => {
                                (&self.pipelines.scene, &self.pipelines.read_depth)
                            }
                            DrawMode::Overlay | DrawMode::SceneNoDepth => {
                                (&self.pipelines.overlay, &self.pipelines.overlay_depth)
                            }
                        };
                        render.set_render_pipeline_state(pipeline);
                        render.set_depth_stencil_state(depth);
                        let target = ViewportRect::for_mode(scene, batch.mode, plan.extent);
                        set_viewport(render, target);
                    }
                    if batch.texture != last_texture {
                        last_texture = batch.texture;
                        render.set_fragment_texture(
                            0,
                            Some(&resident.textures[batch.texture as usize]),
                        );
                    }
                    if let Some(indices) = &batch.indices {
                        render.draw_indexed_primitives_instanced_base_instance(
                            MTLPrimitiveType::Triangle,
                            indices.len() as u64,
                            MTLIndexType::UInt32,
                            resident.indices.as_ref().expect("validated index stream"),
                            u64::from(indices.start) * size_of::<u32>() as u64,
                            batch.instances.len() as u64,
                            0,
                            batch.instances.start.into(),
                        );
                    } else {
                        render.draw_primitives_instanced_base_instance(
                            MTLPrimitiveType::Triangle,
                            batch.vertices.start.into(),
                            batch.vertices.len() as u64,
                            batch.instances.len() as u64,
                            batch.instances.start.into(),
                        );
                    }
                }
            }
            render.end_encoding();
            if let Some(drawable) = drawable {
                command.present_drawable(drawable);
            }
            command.commit();
            Ok(command.to_owned())
        })?;
        self.pending = Some(command);
        Ok(FrameMetrics {
            draw_calls: resident.batches.len() as u32,
            submits: 1,
            ..Default::default()
        })
    }

    fn wait(&mut self) -> Result<()> {
        match self.pending.take() {
            Some(command) => finish(&command),
            None => Ok(()),
        }
    }

    fn capture(&mut self) -> Result<Vec<u8>> {
        self.wait()?;
        let target = self.target.as_ref().ok_or(Error::SurfaceOutdated)?;
        let color = target
            .color
            .as_ref()
            .ok_or(Error::Invalid("capture requires an offscreen Metal target"))?;
        autoreleasepool(|| {
            let row = target.extent[0] as usize * 4;
            let pitch = row.div_ceil(BLIT_ROW_ALIGNMENT) * BLIT_ROW_ALIGNMENT;
            let bytes = pitch * target.extent[1] as usize;
            let buffer = self
                .device
                .new_buffer(bytes as u64, MTLResourceOptions::StorageModeShared);
            let command = self.queue.new_command_buffer();
            let blit = command.new_blit_command_encoder();
            blit.copy_from_texture_to_buffer(
                color,
                0,
                0,
                ORIGIN,
                extent(target.extent),
                &buffer,
                0,
                pitch as u64,
                0,
                MTLBlitOption::empty(),
            );
            blit.end_encoding();
            command.commit();
            finish(command)?;
            let mut pixels = Vec::with_capacity(row * target.extent[1] as usize);
            for offset in (0..bytes).step_by(pitch) {
                // Only read the bytes written by the blit, excluding row padding.
                pixels.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(buffer.contents().cast::<u8>().add(offset), row)
                });
            }
            Ok(pixels)
        })
    }
}

impl Drop for Metal {
    fn drop(&mut self) {
        let _ = self.wait();
    }
}

fn texture(
    device: &metal::DeviceRef,
    [w, h]: [u32; 2],
    format: MTLPixelFormat,
    usage: MTLTextureUsage,
) -> Texture {
    let desc = TextureDescriptor::new();
    desc.set_pixel_format(format);
    desc.set_width(w.into());
    desc.set_height(h.into());
    desc.set_usage(usage);
    desc.set_storage_mode(MTLStorageMode::Private);
    device.new_texture(&desc)
}

fn buffer(device: &metal::DeviceRef, bytes: &[u8]) -> Buffer {
    device.new_buffer_with_data(
        bytes.as_ptr().cast(),
        bytes.len() as u64,
        MTLResourceOptions::StorageModeShared,
    )
}

fn write_buffer(buffer: &BufferRef, offset: usize, bytes: &[u8]) -> Result<()> {
    let address = buffer.contents().cast::<u8>();
    if address.is_null()
        || offset as u64 > buffer.length()
        || bytes.len() as u64 > buffer.length() - offset as u64
    {
        return Err(Error::Invalid("invalid Metal shared buffer range"));
    }
    // The buffers use shared storage and the previous submit has completed.
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), address.add(offset), bytes.len()) };
    Ok(())
}

fn extent([w, h]: [u32; 2]) -> MTLSize {
    MTLSize {
        width: w.into(),
        height: h.into(),
        depth: 1,
    }
}

fn set_viewport(render: &RenderCommandEncoderRef, rect: ViewportRect) {
    // Metal uses the same top-left origin. The rectangle was already clamped
    // to the configured drawable extent by the shared conversion.
    render.set_viewport(MTLViewport {
        originX: f64::from(rect.x),
        originY: f64::from(rect.y),
        width: f64::from(rect.width),
        height: f64::from(rect.height),
        znear: 0.,
        zfar: 1.,
    });
    let [x, y, width, height] = rect.pixels;
    render.set_scissor_rect(x.into(), y.into(), width.into(), height.into());
}

fn finish(command: &CommandBufferRef) -> Result<()> {
    command.wait_until_completed();
    if command.status() == MTLCommandBufferStatus::Completed {
        Ok(())
    } else {
        Err(Error::Backend(format!(
            "Metal command ended with status {:?}",
            command.status()
        )))
    }
}
