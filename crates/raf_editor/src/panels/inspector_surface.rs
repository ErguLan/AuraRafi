//! Declarative retained RafUI surface for the game Inspector.
//!
//! The document contains presentation and semantic events only. Scene edits
//! are translated by `inspector_surface_host.rs` and committed by the editor
//! application so they share selection, history and persistence with
//! Hierarchy.

use glam::Vec3;
use raf_core::scene::{
    NodeColor, Primitive, SceneGraph, SceneNodeId, SceneVariable, VariableValue,
};
use raf_core::session::{ProjectSessionKind, ProjectSessionRegistry};
use raf_core::units::DisplayUnit;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiImage, UiImageFit, UiImageSource, UiSurface,
};
use raf_ui::components::select_trigger;
use raf_ui::{
    UiAlign, UiColorPicker, UiEventBinding, UiEventKind, UiFlow, UiIconSize, UiJustify, UiLayout,
    UiNode, UiNodeKind, UiOverflow, UiRange, UiRect, UiScrollAxis, UiSelect, UiSelectOption,
    UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector,
    UiStyleSheet, UiSurfaceMaterial, UiTextInput, UiTextOverflow, UiTextStyle, UiToggle,
};

use crate::color_math::rgb_to_hsv;

#[path = "inspector_behavior_surface.rs"]
pub(crate) mod behavior_surface;

mod behavior;
mod components;
mod fields;
mod runtime;
mod sections;
#[cfg(test)]
mod tests;
mod transform;

