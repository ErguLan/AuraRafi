use eframe::{egui, egui_wgpu};
use glam::Vec2;
use raf_electronics::CadScene;
use raf_render::api_graphic_basic::cad_surface::{
    build_cad_surface_frame, CadSurfaceFrame, CadSurfaceHitRegion, CadSurfaceOptions,
};
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime, RenderRuntimeSnapshot};

use super::gpu_canvas::GpuCanvas;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CadSurfaceCacheStats {
    pub frame_builds: u64,
    pub frame_cache_hits: u64,
    pub presentation_cache_hits: u64,
    pub last_frame_reused: bool,
}

/// Stable selection payload consumed directly by the CPU/GPU CAD surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CadSurfaceSelection {
    pub object_ids: Vec<String>,
    pub source_ids: Vec<[u8; 16]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CadSurfaceCacheKey {
    scene_fingerprint: u64,
    size: [u32; 2],
    world_bounds_bits: [u32; 4],
    dark_mode: bool,
    selection: CadSurfaceSelection,
}

impl CadSurfaceCacheKey {
    fn new(
        scene: &CadScene,
        size: [u32; 2],
        world_bounds: [f32; 4],
        dark_mode: bool,
        selection: &CadSurfaceSelection,
    ) -> Self {
        Self {
            scene_fingerprint: scene.stable_fingerprint(),
            size,
            world_bounds_bits: world_bounds.map(f32::to_bits),
            dark_mode,
            selection: normalized_selection(selection),
        }
    }
}

pub struct ElectronicsCadSurfaceHost {
    canvas: GpuCanvas,
    last_runtime: RenderRuntimeSnapshot,
    last_objects: usize,
    cached_frame: Option<CadSurfaceFrame>,
    cached_key: Option<CadSurfaceCacheKey>,
    cache_stats: CadSurfaceCacheStats,
    last_presented_device_generation: Option<u64>,
}

impl ElectronicsCadSurfaceHost {
    pub fn new(texture_name: &'static str) -> Self {
        Self {
            canvas: GpuCanvas::new(texture_name),
            last_runtime: RenderRuntimeSnapshot::default(),
            last_objects: 0,
            cached_frame: None,
            cached_key: None,
            cache_stats: CadSurfaceCacheStats::default(),
            last_presented_device_generation: None,
        }
    }

    pub fn present(
        &mut self,
        ctx: &egui::Context,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
        surface: GraphicsSurfaceKind,
        scene: &CadScene,
        size: [u32; 2],
        world_bounds: [f32; 4],
        dark_mode: bool,
        selection: &CadSurfaceSelection,
    ) -> &CadSurfaceFrame {
        render_runtime.activate_surface(surface);
        self.ensure_frame(scene, size, world_bounds, dark_mode, selection);
        let runtime_snapshot = render_runtime.snapshot();
        let should_present = should_submit_cached_frame(
            self.cache_stats.last_frame_reused,
            self.canvas.is_ready(),
            self.last_presented_device_generation,
            runtime_snapshot.device_generation,
        );

        if should_present {
            let render_output = {
                let frame = self
                    .cached_frame
                    .as_ref()
                    .expect("CAD surface cache must contain the current frame");
                render_runtime.render_scene_frame(&frame.frame)
            };
            self.canvas
                .present(ctx, wgpu_render_state, render_output, size[0], size[1]);
            self.last_presented_device_generation = Some(runtime_snapshot.device_generation);
        } else {
            self.cache_stats.presentation_cache_hits =
                self.cache_stats.presentation_cache_hits.saturating_add(1);
        }
        self.last_runtime = render_runtime.snapshot();
        self.cached_frame
            .as_ref()
            .expect("CAD surface cache must contain the current frame")
    }

    fn ensure_frame(
        &mut self,
        scene: &CadScene,
        size: [u32; 2],
        world_bounds: [f32; 4],
        dark_mode: bool,
        selection: &CadSurfaceSelection,
    ) -> &CadSurfaceFrame {
        let key = CadSurfaceCacheKey::new(scene, size, world_bounds, dark_mode, selection);
        let reused = self.cached_key.as_ref() == Some(&key) && self.cached_frame.is_some();

        if reused {
            self.cache_stats.frame_cache_hits = self.cache_stats.frame_cache_hits.saturating_add(1);
        } else {
            let frame = build_cad_surface_frame(
                scene,
                size[0],
                size[1],
                cad_surface_options(dark_mode, world_bounds, selection),
            );
            self.last_objects = frame.objects.len();
            self.cached_frame = Some(frame);
            self.cached_key = Some(key);
            self.cache_stats.frame_builds = self.cache_stats.frame_builds.saturating_add(1);
        }
        self.cache_stats.last_frame_reused = reused;

        self.cached_frame
            .as_ref()
            .expect("CAD surface cache must contain the current frame")
    }

    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect) {
        self.canvas.paint(painter, rect);
    }

    pub fn is_ready(&self) -> bool {
        self.canvas.is_ready()
    }

    pub fn last_runtime(&self) -> RenderRuntimeSnapshot {
        self.last_runtime
    }

    pub fn last_objects(&self) -> usize {
        self.last_objects
    }

    pub fn cache_stats(&self) -> CadSurfaceCacheStats {
        self.cache_stats
    }

    pub fn invalidate_cache(&mut self) {
        self.cached_frame = None;
        self.cached_key = None;
        self.last_presented_device_generation = None;
        self.cache_stats.last_frame_reused = false;
    }

    pub fn hit_test_world(&self, point: Vec2) -> Option<&CadSurfaceHitRegion> {
        self.cached_frame.as_ref()?.hit_test(point)
    }
}

