//! CPU-neutral selection ID buffer.
//!
//! The GPU implementation can render the same object IDs into an offscreen
//! target later. This buffer defines the selection semantics first: empty
//! pixels are ignored, higher priority wins, and equal priority resolves to
//! the nearest depth.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PickObjectId(u32);

impl PickObjectId {
    pub const NONE: Self = Self(0);

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickPixel {
    pub id: PickObjectId,
    pub depth: f32,
    pub priority: u8,
}

impl PickPixel {
    pub const EMPTY: Self = Self {
        id: PickObjectId::NONE,
        depth: f32::INFINITY,
        priority: 0,
    };

    pub fn should_replace(self, current: Self) -> bool {
        if self.id.is_empty() {
            return false;
        }
        if current.id.is_empty() {
            return true;
        }
        self.priority > current.priority
            || (self.priority == current.priority && self.depth < current.depth)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickHit {
    pub id: PickObjectId,
    pub depth: f32,
    pub priority: u8,
}

#[derive(Debug, Clone)]
pub struct SelectionIdBuffer {
    width: u32,
    height: u32,
    pixels: Vec<PickPixel>,
}

impl SelectionIdBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            pixels: vec![PickPixel::EMPTY; (width * height) as usize],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.pixels
            .resize((width * height) as usize, PickPixel::EMPTY);
        self.clear();
    }

    pub fn clear(&mut self) {
        self.pixels.fill(PickPixel::EMPTY);
    }

    pub fn write_pixel(&mut self, x: i32, y: i32, id: PickObjectId, depth: f32, priority: u8) {
        let Some(index) = self.index(x, y) else {
            return;
        };
        let next = PickPixel {
            id,
            depth,
            priority,
        };
        let current = self.pixels[index];
        if next.should_replace(current) {
            self.pixels[index] = next;
        }
    }

    pub fn write_rect(
        &mut self,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        id: PickObjectId,
        depth: f32,
        priority: u8,
    ) {
        let start_x = min_x.min(max_x).max(0);
        let end_x = min_x.max(max_x).min(self.width as i32 - 1);
        let start_y = min_y.min(max_y).max(0);
        let end_y = min_y.max(max_y).min(self.height as i32 - 1);

        if start_x > end_x || start_y > end_y {
            return;
        }

        for y in start_y..=end_y {
            for x in start_x..=end_x {
                self.write_pixel(x, y, id, depth, priority);
            }
        }
    }

    pub fn read(&self, x: i32, y: i32) -> Option<PickHit> {
        let pixel = self.pixels.get(self.index(x, y)?)?;
        if pixel.id.is_empty() {
            return None;
        }
        Some(PickHit {
            id: pixel.id,
            depth: pixel.depth,
            priority: pixel.priority,
        })
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(y as usize * self.width as usize + x as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_priority_prefers_nearest_depth() {
        let mut buffer = SelectionIdBuffer::new(8, 8);

        buffer.write_pixel(3, 3, PickObjectId::new(10), 0.8, 1);
        buffer.write_pixel(3, 3, PickObjectId::new(20), 0.2, 1);

        assert_eq!(buffer.read(3, 3).unwrap().id, PickObjectId::new(20));
    }

    #[test]
    fn higher_priority_wins_over_depth() {
        let mut buffer = SelectionIdBuffer::new(8, 8);

        buffer.write_pixel(3, 3, PickObjectId::new(10), 0.1, 1);
        buffer.write_pixel(3, 3, PickObjectId::new(20), 0.9, 3);

        assert_eq!(buffer.read(3, 3).unwrap().id, PickObjectId::new(20));
    }

    #[test]
    fn rect_write_clamps_to_buffer() {
        let mut buffer = SelectionIdBuffer::new(4, 4);

        buffer.write_rect(-4, -4, 1, 1, PickObjectId::new(7), 0.5, 1);

        assert_eq!(buffer.read(0, 0).unwrap().id, PickObjectId::new(7));
        assert_eq!(buffer.read(1, 1).unwrap().id, PickObjectId::new(7));
        assert!(buffer.read(2, 2).is_none());
    }
}
