//! Native canvas output slot.
//!
//! The former type owned a widget texture and a WGPU filter mode. AGB now
//! presents [`SceneFrameOutput`] directly, so this type only keeps the
//! reusable frame slot and its presentation metadata.

use raf_render::api_graphic_basic::device::SceneFrameOutput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasSampling {
    Linear,
    Nearest,
}

pub struct GpuCanvas {
    texture_name: String,
    frame: Option<SceneFrameOutput>,
    size: [u32; 2],
    sampling: CanvasSampling,
}

impl GpuCanvas {
    pub fn new(texture_name: impl Into<String>) -> Self {
        Self {
            texture_name: texture_name.into(),
            frame: None,
            size: [1, 1],
            sampling: CanvasSampling::Linear,
        }
    }

    pub fn with_nearest_sampling(mut self) -> Self {
        self.sampling = CanvasSampling::Nearest;
        self
    }

    pub fn texture_name(&self) -> &str {
        &self.texture_name
    }

    pub fn set_frame(&mut self, frame: SceneFrameOutput, size: [u32; 2]) {
        self.frame = Some(frame);
        self.size = [size[0].max(1), size[1].max(1)];
    }

    pub fn take_frame(&mut self) -> Option<SceneFrameOutput> {
        self.frame.take()
    }

    pub fn frame(&self) -> Option<&SceneFrameOutput> {
        self.frame.as_ref()
    }

    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    pub fn sampling(&self) -> CanvasSampling {
        self.sampling
    }

    pub fn is_ready(&self) -> bool {
        self.frame.is_some()
    }

    pub fn invalidate(&mut self) {
        self.frame = None;
    }
}
