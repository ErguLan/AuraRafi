//! Non-destructive node composition. Full layout replacement stays explicit.
use crate::{UiAlign, UiCompactMode, UiLayout, UiNode, UiSizeMode, UiSpacing};

impl UiNode {
    /// Changes selected layout properties while retaining flow, anchors,
    /// constraints and every other property not changed by the callback.
    pub fn map_layout(mut self, change: impl FnOnce(UiLayout) -> UiLayout) -> Self {
        self.layout = change(self.layout);
        self
    }

    pub fn fill_width(self) -> Self {
        self.map_layout(UiLayout::fill_width)
    }

    pub fn fill_height(self) -> Self {
        self.map_layout(UiLayout::fill_height)
    }

    pub fn fixed_width(self, width: f32) -> Self {
        self.map_layout(|layout| layout.fixed_width(width))
    }

    pub fn fixed_height(self, height: f32) -> Self {
        self.map_layout(|layout| layout.fixed_height(height))
    }

    pub fn fit_content(self) -> Self {
        self.map_layout(|layout| {
            layout
                .with_width_mode(UiSizeMode::FitContent)
                .with_height_mode(UiSizeMode::FitContent)
        })
    }

    pub fn with_gap(self, gap: f32) -> Self {
        self.map_layout(|layout| layout.gap(gap))
    }

    pub fn with_padding(self, padding: UiSpacing) -> Self {
        self.map_layout(|layout| layout.padding(padding))
    }

    pub fn with_alignment(self, alignment: UiAlign) -> Self {
        self.map_layout(|layout| layout.align(alignment))
    }

    pub fn with_compact(self, compact: UiCompactMode) -> Self {
        self.map_layout(|mut layout| {
            layout.compact = compact;
            layout
        })
    }
}
