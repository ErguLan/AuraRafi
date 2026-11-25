mod filter;
mod memory;
mod patch;
mod pipeline;
mod shadow;
mod viewport;
mod window;

use std::mem::size_of;

use windows::core::Interface;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    Graphics::{
        Direct3D::{D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST},
        Direct3D12::*,
        Dxgi::{Common::*, *},
    },
    System::Threading::{CreateEventW, WaitForSingleObject, INFINITE},
};

use crate::{
    AdapterInfo, Backend, Batch, Error, Frame, FrameMetrics, FramePlan, Instance, Output,
    RasterUniforms, ResourcePatch, Result, Vertex, Workload,
};

use memory::{buffer, copy_from_upload, copy_to_readback, footprint, image, transition, write};
use pipeline::{Pipelines, Stage};

const COLOR: DXGI_FORMAT = DXGI_FORMAT_R8G8B8A8_UNORM;
const DEPTH: DXGI_FORMAT = DXGI_FORMAT_D32_FLOAT;
const LIST: D3D12_COMMAND_LIST_TYPE = D3D12_COMMAND_LIST_TYPE_DIRECT;
// Constant buffers must be 256-byte aligned and sized; the shader frame is 160 bytes.
const CAMERA_BYTES: u64 = ((size_of::<RasterUniforms>() as u64 + 255) / 256) * 256;
const GEOMETRY_STATE: D3D12_RESOURCE_STATES = D3D12_RESOURCE_STATES(
    D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER.0 | D3D12_RESOURCE_STATE_INDEX_BUFFER.0,
);

/// Availability of an optional pass, including the reason when it is off.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectSupport {
    pub available: bool,
    pub reason: Option<&'static str>,
}

impl EffectSupport {
    pub(super) const fn yes() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    pub(super) const fn no(reason: &'static str) -> Self {
        Self {
            available: false,
            reason: Some(reason),
        }
    }
}

pub struct Dx12 {
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
    submission: [Option<ID3D12CommandList>; 1],
    fence: ID3D12Fence,
    event: Event,
    submitted: u64,
    completed: u64,
    info: AdapterInfo,
    target: Option<Target>,
    window: Option<window::WindowTarget>,
    shadow: Option<shadow::ShadowPass>,
    filter: Option<filter::FilterPass>,
    pipelines: Pipelines,
    camera: ID3D12Resource,
    shadow_capability: EffectSupport,
    filter_capability: EffectSupport,
}

pub struct Resident {
    geometry: ID3D12Resource,
    vertices: [D3D12_VERTEX_BUFFER_VIEW; 2],
    indices: Option<D3D12_INDEX_BUFFER_VIEW>,
    heaps: [Option<ID3D12DescriptorHeap>; 1],
    textures: Vec<ID3D12Resource>,
    descriptor_step: u64,
    descriptor_start: u64,
    batches: Vec<Batch>,
    shadow_batches: Vec<Batch>,
}

struct Target {
    colors: Vec<ID3D12Resource>,
    _depth: ID3D12Resource,
    _rtv: ID3D12DescriptorHeap,
    _dsv: ID3D12DescriptorHeap,
    rtvs: Vec<D3D12_CPU_DESCRIPTOR_HANDLE>,
    dsv: D3D12_CPU_DESCRIPTOR_HANDLE,
    extent: [u32; 2],
}

impl Dx12 {
    /// # Safety
    /// The Win32 handle must remain valid until the device drops.
    pub unsafe fn window(handle: raw_window_handle::RawWindowHandle) -> Result<Self> {
        let window = window::WindowTarget::new(handle)?;
        let mut backend = Self::headless()?;
        backend.window = Some(window);
        Ok(backend)
    }

