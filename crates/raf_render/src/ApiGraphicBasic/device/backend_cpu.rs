//! CPU software execution.
//!
//! This rasterizer is the recovery contract: identical frames must produce
//! identical pixels whether the product presents them or an agent inspects them.

use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;
use crate::engine_time::Instant;
use crate::render_pipeline::framebuffer::Framebuffer;
use crate::scene_renderer::{rasterize_basic_scene_frame, SceneRenderFrame};

use super::{BasicDevice, SceneFrameCapture, SceneFrameMetrics, SceneFrameOutput};

/// Largest transient buffer the CPU scene filter may allocate.
const CPU_FILTER_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The CPU filter never takes more than 1/16 of the configured budget.
const CPU_FILTER_BUDGET_DIVISOR: u64 = 16;

impl BasicDevice {
    pub(super) fn execute_cpu_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
        let metrics = render_scene_frame_cpu(
            frame,
            &mut self.framebuffer,
            &mut self.cpu_filter_scratch,
            self.memory_budget,
        );
        self.last_frame_metrics = metrics;
        SceneFrameOutput::CpuPixels(self.framebuffer.pixels().to_vec())
    }
}

/// Render one retained scene frame with the CPU rasterizer and report metrics.
///
/// Execute, native-mode capture and the standalone capture helper share this
/// routine, so inspecting a handed-off frame never needs a second renderer.
pub(super) fn render_scene_frame_cpu(
    frame: &SceneRenderFrame,
    framebuffer: &mut Framebuffer,
    filter_scratch: &mut Vec<u8>,
    memory_budget: GraphicsMemoryBudget,
) -> SceneFrameMetrics {
    let frame_start = Instant::now();
    let shadow_start = Instant::now();
    let shadow_reused = frame
        .commands
        .shadows
        .as_ref()
        .is_some_and(|s| s.geometry.cpu_ready());
    if let Some(shadow) = frame.commands.shadows.as_ref() {
        shadow.geometry.cpu_depth();
    }
    let shadow_prepare_cpu_ms = shadow_start.elapsed().as_secs_f32() * 1000.0;
    rasterize_basic_scene_frame(frame, framebuffer);
    let filter_bytes = frame.width as u64 * frame.height as u64 * 4;
    let filter_budget = (memory_budget.gpu_bytes / CPU_FILTER_BUDGET_DIVISOR)
        .min(CPU_FILTER_MAX_BYTES);
    let filter_admitted = frame.commands.optimized_improved && filter_bytes <= filter_budget;
    if filter_admitted {
        if filter_scratch.capacity() as u64 != filter_bytes {
            *filter_scratch = Vec::new();
            filter_scratch.reserve_exact(filter_bytes as usize);
        }
        filter_scratch.resize(filter_bytes as usize, 0);
        filter_scratch.copy_from_slice(framebuffer.pixels());
        crate::optimized_filter::filter_rgba(
            filter_scratch,
            framebuffer.pixels_mut(),
            frame.width,
            frame.height,
        );
    } else {
        *filter_scratch = Vec::new();
    }
    let command_stats = frame.commands.stats();
    SceneFrameMetrics {
        optimized_filter_draw_calls: u32::from(filter_admitted),
        optimized_filter_resident_bytes: if filter_admitted {
            filter_scratch.capacity() as u64
        } else {
            0
        },
        optimized_filter_budget_exceeded: frame.commands.optimized_improved && !filter_admitted,
        frame_cpu_ms: frame_start.elapsed().as_secs_f32() * 1000.0,
        command_count: command_stats.command_count,
        mesh_instances: command_stats.mesh_instances,
        line_instances: command_stats.line_instances,
        overlay_triangles: command_stats.overlay_triangles,
        shadow_active: frame
            .commands
            .shadows
            .as_ref()
            .is_some_and(|s| !s.geometry.cpu_budget_exceeded()),
        shadow_budget_exceeded: frame.commands.shadow_budget_exceeded
            || frame
                .commands
                .shadows
                .as_ref()
                .is_some_and(|s| s.geometry.cpu_budget_exceeded()),
        shadow_resolution: frame
            .commands
            .shadows
            .as_ref()
            .map(|s| s.geometry.cpu_resolution())
            .unwrap_or(0),
        shadow_casters: frame
            .commands
            .shadows
            .as_ref()
            .map(|s| s.geometry.casters.len() as u32)
            .unwrap_or(0),
        shadow_map_rebuilds: u32::from(frame.commands.shadows.is_some() && !shadow_reused),
        shadow_cache_hits: u32::from(shadow_reused),
        shadow_resident_bytes: frame
            .commands
            .shadows
            .as_ref()
            .map(|s| (s.geometry.cpu_resolution() as u64).pow(2) * 4)
            .unwrap_or(0),
        shadow_prepare_cpu_ms,
        ..SceneFrameMetrics::default()
    }
}

/// Render a retained frame with the CPU rasterizer and return its RGBA8 capture.
///
/// This is the inspection path for frames handed to the AGB compositor. It
/// allocates only CPU memory and leaves the retained frame untouched.
pub fn capture_scene_frame_cpu(
    frame: &SceneRenderFrame,
    memory_budget: GraphicsMemoryBudget,
) -> Result<SceneFrameCapture, String> {
    let mut framebuffer = Framebuffer::new(frame.width.max(1), frame.height.max(1));
    let mut filter_scratch = Vec::new();
    render_scene_frame_cpu(frame, &mut framebuffer, &mut filter_scratch, memory_budget);
    capture_framebuffer_rgba(&framebuffer)
}

pub(super) fn capture_framebuffer_rgba(
    framebuffer: &Framebuffer,
) -> Result<SceneFrameCapture, String> {
    let width = framebuffer.width();
    let height = framebuffer.height();
    if width == 0 || height == 0 {
        return Err("The CPU scene framebuffer has no rendered pixels.".to_string());
    }
    let rgba8 = framebuffer.pixels().to_vec();
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "The CPU scene framebuffer dimensions overflowed.".to_string())?;
    if rgba8.len() != expected {
        return Err("The CPU scene framebuffer has an invalid RGBA8 length.".to_string());
    }
    Ok(SceneFrameCapture {
        width,
        height,
        rgba8,
    })
}
