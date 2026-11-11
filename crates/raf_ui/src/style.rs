use serde::{Deserialize, Serialize};

use crate::controls::UiControl;
use crate::environment::UiColorMode;
use crate::focus::UiFocusState;
use crate::node::{UiNode, UiNodeKind};

/// Semantic material requested by retained UI chrome.
///
/// RafUI records intent only. ApiGraphicBasic resolves the effective fill and
/// border alpha for the active palette, accessibility preferences, and render
/// budget before producing the shared CPU/GPU draw list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiSurfaceMaterial {
    #[default]
    Opaque,
    TranslucentChrome,
    TranslucentRaised,
    ModalSurface,
    BackdropScrim,
}

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
    pub fn with_fill(mut self, color: [u8; 4]) -> Self {
        self.fill = color;
        self
    }
    pub fn with_text(mut self, color: [u8; 4]) -> Self {
        self.text = color;
        self
    }
    pub fn with_border(mut self, color: [u8; 4], width: f32) -> Self {
        self.border = color;
        self.border_width = width.max(0.0);
        self
    }
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius.max(0.0);
        self
    }
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
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
    /// All transient inputs consumed by style rules and interaction defaults.
    /// Retained presenters can reuse a target style until this key changes.
    pub fn style_state_key(self, node: &UiNode) -> u8 {
        u8::from(self.is_hovered(&node.id))
            | (u8::from(self.is_focused(&node.id)) << 1)
            | (u8::from(self.is_active(&node.id)) << 2)
            | (u8::from(node.disabled) << 3)
            | (u8::from(self.is_selected(node)) << 4)
            | (u8::from(self.is_open(node)) << 5)
            | (u8::from(self.is_invalid(node)) << 6)
    }

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

    fn is_selected(self, node: &UiNode) -> bool {
        self.selected_id == Some(node.id.as_str())
            || node.accessibility_selected == Some(true)
            || matches!(&node.control, UiControl::Toggle(toggle) if toggle.value)
    }

    fn is_open(self, node: &UiNode) -> bool {
        self.open_id == Some(node.id.as_str())
            || node.accessibility_expanded == Some(true)
            || matches!(&node.control, UiControl::Select(select) if select.open)
    }

    fn is_invalid(self, node: &UiNode) -> bool {
        self.invalid_id == Some(node.id.as_str()) || node.invalid
    }
}

/// Effective semantic control carried by a node. Explicit control data takes
/// precedence over the structural node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiControlKind {
    Button,
    Image,
    TextInput,
    Toggle,
    Range,
    ColorPicker,
    Select,
    ScrollView,
    Grid,
    Skeleton,
}

impl UiControlKind {
    fn from_node(node: &UiNode) -> Option<Self> {
        match &node.control {
            UiControl::Image(_) => Some(Self::Image),
            UiControl::TextInput(_) => Some(Self::TextInput),
            UiControl::Toggle(_) => Some(Self::Toggle),
            UiControl::Range(_) => Some(Self::Range),
            UiControl::ColorPicker(_) => Some(Self::ColorPicker),
            UiControl::Select(_) => Some(Self::Select),
            UiControl::ScrollView { .. } => Some(Self::ScrollView),
            UiControl::Grid => Some(Self::Grid),
            UiControl::Skeleton(_) => Some(Self::Skeleton),
            UiControl::None => match node.kind {
                UiNodeKind::Button => Some(Self::Button),
                UiNodeKind::Image => Some(Self::Image),
                UiNodeKind::TextInput => Some(Self::TextInput),
                UiNodeKind::ScrollView => Some(Self::ScrollView),
                UiNodeKind::Grid => Some(Self::Grid),
                UiNodeKind::Skeleton => Some(Self::Skeleton),
                _ => None,
            },
        }
    }

    fn has_interaction_states(self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::TextInput
                | Self::Toggle
                | Self::Range
                | Self::ColorPicker
                | Self::Select
        )
    }
}

/// Small, deterministic selector vocabulary for Rust-authored UI documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiStyleSelector {
    Id(String),
    Class(String),
    Kind(UiNodeKind),
    Control(UiControlKind),
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
            Self::Selected => !node.disabled && state.is_selected(node),
            Self::Open => !node.disabled && state.is_open(node),
            Self::Invalid => state.is_invalid(node),
        }
    }
}