    pub fn headless() -> Result<Self> {
        unsafe {
            let factory: IDXGIFactory6 =
                CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).map_err(dx_error)?;
            let (adapter, desc, device) = (0..)
                .map_while(|index| {
                    factory
                        .EnumAdapterByGpuPreference::<IDXGIAdapter1>(
                            index,
                            DXGI_GPU_PREFERENCE_MINIMUM_POWER,
                        )
                        .ok()
                })
                .find_map(|adapter| {
                    let desc = adapter.GetDesc1().ok()?;
                    if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                        return None;
                    }
                    let mut device = None;
                    D3D12CreateDevice::<_, ID3D12Device>(
                        &adapter,
                        D3D_FEATURE_LEVEL_11_0,
                        &mut device,
                    )
                    .ok()?;
                    Some((adapter, desc, device?))
                })
                .ok_or_else(|| Error::Backend("no hardware Direct3D 12 adapter".into()))?;
            let name = String::from_utf16_lossy(
                desc.Description
                    .split(|&c| c == 0)
                    .next()
                    .unwrap_or_default(),
            );
            let driver = adapter
                .CheckInterfaceSupport(&<IDXGIDevice as windows::core::Interface>::IID)
                .map(|version| format!("{version:#018x}"))
                .unwrap_or_default();
            let queue = device
                .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                    Type: LIST,
                    ..Default::default()
                })
                .map_err(dx_error)?;
            let allocator: ID3D12CommandAllocator =
                device.CreateCommandAllocator(LIST).map_err(dx_error)?;
            let list: ID3D12GraphicsCommandList = device
                .CreateCommandList(0, LIST, &allocator, None::<&ID3D12PipelineState>)
                .map_err(dx_error)?;
            list.Close().map_err(dx_error)?;
            let submission = [Some(list.cast().map_err(dx_error)?)];
            let fence = device
                .CreateFence(0, D3D12_FENCE_FLAG_NONE)
                .map_err(dx_error)?;
            let event = Event(
                CreateEventW(None, false, false, windows::core::PCWSTR::null())
                    .map_err(dx_error)?,
            );
            let pipelines = Pipelines::new(&device)?;
            let camera = buffer(
                &device,
                CAMERA_BYTES,
                D3D12_HEAP_TYPE_UPLOAD,
                D3D12_RESOURCE_STATE_GENERIC_READ,
            )?;
            let shadow_capability = shadow::capability(&device);
            let filter_capability = filter::capability(&device);
            let info = AdapterInfo {
                backend: "Direct3D 12",
                name,
                driver,
                hardware: true,
                max_texture: D3D12_REQ_TEXTURE2D_U_OR_V_DIMENSION,
                max_buffer: u32::MAX.into(),
                gpu_timestamps: false,
            };
            Ok(Self {
                device,
                queue,
                allocator,
                list,
                submission,
                fence,
                event,
                submitted: 0,
                completed: 0,
                pipelines,
                camera,
                info,
                target: None,
                window: None,
                shadow: None,
                filter: None,
                shadow_capability,
                filter_capability,
            })
        }
    }

    /// Format and shader support for the directional shadow map on this device.
    pub fn shadow_support(&self) -> EffectSupport {
        self.shadow_capability
    }

    /// Format support for the scene filter. A configured output owns the
    /// transient target, so the query is false before `configure`.
    pub fn filter_support(&self) -> EffectSupport {
        if self.target.is_none() {
            EffectSupport::no("configure an output before the scene filter")
        } else {
            self.filter_capability
        }
    }

    fn begin(&mut self) -> Result<()> {
        self.wait()?;
        unsafe {
            self.allocator.Reset().map_err(dx_error)?;
            self.list
                .Reset(&self.allocator, None::<&ID3D12PipelineState>)
                .map_err(dx_error)
        }
    }

    fn submit(&mut self) -> Result<()> {
        unsafe {
            self.list.Close().map_err(dx_error)?;
            self.queue.ExecuteCommandLists(&self.submission);
            self.submitted += 1;
            self.queue
                .Signal(&self.fence, self.submitted)
                .map_err(dx_error)
        }
    }
}

impl Backend for Dx12 {
    type Resident = Resident;
    fn info(&self) -> &AdapterInfo {
        &self.info
    }

