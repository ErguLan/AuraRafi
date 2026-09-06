use serde::{Deserialize, Serialize};

/// A stable resource key. The renderer or host resolves it to a project asset
/// or built-in icon; UI documents never embed file system paths or image bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiImageSource {
    pub key: String,
}

impl UiImageSource {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiImageFit {
    Contain,
    Cover,
    Stretch,
}

impl Default for UiImageFit {
    fn default() -> Self {
        Self::Contain
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiImage {
    pub source: UiImageSource,
    #[serde(default)]
    pub fit: UiImageFit,
    #[serde(default)]
    pub tint: Option<[u8; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiTextInput {
    /// Key in transient UI state. The host can mirror it into a command,
    /// property, or runtime binding without mutating the document itself.
    pub value_key: String,
    #[serde(default)]
    pub placeholder_key: Option<String>,
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub password: bool,
    #[serde(default)]
    pub submit_command: Option<String>,
}

fn default_max_length() -> usize {
    4096
}

impl UiTextInput {
    pub fn new(value_key: impl Into<String>) -> Self {
        Self {
            value_key: value_key.into(),
            placeholder_key: None,
            max_length: default_max_length(),
            multiline: false,
            password: false,
            submit_command: None,
        }
    }
}

/// A renderer-neutral option used by Select/ComboBox recipes. Values are
/// emitted to the host; labels remain localization keys owned by the surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSelectOption {
    pub value: String,
    pub label_key: String,
    #[serde(default)]
    pub disabled: bool,
}

impl UiSelectOption {
    pub fn new(value: impl Into<String>, label_key: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label_key: label_key.into(),
            disabled: false,
        }
    }
}

/// Additive Select contract. Existing button/menu recipes remain compatible;
/// this control centralizes the behavior for new and migrated dropdowns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSelect {
    pub value_key: String,
    pub options: Vec<UiSelectOption>,
    #[serde(default)]
    pub selected_index: usize,
    #[serde(default)]
    pub active_index: usize,
    #[serde(default)]
    pub open: bool,
    #[serde(default = "default_select_wrap")]
    pub wrap: bool,
    /// Optional retained node id for the popup that renders this select's
    /// options. Keeping the association in the control lets the interaction
    /// layer close a menu on click-away even when the popup is authored as a
    /// sibling overlay rather than a child of the trigger.
    #[serde(default)]
    pub popup_id: Option<String>,
}

fn default_select_wrap() -> bool {
    true
}

impl UiSelect {
    pub fn new(
        value_key: impl Into<String>,
        options: Vec<UiSelectOption>,
        selected_index: usize,
    ) -> Self {
        let selected_index = selected_index.min(options.len().saturating_sub(1));
        Self {
            value_key: value_key.into(),
            options,
            selected_index,
            active_index: selected_index,
            open: false,
            wrap: true,
            popup_id: None,
        }
    }

    pub fn with_popup_id(mut self, popup_id: impl Into<String>) -> Self {
        self.popup_id = Some(popup_id.into());
        self
    }

    pub fn active_option(&self) -> Option<&UiSelectOption> {
        self.options.get(self.active_index)
    }

    pub fn selected_option(&self) -> Option<&UiSelectOption> {
        self.options.get(self.selected_index)
    }

    pub fn next_enabled_index(&self, from: usize, forward: bool) -> Option<usize> {
        if self.options.is_empty() {
            return None;
        }
        let mut index = from.min(self.options.len() - 1);
        for _ in 0..self.options.len() {
            let next = if forward {
                if index + 1 >= self.options.len() {
                    if !self.wrap {
                        return None;
                    }
                    0
                } else {
                    index + 1
                }
            } else if index == 0 {
                if !self.wrap {
                    return None;
                }
                self.options.len() - 1
            } else {
                index - 1
            };
            index = next;
            if !self.options[index].disabled {
                return Some(index);
            }
        }
        None
    }
}

/// Visual treatment for a retained binary control. Interaction and
/// accessibility semantics remain identical for both variants; this only
/// changes how the renderer presents the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiTogglePresentation {
    Switch,
    Checkbox,
}

impl Default for UiTogglePresentation {
    fn default() -> Self {
        Self::Switch
    }
}

/// A retained binary control. The host owns the authoritative value and
/// rebuilds the document after it receives a `UiAction::SetToggle` action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiToggle {
    pub value_key: String,
    #[serde(default)]
    pub value: bool,
    #[serde(default)]
    pub presentation: UiTogglePresentation,
}