impl UiStyleSelector {
    pub fn matches(&self, node: &UiNode) -> bool {
        match self {
            Self::Id(id) => node.id == *id,
            Self::Class(class_name) => node.classes.iter().any(|class| class == class_name),
            Self::Kind(kind) => node.kind == *kind,
            Self::Control(kind) => UiControlKind::from_node(node) == Some(*kind),
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

/// Semantic precedence group for author-provided style rules.
///
/// [`UiStyleLayers::compile`] orders these as base, component, variant, state,
/// then explicit override. This keeps state rules above ordinary variants
/// without changing the serialized `UiStyleSheet` shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiStyleLayer {
    Base,
    Component,
    Variant,
    State,
    Override,
}

/// Authoring-time style groups compiled into the existing ordered rule list.
/// State rules outrank base/component/variant rules; the explicit `overrides`
/// group is the only group with higher precedence than state rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiStyleLayers {
    pub base: Vec<UiStyleRule>,
    pub component: Vec<UiStyleRule>,
    pub variant: Vec<UiStyleRule>,
    pub state: Vec<UiStyleRule>,
    pub overrides: Vec<UiStyleRule>,
}

impl UiStyleLayers {
    pub fn rules(&self, layer: UiStyleLayer) -> &[UiStyleRule] {
        match layer {
            UiStyleLayer::Base => &self.base,
            UiStyleLayer::Component => &self.component,
            UiStyleLayer::Variant => &self.variant,
            UiStyleLayer::State => &self.state,
            UiStyleLayer::Override => &self.overrides,
        }
    }

    pub fn rules_mut(&mut self, layer: UiStyleLayer) -> &mut Vec<UiStyleRule> {
        match layer {
            UiStyleLayer::Base => &mut self.base,
            UiStyleLayer::Component => &mut self.component,
            UiStyleLayer::Variant => &mut self.variant,
            UiStyleLayer::State => &mut self.state,
            UiStyleLayer::Override => &mut self.overrides,
        }
    }

    pub fn push(&mut self, layer: UiStyleLayer, rule: UiStyleRule) {
        self.rules_mut(layer).push(rule);
    }

    pub fn extend(&mut self, layer: UiStyleLayer, rules: impl IntoIterator<Item = UiStyleRule>) {
        self.rules_mut(layer).extend(rules);
    }

    /// Flattens semantic groups to the legacy rule representation in stable
    /// low-to-high precedence order.
    pub fn compile(&self) -> Vec<UiStyleRule> {
        self.base
            .iter()
            .chain(&self.component)
            .chain(&self.variant)
            .chain(&self.state)
            .chain(&self.overrides)
            .cloned()
            .collect()
    }
}

impl UiStyleSheet {
    /// Appends a rule group after existing rules, preserving its order.
    /// Appended matching rules therefore have higher precedence.
    pub fn append_layer(&mut self, rules: impl IntoIterator<Item = UiStyleRule>) {
        self.rules.extend(rules);
    }

