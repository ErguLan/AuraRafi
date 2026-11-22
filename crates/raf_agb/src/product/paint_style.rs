//! Retained RafUI paint description shared by every producer.
//!
//! Geometry is expressed in the producer's normalized target space: the
//! painted rectangle and its clip use the same top-left origin as the paint
//! quad, corner radii store horizontal and vertical extents, border widths
//! follow `top, right, bottom, left`, and image insets follow
//! `top, right, bottom, left` as well.
//!
//! Every value here is data. Producers own the paint order, the clip stack and
//! the alpha convention; this module only describes the shape they want.

/// Total rounded shapes a single paint may intersect against. RafUI validates a
/// declared child clip depth of eight masks and reserves the last slot for the
/// painted surface or image edge.
pub const MAX_ROUNDED_CLIPS: usize = 9;

/// Declared ancestor clip masks a producer may send with one paint.
pub const MAX_DECLARED_CLIPS: usize = MAX_ROUNDED_CLIPS - 1;

/// Border sides in `top, right, bottom, left` order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    pub const ALL: [Self; 4] = [Self::Top, Self::Right, Self::Bottom, Self::Left];

    pub const fn index(self) -> usize {
        match self {
            Self::Top => 0,
            Self::Right => 1,
            Self::Bottom => 2,
            Self::Left => 3,
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Right => Self::Left,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
        }
    }
}

/// Rounded shapes use normalized target coordinates. Each corner stores
/// horizontal and vertical radii, starting at top-left and going clockwise.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoundedClip {
    pub rect: [f32; 4],
    pub radii: [[f32; 2]; 4],
}

impl RoundedClip {
    /// A clip with one radius applied to every corner.
    pub const fn uniform(rect: [f32; 4], radius: f32) -> Self {
        Self {
            rect,
            radii: [[radius, radius]; 4],
        }
    }

    /// A clip with square corners.
    pub const fn square(rect: [f32; 4]) -> Self {
        Self {
            rect,
            radii: [[0.; 2]; 4],
        }
    }

    /// True when the clip encloses no area. Producers may skip the paint.
    pub fn is_empty(&self) -> bool {
        self.rect[2] <= 0. || self.rect[3] <= 0.
    }

    /// True when at least one corner has a curved edge.
    pub fn is_rounded(&self) -> bool {
        self.radii
            .iter()
            .flatten()
            .any(|radius| *radius > 0.)
    }

    /// Replaces the corner radii with one uniform logical radius.
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radii = [[radius, radius]; 4];
        self
    }
}

/// Two-color linear gradient in local shape coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    /// Normalized direction in local coordinates; `[0, 1]` runs downward.
    pub direction: [f32; 2],
    pub start: [f32; 4],
    pub end: [f32; 4],
}

impl LinearGradient {
    pub const fn new(direction: [f32; 2], start: [f32; 4], end: [f32; 4]) -> Self {
        Self {
            direction,
            start,
            end,
        }
    }

    /// Top edge to bottom edge.
    pub const fn vertical(start: [f32; 4], end: [f32; 4]) -> Self {
        Self::new([0., 1.], start, end)
    }

    /// Left edge to right edge.
    pub const fn horizontal(start: [f32; 4], end: [f32; 4]) -> Self {
        Self::new([1., 0.], start, end)
    }

    /// Top-left corner to bottom-right corner.
    pub const fn diagonal(start: [f32; 4], end: [f32; 4]) -> Self {
        Self::new([1., 1.], start, end)
    }

    /// Multiplies both stops by a group opacity.
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        let opacity = opacity.clamp(0., 1.);
        self.start[3] *= opacity;
        self.end[3] *= opacity;
        self
    }
}