pub(crate) use behavior::variable_value_text;
pub(crate) use components::{action_button, sessions_content};
pub(crate) use fields::{dropdown_field, section_label};
use components::{header, tabs};
use sections::content;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorDropdown {
    Primitive,
    Collider,
    BodyType,
    Texture,
    Parameter { source: Option<usize>, index: usize },
    BehaviorSource,
    PrefabSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorTab {
    Properties,
    Sessions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorSection {
    Identity,
    Transform,
    Appearance,
    Components,
    Variables,
    Audio,
    Physics,
    Metadata,
    Debug,
}

impl InspectorSection {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Transform => "transform",
            Self::Appearance => "appearance",
            Self::Components => "components",
            Self::Variables => "variables",
            Self::Audio => "audio",
            Self::Physics => "physics",
            Self::Metadata => "metadata",
            Self::Debug => "debug",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InspectorViewState {
    pub tab: InspectorTab,
    pub identity: bool,
    pub transform: bool,
    pub appearance: bool,
    pub components: bool,
    pub variables: bool,
    pub audio: bool,
    pub physics: bool,
    pub metadata: bool,
    pub debug: bool,
    pub dropdown: Option<InspectorDropdown>,
    pub color_picker: bool,
    pub behavior_parameters: Option<usize>,
}

impl Default for InspectorViewState {
    fn default() -> Self {
        Self {
            tab: InspectorTab::Properties,
            identity: true,
            transform: true,
            appearance: true,
            components: true,
            variables: false,
            audio: false,
            physics: false,
            metadata: false,
            debug: false,
            dropdown: None,
            color_picker: false,
            behavior_parameters: None,
        }
    }
}

impl InspectorViewState {
    pub fn section(self, section: InspectorSection) -> bool {
        match section {
            InspectorSection::Identity => self.identity,
            InspectorSection::Transform => self.transform,
            InspectorSection::Appearance => self.appearance,
            InspectorSection::Components => self.components,
            InspectorSection::Variables => self.variables,
            InspectorSection::Audio => self.audio,
            InspectorSection::Physics => self.physics,
            InspectorSection::Metadata => self.metadata,
            InspectorSection::Debug => self.debug,
        }
    }
}

pub fn build_inspector_surface(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    sessions: &ProjectSessionRegistry,
    transition: f32,
    view: InspectorViewState,
) -> UiSurface {
    build_inspector_surface_with_unit(
        palette,
        scene,
        selected,
        sessions,
        transition,
        view,
        DisplayUnit::Metric,
    )
}

pub fn build_inspector_surface_with_unit(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    sessions: &ProjectSessionRegistry,
    transition: f32,
    view: InspectorViewState,
    display_unit: DisplayUnit,
) -> UiSurface {
    build_inspector_surface_with_assets(
        palette,
        scene,
        selected,
        sessions,
        transition,
        view,
        display_unit,
        &[],
    )
}

pub fn build_inspector_surface_with_assets(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    selected: Option<SceneNodeId>,
    sessions: &ProjectSessionRegistry,
    transition: f32,
    view: InspectorViewState,
    display_unit: DisplayUnit,
    project_assets: &[String],
) -> UiSurface {
    let body=content(palette,scene,selected,sessions,view,display_unit,project_assets);
    build_inspector_with_content(palette,transition,view,body)
}

/// Shared inspector shell for scene properties and contextual document properties.
pub fn build_inspector_with_content(palette:StudioUiPalette,transition:f32,view:InspectorViewState,body:UiNode)->UiSurface {
    let mut root_style = palette.panel_style();
    root_style.opacity = (0.55 + transition.clamp(0.0, 1.0) * 0.45).clamp(0.0, 1.0);
    let root = UiNode::new("inspector.root", UiNodeKind::Root)
        .with_class("inspector-root")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            padding: UiSpacing::same(6.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(root_style)
        .with_child(header(palette))
        .with_child(tabs(palette, view.tab))
        .with_child(body);

    let mut surface = UiSurface::new("editor.inspector", palette, root);
    surface.style_sheet = inspector_style_sheet(palette);
    surface
}

pub(crate) fn inspector_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    UiStyleSheet {
        rules: vec![
            class_rule(
                "inspector-root",
                with_alpha(tokens.surface, 238),
                with_alpha(tokens.border, 180),
                tokens.text,
            ),
            class_rule(
                "inspector-header",
                with_alpha(tokens.surface_raised, 238),
                with_alpha(tokens.border, 190),
                tokens.text,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-tabs".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_alt, 220)),
                    border: Some(with_alpha(tokens.border, 140)),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-tab".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-tab-active".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 245)),
                    border: Some(with_alpha(tokens.border, 180)),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            class_rule(
                "inspector-content",
                with_alpha(tokens.surface, 220),
                with_alpha(tokens.border, 150),
                tokens.text,
            ),
            class_rule(
                "inspector-sessions-content",
                with_alpha(tokens.surface, 210),
                with_alpha(tokens.border, 140),
                tokens.text,
            ),
            class_rule(
                "inspector-sessions-controls",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text,
            ),
            class_rule(
                "inspector-sessions-heading",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text_muted,
            ),
            class_rule(
                "inspector-session-create-row",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text,
            ),
            class_rule(
                "inspector-session-list",
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                tokens.text,
            ),
            class_rule(
                "inspector-session-row",
                with_alpha(tokens.surface_alt, 175),
                with_alpha(tokens.border, 130),
                tokens.text,
            ),
            class_rule(
                "inspector-session-active",
                with_alpha(tokens.surface_raised, 240),
                with_alpha(tokens.accent, 160),
                tokens.text,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-kind-chip".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_alt, 210)),
                    border: Some(with_alpha(tokens.border, 110)),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-active-badge".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.accent, 25)),
                    border: Some(with_alpha(tokens.accent, 120)),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-action-btn".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-delete-btn".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            class_rule(
                "inspector-session-create",
                palette.accent_style().fill,
                palette.accent_style().border,
                palette.accent_style().text,
            ),
            class_rule(
                "inspector-input",
                with_alpha(tokens.surface_alt, 205),
                with_alpha(tokens.border, 180),
                tokens.text,
            ),
            class_rule(
                "inspector-session-context",
                with_alpha(tokens.surface_alt, 190),
                with_alpha(tokens.border, 150),
                tokens.text,
            ),
            class_rule(
                "inspector-number-input",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-button",
                with_alpha(tokens.surface_alt, 195),
                with_alpha(tokens.border, 175),
                tokens.text,
            ),
            class_rule(
                "inspector-icon-button",
                with_alpha(tokens.surface_alt, 175),
                with_alpha(tokens.border, 160),
                tokens.text_muted,
            ),
            class_rule(
                "inspector-dropdown-trigger",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-dropdown-menu",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-dropdown-option",
                tokens.surface_alt,
                [0, 0, 0, 0],
                tokens.text_muted,
            ),
            class_rule(
                "inspector-dropdown-option-selected",
                [116, 67, 24, 72],
                tokens.accent,
                tokens.text,
            ),
            class_rule(
                "inspector-color-trigger",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-color-popover",
                tokens.surface_raised,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-color-hex-input",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-color-visual",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-status-on",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-status-off",
                tokens.surface,
                tokens.border,
                tokens.text_muted,
            ),
            class_rule(
                "inspector-range",
                tokens.surface_alt,
                tokens.border,
                tokens.text,
            ),
            class_rule(
                "inspector-section-title",
                tokens.surface_alt,
                tokens.border,
                tokens.text_muted,
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-section-title".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 225)),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-dropdown-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-button".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 225)),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-row".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 215)),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-tab".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 130)),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-tab-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(with_alpha(tokens.accent, 170)),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-tab".to_string()),
                UiStylePatch {
                    border: Some(with_alpha(tokens.accent, 140)),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-tab-active".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-create".to_string()),
                UiStylePatch {
                    fill: Some(palette.accent_style().border),
                    border: Some(palette.accent_style().border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-action-btn".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 240)),
                    border: Some(with_alpha(tokens.border, 180)),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-action-btn".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-delete-btn".to_string()),
                UiStylePatch {
                    fill: Some([180, 45, 45, 120]),
                    border: Some([220, 60, 60, 200]),
                    border_width: Some(1.0),
                    text: Some([255, 140, 140, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-delete-btn".to_string()),
                UiStylePatch {
                    border: Some([220, 60, 60, 255]),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-icon-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-create".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-button".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface, 150)),
                    border: Some(with_alpha(tokens.border, 110)),
                    text: Some(tokens.text_muted),
                    opacity: Some(0.72),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Disabled),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-manage".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 200)),
                    border: Some(with_alpha(tokens.border, 150)),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-manage".to_string()),
                UiStylePatch {
                    fill: Some(with_alpha(tokens.surface_raised, 240)),
                    border: Some(tokens.accent),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-session-manage".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-number-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-color-hex-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("inspector-color-surface".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
        ],
    }
}

fn with_alpha(color: [u8; 4], alpha: u8) -> [u8; 4] {
    [color[0], color[1], color[2], alpha]
}

fn class_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(1.0),
            radius: Some(3.0),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}
