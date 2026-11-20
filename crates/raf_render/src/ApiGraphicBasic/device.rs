use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use raf_core::config::RenderExecutionPolicy;

use crate::api_graphic_basic::capabilities::{
    GraphicsAdapterPreference, GraphicsBackendId, GraphicsCapabilities, GraphicsMemoryBudget,
};
use crate::api_graphic_basic::command_list::BasicMeshInstance;
use crate::api_graphic_basic::handles::{MeshHandle, TextureHandle};
use crate::api_graphic_basic::memory_ledger::GraphicsResourceLedger;
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::api_graphic_basic::resource_registry::{MeshRegistry, TextureRegistry};
use crate::engine_time::Instant;
use crate::render_pipeline::framebuffer::Framebuffer;
use crate::scene_renderer::SceneRenderFrame;

use backend_wgpu::create_scene_mesh_pipeline;
use resources::create_gpu_mesh_buffers;
use timestamps::GpuTimestampState;

pub(crate) use backend_wgpu::wgpu_timestamp_features;

mod resources;
mod frame;
mod backend_cpu;
mod backend_wgpu;
mod timestamps;
#[path = "shadow_gpu.rs"]
mod shadow_gpu;
#[cfg(test)]
mod tests;

/// Supported execution backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BasicBackendType {
    /// GPU hardware rendering through the current private WGPU adapter.
    GpuHardware,
    /// CPU recovery/software rendering.
    CpuSoftware,
}

impl BasicBackendType {
    pub const fn id(self) -> GraphicsBackendId {
        match self {
            Self::GpuHardware => GraphicsBackendId::Wgpu,
            Self::CpuSoftware => GraphicsBackendId::CpuSoftware,
        }
    }
}

pub enum SceneFrameOutput {
    CpuPixels(Vec<u8>),
    GpuTexture {
        view: GpuTextureView,
        width: u32,
        height: u32,
    },
}

/// A compact RGBA8 snapshot of the last rendered scene frame.
///
/// This is intentionally backend-neutral: native Agent perception and
/// artifact exporters must not know whether the frame came from the CPU
/// rasterizer or a private WGPU target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneFrameCapture {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

/// A backend-neutral scene texture result with a WGPU view for native
/// presentation hosts.
#[derive(Clone, Debug)]
pub struct GpuTextureView {
    view: Arc<wgpu::TextureView>,
    handle: TextureHandle,
}

impl GpuTextureView {
    /// Construct a native presentation view from a registered texture.
    pub fn from_wgpu(view: Arc<wgpu::TextureView>, handle: TextureHandle) -> Self {
        Self { view, handle }
    }

    pub fn handle(&self) -> TextureHandle {
        self.handle
    }

    /// Access the native view for the compositor pass.
    pub fn as_wgpu(&self) -> &wgpu::TextureView {
        self.view.as_ref()
    }