/// Per-side border geometry and color.
///
/// `color` applies to every side that has no explicit entry in `sides`. A
/// producer that needs four different borders sets each side; a uniform border
/// leaves `sides` at `None` and keeps one color for the whole outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintBorder {
    /// Top, right, bottom and left in normalized target units.
    pub widths: [f32; 4],
    /// Fallback color for sides without an override.
    pub color: [f32; 4],
    /// Optional per-side override in `top, right, bottom, left` order.
    pub sides: [Option<[f32; 4]>; 4],
}

impl Default for PaintBorder {
    fn default() -> Self {
        Self {
            widths: [0.; 4],
            color: [0.; 4],
            sides: [None; 4],
        }
    }
}

impl PaintBorder {
    /// One width per side and one shared color.
    pub const fn uniform(widths: [f32; 4], color: [f32; 4]) -> Self {
        Self {
            widths,
            color,
            sides: [None; 4],
        }
    }

    /// The same width on every side with one shared color.
    pub const fn outline(width: f32, color: [f32; 4]) -> Self {
        Self::uniform([width; 4], color)
    }

    /// One width per side with an explicit color for each side.
    pub const fn per_side(widths: [f32; 4], colors: [[f32; 4]; 4]) -> Self {
        Self {
            widths,
            color: colors[0],
            sides: [
                Some(colors[0]),
                Some(colors[1]),
                Some(colors[2]),
                Some(colors[3]),
            ],
        }
    }

    /// Overrides one side without touching the others.
    pub fn with_side(mut self, side: Side, color: [f32; 4]) -> Self {
        self.sides[side.index()] = Some(color);
        self
    }

    /// Overrides every side width.
    pub fn with_widths(mut self, widths: [f32; 4]) -> Self {
        self.widths = widths;
        self
    }

    pub fn side_color(&self, side: Side) -> [f32; 4] {
        self.sides[side.index()].unwrap_or(self.color)
    }

    /// Width of one side clamped to a non-negative value.
    pub fn side_width(&self, side: Side) -> f32 {
        self.widths[side.index()].max(0.)
    }

    /// True when the border can paint at least one visible edge.
    pub fn is_visible(&self) -> bool {
        self.widths.iter().any(|width| *width > 0.)
            && Side::ALL
                .into_iter()
                .any(|side| self.side_color(side)[3] > 0.)
    }

    /// True when every painted side shares the fallback color.
    pub fn is_uniform_color(&self) -> bool {
        self.sides
            .iter()
            .all(|side| side.is_none_or(|color| color == self.color))
    }

    /// Clamps widths so opposite sides cannot cross the given extent.
    pub fn clamped(self, [_, _, width, height]: [f32; 4]) -> Self {
        let width = width.max(0.);
        let height = height.max(0.);
        let [top, right, bottom, left] = self.widths.map(|value| value.max(0.));
        let horizontal = if left + right > width && left + right > 0. {
            width / (left + right)
        } else {
            1.
        };
        let vertical = if top + bottom > height && top + bottom > 0. {
            height / (top + bottom)
        } else {
            1.
        };
        Self {
            widths: [
                top * vertical,
                right * horizontal,
                bottom * vertical,
                left * horizontal,
            ],
            ..self
        }
    }
}

/// Image fitting policy for a painted image quad.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImageFit {
    /// Fill the destination rectangle, changing the source aspect ratio.
    #[default]
    Stretch,
    /// Fit the whole source inside the destination rectangle.
    Contain,
    /// Fill the destination rectangle and crop the overflow.
    Cover,
}

/// Nine-slice insets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NineSlice {
    /// Top, right, bottom and left in source texture pixels.
    pub source: [f32; 4],
    /// Top, right, bottom and left in normalized target units.
    pub target: [f32; 4],
}

impl NineSlice {
    pub const fn new(source: [f32; 4], target: [f32; 4]) -> Self {
        Self { source, target }
    }

    /// Builds destination insets from source texels and the physical extent of
    /// the destination target. The target extent is required because normalized
    /// units are not square.
    pub fn from_source_pixels(source: [f32; 4], target_extent: [u32; 2]) -> Self {
        let [width, height] = target_extent.map(|value| value.max(1) as f32);
        Self {
            source,
            target: [
                source[0] / height,
                source[1] / width,
                source[2] / height,
                source[3] / width,
            ],
        }
    }
}

