use serde::{Deserialize, Serialize};

use crate::environment::UiColorMode;
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
    pub selected_id: Option<&'a str>,
    pub open_id: Option<&'a str>,
    pub invalid_id: Option<&'a str>,
}

impl<'a> UiVisualState<'a> {
    pub fn from_focus(focus: &'a UiFocusState) -> Self {
        Self {
            hovered_id: focus.hovered.as_deref(),
            focused_id: focus.focused.as_deref(),
            active_id: focus.active.as_deref(),
            selected_id: None,
            open_id: None,
            invalid_id: None,
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

    fn is_selected(self, id: &str) -> bool {
        self.selected_id == Some(id)
    }

    fn is_open(self, id: &str) -> bool {
        self.open_id == Some(id)
    }

    fn is_invalid(self, id: &str) -> bool {
        self.invalid_id == Some(id)
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
    Selected,
    Open,
    Invalid,
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
            Self::Selected => !node.disabled && state.is_selected(&node.id),
            Self::Open => !node.disabled && state.is_open(&node.id),
            Self::Invalid => state.is_invalid(&node.id),
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

        // Text inputs need a usable interaction affordance even when a surface
        // did not author a dedicated `:hover` rule. Explicit rules still win;
        // this fallback only gives the retained control a quiet visual lift.
        if node.control.text_input().is_some() {
            if state.is_hovered(&node.id)
                && !self.has_state_rule(node, state, UiStyleRuleState::Hovered)
            {
                resolved.fill = lift_hover_color(resolved.fill, 8);
                resolved.border = lift_hover_color(resolved.border, 24);
                resolved.border_width = resolved.border_width.max(1.0);
            } else if state.is_focused(&node.id)
                && !self.has_state_rule(node, state, UiStyleRuleState::Focused)
            {
                resolved.border = lift_focus_color(resolved.border);
                resolved.border_width = resolved.border_width.max(1.0);
            }
        }
        resolved
    }

    fn has_state_rule(
        &self,
        node: &UiNode,
        state: UiVisualState<'_>,
        requested: UiStyleRuleState,
    ) -> bool {
        self.rules.iter().any(|rule| {
            rule.state == requested
                && rule.selector.matches(node)
                && rule.state.matches(node, state)
        })
    }
}

fn lift_hover_color(mut color: [u8; 4], amount: u8) -> [u8; 4] {
    if color[3] == 0 {
        color = [112, 120, 132, 220];
    } else {
        color[0] = color[0].saturating_add(amount);
        color[1] = color[1].saturating_add(amount);
        color[2] = color[2].saturating_add(amount);
    }
    color
}

fn lift_focus_color(mut color: [u8; 4]) -> [u8; 4] {
    if color[3] == 0 {
        return [232, 133, 28, 240];
    }
    color[0] = color[0].saturating_add(32);
    color[1] = color[1].saturating_add(32);
    color[2] = color[2].saturating_add(32);
    color
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StudioUiPalette {
    IndustrialDark,
    PaperLight,
}

/// Named, serializable theme configuration. The engine ships the RafUI dark
/// and light defaults, but a project or user can replace every semantic token
/// without touching any panel code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiTheme {
    pub name: String,
    #[serde(default)]
    pub preferred_mode: UiColorMode,
    pub dark: UiTokens,
    pub light: UiTokens,
    #[serde(default)]
    pub metrics: UiThemeMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiThemeMetrics {
    /// User scale applied by the host before a surface is laid out.
    #[serde(default = "default_theme_scale")]
    pub scale: f32,
    #[serde(default = "default_control_height")]
    pub control_height: f32,
    #[serde(default = "default_corner_radius")]
    pub corner_radius: f32,
    #[serde(default = "default_spacing")]
    pub spacing: f32,
}

fn default_theme_scale() -> f32 {
    1.0
}

fn default_control_height() -> f32 {
    28.0
}

fn default_corner_radius() -> f32 {
    5.0
}

fn default_spacing() -> f32 {
    8.0
}

impl Default for UiThemeMetrics {
    fn default() -> Self {
        Self {
            scale: default_theme_scale(),
            control_height: default_control_height(),
            corner_radius: default_corner_radius(),
            spacing: default_spacing(),
        }
    }
}

impl UiTheme {
    pub fn raf_ui() -> Self {
        Self {
            name: "RafUI".to_string(),
            preferred_mode: UiColorMode::System,
            dark: StudioUiPalette::IndustrialDark.tokens(),
            light: StudioUiPalette::PaperLight.tokens(),
            metrics: UiThemeMetrics::default(),
        }
    }

    pub fn tokens_for(&self, mode: UiColorMode, system_dark: bool) -> UiTokens {
        match mode {
            UiColorMode::Dark => self.dark,
            UiColorMode::Light => self.light,
            UiColorMode::System => {
                if system_dark {
                    self.dark
                } else {
                    self.light
                }
            }
        }
    }

    pub fn with_dark_tokens(mut self, tokens: UiTokens) -> Self {
        self.dark = tokens;
        self
    }

    pub fn with_light_tokens(mut self, tokens: UiTokens) -> Self {
        self.light = tokens;
        self
    }

    pub fn root_style(&self, mode: UiColorMode, system_dark: bool) -> UiStyle {
        let tokens = self.tokens_for(mode, system_dark);
        UiStyle {
            fill: tokens.background,
            border: tokens.border,
            text: tokens.text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        }
    }

    pub fn panel_style(&self, mode: UiColorMode, system_dark: bool) -> UiStyle {
        let tokens = self.tokens_for(mode, system_dark);
        UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: self.metrics.corner_radius.max(0.0),
            opacity: 1.0,
        }
    }

    pub fn input_style(&self, mode: UiColorMode, system_dark: bool) -> UiStyle {
        let tokens = self.tokens_for(mode, system_dark);
        UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: self.metrics.corner_radius.max(0.0),
            opacity: 1.0,
        }
    }

    pub fn accent_style(&self, mode: UiColorMode, system_dark: bool) -> UiStyle {
        let tokens = self.tokens_for(mode, system_dark);
        UiStyle {
            fill: tokens.accent,
            border: tokens.accent_hot,
            text: if system_dark || mode == UiColorMode::Dark {
                [18, 18, 20, 255]
            } else {
                [255, 255, 255, 255]
            },
            border_width: 1.0,
            radius: self.metrics.corner_radius.max(0.0),
            opacity: 1.0,
        }
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::raf_ui()
    }
}

impl StudioUiPalette {
    pub fn tokens(self) -> UiTokens {
        match self {
            Self::IndustrialDark => UiTokens {
                background: [8, 11, 15, 255],
                surface: [13, 17, 22, 255],
                surface_alt: [18, 23, 29, 255],
                surface_raised: [25, 31, 38, 255],
                canvas: [9, 12, 16, 255],
                border: [38, 45, 54, 255],
                text: [237, 239, 242, 255],
                text_muted: [151, 159, 170, 255],
                accent: [232, 133, 28, 255],
                accent_hot: [255, 166, 61, 255],
                focus: [255, 166, 61, 255],
                selection: [232, 133, 28, 72],
                positive: [78, 162, 102, 255],
                warning: [224, 160, 42, 255],
                danger: [203, 73, 73, 255],
                skeleton: [48, 48, 48, 255],
            },
            Self::PaperLight => UiTokens {
                background: [250, 250, 250, 255],
                surface: [255, 255, 255, 250],
                surface_alt: [238, 238, 238, 255],
                surface_raised: [255, 255, 255, 255],
                canvas: [247, 247, 247, 255],
                border: [210, 210, 210, 255],
                text: [28, 28, 30, 255],
                text_muted: [96, 96, 100, 255],
                accent: [224, 116, 24, 255],
                accent_hot: [178, 82, 16, 255],
                focus: [178, 82, 16, 255],
                selection: [224, 116, 24, 58],
                positive: [53, 126, 74, 255],
                warning: [170, 111, 19, 255],
                danger: [177, 57, 57, 255],
                skeleton: [222, 222, 222, 255],
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
    pub surface_raised: [u8; 4],
    pub canvas: [u8; 4],
    pub border: [u8; 4],
    pub text: [u8; 4],
    pub text_muted: [u8; 4],
    pub accent: [u8; 4],
    pub accent_hot: [u8; 4],
    pub focus: [u8; 4],
    pub selection: [u8; 4],
    pub positive: [u8; 4],
    pub warning: [u8; 4],
    pub danger: [u8; 4],
    pub skeleton: [u8; 4],
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

    #[test]
    fn text_input_gets_a_default_hover_affordance_without_a_surface_rule() {
        let node = UiNode::text_input("query", crate::UiTextInput::new("query.value"));
        let base = UiStyleSheet::default().resolve(&node);
        let hovered = UiStyleSheet::default().resolve_with_state(
            &node,
            UiVisualState {
                hovered_id: Some("query"),
                ..UiVisualState::default()
            },
        );

        assert_ne!(hovered.fill, base.fill);
        assert_ne!(hovered.border, base.border);
        assert_eq!(hovered.border_width, 1.0);
    }
}