    pub(crate) fn arc(&self) -> Arc<wgpu::TextureView> {
        self.view.clone()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SceneFrameMetrics {
    /// One optional scene-only spatial pass, never a RafUI pass.
    pub optimized_filter_draw_calls: u32,
    pub optimized_filter_resident_bytes: u64,
    pub optimized_filter_budget_exceeded: bool,
    pub frame_cpu_ms: f32,
    pub gpu_frame_ms: f32,
    pub gpu_timing_supported: bool,
    pub gpu_timing_sampled: bool,
    pub command_count: u32,
    pub mesh_instances: u32,
    pub line_instances: u32,
    pub overlay_triangles: u32,
    pub target_rebuilds: u32,
    pub mesh_draw_calls: u32,
    pub line_draw_calls: u32,
    pub overlay_draw_calls: u32,
    pub environment_draw_calls: u32,
    pub shadow_active: bool,
    pub shadow_budget_exceeded: bool,
    pub shadow_resolution: u32,
    pub shadow_casters: u32,
    pub shadow_draw_calls: u32,
    pub shadow_map_rebuilds: u32,
    pub shadow_cache_hits: u32,
    pub shadow_resident_bytes: u64,
    pub shadow_prepare_cpu_ms: f32,
    pub mesh_cache_hits: u32,
    pub mesh_cache_misses: u32,
    pub mesh_uniform_slot_creations: u32,
    pub mesh_instance_slot_creations: u32,
    pub transient_mesh_slot_creations: u32,
    pub line_slot_creations: u32,
    pub mesh_upload_bytes: u64,
    pub texture_upload_bytes: u64,
    pub uniform_upload_bytes: u64,
    pub uniform_uploads_skipped: u64,
    pub mesh_instance_upload_bytes: u64,
    pub line_upload_bytes: u64,
    pub overlay_upload_bytes: u64,
    pub overlay_slot_creations: u32,
    pub mesh_resident_bytes: u64,
    pub mesh_resident_entries: u32,
    pub texture_resident_bytes: u64,
    pub texture_resident_entries: u32,
    pub texture_cache_evictions: u64,
    pub mesh_cache_evictions: u64,
    pub frame_upload_bytes: u64,
    pub upload_budget_exceeded: bool,
    /// Bytes tracked by the cross-family graphics memory ledger.
    pub ledger_resident_bytes: u64,
    /// Peak of the summed family residency since the device was created.
    pub ledger_peak_bytes: u64,
    /// Admissions rejected by a family or arena budget.
    pub ledger_rejected_reservations: u64,
    /// Highest family budget utilization in `0.0..=1.0`.
    pub ledger_pressure: f32,
    /// Optional texture uploads skipped because the frame upload budget was full.
    pub frame_upload_rejected: u64,
    /// Frame upload bytes divided by the frame upload budget.
    pub frame_upload_pressure: f32,
}

impl SceneFrameMetrics {
    pub const fn total_draw_calls(self) -> u32 {
        self.mesh_draw_calls
            .saturating_add(self.optimized_filter_draw_calls)
            .saturating_add(self.line_draw_calls)
            .saturating_add(self.overlay_draw_calls)
            .saturating_add(self.environment_draw_calls)
            .saturating_add(self.shadow_draw_calls)
    }

    pub const fn total_upload_bytes(self) -> u64 {
        self.mesh_upload_bytes
            .saturating_add(self.texture_upload_bytes)
            .saturating_add(self.uniform_upload_bytes)
            .saturating_add(self.mesh_instance_upload_bytes)
            .saturating_add(self.line_upload_bytes)
            .saturating_add(self.overlay_upload_bytes)
    }
}

#[derive(Debug, Clone)]
pub struct SharedGraphicsContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl SharedGraphicsContext {
    /// Explicit adapter boundary for the current native WGPU host.
    pub fn from_host(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        Self { device, queue }
    }

    fn device(&self) -> Arc<wgpu::Device> {
        self.device.clone()
    }

    fn queue(&self) -> Arc<wgpu::Queue> {
        self.queue.clone()
    }
}

/// Transitional name retained for callers that have not migrated to the
/// backend-neutral context name yet.
pub type SharedWgpuContext = SharedGraphicsContext;

/// Configuration settings for device initialization.
#[derive(Debug, Clone)]
pub struct BasicDeviceConfig {
    /// Allow GPU rendering if a GPU adapter is found.
    pub allow_gpu: bool,
    /// Force CPU software rendering regardless of GPU availability.
    pub force_cpu: bool,
    /// Optional shared wgpu context supplied by the host editor.
    pub shared_graphics_context: Option<SharedGraphicsContext>,
    /// Backend-neutral memory and frame budget.
    pub memory_budget: GraphicsMemoryBudget,
    /// Adapter preference used only when ApiGraphicBasic creates the adapter.
    pub adapter_preference: GraphicsAdapterPreference,
}

impl Default for BasicDeviceConfig {
    fn default() -> Self {
        Self {
            allow_gpu: true,
            force_cpu: false,
            shared_graphics_context: None,
            memory_budget: GraphicsMemoryBudget::default(),
            adapter_preference: GraphicsAdapterPreference::default(),
        }
    }
}

impl BasicDeviceConfig {
    /// Build a device config from the engine render execution policy.
    pub fn from_render_policy(policy: RenderExecutionPolicy) -> Self {
        match policy {
            RenderExecutionPolicy::Auto | RenderExecutionPolicy::GpuPreferred => Self {
                allow_gpu: true,
                force_cpu: false,
                shared_graphics_context: None,
                memory_budget: if matches!(policy, RenderExecutionPolicy::GpuPreferred) {
                    GraphicsMemoryBudget::desktop()
                } else {
                    GraphicsMemoryBudget::potato()
                },
                adapter_preference: if matches!(policy, RenderExecutionPolicy::GpuPreferred) {
                    GraphicsAdapterPreference::HighPerformance
                } else {
                    GraphicsAdapterPreference::LowPower
                },
            },
            RenderExecutionPolicy::CpuOnly => Self {
                allow_gpu: false,
                force_cpu: true,
                shared_graphics_context: None,
                memory_budget: GraphicsMemoryBudget::potato(),
                adapter_preference: GraphicsAdapterPreference::LowPower,
            },
        }
    }
}

/// The unified graphics device driver.
/// Orchestrates commands recording and maps them onto the active execution backend (GPU or CPU).
#[allow(dead_code)]
pub struct BasicDevice {
    cpu_filter_scratch: Vec<u8>,
    backend: BasicBackendType,
    capabilities: GraphicsCapabilities,
    memory_budget: GraphicsMemoryBudget,
    framebuffer: Framebuffer,
    gpu_scene: Option<GpuSceneState>,
    last_scene_frame_ready: bool,
    last_frame_metrics: SceneFrameMetrics,
    // Private wgpu instances (only populated if running in GPU mode)
    wgpu_instance: Option<wgpu::Instance>,
    wgpu_adapter: Option<wgpu::Adapter>,
    wgpu_device: Option<Arc<wgpu::Device>>,
    wgpu_queue: Option<Arc<wgpu::Queue>>,
}

impl BasicDevice {
    /// Initialize the basic graphics device, attempting to use the GPU backend if possible.
    pub fn new(config: BasicDeviceConfig) -> Self {
        if !config.force_cpu && config.allow_gpu {
            if let Some(shared_graphics_context) = config.shared_graphics_context {
                tracing::info!(
                    "ApiGraphicBasic initialized GPU Hardware backend using the shared native WGPU device."
                );
                let device = shared_graphics_context.device();
                let queue = shared_graphics_context.queue();
                return Self {
                    backend: BasicBackendType::GpuHardware,
                    framebuffer: Framebuffer::new(1, 1),
                    capabilities: GraphicsCapabilities::wgpu_with_timestamp_queries(
                        device.limits().max_texture_dimension_2d,
                        device.limits().max_buffer_size,
                        device.features().contains(wgpu_timestamp_features()),
                    ),
                    memory_budget: config.memory_budget,
                    gpu_scene: Some(GpuSceneState::new(&device, &queue, config.memory_budget)),
                    cpu_filter_scratch: Vec::new(),
                    last_scene_frame_ready: false,
                    last_frame_metrics: SceneFrameMetrics::default(),
                    wgpu_instance: None,
                    wgpu_adapter: None,
                    wgpu_device: Some(device),
                    wgpu_queue: Some(queue),
                };
            }
        }

        if !config.force_cpu && config.allow_gpu {
            // The primary path is native GPU rendering. CPU remains a fallback;
            // compatibility limits should not silently force the renderer down
            // to a WebGL2-era feature floor on desktop hardware.
            if let Some(gpu_state) = Self::try_init_gpu(config.adapter_preference) {
                tracing::info!(
                    "ApiGraphicBasic successfully initialized GPU Hardware backend (wgpu)."
                );
                let gpu_scene =
                    GpuSceneState::new(&gpu_state.device, &gpu_state.queue, config.memory_budget);
                return Self {
                    backend: BasicBackendType::GpuHardware,
                    capabilities: gpu_state.capabilities,
                    memory_budget: config.memory_budget,
                    framebuffer: Framebuffer::new(1, 1),
                    gpu_scene: Some(gpu_scene),
                    cpu_filter_scratch: Vec::new(),
                    last_scene_frame_ready: false,
                    last_frame_metrics: SceneFrameMetrics::default(),
                    wgpu_instance: Some(gpu_state.instance),
                    wgpu_adapter: Some(gpu_state.adapter),
                    wgpu_device: Some(Arc::new(gpu_state.device)),
                    wgpu_queue: Some(Arc::new(gpu_state.queue)),
                };
            }
            tracing::warn!(
                "ApiGraphicBasic failed to initialize GPU. Falling back to CPU Software rendering."
            );
        }

        tracing::info!("ApiGraphicBasic initialized CPU Software backend.");
        Self {
            backend: BasicBackendType::CpuSoftware,
            capabilities: GraphicsCapabilities::cpu(),
            memory_budget: config.memory_budget,
            framebuffer: Framebuffer::new(1, 1),
            gpu_scene: None,
            cpu_filter_scratch: Vec::new(),
            last_scene_frame_ready: false,
            last_frame_metrics: SceneFrameMetrics::default(),
            wgpu_instance: None,
            wgpu_adapter: None,
            wgpu_device: None,
            wgpu_queue: None,
        }
    }

    /// Retrieve the currently active backend.
    pub fn backend(&self) -> BasicBackendType {
        self.backend
    }

    pub fn capabilities(&self) -> GraphicsCapabilities {
        self.capabilities
    }

    pub fn memory_budget(&self) -> GraphicsMemoryBudget {
        self.memory_budget
    }

    pub fn last_frame_metrics(&self) -> SceneFrameMetrics {
        self.last_frame_metrics
    }

    /// Execute a scene frame recorded through `BasicCommandList`.
    pub fn execute_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        let output = match self.backend {
            BasicBackendType::GpuHardware => self.execute_gpu_scene_frame(frame),
            BasicBackendType::CpuSoftware => self.execute_cpu_scene_frame(frame),
        };
        self.last_scene_frame_ready = true;
        output
    }

    /// Read the last scene target without changing the current editor frame.
    ///
    /// CPU frames are already resident in the software framebuffer. GPU
    /// frames are copied from the private scene target through a padded
    /// staging buffer because WebGPU requires `bytes_per_row` alignment.
    pub fn capture_last_scene_rgba(&self) -> Result<SceneFrameCapture, String> {
        if !self.last_scene_frame_ready {
            return Err("The graphics device has not rendered a scene frame yet.".to_string());
        }
        match self.backend {
            BasicBackendType::CpuSoftware => {
                let width = self.framebuffer.width();
                let height = self.framebuffer.height();
                if width == 0 || height == 0 {
                    return Err("The CPU scene framebuffer has no rendered pixels.".to_string());
                }
                let rgba8 = self.framebuffer.pixels().to_vec();
                let expected = (width as usize)
                    .checked_mul(height as usize)
                    .and_then(|pixels| pixels.checked_mul(4))
                    .ok_or_else(|| {
                        "The CPU scene framebuffer dimensions overflowed.".to_string()
                    })?;
                if rgba8.len() != expected {
                    return Err(
                        "The CPU scene framebuffer has an invalid RGBA8 length.".to_string()
                    );
                }
                Ok(SceneFrameCapture {
                    width,
                    height,
                    rgba8,
                })
            }
            BasicBackendType::GpuHardware => self.capture_gpu_scene_rgba(),
        }
    }

    /// Execute the commands list and output pixel values.
    /// In CPU mode, it writes pixels using the Cohen-Sutherland clipped rasterizer.
    /// In GPU mode, it uploads buffers and issues draw calls to the graphics card.
    pub fn execute(
        &self,
        commands: &super::command_list::BasicCommandList,
        width: u32,
        height: u32,
    ) {
        match self.backend {
            BasicBackendType::GpuHardware => {
                self.execute_gpu(commands, width, height);
            }
            BasicBackendType::CpuSoftware => {
                self.execute_cpu(commands, width, height);
            }
        }
    }
}

struct GpuSceneTarget {
    width: u32,
    height: u32,
    _color_texture: wgpu::Texture,
    color_view: Arc<wgpu::TextureView>,
    _depth_texture: wgpu::Texture,
    depth_view: Arc<wgpu::TextureView>,
}

#[derive(Clone)]
struct GpuMeshBuffers {
    vertex_buffer: Arc<wgpu::Buffer>,
    index_buffer: Arc<wgpu::Buffer>,
    index_count: u32,
    vertex_bytes: u64,
    index_bytes: u64,
}

struct GpuSceneTexture {
    _texture: Arc<wgpu::Texture>,
    bind_group: Arc<wgpu::BindGroup>,
}

#[derive(Clone)]
struct GpuUniformSlot {
    buffer: Arc<wgpu::Buffer>,
    bind_group: Arc<wgpu::BindGroup>,
}

#[derive(Clone)]
struct GpuLineSlot {
    vertex_buffer: Arc<wgpu::Buffer>,
    uniform: GpuUniformSlot,
    capacity: usize,
}

#[derive(Clone)]
struct GpuOverlaySlot {
    vertex_buffer: Arc<wgpu::Buffer>,
    uniform: GpuUniformSlot,
    capacity: usize,
}

#[derive(Clone)]
struct GpuMeshInstanceSlot {
    buffer: Arc<wgpu::Buffer>,
    capacity: usize,
}

#[derive(Clone)]
struct GpuTransientMeshSlot {
    vertex_buffer: Arc<wgpu::Buffer>,
    index_buffer: Arc<wgpu::Buffer>,
    vertex_capacity: u64,
    index_capacity: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuMeshVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

impl GpuMeshVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRS: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuMeshVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuMeshInstance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
}

impl GpuMeshInstance {
    fn from_basic(instance: BasicMeshInstance) -> Self {
        Self {
            model: instance.transform.to_cols_array_2d(),
            color: rgba8_to_f32(instance.color),
        }
    }

    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRS: [wgpu::VertexAttribute; 5] = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 64,
                shader_location: 7,
            },
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuMeshInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRS,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuLineVertex {
    start: [f32; 3],
    _start_padding: f32,
    end: [f32; 3],
    _end_padding: f32,
    color: [f32; 4],
    width: f32,
    depth_bias: f32,
    _padding: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuOverlayTriangle {
    point_0: [f32; 4],
    point_1: [f32; 4],
    point_2: [f32; 4],
    color: [f32; 4],
}

impl GpuOverlayTriangle {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRS: [wgpu::VertexAttribute; 4] = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 48,
                shader_location: 3,
            },
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuOverlayTriangle>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRS,
        }
    }
}

