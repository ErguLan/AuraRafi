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

/// A retained binary control. The host owns the authoritative value and
/// rebuilds the document after it receives a `UiAction::SetToggle` action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiToggle {
    pub value_key: String,
    #[serde(default)]
    pub value: bool,
}

impl UiToggle {
    pub fn new(value_key: impl Into<String>, value: bool) -> Self {
        Self {
            value_key: value_key.into(),
            value,
        }
    }
}

/// A retained numeric range. It deliberately stores only presentation and
/// input metadata; persistence remains at the application boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiRange {
    pub value_key: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    #[serde(default = "default_range_step")]
    pub step: f32,
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
        }
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
}
