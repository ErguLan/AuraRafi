//! Optional presentation requests. Documents never contain backend resources.
use serde::{Deserialize, Serialize};

/// Optional component colors; absent overrides derive from semantic theme tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiChromeTokens {
    pub icon: [u8; 4],
    pub icon_muted: [u8; 4],
    pub icon_accent: [u8; 4],
    pub icon_on_accent: [u8; 4],
    pub icon_disabled: [u8; 4],
    pub divider: [u8; 4],
    pub scrollbar: [u8; 4],
    pub scrollbar_hover: [u8; 4],
    pub focus_ring: [u8; 4],
    pub shadow: [u8; 4],
    pub overlay_scrim: [u8; 4],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiContentAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiIconPlacement {
    #[default]
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiContentLayout {
    pub align: UiContentAlign,
    pub icon_placement: UiIconPlacement,
    pub gap: f32,
}

impl Default for UiContentLayout {
    fn default() -> Self {
        Self {
            align: UiContentAlign::Start,
            icon_placement: UiIconPlacement::Start,
            gap: 6.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiLinearGradient {
    pub start: [u8; 4],
    pub end: [u8; 4],
    /// Normalized direction in local coordinates; [0, 1] is vertical.
    pub direction: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiShadow {
    pub offset: [f32; 2],
    /// Bounded analytic edge falloff, not a framebuffer blur.
    pub softness: f32,
    pub spread: f32,
    pub color: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiFocusRing {
    pub width: f32,
    pub offset: f32,
    pub color: [u8; 4],
}

/// Corners are top-left, top-right, bottom-right, bottom-left. Border widths
/// are top, right, bottom, left. Opacity remains per primitive, not isolation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiVisual {
    /// Explicitly inherit the parent's resolved text/icon color. Geometry,
    /// borders and group opacity are never inherited implicitly.
    pub inherit_text: bool,
    pub corners: Option<[f32; 4]>,
    pub border_widths: Option<[f32; 4]>,
    pub gradient: Option<UiLinearGradient>,
    pub shadow: Option<UiShadow>,
    pub focus_ring: Option<UiFocusRing>,
    pub clip_children: bool,
    pub content: UiContentLayout,
}
