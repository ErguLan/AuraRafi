use serde::{Deserialize, Serialize};

use crate::focus::UiFocusState;
use crate::node::{UiNode, UiNodeKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStyle {
    pub fill: [u8; 4],
    pub border: [u8; 4],
    pub text: [u8; 4],
    pub border_width: f32,
    pub radius: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

fn default_opacity() -> f32 {
    1.0
}

impl UiStyle {
    pub fn transparent() -> Self {
        Self {
            fill: [0, 0, 0, 0],
            border: [0, 0, 0, 0],
            text: [220, 220, 224, 255],
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        }
    }
}

/// Serializable partial style used by the CSS-like retained style sheet.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiStylePatch {
    pub fill: Option<[u8; 4]>,
    pub border: Option<[u8; 4]>,
    pub text: Option<[u8; 4]>,
    pub border_width: Option<f32>,
    pub radius: Option<f32>,
    pub opacity: Option<f32>,
}

impl UiStylePatch {
    pub fn apply_to(&self, style: &mut UiStyle) {
        if let Some(value) = self.fill {
            style.fill = value;
        }
        if let Some(value) = self.border {
            style.border = value;
        }
        if let Some(value) = self.text {
            style.text = value;
        }
        if let Some(value) = self.border_width {
            style.border_width = value.max(0.0);
        }
        if let Some(value) = self.radius {
            style.radius = value.max(0.0);
        }
        if let Some(value) = self.opacity {
            style.opacity = value.clamp(0.0, 1.0);
        }
    }
}

/// Per-frame UI state used when resolving retained style rules. It intentionally
/// borrows focus ids instead of storing them in a document, keeping saved UI
/// data independent from one editor window's transient pointer state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiVisualState<'a> {
    pub hovered_id: Option<&'a str>,
    pub focused_id: Option<&'a str>,
    pub active_id: Option<&'a str>,
}

impl<'a> UiVisualState<'a> {
    pub fn from_focus(focus: &'a UiFocusState) -> Self {
        Self {
            hovered_id: focus.hovered.as_deref(),
            focused_id: focus.focused.as_deref(),
            active_id: focus.active.as_deref(),
        }
    }

    fn is_hovered(self, id: &str) -> bool {
        self.hovered_id == Some(id)
    }

    fn is_focused(self, id: &str) -> bool {
        self.focused_id == Some(id)
    }

    fn is_active(self, id: &str) -> bool {
        self.active_id == Some(id)
    }
}

/// Small, deterministic selector vocabulary for Rust-authored UI documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiStyleSelector {
    Id(String),
    Class(String),
    Kind(UiNodeKind),
}

/// Optional pseudo-state for a style rule. `Always` is the normal cascade
/// layer, while later state-specific rules can override it without embedding
/// runtime state into a `UiNode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiStyleRuleState {
    Always,
    Hovered,
    Focused,
    Active,
    Disabled,
}

impl Default for UiStyleRuleState {
    fn default() -> Self {
        Self::Always
    }
}

impl UiStyleRuleState {
    fn matches(self, node: &UiNode, state: UiVisualState<'_>) -> bool {
        match self {
            Self::Always => true,
            Self::Hovered => !node.disabled && state.is_hovered(&node.id),
            Self::Focused => !node.disabled && state.is_focused(&node.id),
            Self::Active => !node.disabled && state.is_active(&node.id),
            Self::Disabled => node.disabled,
        }
    }
}