impl UiToggle {
    pub fn new(value_key: impl Into<String>, value: bool) -> Self {
        Self {
            value_key: value_key.into(),
            value,
            presentation: UiTogglePresentation::Switch,
        }
    }

    pub fn with_presentation(mut self, presentation: UiTogglePresentation) -> Self {
        self.presentation = presentation;
        self
    }
}

/// A retained numeric range. It deliberately stores only presentation and
/// input metadata; persistence remains at the application boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiRangeOrientation {
    Horizontal,
    Vertical,
}

impl Default for UiRangeOrientation {
    fn default() -> Self {
        Self::Horizontal
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiRange {
    pub value_key: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    #[serde(default = "default_range_step")]
    pub step: f32,
    #[serde(default)]
    pub orientation: UiRangeOrientation,
}

fn default_range_step() -> f32 {
    1.0
}

impl UiRange {
    pub fn new(value_key: impl Into<String>, value: f32, min: f32, max: f32, step: f32) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let step = if step.is_finite() && step > 0.0 {
            step
        } else {
            default_range_step()
        };
        Self {
            value_key: value_key.into(),
            value: value.clamp(min, max),
            min,
            max,
            step,
            orientation: UiRangeOrientation::Horizontal,
        }
    }

    pub fn with_orientation(mut self, orientation: UiRangeOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn value_from_fraction(&self, fraction: f32) -> f32 {
        let raw = self.min + (self.max - self.min) * fraction.clamp(0.0, 1.0);
        let steps = ((raw - self.min) / self.step).round();
        (self.min + steps * self.step).clamp(self.min, self.max)
    }

    pub fn fraction(&self) -> f32 {
        let span = self.max - self.min;
        if span <= f32::EPSILON {
            0.0
        } else {
            ((self.value - self.min) / span).clamp(0.0, 1.0)
        }
    }
}

/// A two-dimensional HSV surface. The retained control carries the current
/// hue/saturation/value so the interaction layer can emit a complete color
/// update from a pointer position without coupling RafUI to a renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiColorPicker {
    pub value_key: String,
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiColorPickerHit {
    Hue(f32),
    SaturationValue { saturation: f32, value: f32 },
}

impl UiColorPicker {
    pub fn new(value_key: impl Into<String>, hue: f32, saturation: f32, value: f32) -> Self {
        Self {
            value_key: value_key.into(),
            hue: hue.rem_euclid(360.0),
            saturation: saturation.clamp(0.0, 1.0),
            value: value.clamp(0.0, 1.0),
        }
    }

    pub const CANVAS_SIZE: f32 = 256.0;
    pub const RING_INNER_RADIUS: f32 = 96.0;
    pub const RING_OUTER_RADIUS: f32 = 114.0;
    pub const SQUARE_START: f32 = 62.0;
    pub const SQUARE_SIDE: f32 = 132.0;

    pub fn hsv_to_rgb_bytes(hue: f32, saturation: f32, value: f32) -> [u8; 3] {
        let hue = hue.rem_euclid(360.0) / 60.0;
        let sector = hue.floor() as u32;
        let fraction = hue - sector as f32;
        let saturation = saturation.clamp(0.0, 1.0);
        let value = value.clamp(0.0, 1.0);
        let p = value * (1.0 - saturation);
        let q = value * (1.0 - saturation * fraction);
        let t = value * (1.0 - saturation * (1.0 - fraction));
        let (red, green, blue) = match sector {
            0 => (value, t, p),
            1 => (q, value, p),
            2 => (p, value, t),
            3 => (p, q, value),
            4 => (t, p, value),
            _ => (value, p, q),
        };
        [
            (red * 255.0).round() as u8,
            (green * 255.0).round() as u8,
            (blue * 255.0).round() as u8,
        ]
    }

    /// Maps a pointer in the rendered picker bounds to the semantic HSV area
    /// that was hit. The renderer can change the visual size without changing
    /// color math or input behavior.
    pub fn hit_test(&self, position: [f32; 2], size: [f32; 2]) -> Option<UiColorPickerHit> {
        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
            return None;
        }
        let x = position[0].clamp(0.0, size[0]) / size[0] * Self::CANVAS_SIZE;
        let y = position[1].clamp(0.0, size[1]) / size[1] * Self::CANVAS_SIZE;
        let square_end = Self::SQUARE_START + Self::SQUARE_SIDE;
        if (Self::SQUARE_START..square_end).contains(&x)
            && (Self::SQUARE_START..square_end).contains(&y)
        {
            return Some(UiColorPickerHit::SaturationValue {
                saturation: ((x - Self::SQUARE_START) / (Self::SQUARE_SIDE - 1.0)).clamp(0.0, 1.0),
                value: (1.0 - (y - Self::SQUARE_START) / (Self::SQUARE_SIDE - 1.0)).clamp(0.0, 1.0),
            });
        }

        let dx = x - Self::CANVAS_SIZE * 0.5;
        let dy = y - Self::CANVAS_SIZE * 0.5;
        let distance = (dx * dx + dy * dy).sqrt();
        if (Self::RING_INNER_RADIUS..=Self::RING_OUTER_RADIUS).contains(&distance) {
            return Some(UiColorPickerHit::Hue(
                (dy.atan2(dx).to_degrees() + 90.0).rem_euclid(360.0),
            ));
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiScrollAxis {
    Vertical,
    Horizontal,
    Both,
}

impl Default for UiScrollAxis {
    fn default() -> Self {
        Self::Vertical
    }
}

impl UiScrollAxis {
    pub fn scrolls_horizontally(self) -> bool {
        matches!(self, Self::Horizontal | Self::Both)
    }

    pub fn scrolls_vertically(self) -> bool {
        matches!(self, Self::Vertical | Self::Both)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiSkeletonShape {
    Text,
    Rectangle,
    Circle,
}

impl Default for UiSkeletonShape {
    fn default() -> Self {
        Self::Rectangle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiSkeleton {
    #[serde(default)]
    pub shape: UiSkeletonShape,
    #[serde(default = "default_skeleton_phase")]
    pub phase: f32,
}

fn default_skeleton_phase() -> f32 {
    1.0
}

impl Default for UiSkeleton {
    fn default() -> Self {
        Self {
            shape: UiSkeletonShape::Rectangle,
            phase: default_skeleton_phase(),
        }
    }
}

/// Extra data for a node kind. Keeping this enum on the retained node makes
/// controls serializable while leaving platform input and rendering elsewhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiControl {
    None,
    Image(UiImage),
    TextInput(UiTextInput),
    Toggle(UiToggle),
    Range(UiRange),
    ColorPicker(UiColorPicker),
    Select(UiSelect),
    ScrollView {
        #[serde(default)]
        axis: UiScrollAxis,
    },
    Grid,
    Skeleton(UiSkeleton),
}

impl Default for UiControl {
    fn default() -> Self {
        Self::None
    }
}

impl UiControl {
    pub fn text_input(&self) -> Option<&UiTextInput> {
        match self {
            Self::TextInput(input) => Some(input),
            _ => None,
        }
    }

    pub fn scroll_axis(&self) -> Option<UiScrollAxis> {
        match self {
            Self::ScrollView { axis } => Some(*axis),
            _ => None,
        }
    }

    pub fn toggle(&self) -> Option<&UiToggle> {
        match self {
            Self::Toggle(toggle) => Some(toggle),
            _ => None,
        }
    }

    pub fn range(&self) -> Option<&UiRange> {
        match self {
            Self::Range(range) => Some(range),
            _ => None,
        }
    }

    pub fn color_picker(&self) -> Option<&UiColorPicker> {
        match self {
            Self::ColorPicker(picker) => Some(picker),
            _ => None,
        }
    }

    pub fn select(&self) -> Option<&UiSelect> {
        match self {
            Self::Select(select) => Some(select),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UiColorPicker, UiColorPickerHit};

    #[test]
    fn color_picker_hit_test_maps_square_and_ring() {
        let picker = UiColorPicker::new("color", 210.0, 0.4, 0.6);

        assert_eq!(
            picker.hit_test([193.0, 62.0], [256.0, 256.0]),
            Some(UiColorPickerHit::SaturationValue {
                saturation: 1.0,
                value: 1.0,
            })
        );
        assert_eq!(
            picker.hit_test([62.0, 193.0], [256.0, 256.0]),
            Some(UiColorPickerHit::SaturationValue {
                saturation: 0.0,
                value: 0.0,
            })
        );

        match picker.hit_test([242.0, 128.0], [256.0, 256.0]) {
            Some(UiColorPickerHit::Hue(hue)) => assert!((hue - 90.0).abs() < 0.001),
            other => panic!("expected hue hit, got {other:?}"),
        }
    }

    #[test]
    fn color_picker_hit_test_scales_with_visual_bounds() {
        let picker = UiColorPicker::new("color", 0.0, 0.0, 1.0);
        match picker.hit_test([165.0, 54.0], [220.0, 220.0]) {
            Some(UiColorPickerHit::SaturationValue { saturation, value }) => {
                assert!(saturation > 0.98);
                assert!(value > 0.98);
            }
            other => panic!("expected square hit, got {other:?}"),
        }
    }
}
