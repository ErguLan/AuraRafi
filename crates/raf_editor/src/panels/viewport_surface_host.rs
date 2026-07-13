use eframe::{egui, egui_wgpu};
use glam::Vec3;
use raf_core::scene::graph::SceneNodeId;
use std::time::{Duration, Instant};

use raf_render::api_graphic_basic::device::SceneFrameOutput;
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime};
use raf_render::scene_renderer::{RenderMode, RenderOptions, SceneRenderFrame};
use raf_render::{Camera, CameraMode, RenderConfig, RenderResourceProfile};

use super::gpu_canvas::GpuCanvas;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportHostMode {
    EguiTextureBridge,
    NativeWgpuPrepared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportPresentationBackend {
    CpuPixels,
    GpuTexture,
}

#[derive(Debug, Clone, Copy)]
pub struct ViewportPresentationStats {
    pub mode: ViewportHostMode,
    pub backend: ViewportPresentationBackend,
    pub size: [u32; 2],
    pub plan: ViewportSurfacePlan,
    pub cache: ViewportSurfaceCacheStats,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewportSurfaceCacheStats {
    pub frame_builds: u64,
    pub frame_cache_hits: u64,
    pub presentation_cache_hits: u64,
    pub last_frame_reused: bool,
}

/// All renderer inputs that can change a retained viewport command frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewportFrameKey {
    scene_fingerprint: u64,
    camera_bits: [u32; 14],
    size: [u32; 2],
    selected: Vec<usize>,
    background: [u8; 4],
    light_dir_bits: [u32; 3],
    options_fingerprint: u64,
    vertex_edit_enabled: bool,
    volatile_epoch: u64,
}

impl ViewportFrameKey {
    pub fn new(
        scene_fingerprint: u64,
        camera: &Camera,
        size: [u32; 2],
        selected: &[SceneNodeId],
        background: [u8; 4],
        light_dir: Vec3,
        options: RenderOptions,
        vertex_edit_enabled: bool,
        volatile_epoch: u64,
    ) -> Self {
        Self {
            scene_fingerprint,
            camera_bits: camera_fingerprint(camera),
            size: [size[0].max(1), size[1].max(1)],
            selected: selected.iter().map(|id| id.0).collect(),
            background,
            light_dir_bits: [
                light_dir.x.to_bits(),
                light_dir.y.to_bits(),
                light_dir.z.to_bits(),
            ],
            options_fingerprint: render_options_fingerprint(options),
            vertex_edit_enabled,
            volatile_epoch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportPreparedFeatureFlags {
    pub shadows: bool,
    pub post_processing: bool,
    pub pbr: bool,
    pub particles: bool,
    pub skeletal_animation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSurfacePlan {
    pub logical_size: [u32; 2],
    pub preferred_render_size: [u32; 2],
    pub preferred_surface_scale: f32,
    pub frame_budget_ms: f32,
    pub max_triangles: u32,
    pub max_texture_size: u32,
    pub shadow_resolution: u32,
    pub max_point_lights: u32,
    pub post_process_passes: u8,
    pub particle_budget: u32,
    pub skeletal_animation_budget: u32,
    pub requires_gpu: bool,
    pub prepared_features: ViewportPreparedFeatureFlags,
}

impl Default for ViewportSurfacePlan {
    fn default() -> Self {
        Self::from_profile([1, 1], RenderConfig::potato().resource_profile())
    }
}

impl ViewportSurfacePlan {
    pub fn from_profile(logical_size: [u32; 2], profile: RenderResourceProfile) -> Self {
        let logical_size = [logical_size[0].max(1), logical_size[1].max(1)];
        let preferred_surface_scale = profile.preferred_surface_scale.clamp(0.25, 1.0);
        let preferred_render_size = [
            ((logical_size[0] as f32) * preferred_surface_scale)
                .ceil()
                .max(1.0) as u32,
            ((logical_size[1] as f32) * preferred_surface_scale)
                .ceil()
                .max(1.0) as u32,
        ];

        Self {
            logical_size,
            preferred_render_size,
            preferred_surface_scale,
            frame_budget_ms: profile.frame_budget_ms,
            max_triangles: profile.max_triangles,
            max_texture_size: profile.max_texture_size,
            shadow_resolution: profile.shadow_resolution,
            max_point_lights: profile.max_point_lights,
            post_process_passes: profile.post_process_passes,
            particle_budget: profile.particle_budget,
            skeletal_animation_budget: profile.skeletal_animation_budget,
            requires_gpu: profile.requires_gpu,
            prepared_features: ViewportPreparedFeatureFlags {
                shadows: profile.shadow_resolution > 0,
                post_processing: profile.post_process_passes > 0,
                pbr: profile.pbr_enabled,
                particles: profile.particle_budget > 0,
                skeletal_animation: profile.skeletal_animation_budget > 0,
            },
        }
    }
}

pub struct ViewportSurfaceHost {
    mode: ViewportHostMode,
    canvas: GpuCanvas,
    plan: ViewportSurfacePlan,
    last_stats: Option<ViewportPresentationStats>,
    cached_frame: Option<SceneRenderFrame>,
    cached_key: Option<ViewportFrameKey>,
    cache_stats: ViewportSurfaceCacheStats,
    last_presented_device_generation: Option<u64>,
    last_submission_at: Option<Instant>,
}

impl Default for ViewportSurfaceHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportSurfaceHost {
    pub fn new() -> Self {
        Self {
            mode: ViewportHostMode::EguiTextureBridge,
            canvas: GpuCanvas::new("viewport_render"),
            plan: ViewportSurfacePlan::default(),
            last_stats: None,
            cached_frame: None,
            cached_key: None,
            cache_stats: ViewportSurfaceCacheStats::default(),
            last_presented_device_generation: None,
            last_submission_at: None,
        }
    }

    pub fn mode(&self) -> ViewportHostMode {
        self.mode
    }

    pub fn prepare_native_wgpu_mode(&mut self) {
        self.mode = ViewportHostMode::NativeWgpuPrepared;
    }

    pub fn configure_from_render_config(
        &mut self,
        config: &RenderConfig,
        logical_size: [u32; 2],
    ) -> ViewportSurfacePlan {
        self.plan = ViewportSurfacePlan::from_profile(logical_size, config.resource_profile());
        self.plan
    }

    pub fn present_frame<F>(
        &mut self,
        ctx: &egui::Context,
        wgpu_render_state: Option<&egui_wgpu::RenderState>,
        render_runtime: &mut RenderRuntime,
        min_frame_interval: Option<Duration>,
        key: ViewportFrameKey,
        build_frame: F,
    ) -> ViewportPresentationStats
    where
        F: FnOnce() -> SceneRenderFrame,
    {
        if should_defer_presentation(
            min_frame_interval,
            self.canvas.is_ready(),
            self.last_stats.is_some(),
            self.last_submission_at,
        ) {
            self.cache_stats.presentation_cache_hits =
                self.cache_stats.presentation_cache_hits.saturating_add(1);
            self.cache_stats.last_frame_reused = true;
            let mut stats = self
                .last_stats
                .expect("a throttled viewport requires prior presentation stats");
            stats.cache = self.cache_stats;
            self.last_stats = Some(stats);
            return stats;
        }

        render_runtime.activate_surface(GraphicsSurfaceKind::SceneViewport);
        let frame_reused = self.ensure_frame(key, build_frame);
        let runtime_snapshot = render_runtime.snapshot();
        let should_present = should_submit_cached_frame(
            frame_reused,
            self.canvas.is_ready(),
            self.last_presented_device_generation,
            runtime_snapshot.device_generation,
        );

        let (backend, size) = if should_present {
            let frame = self
                .cached_frame
                .as_ref()
                .expect("viewport surface cache must contain the current frame");
            let output = render_runtime.render_scene_frame(frame);
            let (backend, size) = presentation_details(&output, frame.width, frame.height);
            self.canvas
                .present(ctx, wgpu_render_state, output, frame.width, frame.height);
            self.last_presented_device_generation = Some(runtime_snapshot.device_generation);
            self.last_submission_at = Some(Instant::now());
            (backend, size)
        } else {
            self.cache_stats.presentation_cache_hits =
                self.cache_stats.presentation_cache_hits.saturating_add(1);
            let previous = self
                .last_stats
                .expect("a cached viewport texture requires prior presentation stats");
            (previous.backend, previous.size)
        };

        let stats = ViewportPresentationStats {
            mode: self.mode,
            backend,
            size,
            plan: self.plan,
            cache: self.cache_stats,
        };
        self.last_stats = Some(stats);
        stats
    }

    pub fn paint(&self, painter: &egui::Painter, rect: egui::Rect) {
        self.canvas.paint(painter, rect);
    }

    pub fn is_ready(&self) -> bool {
        self.canvas.is_ready()
    }

    pub fn last_stats(&self) -> Option<ViewportPresentationStats> {
        self.last_stats
    }

    pub fn plan(&self) -> ViewportSurfacePlan {
        self.plan
    }

    pub fn cache_stats(&self) -> ViewportSurfaceCacheStats {
        self.cache_stats
    }

    pub fn invalidate_cache(&mut self) {
        self.cached_frame = None;
        self.cached_key = None;
        self.last_presented_device_generation = None;
        self.last_submission_at = None;
        self.cache_stats.last_frame_reused = false;
    }

    fn ensure_frame<F>(&mut self, key: ViewportFrameKey, build_frame: F) -> bool
    where
        F: FnOnce() -> SceneRenderFrame,
    {
        let reused = self.cached_key.as_ref() == Some(&key) && self.cached_frame.is_some();
        if reused {
            self.cache_stats.frame_cache_hits = self.cache_stats.frame_cache_hits.saturating_add(1);
        } else {
            self.cached_frame = Some(build_frame());
            self.cached_key = Some(key);
            self.cache_stats.frame_builds = self.cache_stats.frame_builds.saturating_add(1);
        }
        self.cache_stats.last_frame_reused = reused;
        reused
    }
}

fn should_defer_presentation(
    min_frame_interval: Option<Duration>,
    canvas_ready: bool,
    has_previous_presentation: bool,
    last_submission_at: Option<Instant>,
) -> bool {
    let Some(interval) = min_frame_interval else {
        return false;
    };
    canvas_ready
        && has_previous_presentation
        && last_submission_at
            .map(|last| last.elapsed() < interval)
            .unwrap_or(false)
}

fn presentation_details(
    output: &SceneFrameOutput,
    fallback_width: u32,
    fallback_height: u32,
) -> (ViewportPresentationBackend, [u32; 2]) {
    match output {
        SceneFrameOutput::CpuPixels(_) => (
            ViewportPresentationBackend::CpuPixels,
            [fallback_width, fallback_height],
        ),
        SceneFrameOutput::GpuTexture { width, height, .. } => {
            (ViewportPresentationBackend::GpuTexture, [*width, *height])
        }
    }
}

fn should_submit_cached_frame(
    frame_reused: bool,
    canvas_ready: bool,
    last_device_generation: Option<u64>,
    current_device_generation: u64,
) -> bool {
    !frame_reused || !canvas_ready || last_device_generation != Some(current_device_generation)
}

fn camera_fingerprint(camera: &Camera) -> [u32; 14] {
    [
        camera.position.x.to_bits(),
        camera.position.y.to_bits(),
        camera.position.z.to_bits(),
        camera.target.x.to_bits(),
        camera.target.y.to_bits(),
        camera.target.z.to_bits(),
        camera.up.x.to_bits(),
        camera.up.y.to_bits(),
        camera.up.z.to_bits(),
        camera.fov.to_bits(),
        camera.near.to_bits(),
        camera.far.to_bits(),
        camera.ortho_scale.to_bits(),
        match camera.mode {
            CameraMode::Orthographic => 0,
            CameraMode::Perspective => 1,
        },
    ]
}

fn render_options_fingerprint(options: RenderOptions) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut mix = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    mix(match options.mode {
        RenderMode::Solid => 0,
        RenderMode::Wireframe => 1,
        RenderMode::Preview => 2,
    });
    for value in [
        options.show_grid_3d as u64,
        options.solid_show_surface_edges as u64,
        options.solid_xray_mode as u64,
        options.solid_face_tonality as u64,
        options.selection_outline as u64,
        options.grid_no_depth_test as u64,
        options.world_streaming_enabled as u64,
        options.primary_selected.unwrap_or(u64::MAX),
        options.triangle_budget as u64,
        options.world_stream_load_radius as u64,
    ] {
        mix(value);
    }
    for value in [
        options.grid_spacing,
        options.grid_load_distance,
        options.grid_y,
        options.world_stream_region_size,
    ] {
        mix(value.to_bits() as u64);
    }
    for channel in options
        .selection_outline_color
        .into_iter()
        .chain(options.secondary_selection_outline_color)
    {
        mix(channel as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;
    use raf_render::api_graphic_basic::command_list::BasicCommandList;
    use raf_render::scene_renderer::FrameStats;

    fn empty_frame() -> SceneRenderFrame {
        SceneRenderFrame {
            commands: BasicCommandList::new(),
            view_proj: Mat4::IDENTITY,
            light_dir: Vec3::Z,
            width: 320,
            height: 200,
            stats: FrameStats::default(),
        }
    }

    fn frame_key(camera: &Camera, selected: &[SceneNodeId]) -> ViewportFrameKey {
        ViewportFrameKey::new(
            42,
            camera,
            [320, 200],
            selected,
            [240, 240, 242, 255],
            Vec3::new(0.4, 0.8, 0.6),
            RenderOptions::default(),
            false,
            0,
        )
    }

    #[test]
    fn potato_plan_scales_surface_without_gpu_features() {
        let plan = ViewportSurfacePlan::from_profile(
            [1000, 800],
            RenderConfig::potato().resource_profile(),
        );

        assert_eq!(plan.preferred_render_size, [750, 600]);
        assert!(!plan.prepared_features.shadows);
        assert!(!plan.prepared_features.post_processing);
        assert_eq!(plan.particle_budget, 0);
    }

    #[test]
    fn medium_plan_marks_future_features() {
        let plan = ViewportSurfacePlan::from_profile(
            [1000, 800],
            RenderConfig::medium().resource_profile(),
        );

        assert!(plan.prepared_features.shadows);
        assert!(plan.prepared_features.post_processing);
        assert!(plan.prepared_features.particles);
        assert!(plan.requires_gpu);
    }

    #[test]
    fn frame_key_changes_for_camera_or_selection() {
        let camera = Camera::default();
        let key = frame_key(&camera, &[]);

        let mut moved_camera = camera.clone();
        moved_camera.position.x += 1.0;
        assert_ne!(key, frame_key(&moved_camera, &[]));
        assert_ne!(key, frame_key(&camera, &[SceneNodeId(4)]));

        let volatile = ViewportFrameKey::new(
            42,
            &camera,
            [320, 200],
            &[],
            [240, 240, 242, 255],
            Vec3::new(0.4, 0.8, 0.6),
            RenderOptions::default(),
            true,
            1,
        );
        assert_ne!(key, volatile);
    }

    #[test]
    fn retained_frame_reuses_matching_viewport_key() {
        let camera = Camera::default();
        let key = frame_key(&camera, &[]);
        let mut host = ViewportSurfaceHost::new();
        let mut frame_builds = 0;

        assert!(!host.ensure_frame(key.clone(), || {
            frame_builds += 1;
            empty_frame()
        }));
        assert!(host.ensure_frame(key, || {
            frame_builds += 1;
            empty_frame()
        }));

        assert_eq!(frame_builds, 1);
        assert_eq!(host.cache_stats().frame_builds, 1);
        assert_eq!(host.cache_stats().frame_cache_hits, 1);
    }

    #[test]
    fn cached_presentation_requires_a_ready_canvas_on_the_same_device() {
        assert!(should_submit_cached_frame(true, false, Some(3), 3));
        assert!(should_submit_cached_frame(true, true, Some(3), 4));
        assert!(should_submit_cached_frame(false, true, Some(3), 3));
        assert!(!should_submit_cached_frame(true, true, Some(3), 3));
    }

    #[test]
    fn frame_limit_only_defers_after_a_cached_presentation_exists() {
        let interval = Duration::from_secs(1);
        assert!(!should_defer_presentation(
            interval.into(),
            false,
            true,
            Some(Instant::now())
        ));
        assert!(!should_defer_presentation(
            interval.into(),
            true,
            false,
            Some(Instant::now())
        ));
        assert!(!should_defer_presentation(
            interval.into(),
            true,
            true,
            None
        ));
        assert!(should_defer_presentation(
            interval.into(),
            true,
            true,
            Some(Instant::now())
        ));
    }
}