    /// Builder counterpart to [`Self::append_layer`].
    pub fn with_layer(mut self, rules: impl IntoIterator<Item = UiStyleRule>) -> Self {
        self.append_layer(rules);
        self
    }

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
        let has_interaction_defaults = UiControlKind::from_node(node)
            .map(UiControlKind::has_interaction_states)
            .unwrap_or(false);
        if has_interaction_defaults {
            if !node.disabled
                && state.is_hovered(&node.id)
                && !self.has_state_rule(node, state, UiStyleRuleState::Hovered)
            {
                // A transparent authored fill must not turn into an opaque
                // grey block when the control is hovered. Use a faint tint of
                // the resolved ink instead, so the affordance stays quiet on
                // any authored surface (including surface-less controls).
                resolved.fill = if resolved.fill[3] == 0 {
                    [resolved.text[0], resolved.text[1], resolved.text[2], 18]
                } else {
                    lift_hover_color(resolved.fill, 8)
                };
                resolved.border = if resolved.border[3] == 0 {
                    [resolved.text[0], resolved.text[1], resolved.text[2], 44]
                } else {
                    lift_hover_color(resolved.border, 24)
                };
                resolved.border_width = resolved.border_width.max(1.0);
            } else if !node.disabled
                && state.is_focused(&node.id)
                && !self.has_state_rule(node, state, UiStyleRuleState::Focused)
            {
                resolved.border = lift_focus_color(resolved.border);
                resolved.border_width = resolved.border_width.max(1.0);
            }
            if node.disabled && !self.has_state_rule(node, state, UiStyleRuleState::Disabled) {
                resolved.opacity = resolved.opacity.min(0.56);
            }
            if !node.disabled
                && state.is_active(&node.id)
                && !self.has_state_rule(node, state, UiStyleRuleState::Active)
            {
                resolved.opacity = resolved.opacity.min(0.92);
            }
        }
        if state.is_invalid(node) && !self.has_state_rule(node, state, UiStyleRuleState::Invalid) {
            // A validation state must remain visible even when a surface did
            // not author a dedicated invalid rule. Surfaces can still
            // override this fallback through the normal cascade.
            resolved.border = [203, 73, 73, 255];
            resolved.border_width = resolved.border_width.max(1.0);
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
    #[serde(default)]
    pub components: UiComponentMetrics,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiComponentMetricOverrides {
    pub height: Option<f32>,
    pub horizontal_padding: Option<f32>,
    pub icon_gap: Option<f32>,
    pub corner_radius: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiComponentMetrics {
    pub button: UiComponentMetricOverrides,
    pub image: UiComponentMetricOverrides,
    pub text_input: UiComponentMetricOverrides,
    pub toggle: UiComponentMetricOverrides,
    pub range: UiComponentMetricOverrides,
    pub color_picker: UiComponentMetricOverrides,
    pub select: UiComponentMetricOverrides,
    pub scroll_view: UiComponentMetricOverrides,
    pub grid: UiComponentMetricOverrides,
    pub skeleton: UiComponentMetricOverrides,
}

impl UiComponentMetrics {
    pub fn overrides_for(self, kind: UiControlKind) -> UiComponentMetricOverrides {
        match kind {
            UiControlKind::Button => self.button,
            UiControlKind::Image => self.image,
            UiControlKind::TextInput => self.text_input,
            UiControlKind::Toggle => self.toggle,
            UiControlKind::Range => self.range,
            UiControlKind::ColorPicker => self.color_picker,
            UiControlKind::Select => self.select,
            UiControlKind::ScrollView => self.scroll_view,
            UiControlKind::Grid => self.grid,
            UiControlKind::Skeleton => self.skeleton,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiResolvedComponentMetrics {
    pub height: f32,
    pub horizontal_padding: f32,
    pub icon_gap: f32,
    pub corner_radius: f32,
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

fn default_info_token() -> [u8; 4] {
    [70, 115, 220, 255]
}

impl Default for UiThemeMetrics {
    fn default() -> Self {
        Self {
            scale: default_theme_scale(),
            control_height: default_control_height(),
            corner_radius: default_corner_radius(),
            spacing: default_spacing(),
            components: UiComponentMetrics::default(),
        }
    }
}

impl UiThemeMetrics {
    pub fn for_control(self, kind: UiControlKind) -> UiResolvedComponentMetrics {
        let overrides = self.components.overrides_for(kind);
        UiResolvedComponentMetrics {
            height: overrides.height.unwrap_or(self.control_height),
            horizontal_padding: overrides.horizontal_padding.unwrap_or(self.spacing),
            icon_gap: overrides.icon_gap.unwrap_or(6.0),
            corner_radius: overrides.corner_radius.unwrap_or(self.corner_radius),
        }
    }
}

impl UiTheme {
    /// Neutral defaults for user-authored game interfaces, independent of editor chrome.
    pub fn game_ui() -> Self {
        let mut theme = Self::raf_ui();
        theme.name = "Game UI".into();
        for tokens in [&mut theme.dark, &mut theme.light] {
            tokens.background = [0; 4];
            tokens.canvas = [0; 4];
            tokens.surface = [255; 4];
            tokens.surface_raised = [255; 4];
            tokens.surface_alt = [245, 245, 245, 255];
            tokens.border = [200, 200, 200, 255];
            tokens.text = [32, 32, 32, 255];
            tokens.text_muted = [105, 105, 105, 255];
            tokens.accent = [245, 245, 245, 255];
            tokens.accent_hot = [220, 220, 220, 255];
            tokens.focus = [96, 96, 96, 255];
            tokens.selection = [128, 128, 128, 48];
        }
        theme
    }

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
                chrome: None,
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
                info: [70, 115, 220, 255],
                warning: [224, 160, 42, 255],
                danger: [203, 73, 73, 255],
                skeleton: [48, 48, 48, 255],
            },
            Self::PaperLight => UiTokens {
                chrome: None,
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
                info: [44, 95, 184, 255],
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
    #[serde(default)]
    pub chrome: Option<crate::visual::UiChromeTokens>,
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
    /// Informational accent, used by blue world-axis affordances and other
    /// non-destructive status UI.
    #[serde(default = "default_info_token")]
    pub info: [u8; 4],
    pub warning: [u8; 4],
    pub danger: [u8; 4],
    pub skeleton: [u8; 4],
}

impl UiTokens {
    pub fn chrome(self) -> crate::visual::UiChromeTokens {
        self.chrome
            .unwrap_or_else(|| crate::visual::UiChromeTokens {
                icon: self.text,
                icon_muted: self.text_muted,
                icon_accent: self.accent,
                icon_on_accent: self.background,
                icon_disabled: [
                    self.text_muted[0],
                    self.text_muted[1],
                    self.text_muted[2],
                    140,
                ],
                divider: self.border,
                scrollbar: self.text_muted,
                scrollbar_hover: self.accent,
                focus_ring: self.focus,
                shadow: [0, 0, 0, 85],
                overlay_scrim: [0, 0, 0, 128],
            })
    }
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

    #[test]
    fn control_selector_matches_effective_control_data_and_layer_order() {
        let node = UiNode::toggle("visibility", crate::UiToggle::new("visible", true));
        let first_layer = vec![UiStyleRule::new(
            UiStyleSelector::Control(UiControlKind::Toggle),
            UiStylePatch {
                fill: Some([20, 30, 40, 255]),
                ..UiStylePatch::default()
            },
        )];
        let second_layer = vec![UiStyleRule::new(
            UiStyleSelector::Control(UiControlKind::Toggle),
            UiStylePatch {
                fill: Some([40, 50, 60, 255]),
                ..UiStylePatch::default()
            },
        )];
        let sheet = UiStyleSheet::default()
            .with_layer(first_layer)
            .with_layer(second_layer);

        assert_eq!(sheet.resolve(&node).fill, [40, 50, 60, 255]);
    }

    #[test]
    fn named_style_layers_compile_in_semantic_precedence_order() {
        let selector = UiStyleSelector::Class("primary".to_string());
        let make_rule = |color, state| UiStyleRule {
            selector: selector.clone(),
            patch: UiStylePatch {
                fill: Some(color),
                ..UiStylePatch::default()
            },
            state,
        };
        let mut layers = UiStyleLayers {
            base: vec![make_rule([10, 0, 0, 255], UiStyleRuleState::Always)],
            component: vec![make_rule([20, 0, 0, 255], UiStyleRuleState::Always)],
            variant: vec![make_rule([30, 0, 0, 255], UiStyleRuleState::Always)],
            state: vec![make_rule([40, 0, 0, 255], UiStyleRuleState::Hovered)],
            overrides: Vec::new(),
        };
        let node = UiNode::new("launch", UiNodeKind::Button).with_class("primary");
        let state = UiVisualState {
            hovered_id: Some("launch"),
            ..UiVisualState::default()
        };

        let compiled = layers.compile();
        let compiled_fills: Vec<_> = compiled.iter().filter_map(|rule| rule.patch.fill).collect();
        assert_eq!(
            compiled_fills,
            vec![
                [10, 0, 0, 255],
                [20, 0, 0, 255],
                [30, 0, 0, 255],
                [40, 0, 0, 255]
            ]
        );
        assert_eq!(
            UiStyleSheet { rules: compiled }
                .resolve_with_state(&node, state)
                .fill,
            [40, 0, 0, 255]
        );

        layers.push(
            UiStyleLayer::Override,
            make_rule([50, 0, 0, 255], UiStyleRuleState::Always),
        );
        assert_eq!(layers.rules(UiStyleLayer::Override).len(), 1);
        assert_eq!(
            UiStyleSheet {
                rules: layers.compile()
            }
            .resolve_with_state(&node, state)
            .fill,
            [50, 0, 0, 255]
        );
    }

    #[test]
    fn component_metrics_use_theme_defaults_with_typed_overrides() {
        let mut metrics = UiThemeMetrics::default();
        metrics.components.button.height = Some(36.0);
        metrics.components.button.icon_gap = Some(9.0);

        let button = metrics.for_control(UiControlKind::Button);
        let text_input = metrics.for_control(UiControlKind::TextInput);
        assert_eq!(button.height, 36.0);
        assert_eq!(button.icon_gap, 9.0);
        assert_eq!(text_input.height, default_control_height());
        assert_eq!(text_input.horizontal_padding, default_spacing());
    }
}
