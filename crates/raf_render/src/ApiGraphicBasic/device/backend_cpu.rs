use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::engine_time::Instant;
use crate::scene_renderer::{rasterize_basic_scene_frame, SceneRenderFrame};

use super::{BasicDevice, SceneFrameMetrics, SceneFrameOutput};

impl BasicDevice {
    /// Internal CPU software execution mapping commands onto our clipping rasterizer.
    pub(super) fn execute_cpu(
        &self,
        _commands: &BasicCommandList,
        _width: u32,
        _height: u32,
    ) {
        // CPU execution implementation details (encapsulated internally)
    }

    pub(super) fn execute_cpu_scene_frame(&mut self, frame: &SceneRenderFrame) -> SceneFrameOutput {
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
        rasterize_basic_scene_frame(frame, &mut self.framebuffer);
        let filter_bytes = frame.width as u64 * frame.height as u64 * 4;
        let filter_admitted = frame.commands.optimized_improved
            && filter_bytes <= (self.memory_budget.gpu_bytes / 16).min(64 * 1024 * 1024);
        if filter_admitted {
            if self.cpu_filter_scratch.capacity() as u64 != filter_bytes {
                self.cpu_filter_scratch = Vec::new();
                self.cpu_filter_scratch.reserve_exact(filter_bytes as usize);
            }
            self.cpu_filter_scratch.resize(filter_bytes as usize, 0);
            self.cpu_filter_scratch
                .copy_from_slice(self.framebuffer.pixels());
            crate::optimized_filter::filter_rgba(
                &self.cpu_filter_scratch,
                self.framebuffer.pixels_mut(),
                frame.width,
                frame.height,
            );
        } else {
            self.cpu_filter_scratch = Vec::new();
        }
        let command_stats = frame.commands.stats();
        self.last_frame_metrics = SceneFrameMetrics {
            optimized_filter_draw_calls: u32::from(filter_admitted),
            optimized_filter_resident_bytes: if filter_admitted {
                self.cpu_filter_scratch.capacity() as u64
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
        };
        SceneFrameOutput::CpuPixels(self.framebuffer.pixels().to_vec())
    }
}
