use eframe::{egui, egui_wgpu};
use glam::Vec2;
use raf_electronics::CadScene;
use raf_render::api_graphic_basic::cad_surface::{
    build_cad_surface_frame, CadSurfaceFrame, CadSurfaceHitRegion, CadSurfaceOptions,
};
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime, RenderRuntimeSnapshot};

use super::gpu_canvas::canvas_view_projection;
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
    #[cfg(test)]
    fn new(
        scene: &CadScene,
        size: [u32; 2],
        world_bounds: [f32; 4],
        dark_mode: bool,
        selection: &CadSurfaceSelection,
    ) -> Self {
        Self::with_revision(
            scene,
            scene.stable_fingerprint(),
            size,
            world_bounds,
            dark_mode,
            selection,
        )
    }

    fn with_revision(
        _scene: &CadScene,
        scene_revision: u64,
        size: [u32; 2],
        world_bounds: [f32; 4],
        dark_mode: bool,
        selection: &CadSurfaceSelection,
    ) -> Self {
        Self {
            scene_fingerprint: scene_revision,
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
    cached_cull_bounds: Option<[f32; 4]>,
    cache_stats: CadSurfaceCacheStats,
    last_presented_device_generation: Option<u64>,
    last_presented_surface_generation: Option<u64>,
}

impl ElectronicsCadSurfaceHost {
    pub fn new(texture_name: &'static str) -> Self {
        Self {
            canvas: GpuCanvas::new(texture_name),
            last_runtime: RenderRuntimeSnapshot::default(),
            last_objects: 0,
            cached_frame: None,
            cached_key: None,
            cached_cull_bounds: None,
            cache_stats: CadSurfaceCacheStats::default(),
            last_presented_device_generation: None,
            last_presented_surface_generation: None,
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
        let scene_revision = scene.stable_fingerprint();
        self.present_with_revision(
            ctx,
            wgpu_render_state,
            render_runtime,
            surface,
            scene,
            scene_revision,
            size,
            world_bounds,
            dark_mode,
            selection,
        )
    }

    /// Presents a CAD scene with a caller-owned revision.
    ///
    /// Editor documents already maintain a monotonic revision when a real
    /// edit commits. Reusing it avoids hashing every CAD object on every idle
    /// frame while preserving the same cached AGB/WGPU presentation path.
    pub fn present_with_revision(
        &mut self,
        ctx: &egui::Context,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
        surface: GraphicsSurfaceKind,
        scene: &CadScene,
        scene_revision: u64,
        size: [u32; 2],
        world_bounds: [f32; 4],
        dark_mode: bool,
        selection: &CadSurfaceSelection,
    ) -> &CadSurfaceFrame {
        render_runtime.activate_surface(surface);
        self.ensure_frame(
            scene,
            scene_revision,
            size,
            world_bounds,
            dark_mode,
            selection,
        );
        let runtime_snapshot = render_runtime.snapshot();
        let should_present = should_submit_cached_frame(
            self.cache_stats.last_frame_reused,
            self.canvas.is_ready(),
            self.last_presented_device_generation,
            runtime_snapshot.device_generation,
            self.last_presented_surface_generation,
            runtime_snapshot.surface_generation,
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
            self.last_presented_surface_generation = Some(runtime_snapshot.surface_generation);
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
        scene_revision: u64,
        size: [u32; 2],
        world_bounds: [f32; 4],
        dark_mode: bool,
        selection: &CadSurfaceSelection,
    ) -> &CadSurfaceFrame {
        let key = CadSurfaceCacheKey::with_revision(
            scene,
            scene_revision,
            size,
            world_bounds,
            dark_mode,
            selection,
        );
        let exact_reuse = self.cached_key.as_ref() == Some(&key) && self.cached_frame.is_some();
        let geometry_reuse = !exact_reuse
            && self.cached_frame.is_some()
            && self
                .cached_key
                .as_ref()
                .is_some_and(|cached| cached.same_geometry(&key))
            && self
                .cached_cull_bounds
                .is_some_and(|cull_bounds| bounds_contains(cull_bounds, world_bounds));

        if exact_reuse {
            self.cache_stats.frame_cache_hits = self.cache_stats.frame_cache_hits.saturating_add(1);
        } else if geometry_reuse {
            // Camera motion changes only the projection. Keep the AGB command
            // list and hit regions alive while the new view is inside the
            // expanded culling window. This is the hot path for pan/zoom.
            if let Some(frame) = self.cached_frame.as_mut() {
                frame.frame.view_proj = canvas_view_projection(
                    world_bounds[0],
                    world_bounds[1],
                    world_bounds[2],
                    world_bounds[3],
                );
                frame.frame.width = size[0];
                frame.frame.height = size[1];
            }
            self.cached_key = Some(key);
            self.cache_stats.frame_cache_hits = self.cache_stats.frame_cache_hits.saturating_add(1);
        } else {
            let cull_bounds = expanded_cull_bounds(world_bounds);
            let mut frame = build_cad_surface_frame(
                scene,
                size[0],
                size[1],
                cad_surface_options(dark_mode, cull_bounds, selection),
            );
            // The expanded bounds are only for conservative object culling;
            // the first presentation must still use the camera requested by
            // the editor, otherwise the initial view would be zoomed out.
            frame.frame.view_proj = canvas_view_projection(
                world_bounds[0],
                world_bounds[1],
                world_bounds[2],
                world_bounds[3],
            );
            self.last_objects = frame.objects.len();
            self.cached_frame = Some(frame);
            self.cached_key = Some(key);
            self.cached_cull_bounds = Some(cull_bounds);
            self.cache_stats.frame_builds = self.cache_stats.frame_builds.saturating_add(1);
        }
        // A geometry reuse still needs a presentation because the camera
        // matrix changed. `last_frame_reused` means the exact presented frame
        // can be skipped, not merely that its command list was reusable.
        self.cache_stats.last_frame_reused = exact_reuse;

        self.cached_frame
            .as_ref()
            .expect("CAD surface cache must contain the current frame")
    }

    pub fn paint(&mut self, painter: &egui::Painter, rect: egui::Rect) {
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
        self.cached_cull_bounds = None;
        self.last_presented_device_generation = None;
        self.last_presented_surface_generation = None;
        self.cache_stats.last_frame_reused = false;
    }

    pub fn hit_test_world(&self, point: Vec2) -> Option<&CadSurfaceHitRegion> {
        self.cached_frame.as_ref()?.hit_test(point)
    }
}

impl CadSurfaceCacheKey {
    fn same_geometry(&self, other: &Self) -> bool {
        self.scene_fingerprint == other.scene_fingerprint
            && self.size == other.size
            && self.dark_mode == other.dark_mode
            && self.selection == other.selection
    }
}

fn expanded_cull_bounds(bounds: [f32; 4]) -> [f32; 4] {
    let width = (bounds[1] - bounds[0]).abs().max(1.0);
    let height = (bounds[3] - bounds[2]).abs().max(1.0);
    let horizontal_pad = (width * 0.75).max(120.0);
    let vertical_pad = (height * 0.75).max(120.0);
    [
        bounds[0] - horizontal_pad,
        bounds[1] + horizontal_pad,
        bounds[2] - vertical_pad,
        bounds[3] + vertical_pad,
    ]
}

fn bounds_contains(container: [f32; 4], bounds: [f32; 4]) -> bool {
    container[0] <= bounds[0]
        && container[1] >= bounds[1]
        && container[2] <= bounds[2]
        && container[3] >= bounds[3]
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
    last_surface_generation: Option<u64>,
    current_surface_generation: u64,
) -> bool {
    !frame_reused
        || !canvas_ready
        || last_device_generation != Some(current_device_generation)
        || last_surface_generation != Some(current_surface_generation)
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
            clear_color: [9, 12, 16, 255],
            world_bounds: Some(world_bounds),
            grid_color: [104, 119, 136, 18],
            major_grid_color: [136, 150, 166, 34],
            axis_color: [212, 119, 26, 48],
            symbol_color: [224, 226, 232, 255],
            selected_object_ids: selection.object_ids.clone(),
            selected_source_ids: selection.source_ids.clone(),
            ..CadSurfaceOptions::default()
        }
    } else {
        CadSurfaceOptions {
            clear_color: [248, 250, 253, 255],
            world_bounds: Some(world_bounds),
            grid_color: [70, 80, 96, 30],
            major_grid_color: [70, 80, 96, 52],
            axis_color: [212, 119, 26, 82],
            symbol_color: [34, 39, 48, 255],
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
            1,
            [320, 200],
            [0.0, 320.0, 0.0, 200.0],
            true,
            &selection,
        );
        host.ensure_frame(
            &scene,
            1,
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
            1,
            [320, 200],
            [1000.0, 1320.0, 0.0, 200.0],
            true,
            &selection,
        );
        let rebuilt = host.cache_stats();
        assert_eq!(rebuilt.frame_builds, 2);
        assert!(!rebuilt.last_frame_reused);
    }

    #[test]
    fn camera_motion_inside_expanded_bounds_reuses_geometry() {
        let scene = CadScene {
            surface: raf_electronics::CadSurfaceKind::Schematic,
            objects: Vec::new(),
        };
        let mut host = ElectronicsCadSurfaceHost::new("cad_camera_cache");
        let selection = CadSurfaceSelection::default();

        host.ensure_frame(
            &scene,
            1,
            [320, 200],
            [0.0, 320.0, 0.0, 200.0],
            true,
            &selection,
        );
        host.ensure_frame(
            &scene,
            1,
            [320, 200],
            [24.0, 344.0, 16.0, 216.0],
            true,
            &selection,
        );

        let stats = host.cache_stats();
        assert_eq!(stats.frame_builds, 1);
        assert_eq!(stats.frame_cache_hits, 1);
        assert!(!stats.last_frame_reused);
    }

    #[test]
    fn cached_presentation_requires_a_ready_canvas_on_the_same_device() {
        assert!(should_submit_cached_frame(
            true,
            false,
            Some(3),
            3,
            Some(4),
            4
        ));
        assert!(should_submit_cached_frame(
            true,
            true,
            Some(3),
            4,
            Some(4),
            4
        ));
        assert!(should_submit_cached_frame(
            false,
            true,
            Some(3),
            3,
            Some(4),
            4
        ));
        assert!(!should_submit_cached_frame(
            true,
            true,
            Some(3),
            3,
            Some(4),
            4
        ));
        assert!(should_submit_cached_frame(
            true,
            true,
            Some(3),
            3,
            Some(3),
            4
        ));
    }
}