impl UiStyleSelector {
    fn matches(&self, node: &UiNode) -> bool {
        match self {
            Self::Id(id) => node.id == *id,
            Self::Class(class_name) => node.classes.iter().any(|class| class == class_name),
            Self::Kind(kind) => node.kind == *kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStyleRule {
    pub selector: UiStyleSelector,
    pub patch: UiStylePatch,
    #[serde(default)]
    pub state: UiStyleRuleState,
}

impl UiStyleRule {
    pub fn new(selector: UiStyleSelector, patch: UiStylePatch) -> Self {
        Self {
            selector,
            patch,
            state: UiStyleRuleState::Always,
        }
    }

    pub fn when(mut self, state: UiStyleRuleState) -> Self {
        self.state = state;
        self
    }
}

/// Ordered retained style rules. Later matching rules win, just like a small
/// CSS cascade, while node-local styles remain the base layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UiStyleSheet {
    pub rules: Vec<UiStyleRule>,
}

impl UiStyleSheet {
    pub fn resolve(&self, node: &UiNode) -> UiStyle {
        self.resolve_with_state(node, UiVisualState::default())
    }

    pub fn resolve_with_state(&self, node: &UiNode, state: UiVisualState<'_>) -> UiStyle {
        let mut resolved = node.style.clone();
        for rule in &self.rules {
            if rule.selector.matches(node) && rule.state.matches(node, state) {
                rule.patch.apply_to(&mut resolved);
            }
        }
        resolved
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StudioUiPalette {
    IndustrialDark,
    PaperLight,
}

impl StudioUiPalette {
    pub fn tokens(self) -> UiTokens {
        match self {
            Self::IndustrialDark => UiTokens {
                background: [8, 8, 8, 255],
                surface: [17, 17, 17, 248],
                surface_alt: [28, 28, 28, 248],
                border: [52, 52, 52, 255],
                text: [238, 238, 238, 255],
                text_muted: [162, 162, 162, 255],
                accent: [224, 116, 24, 255],
                accent_hot: [255, 151, 46, 255],
            },
            Self::PaperLight => UiTokens {
                background: [250, 250, 250, 255],
                surface: [255, 255, 255, 250],
                surface_alt: [238, 238, 238, 255],
                border: [210, 210, 210, 255],
                text: [28, 28, 30, 255],
                text_muted: [96, 96, 100, 255],
                accent: [224, 116, 24, 255],
                accent_hot: [178, 82, 16, 255],
            },
        }
    }

    pub fn root_style(self) -> UiStyle {
        let tokens = self.tokens();
        UiStyle {
            fill: tokens.background,
            border: tokens.border,
            text: tokens.text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        }
    }

    pub fn panel_style(self) -> UiStyle {
        let tokens = self.tokens();
        UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        }
    }

    pub fn toolbar_style(self) -> UiStyle {
        let tokens = self.tokens();
        UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        }
    }

    pub fn canvas_style(self) -> UiStyle {
        let tokens = self.tokens();
        UiStyle {
            fill: tokens.background,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        }
    }

    pub fn subtle_panel_style(self) -> UiStyle {
        let tokens = self.tokens();
        UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text_muted,
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        }
    }

    pub fn accent_style(self) -> UiStyle {
        let tokens = self.tokens();
        UiStyle {
            fill: tokens.accent,
            border: tokens.accent_hot,
            text: match self {
                Self::IndustrialDark => [18, 18, 20, 255],
                Self::PaperLight => [255, 255, 255, 255],
            },
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiTokens {
    pub background: [u8; 4],
    pub surface: [u8; 4],
    pub surface_alt: [u8; 4],
    pub border: [u8; 4],
    pub text: [u8; 4],
    pub text_muted: [u8; 4],
    pub accent: [u8; 4],
    pub accent_hot: [u8; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stylesheet_applies_ordered_class_and_id_overrides() {
        let node = UiNode::new("launch", UiNodeKind::Button).with_class("primary");
        let sheet = UiStyleSheet {
            rules: vec![
                UiStyleRule::new(
                    UiStyleSelector::Class("primary".to_string()),
                    UiStylePatch {
                        fill: Some([224, 116, 24, 255]),
                        ..UiStylePatch::default()
                    },
                ),
                UiStyleRule::new(
                    UiStyleSelector::Id("launch".to_string()),
                    UiStylePatch {
                        radius: Some(6.0),
                        ..UiStylePatch::default()
                    },
                ),
            ],
        };

        let resolved = sheet.resolve(&node);
        assert_eq!(resolved.fill, [224, 116, 24, 255]);
        assert_eq!(resolved.radius, 6.0);
    }

    #[test]
    fn state_rule_overrides_base_only_while_node_is_hovered() {
        let node = UiNode::new("launch", UiNodeKind::Button).with_class("primary");
        let sheet = UiStyleSheet {
            rules: vec![
                UiStyleRule::new(
                    UiStyleSelector::Class("primary".to_string()),
                    UiStylePatch {
                        fill: Some([224, 116, 24, 255]),
                        ..UiStylePatch::default()
                    },
                ),
                UiStyleRule::new(
                    UiStyleSelector::Class("primary".to_string()),
                    UiStylePatch {
                        fill: Some([255, 151, 46, 255]),
                        ..UiStylePatch::default()
                    },
                )
                .when(UiStyleRuleState::Hovered),
            ],
        };

        assert_eq!(sheet.resolve(&node).fill, [224, 116, 24, 255]);
        assert_eq!(
            sheet
                .resolve_with_state(
                    &node,
                    UiVisualState {
                        hovered_id: Some("launch"),
                        ..UiVisualState::default()
                    },
                )
                .fill,
            [255, 151, 46, 255]
        );
    }
}
