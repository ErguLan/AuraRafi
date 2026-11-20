//! Metrics and translucent-coverage projections for the retained UI compositor.

use super::super::{UiRect, UiSurfaceDrawList};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSurfaceGpuMetrics {
    pub solid_vertices: u32,
    pub stroke_vertices: u32,
    pub text_vertices: u32,
    pub image_vertices: u32,
    pub atlas_uploads: u32,
    pub image_uploads: u32,
    pub image_upload_bytes: u64,
    pub image_evictions: u32,
    pub image_resident_bytes: u64,
    pub image_budget_exceeded: bool,
    pub geometry_cache_hit: bool,
    pub buffer_upload_bytes: u64,
    pub paint_runs: u32,
    pub draw_calls: u32,
    /// Number of submitted solid quads that require alpha blending.
    pub translucent_solid_quads: u32,
    /// Approximate physical-pixel coverage submitted by translucent solids.
    /// Overlapping quads are intentionally counted independently.
    pub estimated_translucent_pixels: u64,
}

pub(super) fn estimate_translucent_solids(
    draw_list: &UiSurfaceDrawList,
    logical_size: [u32; 2],
    physical_size: [u32; 2],
) -> (u32, u64) {
    let bounds = UiRect::new(0.0, 0.0, logical_size[0] as f32, logical_size[1] as f32);
    let scale_x = physical_size[0] as f64 / f64::from(logical_size[0].max(1));
    let scale_y = physical_size[1] as f64 / f64::from(logical_size[1].max(1));
    let mut quads = 0_u32;
    let mut pixels = 0_u64;
    for quad in &draw_list.solids {
        if quad.color[3] == 0 || quad.color[3] == 255 {
            continue;
        }
        let clipped = quad.rect.intersection(quad.clip_rect).intersection(bounds);
        if clipped.is_empty() {
            continue;
        }
        quads = quads.saturating_add(1);
        let area = f64::from(clipped.width.max(0.0))
            * f64::from(clipped.height.max(0.0))
            * scale_x
            * scale_y;
        pixels = pixels.saturating_add(area.round().max(0.0) as u64);
    }
    (quads, pixels)
}