    fn supports_shadows(&self) -> bool {
        self.shadow_capability.available && self.info.max_texture >= crate::device::SHADOW_MAP_SIDE
    }

    fn shadow_bytes(&self) -> u64 {
        self.shadow
            .as_ref()
            .map_or(0, |shadow| crate::device::shadow_map_bytes(shadow.side))
    }

    fn supports_scene_filter(&self) -> bool {
        self.filter_support().available
    }

    fn filter_bytes(&self) -> u64 {
        self.filter.as_ref().map_or(0, filter::FilterPass::bytes)
    }

    fn configure(&mut self, plan: &FramePlan) -> Result<()> {
        if self.window.is_some() != (plan.output == Output::Present) {
            return Err(Error::Invalid(
                "target output does not match Direct3D 12 surface",
            ));
        }
        self.wait()?;
        self.filter = None;
        let colors = if let Some(window) = &mut self.window {
            // ResizeBuffers requires all existing back-buffer references released.
            self.target = None;
            window.colors(&self.queue, plan.extent)?
        } else {
            vec![image(
                &self.device,
                plan.extent,
                COLOR,
                D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            )?]
        };
        let depth = image(
            &self.device,
            plan.extent,
            DEPTH,
            D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL | D3D12_RESOURCE_FLAG_DENY_SHADER_RESOURCE,
            D3D12_RESOURCE_STATE_DEPTH_WRITE,
        )?;
        let rtv = heap(
            &self.device,
            D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            colors.len() as u32,
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        )?;
        let dsv = heap(
            &self.device,
            D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
            1,
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        )?;
        unsafe {
            let start = rtv.GetCPUDescriptorHandleForHeapStart().ptr;
            let step = self
                .device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
                as usize;
            let dsv_handle = dsv.GetCPUDescriptorHandleForHeapStart();
            let rtvs = colors
                .iter()
                .enumerate()
                .map(|(index, color)| {
                    let handle = D3D12_CPU_DESCRIPTOR_HANDLE {
                        ptr: start + step * index,
                    };
                    self.device.CreateRenderTargetView(color, None, handle);
                    handle
                })
                .collect();
            self.device.CreateDepthStencilView(&depth, None, dsv_handle);
            self.target = Some(Target {
                colors,
                _depth: depth,
                _rtv: rtv,
                _dsv: dsv,
                rtvs,
                dsv: dsv_handle,
                extent: plan.extent,
            });
        }
        Ok(())
    }