/// Where and how an image is placed inside its quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageLayout {
    pub fit: ImageFit,
    /// Physical extent of the full target. Used to keep square source pixels
    /// square when the normalized space is not square.
    pub target_extent: [u32; 2],
    /// Normalized focal point inside the source; `[0.5, 0.5]` centers it.
    pub focal_point: [f32; 2],
    pub nine_slice: Option<NineSlice>,
}

impl ImageLayout {
    pub const fn new(fit: ImageFit, target_extent: [u32; 2]) -> Self {
        Self {
            fit,
            target_extent,
            focal_point: [0.5; 2],
            nine_slice: None,
        }
    }

    pub const fn stretch(target_extent: [u32; 2]) -> Self {
        Self::new(ImageFit::Stretch, target_extent)
    }

    pub const fn contain(target_extent: [u32; 2]) -> Self {
        Self::new(ImageFit::Contain, target_extent)
    }

    pub const fn cover(target_extent: [u32; 2]) -> Self {
        Self::new(ImageFit::Cover, target_extent)
    }

    pub fn with_focal_point(mut self, focal_point: [f32; 2]) -> Self {
        self.focal_point = focal_point;
        self
    }

    pub fn with_nine_slice(mut self, slices: NineSlice) -> Self {
        self.nine_slice = Some(slices);
        self
    }

    /// Nine-slice keeps the source corners at their authored pixel size while
    /// the destination rectangle can hold both insets.
    pub fn sliced(source: [f32; 4], target_extent: [u32; 2]) -> Self {
        Self::stretch(target_extent).with_nine_slice(NineSlice::from_source_pixels(
            source,
            target_extent,
        ))
    }
}

/// Retained stylization of one paint command.
/// The producer emits one paint command per styled surface and keeps the
/// original painter order. `clips` is an ancestor stack ordered from outermost
/// to innermost.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaintStyle {
    pub radii: [[f32; 2]; 4],
    pub border: Option<PaintBorder>,
    pub gradient: Option<LinearGradient>,
    pub clips: Vec<RoundedClip>,
    /// Horizontal and vertical feather widths in normalized target units. The
    /// shape edge stays at half coverage and the fade spans the full width.
    pub softness: [f32; 2],
    pub image: Option<ImageLayout>,
}

impl PaintStyle {
    /// A shape with one uniform corner radius.
    pub fn rounded(radius: f32) -> Self {
        Self {
            radii: [[radius, radius]; 4],
            ..Self::default()
        }
    }

    pub fn with_radii(mut self, radii: [[f32; 2]; 4]) -> Self {
        self.radii = radii;
        self
    }

    pub fn with_border(mut self, border: PaintBorder) -> Self {
        self.border = Some(border);
        self
    }

    pub fn with_gradient(mut self, gradient: LinearGradient) -> Self {
        self.gradient = Some(gradient);
        self
    }

    pub fn with_clip(mut self, clip: RoundedClip) -> Self {
        self.clips.push(clip);
        self
    }

    pub fn with_clips(mut self, clips: impl IntoIterator<Item = RoundedClip>) -> Self {
        self.clips.extend(clips);
        self
    }

    pub fn with_softness(mut self, horizontal: f32, vertical: f32) -> Self {
        self.softness = [horizontal, vertical];
        self
    }

    pub fn with_image(mut self, layout: ImageLayout) -> Self {
        self.image = Some(layout);
        self
    }

    /// True when the style adds nothing beyond the plain quad.
    pub fn is_plain(&self) -> bool {
        self.radii == [[0.; 2]; 4]
            && self.border.is_none()
            && self.gradient.is_none()
            && self.clips.is_empty()
            && self.softness == [0.; 2]
            && self.image.is_none()
    }
}
