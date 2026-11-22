//! Placement of canvas layers inside a native editor target.
//!
//! Scene and CPU canvases now reach the target through the AGB product packet.
//! This module only owns the target geometry and the pixel-to-normalized
//! conversion that every layer shares.

mod metrics;

pub use metrics::CanvasTargetRect;

use crate::api_graphic_basic::device::SceneFrameOutput;
use crate::api_graphic_basic::product_composition::append_cpu_canvas;
use raf_agb::product::ProductPacket;

impl CanvasTargetRect {
    /// Rectangle in normalized top-left target coordinates.
    pub fn normalized(self, target_size: [u32; 2]) -> [f32; 4] {
        let width = target_size[0].max(1) as f32;
        let height = target_size[1].max(1) as f32;
        let clipped = self.clipped(target_size);
        [
            clipped.x as f32 / width,
            clipped.y as f32 / height,
            clipped.width as f32 / width,
            clipped.height as f32 / height,
        ]
    }
}

impl SceneFrameOutput {
    /// True when this output carries retained scene commands instead of pixels.
    pub fn is_retained_scene(&self) -> bool {
        matches!(self, Self::Scene(_))
    }
}

/// Appends a CPU-rendered canvas as one image quad.
///
/// Returns `Ok(false)` for an empty frame so a host can keep the rest of the
/// composition without inventing a texture.
pub fn append_cpu_output(
    packet: &mut ProductPacket,
    rect: [f32; 4],
    output: &SceneFrameOutput,
    source_size: [u32; 2],
) -> Result<bool, String> {
    let SceneFrameOutput::CpuPixels(pixels) = output else {
        return Ok(false);
    };
    if pixels.is_empty() {
        return Ok(false);
    }
    append_cpu_canvas(packet, rect, pixels, source_size)?;
    Ok(true)
}
