use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.x
            && point[0] <= self.right()
            && point[1] >= self.y
            && point[1] <= self.bottom()
    }

    pub fn shrink(&self, spacing: UiSpacing) -> Self {
        let width = (self.width - spacing.left - spacing.right).max(0.0);
        let height = (self.height - spacing.top - spacing.bottom).max(0.0);
        Self::new(self.x + spacing.left, self.y + spacing.top, width, height)
    }

    pub fn clamp_inside(&self, bounds: UiRect) -> Self {
        let width = self.width.min(bounds.width).max(0.0);
        let height = self.height.min(bounds.height).max(0.0);
        let max_x = (bounds.right() - width).max(bounds.x);
        let max_y = (bounds.bottom() - height).max(bounds.y);
        Self::new(
            self.x.clamp(bounds.x, max_x),
            self.y.clamp(bounds.y, max_y),
            width,
            height,
        )
    }

    pub fn intersection(&self, other: UiRect) -> Self {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Self::new(left, top, (right - left).max(0.0), (bottom - top).max(0.0))
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiSpacing {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl UiSpacing {
    pub const ZERO: Self = Self::same(0.0);

    pub const fn same(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }

    pub const fn xy(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            right: horizontal,
            top: vertical,
            bottom: vertical,
        }
    }
}