impl GpuLineVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        // `GpuLineVertex` deliberately pads each vec3 to a 16-byte boundary.
        // `vertex_attr_array!` packs attributes back-to-back and therefore
        // cannot describe this struct: it would read color bytes as width and
        // alpha as depth bias, clipping every opaque CAD line on the GPU.
        const ATTRS: [wgpu::VertexAttribute; 5] = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 16,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 48,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 52,
                shader_location: 4,
            },
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuLineVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRS,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct MeshUniforms {
    mvp: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
    color: [f32; 4],
    light_dir: [f32; 4],
    params: [f32; 4],
    sun: [f32; 4],
    ambient: [f32; 4],
}

#[derive(Debug)]
struct UniformReuse<T> {
    values: Vec<Option<T>>,
}

impl<T> Default for UniformReuse<T> {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}

impl<T> UniformReuse<T>
where
    T: Copy + PartialEq,
{
    fn should_upload(&mut self, draw_index: usize, value: T) -> bool {
        if self
            .values
            .get(draw_index)
            .and_then(|value| *value)
            .is_some_and(|previous| previous == value)
        {
            return false;
        }

        if self.values.len() <= draw_index {
            self.values.resize(draw_index + 1, None);
        }
        self.values[draw_index] = Some(value);
        true
    }
}

