//! Canvas target geometry reported by the native presentation path.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasTargetRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CanvasTargetRect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn full(size: [u32; 2]) -> Self {
        Self::new(0, 0, size[0], size[1])
    }

    pub(crate) fn clipped(self, target_size: [u32; 2]) -> Self {
        let x = self.x.min(target_size[0]);
        let y = self.y.min(target_size[1]);
        Self {
            x,
            y,
            width: self.width.min(target_size[0].saturating_sub(x)),
            height: self.height.min(target_size[1].saturating_sub(y)),
        }
    }
}
