//! Native Game viewport presentation state.
//!
//! Scene rendering and input live in `viewport_controller.rs`; this host
//! keeps the old public presentation contract without owning a foreign UI
//! texture. ApiGraphicBasic's compositor consumes the resulting canvas layer.

use std::time::{Duration, Instant};

use raf_core::scene::SceneNodeId;
use raf_render::api_graphic_basic::device::SceneFrameOutput;
use raf_render::{RenderConfig, RenderResourceProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportHostMode {
    NativeApiGraphicBasic,
    CpuRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportPresentationBackend {
    CpuPixels,
    GpuTexture,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewportFrameKey {
    pub scene_fingerprint: u64,
    pub size: [u32; 2],
    pub selected: Vec<usize>,
    pub volatile_epoch: u64,
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
        let scale = profile.preferred_surface_scale.clamp(0.25, 1.0);
        Self {
            logical_size,
            preferred_render_size: [
                ((logical_size[0] as f32) * scale).ceil().max(1.0) as u32,
                ((logical_size[1] as f32) * scale).ceil().max(1.0) as u32,
            ],
            preferred_surface_scale: scale,
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
    plan: ViewportSurfacePlan,
    last_key: Option<ViewportFrameKey>,
    last_stats: Option<ViewportPresentationStats>,
    cache: ViewportSurfaceCacheStats,
    last_submission_at: Option<Instant>,
}

impl Default for ViewportSurfaceHost {
    fn default() -> Self {
        Self {
            mode: ViewportHostMode::NativeApiGraphicBasic,
            plan: ViewportSurfacePlan::default(),
            last_key: None,
            last_stats: None,
            cache: ViewportSurfaceCacheStats::default(),
            last_submission_at: None,
        }
    }
}

impl ViewportSurfaceHost {
    pub fn mode(&self) -> ViewportHostMode {
        self.mode
    }

    pub fn prepare_native_wgpu_mode(&mut self) {
        self.mode = ViewportHostMode::NativeApiGraphicBasic;
    }

    pub fn configure_from_render_config(
        &mut self,
        config: &RenderConfig,
        logical_size: [u32; 2],
    ) -> ViewportSurfacePlan {
        self.plan = ViewportSurfacePlan::from_profile(logical_size, config.resource_profile());
        self.plan
    }

    pub fn begin_frame(&mut self, key: ViewportFrameKey) -> bool {
        let reused = self.last_key.as_ref() == Some(&key);
        if reused {
            self.cache.frame_cache_hits = self.cache.frame_cache_hits.saturating_add(1);
        } else {
            self.cache.frame_builds = self.cache.frame_builds.saturating_add(1);
            self.last_key = Some(key);
        }
        self.cache.last_frame_reused = reused;
        reused
    }

    pub fn record_presentation(
        &mut self,
        output: &SceneFrameOutput,
        size: [u32; 2],
        min_frame_interval: Option<Duration>,
    ) -> ViewportPresentationStats {
        if min_frame_interval.is_some_and(|interval| {
            self.last_submission_at
                .is_some_and(|last| last.elapsed() < interval)
        }) {
            self.cache.presentation_cache_hits =
                self.cache.presentation_cache_hits.saturating_add(1);
        } else {
            self.last_submission_at = Some(Instant::now());
        }
        let backend = match output {
            SceneFrameOutput::CpuPixels(_) => ViewportPresentationBackend::CpuPixels,
            SceneFrameOutput::GpuTexture { .. } => ViewportPresentationBackend::GpuTexture,
        };
        let stats = ViewportPresentationStats {
            mode: self.mode,
            backend,
            size,
            plan: self.plan,
            cache: self.cache,
        };
        self.last_stats = Some(stats);
        stats
    }

    pub fn last_stats(&self) -> Option<ViewportPresentationStats> {
        self.last_stats
    }

    pub fn cache_stats(&self) -> ViewportSurfaceCacheStats {
        self.cache
    }

    pub fn invalidate_cache(&mut self) {
        self.last_key = None;
        self.last_submission_at = None;
        self.cache.last_frame_reused = false;
    }
}

pub fn frame_key(
    scene_fingerprint: u64,
    size: [u32; 2],
    selected: &[SceneNodeId],
    volatile_epoch: u64,
) -> ViewportFrameKey {
    ViewportFrameKey {
        scene_fingerprint,
        size: [size[0].max(1), size[1].max(1)],
        selected: selected.iter().map(|id| id.0).collect(),
        volatile_epoch,
    }
}