fn write_uniform_if_changed<T>(
    queue: &wgpu::Queue,
    slot: &GpuUniformSlot,
    reuse: &mut UniformReuse<T>,
    draw_index: usize,
    value: T,
    metrics: &mut SceneFrameMetrics,
) where
    T: bytemuck::Pod + PartialEq,
{
    if !reuse.should_upload(draw_index, value) {
        metrics.uniform_uploads_skipped = metrics.uniform_uploads_skipped.saturating_add(1);
        return;
    }

    queue.write_buffer(&slot.buffer, 0, bytemuck::bytes_of(&value));
    metrics.uniform_upload_bytes = metrics
        .uniform_upload_bytes
        .saturating_add(std::mem::size_of::<T>() as u64);
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct LineUniforms {
    mvp: [[f32; 4]; 4],
    viewport: [f32; 2],
    _padding: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct OverlayUniforms {
    viewport: [f32; 2],
    _padding: [f32; 2],
}

struct GpuSceneState {
    optimized_filter: Option<super::optimized_filter_gpu::OptimizedFilterGpu>,
    optimized_filter_memory_budget: u64,
    shadows: Option<shadow_gpu::ShadowGpuState>,
    shadow_receiver: bool,
    shadow_memory_budget: u64,
    environment: Option<super::environment_gpu::EnvironmentGpuState>,
    color_format: wgpu::TextureFormat,
    mesh_bind_group_layout: wgpu::BindGroupLayout,
    scene_texture_bind_group_layout: wgpu::BindGroupLayout,
    scene_texture_sampler: wgpu::Sampler,
    ui_texture_sampler: wgpu::Sampler,
    fallback_texture_bind_group: Arc<wgpu::BindGroup>,
    line_bind_group_layout: wgpu::BindGroupLayout,
    overlay_bind_group_layout: wgpu::BindGroupLayout,
    mesh_pipeline: wgpu::RenderPipeline,
    mesh_two_sided_pipeline: wgpu::RenderPipeline,
    ui_surface_pipeline: Option<wgpu::RenderPipeline>,
    ui_overlay_pipeline: Option<wgpu::RenderPipeline>,
    ui_mode: Option<bool>,
    ui_premultiplied: bool,
    mesh_instanced_pipeline: wgpu::RenderPipeline,
    mesh_instanced_two_sided_pipeline: wgpu::RenderPipeline,
    line_pipeline_depth: wgpu::RenderPipeline,
    line_pipeline_xray: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    mesh_cache: HashMap<usize, MeshHandle>,
    mesh_registry: MeshRegistry<GpuMeshBuffers>,
    mesh_cache_limit: usize,
    texture_registry: TextureRegistry<GpuSceneTexture>,
    texture_handles: HashMap<PathBuf, TextureHandle>,
    external_texture_bindings: HashMap<usize, (GpuTextureView, Arc<wgpu::BindGroup>)>,
    texture_cache_limit: usize,
    texture_cache_budget_bytes: u64,
    mesh_uniform_reuse: UniformReuse<MeshUniforms>,
    line_uniform_reuse: UniformReuse<LineUniforms>,
    overlay_uniform_reuse: UniformReuse<OverlayUniforms>,
    mesh_uniform_slots: Vec<GpuUniformSlot>,
    mesh_instance_slots: Vec<GpuMeshInstanceSlot>,
    transient_mesh_slots: Vec<GpuTransientMeshSlot>,
    line_slots: Vec<GpuLineSlot>,
    overlay_slots: Vec<GpuOverlaySlot>,
    transient_vertex_scratch: Vec<GpuMeshVertex>,
    line_vertex_scratch: Vec<GpuLineVertex>,
    overlay_vertex_scratch: Vec<GpuOverlayTriangle>,
    mesh_instance_scratch: Vec<GpuMeshInstance>,
    target: Option<GpuSceneTarget>,
    target_generation: u32,
    frame_index: u64,
    frame_upload_budget: u64,
    frame_upload_rejected: u64,
    upload_pressure: f32,
    ledger: GraphicsResourceLedger,
    gpu_timing: Option<GpuTimestampState>,
}

fn rgba8_to_f32(color: [u8; 4]) -> [f32; 4] {
    [
        crate::post_process::srgb_to_linear(color[0] as f32 / 255.0),
        crate::post_process::srgb_to_linear(color[1] as f32 / 255.0),
        crate::post_process::srgb_to_linear(color[2] as f32 / 255.0),
        color[3] as f32 / 255.0,
    ]
}