    fn upload(&mut self, workload: &Workload) -> Result<Resident> {
        let vertices: &[u8] = bytemuck::cast_slice(&workload.vertices);
        let instances: &[u8] = bytemuck::cast_slice(&workload.instances);
        let indices: &[u8] = bytemuck::cast_slice(&workload.indices);
        let index_offset = vertices.len() + instances.len();
        let geometry_size = (index_offset + indices.len()) as u64;
        let upload = buffer(
            &self.device,
            geometry_size,
            D3D12_HEAP_TYPE_UPLOAD,
            D3D12_RESOURCE_STATE_GENERIC_READ,
        )?;
        write(
            &upload,
            |mapped| {
                mapped[..vertices.len()].copy_from_slice(vertices);
                mapped[vertices.len()..index_offset].copy_from_slice(instances);
                mapped[index_offset..].copy_from_slice(indices);
            },
            geometry_size as usize,
        )?;
        let geometry = buffer(
            &self.device,
            geometry_size,
            D3D12_HEAP_TYPE_DEFAULT,
            D3D12_RESOURCE_STATE_COPY_DEST,
        )?;
        let mut images = Vec::with_capacity(workload.textures.len());
        for texture in &workload.textures {
            let image = image(
                &self.device,
                texture.size,
                COLOR,
                D3D12_RESOURCE_FLAG_NONE,
                D3D12_RESOURCE_STATE_COPY_DEST,
            )?;
            let (layout, bytes) = footprint(&self.device, &image);
            let staging = buffer(
                &self.device,
                bytes,
                D3D12_HEAP_TYPE_UPLOAD,
                D3D12_RESOURCE_STATE_GENERIC_READ,
            )?;
            let row = (texture.size[0] * 4) as usize;
            let pitch = layout.Footprint.RowPitch as usize;
            write(
                &staging,
                |mapped| {
                    for (src, dst) in texture.rgba.chunks_exact(row).zip(mapped.chunks_mut(pitch)) {
                        dst[..row].copy_from_slice(src);
                    }
                },
                bytes as usize,
            )?;
            images.push((image, staging, layout));
        }
        // One extra descriptor is reserved for the directional shadow map. Direct3D 12
        // allows one shader-visible view heap per list, so scene draws bind the map
        // through this heap instead of a second one.
        let heap = heap(
            &self.device,
            D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            images.len() as u32 + 1,
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        )?;
        self.begin()?;
        let descriptor_step = unsafe {
            self.device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
        } as u64;
        unsafe {
            self.list
                .CopyBufferRegion(&geometry, 0, &upload, 0, geometry_size);
            transition(
                &self.list,
                &geometry,
                D3D12_RESOURCE_STATE_COPY_DEST,
                GEOMETRY_STATE,
            );
            let start = heap.GetCPUDescriptorHandleForHeapStart().ptr;
            for (index, (image, staging, layout)) in images.iter().enumerate() {
                copy_from_upload(&self.list, image, staging, *layout);
                transition(
                    &self.list,
                    image,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                );
                self.device.CreateShaderResourceView(
                    image,
                    None,
                    D3D12_CPU_DESCRIPTOR_HANDLE {
                        ptr: start + index * descriptor_step as usize,
                    },
                );
            }
        }
        self.submit()?;
        self.wait()?;
        let address = unsafe { geometry.GetGPUVirtualAddress() };
        let descriptor_start = unsafe { heap.GetGPUDescriptorHandleForHeapStart() }.ptr;
        Ok(Resident {
            geometry,
            vertices: [
                D3D12_VERTEX_BUFFER_VIEW {
                    BufferLocation: address,
                    SizeInBytes: vertices.len() as u32,
                    StrideInBytes: size_of::<Vertex>() as u32,
                },
                D3D12_VERTEX_BUFFER_VIEW {
                    BufferLocation: address + vertices.len() as u64,
                    SizeInBytes: instances.len() as u32,
                    StrideInBytes: size_of::<Instance>() as u32,
                },
            ],
            indices: (!indices.is_empty()).then_some(D3D12_INDEX_BUFFER_VIEW {
                BufferLocation: address + index_offset as u64,
                SizeInBytes: indices.len() as u32,
                Format: DXGI_FORMAT_R32_UINT,
            }),
            heaps: [Some(heap)],
            textures: images.into_iter().map(|(image, _, _)| image).collect(),
            descriptor_step,
            descriptor_start,
            batches: workload.batches.clone(),
            shadow_batches: workload.shadow_batches.clone(),
        })
    }

    fn patch_row_alignment(&self) -> u64 {
        u64::from(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT)
    }

    unsafe fn patch(&mut self, resident: &mut Resident, patch: ResourcePatch<'_>) -> Result<()> {
        patch::update(self, resident, patch)
    }

