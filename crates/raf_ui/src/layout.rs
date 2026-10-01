use serde::{Deserialize, Serialize};

use crate::geometry::{UiRect, UiSpacing};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiFlow {
    None,
    Row,
    Column,
    RowWrap,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiJustify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Default for UiJustify {
    fn default() -> Self {
        Self::Start
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiAlign {
    Start,
    Center,
    End,
    Stretch,
}

impl Default for UiAlign {
    fn default() -> Self {
        Self::Stretch
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiOverflow {
    Visible,
    Clip,
    ScrollX,
    ScrollY,
    ScrollBoth,
}

/// How one layout axis obtains its size before a parent distributes free
/// space. `Auto` preserves the historic RafUI behavior, while the intrinsic
/// modes make popovers, labels, and compact controls express their intent
/// without sentinel pixel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiSizeMode {
    Auto,
    Fixed,
    Fill,
    FitContent,
    MinContent,
    MaxContent,
}

impl Default for UiSizeMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl Default for UiOverflow {
    fn default() -> Self {
        Self::Visible
    }
}

impl UiOverflow {
    pub fn clips_children(self) -> bool {
        !matches!(self, Self::Visible)
    }

    pub fn scrolls_horizontally(self) -> bool {
        matches!(self, Self::ScrollX | Self::ScrollBoth)
    }

    pub fn scrolls_vertically(self) -> bool {
        matches!(self, Self::ScrollY | Self::ScrollBoth)
    }
}

/// How a flex container responds when its requested tracks no longer fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiCompactMode {
    None,
    /// Preserve reading order and put controls onto additional rows.
    Wrap,
    /// Preserve reading order and turn a row into a column.
    Stack,
    /// Pick wrap when a child can retain its minimum width, otherwise stack.
    Auto,
}

impl Default for UiCompactMode {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiGridLayout {
    /// Zero means calculate columns from `min_column_width`.
    #[serde(default)]
    pub columns: u16,
    #[serde(default = "default_grid_min_column_width")]
    pub min_column_width: f32,
    #[serde(default)]
    pub row_height: f32,
}

fn default_grid_min_column_width() -> f32 {
    180.0
}

impl Default for UiGridLayout {
    fn default() -> Self {
        Self {
            columns: 0,
            min_column_width: default_grid_min_column_width(),
            row_height: 0.0,
        }
    }
}

/// CSS-media-query-like override evaluated against the containing surface.
/// The first matching rule in declaration order is applied, which makes narrow
/// rules easy to read and keeps serialization compact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiResponsiveRule {
    pub max_width: f32,
    #[serde(default)]
    pub flow: Option<UiFlow>,
    #[serde(default)]
    pub basis: Option<[f32; 2]>,
    #[serde(default)]
    pub padding: Option<UiSpacing>,
    #[serde(default)]
    pub gap: Option<f32>,
    #[serde(default)]
    pub compact: Option<UiCompactMode>,
    #[serde(default)]
    pub grid_columns: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiPositionMode {
    Flow,
    Absolute,
    Docked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiLayout {
    pub rect: Option<UiRect>,
    pub flow: UiFlow,
    pub padding: UiSpacing,
    pub gap: f32,
    pub basis: [f32; 2],
    pub grow: f32,
    #[serde(default)]
    pub width_mode: UiSizeMode,
    #[serde(default)]
    pub height_mode: UiSizeMode,
    #[serde(default)]
    pub position_mode: UiPositionMode,
    #[serde(default)]
    pub z_index: i16,
    #[serde(default)]
    pub min_size: [f32; 2],
    #[serde(default)]
    pub max_size: [f32; 2],
    #[serde(default)]
    pub justify_content: UiJustify,
    #[serde(default)]
    pub align_items: UiAlign,
    #[serde(default)]
    pub align_self: Option<UiAlign>,
    #[serde(default)]
    pub overflow: UiOverflow,
    #[serde(default)]
    pub compact: UiCompactMode,
    #[serde(default)]
    pub grid: UiGridLayout,
    #[serde(default)]
    pub responsive: Vec<UiResponsiveRule>,
}

impl Default for UiPositionMode {
    fn default() -> Self {
        Self::Flow
    }
}

impl Default for UiLayout {
    fn default() -> Self {
        Self {
            rect: None,
            flow: UiFlow::None,
            padding: UiSpacing::ZERO,
            gap: 0.0,
            basis: [0.0, 0.0],
            grow: 0.0,
            width_mode: UiSizeMode::Auto,
            height_mode: UiSizeMode::Auto,
            position_mode: UiPositionMode::Flow,
            z_index: 0,
            min_size: [0.0, 0.0],
            max_size: [0.0, 0.0],
            justify_content: UiJustify::Start,
            align_items: UiAlign::Stretch,
            align_self: None,
            overflow: UiOverflow::Visible,
            compact: UiCompactMode::None,
            grid: UiGridLayout::default(),
            responsive: Vec::new(),
        }
    }
}

impl UiLayout {
    /// Minimum horizontal breathing room for authored text. RafUI applies
    /// this only when a surface opts into the semantic text-safe pass; it is
    /// kept here so every renderer uses the same spacing contract.
    pub const TEXT_SAFE_INSET_X: f32 = 8.0;
    pub const TEXT_SAFE_INSET_Y: f32 = 4.0;
    pub const LABEL_SAFE_INSET_X: f32 = 4.0;

    pub fn row() -> Self {
        Self {
            flow: UiFlow::Row,
            ..Self::default()
        }
    }

    pub fn column() -> Self {
        Self {
            flow: UiFlow::Column,
            ..Self::default()
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap.max(0.0);
        self
    }

    pub fn padding(mut self, padding: UiSpacing) -> Self {
        self.padding = padding;
        self
    }

    pub fn padding_xy(mut self, x: f32, y: f32) -> Self {
        self.padding = UiSpacing::xy(x, y);
        self
    }

    pub fn align(mut self, align: UiAlign) -> Self {
        self.align_items = align;
        self
    }

    pub fn justify(mut self, justify: UiJustify) -> Self {
        self.justify_content = justify;
        self
    }

    pub fn overflow(mut self, overflow: UiOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn fill_width(mut self) -> Self {
        self.width_mode = UiSizeMode::Fill;
        self
    }

    pub fn fill_height(mut self) -> Self {
        self.height_mode = UiSizeMode::Fill;
        self
    }

    pub fn fixed_width(mut self, width: f32) -> Self {
        self.basis[0] = width.max(0.0);
        self.width_mode = UiSizeMode::Fixed;
        self
    }

    pub fn fixed_height(mut self, height: f32) -> Self {
        self.basis[1] = height.max(0.0);
        self.height_mode = UiSizeMode::Fixed;
        self
    }

    pub fn grow(mut self, grow: f32) -> Self {
        self.grow = grow.max(0.0);
        self
    }

    pub fn fill(flow: UiFlow) -> Self {
        Self {
            grow: 1.0,
            flow,
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fill,
            ..Self::default()
        }
    }

    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            basis: [width.max(0.0), height.max(0.0)],
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fixed,
            ..Self::default()
        }
    }

    pub fn fit_content() -> Self {
        Self {
            width_mode: UiSizeMode::FitContent,
            height_mode: UiSizeMode::FitContent,
            ..Self::default()
        }
    }

    pub fn with_width_mode(mut self, mode: UiSizeMode) -> Self {
        self.width_mode = mode;
        self
    }

    pub fn with_height_mode(mut self, mode: UiSizeMode) -> Self {
        self.height_mode = mode;
        self
    }

    /// Adds the minimum visual safe area around text without removing larger
    /// spacing authored by a surface. Compact controls receive vertical
    /// breathing room; labels and structural text only receive a horizontal
    /// inset so an 18px label is not clipped inside an 18px track.
    pub fn with_text_safe_area(mut self, compact_control: bool) -> Self {
        let horizontal = if compact_control {
            Self::TEXT_SAFE_INSET_X
        } else {
            Self::LABEL_SAFE_INSET_X
        };
        self.padding.left = self.padding.left.max(horizontal);
        self.padding.right = self.padding.right.max(horizontal);
        if compact_control {
            self.padding.top = self.padding.top.max(Self::TEXT_SAFE_INSET_Y);
            self.padding.bottom = self.padding.bottom.max(Self::TEXT_SAFE_INSET_Y);
        }
        self
    }

    pub fn absolute(rect: UiRect) -> Self {
        Self {
            rect: Some(rect),
            basis: [rect.width, rect.height],
            position_mode: UiPositionMode::Absolute,
            ..Self::default()
        }
    }

    pub fn with_z_index(mut self, z_index: i16) -> Self {
        self.z_index = z_index;
        self
    }

    pub fn responsive(mut self, rule: UiResponsiveRule) -> Self {
        self.responsive.push(rule);
        self.responsive
            .sort_by(|left, right| left.max_width.total_cmp(&right.max_width));
        self
    }

    pub fn resolved_for(&self, container_width: f32) -> Self {
        let mut resolved = self.clone();
        let Some(rule) = self
            .responsive
            .iter()
            .find(|rule| rule.max_width > 0.0 && container_width <= rule.max_width)
        else {
            return resolved;
        };
        if let Some(flow) = rule.flow {
            resolved.flow = flow;
        }
        if let Some(basis) = rule.basis {
            resolved.basis = basis;
        }
        if let Some(padding) = rule.padding {
            resolved.padding = padding;
        }
        if let Some(gap) = rule.gap {
            resolved.gap = gap.max(0.0);
        }
        if let Some(compact) = rule.compact {
            resolved.compact = compact;
        }
        if let Some(columns) = rule.grid_columns {
            resolved.grid.columns = columns;
        }
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluent_row_layout_matches_manual_initialization() {
        let manual = UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 6.0),
            width_mode: UiSizeMode::Fill,
            basis: [0.0, 26.0],
            height_mode: UiSizeMode::Fixed,
            overflow: UiOverflow::ScrollX,
            ..UiLayout::default()
        };

        let fluent = UiLayout::row()
            .align(UiAlign::Center)
            .gap(4.0)
            .padding_xy(8.0, 6.0)
            .fill_width()
            .fixed_height(26.0)
            .overflow(UiOverflow::ScrollX);

        assert_eq!(manual, fluent);
    }

    #[test]
    fn fluent_column_layout_matches_manual_initialization() {
        let manual = UiLayout {
            flow: UiFlow::Column,
            justify_content: UiJustify::SpaceBetween,
            gap: 10.0,
            padding: UiSpacing::same(12.0),
            grow: 1.0,
            width_mode: UiSizeMode::Fixed,
            basis: [200.0, 0.0],
            height_mode: UiSizeMode::Fill,
            ..UiLayout::default()
        };

        let fluent = UiLayout::column()
            .justify(UiJustify::SpaceBetween)
            .gap(10.0)
            .padding(UiSpacing::same(12.0))
            .grow(1.0)
            .fixed_width(200.0)
            .fill_height();

        assert_eq!(manual, fluent);
    }
}
