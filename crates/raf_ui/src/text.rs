//! Renderer-neutral typography requests.
//!
//! RafUI declares text roles, semantic weights, and measurement inputs. Font
//! loading, shaping, rasterization, atlas allocation, and texture upload live
//! behind ApiGraphicBasic so the UI model stays portable and testable.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiFontWeight {
    Regular,
    Medium,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiTextRole {
    Body,
    Label,
    /// Product wordmark text. Presentation backends may choose a dedicated
    /// face and tracking policy for this role.
    Brand,
    Button,
    PanelTitle,
    Toolbar,
    Tooltip,
    Monospace,
}

/// Overflow policy for a retained text request. The renderer remains free to
/// choose its raster implementation, but every backend receives the same
/// single-line/wrap contract. `Wrap` prefers word boundaries and hard-breaks
/// only when a single token is wider than the available line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiTextOverflow {
    Wrap,
    Clip,
    Ellipsis,
}

impl Default for UiTextOverflow {
    fn default() -> Self {
        Self::Wrap
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiTextStyle {
    pub role: UiTextRole,
    pub size_px: f32,
    pub line_height_px: f32,
    pub weight: UiFontWeight,
    pub color: [u8; 4],
    #[serde(default)]
    pub inherit_color: bool,
}

impl UiTextStyle {
    /// Shared semantic typography; panels should not recreate role metrics.
    pub fn for_role(role: UiTextRole, color: [u8; 4]) -> Self {
        let mut style = match role {
            UiTextRole::Button | UiTextRole::Toolbar => Self::button(color),
            UiTextRole::PanelTitle => Self::panel_title(color),
            UiTextRole::Label | UiTextRole::Tooltip => Self {
                role, size_px: 12.0, line_height_px: 16.0,
                weight: UiFontWeight::Regular, color, inherit_color: false,
            },
            _ => Self::body(color),
        };
        style.role = role;
        style
    }

    pub fn label(color: [u8; 4]) -> Self { Self::for_role(UiTextRole::Label, color) }

    pub fn toolbar(color: [u8; 4]) -> Self { Self::for_role(UiTextRole::Toolbar, color) }

    /// Changes type size while keeping the authored line-height proportion.
    pub fn with_size(self, size: f32) -> Self {
        if !size.is_finite() || size <= 0.0 { return self; }
        let ratio = self.line_height_px / self.size_px.max(1.0);
        Self { size_px: size, line_height_px: size * ratio, ..self }
    }

    pub fn body(color: [u8; 4]) -> Self {
        Self {
            role: UiTextRole::Body,
            size_px: 13.0,
            line_height_px: 18.0,
            weight: UiFontWeight::Regular,
            color,
            inherit_color: false,
        }
    }

    pub fn button(color: [u8; 4]) -> Self {
        Self {
            role: UiTextRole::Button,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: UiFontWeight::Medium,
            color,
            inherit_color: false,
        }
    }

    pub fn panel_title(color: [u8; 4]) -> Self {
        Self {
            role: UiTextRole::PanelTitle,
            size_px: 12.0,
            line_height_px: 16.0,
            weight: UiFontWeight::Bold,
            color,
            inherit_color: false,
        }
    }

    pub fn inherit_theme_color(mut self) -> Self {
        self.inherit_color = true;
        self
    }

    /// Scales semantic typography while preserving role, weight, and color.
    /// Raster density is applied later by `UiTextAtlasRequest::scaled_for_raster`.
    pub fn scaled_for_ui(&self, scale: f32) -> Self {
        let scale = if scale.is_finite() {
            scale.clamp(0.5, 3.0)
        } else {
            1.0
        };
        let mut style = *self;
        style.size_px = (style.size_px * scale).max(1.0);
        style.line_height_px = (style.line_height_px * scale).max(1.0);
        style
    }
}

/// Source for one styled fragment in a rendered inline text row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiTextRunContent {
    /// Resolve this value through the surface's translation callback.
    Key(String),
    /// Render this literal runtime value without translation.
    Value(String),
}

/// One styled fragment consumed by `components::inline_text_row`.
///
/// Fragments are ordinary retained text nodes. The row does not wrap between
/// runs; callers that need wrapping should author separate rows explicitly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiTextRun {
    pub content: UiTextRunContent,
    pub style: UiTextStyle,
}

impl UiTextRun {
    pub fn key(key: impl Into<String>, style: UiTextStyle) -> Self {
        Self {
            content: UiTextRunContent::Key(key.into()),
            style,
        }
    }

    pub fn value(value: impl Into<String>, style: UiTextStyle) -> Self {
        Self {
            content: UiTextRunContent::Value(value.into()),
            style,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiTextAtlasRequest {
    pub node_id: String,
    pub text_key: String,
    pub style: UiTextStyle,
    pub max_width: f32,
    #[serde(default)]
    pub overflow: UiTextOverflow,
    #[serde(default)]
    pub single_line: bool,
}

impl UiTextAtlasRequest {
    pub fn new(
        node_id: impl Into<String>,
        text_key: impl Into<String>,
        style: UiTextStyle,
        max_width: f32,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            text_key: text_key.into(),
            style,
            max_width: max_width.max(0.0),
            overflow: UiTextOverflow::Wrap,
            single_line: false,
        }
    }

    pub fn with_overflow(mut self, overflow: UiTextOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn with_single_line(mut self, single_line: bool) -> Self {
        self.single_line = single_line;
        if single_line && self.overflow == UiTextOverflow::Wrap {
            self.overflow = UiTextOverflow::Clip;
        }
        self
    }

    /// Scales only raster inputs. Layout and pointer coordinates remain in
    /// logical points, which prevents a DPI change from changing hit regions.
    pub fn scaled_for_raster(&self, scale: f32) -> Self {
        let scale = scale.clamp(1.0, 4.0);
        let mut request = self.clone();
        request.style.size_px = (request.style.size_px * scale).max(1.0);
        request.style.line_height_px = (request.style.line_height_px * scale).max(1.0);
        request.max_width = (request.max_width * scale).max(0.0);
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_scaling_preserves_semantic_identity() {
        let request = UiTextAtlasRequest::new(
            "panel.title",
            "app.settings",
            UiTextStyle::panel_title([255, 255, 255, 255]),
            180.0,
        );
        let scaled = request.scaled_for_raster(1.5);

        assert_eq!(scaled.node_id, request.node_id);
        assert_eq!(scaled.text_key, request.text_key);
        assert_eq!(scaled.style.weight, request.style.weight);
        assert_eq!(scaled.style.size_px, 18.0);
        assert_eq!(scaled.max_width, 270.0);
        assert_eq!(scaled.overflow, request.overflow);
        assert_eq!(scaled.single_line, request.single_line);
    }
}