    /// Records scene, optional shadow map, optional scene-only filter, overlays
    /// and UI in the same order as the Vulkan executor, then submits once.
    fn draw(
        &mut self,
        resident: &Resident,
        plan: &FramePlan,
        frame: &Frame,
    ) -> Result<FrameMetrics> {
        if self.window.is_some() && self.completed < self.submitted {
            let complete = unsafe { self.fence.GetCompletedValue() };
            if complete == u64::MAX {
                return Err(Error::DeviceLost);
            }
            if complete < self.submitted {
                return Err(Error::Busy);
            }
        }
        let index = self
            .window
            .as_ref()
            .map_or(Ok(0), window::WindowTarget::current_index)?;
        self.wait()?;
        let extent = self.target.as_ref().ok_or(Error::SurfaceOutdated)?.extent;
        if frame.scene_filter {
            let recreate = self
                .filter
                .as_ref()
                .map_or(true, |pass| pass.extent() != extent);
            if recreate {
                self.filter = Some(filter::FilterPass::new(&self.device, extent)?);
            }
        } else {
            self.filter = None;
        }
        let (side, depth_key) = frame
            .shadow_map
            .map_or((crate::device::SHADOW_MAP_SIDE, None), |(side, key)| {
                (side, Some(key))
            });
        let shadow_upload = if let Some(parameters) = frame.shadow {
            if self
                .shadow
                .as_ref()
                .is_none_or(|shadow| shadow.side != side)
            {
                self.shadow = None;
                self.shadow = Some(shadow::ShadowPass::new(
                    &self.device,
                    self.pipelines.root(),
                    side,
                )?);
            }
            self.shadow.as_mut().unwrap().prepare(parameters)
        } else {
            self.shadow = None;
            0
        };
        self.begin()?;
        let target = self.target.as_ref().ok_or(Error::SurfaceOutdated)?;
        let full = viewport::PixelRect::full(target.extent);
        let scene = viewport::scene_rect(frame.viewport, target.extent);
        let list = &self.list;
        let pipelines = &self.pipelines;
        let shadow = self.shadow.as_ref();
        let filter = self.filter.as_ref();
        let uniforms = frame.uniforms(plan.extent);
        write(
            &self.camera,
            |mapped| {
                mapped[..size_of::<RasterUniforms>()].copy_from_slice(bytemuck::bytes_of(&uniforms));
            },
            size_of::<RasterUniforms>(),
        )?;
        let camera = unsafe { self.camera.GetGPUVirtualAddress() };
        let scene_end = resident
            .batches
            .partition_point(|batch| batch.mode.is_scene());
        let rtv = target.rtvs[index];
        let mut shadow_draws = 0;
        let mut filter_draws = 0;
        unsafe {
            if self.window.is_some() {
                transition(
                    list,
                    &target.colors[index],
                    D3D12_RESOURCE_STATE_PRESENT,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                );
            }
            if let Some(shadow) = shadow {
                shadow.write_view(&self.device, resident);
                if depth_key.is_none() || shadow.depth_key != depth_key {
                    shadow_draws =
                        shadow.record(list, pipelines.root(), resident, depth_key.is_some());
                }
            }
            if let Some(filter) = filter {
                filter_draws = 1;
                transition(
                    list,
                    filter.source(),
                    filter::SOURCE_STATE,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                );
                list.OMSetRenderTargets(1, Some(&filter.rtv()), false, Some(&target.dsv));
                set_view(list, full);
                list.ClearRenderTargetView(filter.rtv(), &frame.clear, None);
                list.ClearDepthStencilView(target.dsv, D3D12_CLEAR_FLAG_DEPTH, 1., 0, &[]);
                bind_scene(list, pipelines, resident, camera, shadow);
                set_view(list, scene);
                draw_batches(
                    list,
                    pipelines,
                    shadow,
                    resident,
                    &resident.batches[..scene_end],
                );
                transition(
                    list,
                    filter.source(),
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    filter::SOURCE_STATE,
                );
                // The filtered scene replaces the scene pass; only overlays and
                // UI are recorded after the fullscreen composite.
                list.OMSetRenderTargets(1, Some(&rtv), true, Some(&target.dsv));
                set_view(list, full);
                list.ClearRenderTargetView(rtv, &frame.clear, None);
                list.ClearDepthStencilView(target.dsv, D3D12_CLEAR_FLAG_DEPTH, 1., 0, &[]);
                bind_scene(list, pipelines, resident, camera, shadow);
                filter.draw(list);
                bind_scene(list, pipelines, resident, camera, shadow);
                set_view(list, full);
                draw_batches(
                    list,
                    pipelines,
                    shadow,
                    resident,
                    &resident.batches[scene_end..],
                );
            } else {
                list.OMSetRenderTargets(1, Some(&rtv), true, Some(&target.dsv));
                set_view(list, full);
                list.ClearRenderTargetView(rtv, &frame.clear, None);
                list.ClearDepthStencilView(target.dsv, D3D12_CLEAR_FLAG_DEPTH, 1., 0, &[]);
                bind_scene(list, pipelines, resident, camera, shadow);
                for pass in &plan.graph.order {
                    let (batches, rect) = match pass {
                        0 => (&resident.batches[..scene_end], scene),
                        1 => (&resident.batches[scene_end..], full),
                        _ => break,
                    };
                    set_view(list, rect);
                    draw_batches(list, pipelines, shadow, resident, batches);
                }
            }
            if self.window.is_some() {
                transition(
                    list,
                    &target.colors[index],
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATE_PRESENT,
                );
            }
        }
        self.submit()?;
        if let Some(shadow) = self.shadow.as_mut() {
            shadow.depth_key = depth_key;
        }
        if let Some(window) = &self.window {
            window.present()?;
        }
        Ok(FrameMetrics {
            draw_calls: (resident.batches.len() as u32)
                .saturating_add(shadow_draws)
                .saturating_add(filter_draws),
            submits: 1,
            upload_bytes: shadow_upload,
            shadow_draw_calls: shadow_draws,
            ..Default::default()
        })
    }

