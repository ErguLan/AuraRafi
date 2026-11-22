//! ApiGraphicBasic scene device.
//!
//! The device owns exactly two execution paths:
//!
//! - [`BasicBackendType::CpuSoftware`] rasterizes the retained command list for
//!   recovery, headless hosts and agent capture.
//! - [`BasicBackendType::Native`] hands the retained [`SceneRenderFrame`] to the
//!   AGB compositor. It opens no graphics API and owns no device, target,
//!   pipeline, texture or upload.
//!
//! Presentation, resource residency and platform selection belong to the
//! compositor that owns the `raf_agb` product device.

use std::cell::RefCell;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
pub use raf_agb::backends::native::NativeApi;
#[cfg(target_arch = "wasm32")]
pub use backend::NativeApi;

use crate::api_graphic_basic::capabilities::{GraphicsCapabilities, GraphicsMemoryBudget};
use crate::api_graphic_basic::product_composition::ProductFrameBuilder;
use crate::render_pipeline::framebuffer::Framebuffer;
use crate::scene_renderer::SceneRenderFrame;

mod backend;
mod backend_cpu;
mod frame;
#[cfg(test)]
mod tests;

pub use backend::{preferred_native_api, BasicBackendType, BasicDeviceConfig};
pub use backend_cpu::capture_scene_frame_cpu;
pub use frame::native_handoff_metrics;

use backend_cpu::capture_framebuffer_rgba;

/// Scene result produced by one execution.
pub enum SceneFrameOutput {
    /// CPU recovery pixels, RGBA8 top-left origin.
    CpuPixels(Vec<u8>),
    /// Retained frame handed to the AGB compositor.
    ///
    /// The handle is an `Arc` clone of the retained frame, so the handoff copies
    /// no pixels, command list, mesh or texture.
    Scene(Arc<SceneRenderFrame>),
}

/// A compact RGBA8 snapshot of the last rendered scene frame.
///
/// This is intentionally backend-neutral: native Agent perception and artifact
/// exporters must not know whether the frame came from the CPU rasterizer or
/// from the retained frame the compositor drew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneFrameCapture {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
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
    /// Revision of the retained product composition for the last scene packet.
    pub product_packet_revision: u64,
    /// 1 when the product packet rebuilt its commands and resources, 0 when a
    /// repeated revision reused the retained packet.
    pub product_packet_rebuilds: u32,
    /// Product commands currently in the packet, including appended CAD/RafUI
    /// work. This differs from `command_count`, which counts recorded basic
    /// commands before conversion.
    pub product_command_count: u32,
    pub product_mesh_count: u32,
    pub product_texture_count: u32,
    pub product_material_count: u32,
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

/// Off-path buffers used when an agent inspects a native handoff.
struct NativeCaptureScratch {
    framebuffer: Framebuffer,
    filter_scratch: Vec<u8>,
}

impl NativeCaptureScratch {
    fn new() -> Self {
        Self {
            framebuffer: Framebuffer::new(1, 1),
            filter_scratch: Vec::new(),
        }
    }
}

/// The unified scene device driver.
///
/// It records no graphics API state: it either runs the CPU recovery
/// rasterizer or returns the retained frame for the compositor to draw.
pub struct BasicDevice {
    cpu_filter_scratch: Vec<u8>,
    backend: BasicBackendType,
    capabilities: GraphicsCapabilities,
    memory_budget: GraphicsMemoryBudget,
    framebuffer: Framebuffer,
    /// Frame retained for the compositor in native mode.
    last_scene_frame: Option<Arc<SceneRenderFrame>>,
    last_scene_frame_ready: bool,
    native_capture_scratch: RefCell<NativeCaptureScratch>,
    last_frame_metrics: SceneFrameMetrics,
    /// Retained scene half of the shared product composition. It never opens a
    /// window or owns a surface; the window compositor owns `ProductDevice`.
    product_composition: ProductFrameBuilder,
}

impl BasicDevice {
    /// Initialize the scene device for the backend its config resolves to.
    pub fn new(config: BasicDeviceConfig) -> Self {
        let backend = config.resolved_backend();
        match backend {
            BasicBackendType::CpuSoftware => {
                if config.allow_gpu && !config.force_cpu {
                    tracing::warn!(
                        "No native executor presents this target. ApiGraphicBasic uses CPU software rendering."
                    );
                } else {
                    tracing::info!("ApiGraphicBasic initialized CPU Software backend.");
                }
            }
            BasicBackendType::Native(api) => {
                tracing::info!(
                    "ApiGraphicBasic hands retained scene frames to the AGB {api} compositor."
                );
            }
        }
        Self {
            backend,
            capabilities: backend.capabilities(),
            memory_budget: config.memory_budget,
            framebuffer: Framebuffer::new(1, 1),
            cpu_filter_scratch: Vec::new(),
            last_scene_frame: None,
            last_scene_frame_ready: false,
            native_capture_scratch: RefCell::new(NativeCaptureScratch::new()),
            last_frame_metrics: SceneFrameMetrics::default(),
            product_composition: ProductFrameBuilder::default(),
        }
    }

    /// Retrieve the currently active backend.
    pub fn backend(&self) -> BasicBackendType {
        self.backend
    }

    /// Native API whose compositor consumes this device's frames, if any.
    pub fn native_api(&self) -> Option<NativeApi> {
        match self.backend {
            BasicBackendType::CpuSoftware => None,
            BasicBackendType::Native(api) => Some(api),
        }
    }

    /// Whether a graphics API draws this device's frames.
    pub fn is_gpu_active(&self) -> bool {
        self.backend.is_gpu_active()
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

    /// Encode the scene half of the product frame without presenting it.
    ///
    /// The camera travels in `FrameUniforms`, so a camera-only frame at the
    /// same revision reuses the retained packet instead of rebuilding geometry
    /// and textures. The window compositor owns the `ProductDevice` and hands
    /// the packet over with `prepare_product_scene`.
    pub fn append_product_scene(
        &mut self,
        frame: &SceneRenderFrame,
        revision: u64,
    ) -> Result<SceneFrameMetrics, String> {
        self.product_composition.append_scene(frame, revision)
    }

    /// Shared composition seam consumed by the CAD and RafUI halves and by the
    /// native compositor that draws the packet.
    pub fn product_composition(&self) -> &ProductFrameBuilder {
        &self.product_composition
    }

    pub fn product_composition_mut(&mut self) -> &mut ProductFrameBuilder {
        &mut self.product_composition
    }

    /// Execute a scene frame recorded through `BasicCommandList`.
    pub fn execute_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        let output = match self.backend {
            BasicBackendType::CpuSoftware => self.execute_cpu_scene_frame(frame),
            BasicBackendType::Native(_) => self.execute_native_scene_frame(frame),
        };
        self.last_scene_frame_ready = true;
        output
    }

    /// Read the last scene frame without changing the current editor frame.
    ///
    /// CPU frames are already resident in the software framebuffer. Native
    /// frames are re-rasterized on demand with the CPU recovery path so an
    /// agent can inspect exactly what the compositor was asked to draw.
    pub fn capture_last_scene_rgba(&self) -> Result<SceneFrameCapture, String> {
        if !self.last_scene_frame_ready {
            return Err("The graphics device has not rendered a scene frame yet.".to_string());
        }
        match self.backend {
            BasicBackendType::CpuSoftware => capture_framebuffer_rgba(&self.framebuffer),
            BasicBackendType::Native(_) => self.capture_native_scene_rgba(),
        }
    }
}