fn normalized_selection(selection: &CadSurfaceSelection) -> CadSurfaceSelection {
    let mut normalized = selection.clone();
    normalized.object_ids.sort_unstable();
    normalized.object_ids.dedup();
    normalized.source_ids.sort_unstable();
    normalized.source_ids.dedup();
    normalized
}

fn should_submit_cached_frame(
    frame_reused: bool,
    canvas_ready: bool,
    last_device_generation: Option<u64>,
    current_device_generation: u64,
) -> bool {
    !frame_reused || !canvas_ready || last_device_generation != Some(current_device_generation)
}

impl Default for ElectronicsCadSurfaceHost {
    fn default() -> Self {
        Self::new("electronics_cad_surface")
    }
}

fn cad_surface_options(
    dark_mode: bool,
    world_bounds: [f32; 4],
    selection: &CadSurfaceSelection,
) -> CadSurfaceOptions {
    if dark_mode {
        CadSurfaceOptions {
            clear_color: [10, 10, 11, 255],
            world_bounds: Some(world_bounds),
            grid_color: [180, 186, 200, 14],
            major_grid_color: [220, 226, 240, 30],
            axis_color: [212, 119, 26, 46],
            selected_object_ids: selection.object_ids.clone(),
            selected_source_ids: selection.source_ids.clone(),
            ..CadSurfaceOptions::default()
        }
    } else {
        CadSurfaceOptions {
            clear_color: [248, 250, 253, 255],
            world_bounds: Some(world_bounds),
            grid_color: [70, 80, 96, 22],
            major_grid_color: [70, 80, 96, 42],
            axis_color: [212, 119, 26, 74],
            selected_object_ids: selection.object_ids.clone(),
            selected_source_ids: selection.source_ids.clone(),
            ..CadSurfaceOptions::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_changes_with_scene_bounds_or_theme() {
        let scene = CadScene {
            surface: raf_electronics::CadSurfaceKind::Schematic,
            objects: Vec::new(),
        };
        let selection = CadSurfaceSelection::default();
        let key = CadSurfaceCacheKey::new(
            &scene,
            [320, 200],
            [0.0, 320.0, 0.0, 200.0],
            true,
            &selection,
        );

        assert_ne!(
            key,
            CadSurfaceCacheKey::new(
                &scene,
                [320, 200],
                [20.0, 340.0, 0.0, 200.0],
                true,
                &selection,
            )
        );
        let selected = CadSurfaceSelection {
            source_ids: vec![[7; 16]],
            ..CadSurfaceSelection::default()
        };
        assert_ne!(
            key,
            CadSurfaceCacheKey::new(
                &scene,
                [320, 200],
                [0.0, 320.0, 0.0, 200.0],
                true,
                &selected,
            )
        );
        assert_ne!(
            key,
            CadSurfaceCacheKey::new(
                &scene,
                [320, 200],
                [0.0, 320.0, 0.0, 200.0],
                false,
                &selection,
            )
        );
    }

    #[test]
    fn retained_frame_reuses_matching_key_and_rebuilds_after_view_change() {
        let scene = CadScene {
            surface: raf_electronics::CadSurfaceKind::Schematic,
            objects: Vec::new(),
        };
        let mut host = ElectronicsCadSurfaceHost::new("cad_cache");
        let selection = CadSurfaceSelection::default();

        host.ensure_frame(
            &scene,
            [320, 200],
            [0.0, 320.0, 0.0, 200.0],
            true,
            &selection,
        );
        host.ensure_frame(
            &scene,
            [320, 200],
            [0.0, 320.0, 0.0, 200.0],
            true,
            &selection,
        );
        let reused = host.cache_stats();
        assert_eq!(reused.frame_builds, 1);
        assert_eq!(reused.frame_cache_hits, 1);
        assert!(reused.last_frame_reused);

        host.ensure_frame(
            &scene,
            [320, 200],
            [20.0, 340.0, 0.0, 200.0],
            true,
            &selection,
        );
        let rebuilt = host.cache_stats();
        assert_eq!(rebuilt.frame_builds, 2);
        assert!(!rebuilt.last_frame_reused);
    }

    #[test]
    fn cached_presentation_requires_a_ready_canvas_on_the_same_device() {
        assert!(should_submit_cached_frame(true, false, Some(3), 3));
        assert!(should_submit_cached_frame(true, true, Some(3), 4));
        assert!(should_submit_cached_frame(false, true, Some(3), 3));
        assert!(!should_submit_cached_frame(true, true, Some(3), 3));
    }
}