    fn wait(&mut self) -> Result<()> {
        if self.completed == self.submitted {
            return Ok(());
        }
        unsafe {
            let done = self.fence.GetCompletedValue();
            if done == u64::MAX {
                return Err(Error::DeviceLost);
            }
            if done < self.submitted {
                self.fence
                    .SetEventOnCompletion(self.submitted, self.event.0)
                    .map_err(dx_error)?;
                if WaitForSingleObject(self.event.0, INFINITE) != WAIT_OBJECT_0 {
                    return Err(Error::Backend("Direct3D 12 fence wait failed".into()));
                }
            }
        }
        self.completed = self.submitted;
        Ok(())
    }

    fn capture(&mut self) -> Result<Vec<u8>> {
        if self.window.is_some() {
            return Err(Error::Invalid(
                "capture requires an offscreen Direct3D 12 target",
            ));
        }
        self.begin()?;
        let target = self.target.as_ref().ok_or(Error::SurfaceOutdated)?;
        let extent = target.extent;
        let color = &target.colors[0];
        let (layout, bytes) = footprint(&self.device, color);
        let readback = buffer(
            &self.device,
            bytes,
            D3D12_HEAP_TYPE_READBACK,
            D3D12_RESOURCE_STATE_COPY_DEST,
        )?;
        unsafe {
            transition(
                &self.list,
                color,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            );
            copy_to_readback(&self.list, &readback, color, layout);
            transition(
                &self.list,
                color,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            );
        }
        self.submit()?;
        self.wait()?;
        memory::read_rows(
            &readback,
            bytes as usize,
            (4 * extent[0]) as usize,
            layout.Footprint.RowPitch as usize,
        )
    }
}

impl Drop for Dx12 {
    fn drop(&mut self) {
        let _ = self.wait();
    }
}

unsafe fn set_view(list: &ID3D12GraphicsCommandList, rect: viewport::PixelRect) {
    unsafe {
        list.RSSetViewports(&[rect.viewport()]);
        list.RSSetScissorRects(&[rect.scissor()]);
    }
}

unsafe fn bind_scene(
    list: &ID3D12GraphicsCommandList,
    pipelines: &Pipelines,
    resident: &Resident,
    camera: u64,
    shadow: Option<&shadow::ShadowPass>,
) {
    let shadow_slot = D3D12_GPU_DESCRIPTOR_HANDLE {
        ptr: resident.descriptor_start + resident.textures.len() as u64 * resident.descriptor_step,
    };
    unsafe {
        list.SetGraphicsRootSignature(pipelines.root());
        list.SetDescriptorHeaps(&resident.heaps);
        list.SetGraphicsRootConstantBufferView(pipeline::ROOT_FRAME_CBV, camera);
        list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        pipeline::bind_geometry(list, resident);
        if let Some(shadow) = shadow {
            shadow.bind(list);
            list.SetGraphicsRootDescriptorTable(pipeline::ROOT_SHADOW_TABLE, shadow_slot);
        }
    }
}

unsafe fn draw_batches(
    list: &ID3D12GraphicsCommandList,
    pipelines: &Pipelines,
    shadow: Option<&shadow::ShadowPass>,
    resident: &Resident,
    batches: &[Batch],
) {
    let mut last_texture = u32::MAX;
    let mut stage = None;
    unsafe {
        for batch in batches {
            let wanted = Stage::select(batch.mode, shadow.is_some());
            if stage != Some(wanted) {
                stage = Some(wanted);
                let pipeline = match wanted {
                    Stage::ShadowScene => {
                        if let Some(pass) = shadow {
                            pass.scene()
                        } else {
                            pipelines.scene()
                        }
                    }
                    Stage::ShadowSceneTwoSided => {
                        if let Some(pass) = shadow {
                            pass.scene_two_sided()
                        } else {
                            pipelines.scene_two_sided()
                        }
                    }
                    Stage::ShadowSceneTransparent => {
                        if let Some(pass) = shadow {
                            pass.scene_transparent()
                        } else {
                            pipelines.scene_transparent()
                        }
                    }
                    Stage::ShadowSceneTransparentTwoSided => {
                        if let Some(pass) = shadow {
                            pass.scene_transparent_two_sided()
                        } else {
                            pipelines.scene_transparent_two_sided()
                        }
                    }
                    Stage::Scene => pipelines.scene(),
                    Stage::SceneTwoSided => pipelines.scene_two_sided(),
                    Stage::SceneTransparent => pipelines.scene_transparent(),
                    Stage::SceneTransparentTwoSided => pipelines.scene_transparent_two_sided(),
                    Stage::ReadDepth => pipelines.read_depth(),
                    Stage::Overlay => pipelines.overlay(),
                };
                list.SetPipelineState(pipeline);
            }
            if batch.texture != last_texture {
                last_texture = batch.texture;
                list.SetGraphicsRootDescriptorTable(
                    pipeline::ROOT_MATERIAL_TABLE,
                    D3D12_GPU_DESCRIPTOR_HANDLE {
                        ptr: resident.descriptor_start
                            + u64::from(batch.texture) * resident.descriptor_step,
                    },
                );
            }
            pipeline::draw_batch(list, batch);
        }
    }
}

fn format_support(device: &ID3D12Device, format: DXGI_FORMAT) -> D3D12_FORMAT_SUPPORT1 {
    let mut support = D3D12_FEATURE_DATA_FORMAT_SUPPORT {
        Format: format,
        ..Default::default()
    };
    let size = size_of::<D3D12_FEATURE_DATA_FORMAT_SUPPORT>() as u32;
    let queried = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_FORMAT_SUPPORT,
            (&mut support as *mut D3D12_FEATURE_DATA_FORMAT_SUPPORT).cast(),
            size,
        )
    };
    if queried.is_ok() {
        support.Support1
    } else {
        D3D12_FORMAT_SUPPORT1(0)
    }
}

fn heap(
    device: &ID3D12Device,
    kind: D3D12_DESCRIPTOR_HEAP_TYPE,
    count: u32,
    flags: D3D12_DESCRIPTOR_HEAP_FLAGS,
) -> Result<ID3D12DescriptorHeap> {
    unsafe {
        device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
            Type: kind,
            NumDescriptors: count,
            Flags: flags,
            NodeMask: 0,
        })
    }
    .map_err(dx_error)
}

struct Event(HANDLE);
impl Drop for Event {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn dx_error(error: windows::core::Error) -> Error {
    match error.code() {
        DXGI_ERROR_DEVICE_REMOVED | DXGI_ERROR_DEVICE_RESET => Error::DeviceLost,
        _ => Error::Backend(error.to_string()),
    }
}
