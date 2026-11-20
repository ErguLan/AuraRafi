//! Style sheet of the Electronics toolbar band.
//!
//! The band has one resting treatment per control and one saturated accent fill
//! for the tool in hand. Engaged options, unavailable commands, destructive
//! actions and the tool islands keep their own rules here so the surface
//! builders only declare structure.

use raf_render::api_graphic_basic::ui_surface::{
    UiStylePatch, UiStyleRuleState, UiStyleSheet,
};
use raf_ui::UiTokens;

use crate::panels::electronics_surface::{
    electronics_active_rule, electronics_class_rule, electronics_focus_rule,
    electronics_hover_rule, electronics_state_rule, with_alpha, DANGER_HOVER_ALPHA,
    DANGER_PRESS_ALPHA, ELECTRONICS_BORDER_WIDTH, ELECTRONICS_CORNER_RADIUS,
};

use super::{
    CLASS_BUTTON, CLASS_DANGER, CLASS_OPTION_ON, CLASS_TOOL_ACTIVE, CLASS_TOOL_ISLAND_CONNECT,
    CLASS_TOOL_ISLAND_PLACE, CLASS_TOOL_ISLAND_SELECT, CLASS_UNAVAILABLE,
};

/// Alpha of the hover and focus wash of an engaged option, in 0..255. It is
/// derived from the accent token so an option keeps the accent hue without ever
/// using the accent as body text, which would fail contrast in the light theme.
const OPTION_HOVER_ALPHA: u8 = 92;
/// Alpha of the pressed wash of an engaged option, in 0..255.
const OPTION_ACTIVE_ALPHA: u8 = 120;
/// Opacity of a command whose prerequisite is missing, in 0..1.
const UNAVAILABLE_OPACITY: f32 = 0.4;
/// Opacity of that command while hovered or focused, in 0..1. It stays dimmer
/// than every available control so it can never read as ready.
const UNAVAILABLE_ATTENTION_OPACITY: f32 = 0.58;

pub(super) fn style_sheet(tokens: &UiTokens) -> UiStyleSheet {
    let mut rules = vec![
        electronics_class_rule(
            "electronics-toolbar",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        electronics_state_rule(
            UiStyleRuleState::Always,
            "electronics-toolbar-separator",
            UiStylePatch {
                fill: Some(with_alpha(tokens.border, 120)),
                border: Some([0, 0, 0, 0]),
                text: Some([0, 0, 0, 0]),
                border_width: Some(0.0),
                radius: Some(0.0),
                opacity: Some(0.65),
                ..UiStylePatch::default()
            },
        ),
        // The document segment is the only bordered container of the band.
        electronics_class_rule(
            "electronics-toolbar-segment",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        // Every control shares one resting treatment: flat, quiet and
        // borderless, so group boundaries are carried by the hairlines and the
        // band corners instead of sixteen competing outlines.
        electronics_state_rule(
            UiStyleRuleState::Always,
            CLASS_BUTTON,
            UiStylePatch {
                fill: Some([0, 0, 0, 0]),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text_muted),
                border_width: Some(0.0),
                radius: Some(ELECTRONICS_CORNER_RADIUS),
                ..UiStylePatch::default()
            },
        ),
    ];
    // Resting tool islands use quiet neutral fills. Their boundaries are
    // carried by surface tone and spacing; only the tool in hand uses the
    // saturated accent.
    rules.extend([
        electronics_state_rule(
            UiStyleRuleState::Always,
            CLASS_TOOL_ISLAND_SELECT,
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some([0, 0, 0, 0]),
                border_width: Some(0.0),
                radius: Some(ELECTRONICS_CORNER_RADIUS),
                ..UiStylePatch::default()
            },
        ),
        electronics_state_rule(
            UiStyleRuleState::Always,
            CLASS_TOOL_ISLAND_CONNECT,
            UiStylePatch {
                fill: Some(tokens.surface_alt),
                border: Some([0, 0, 0, 0]),
                border_width: Some(0.0),
                radius: Some(ELECTRONICS_CORNER_RADIUS),
                ..UiStylePatch::default()
            },
        ),
        electronics_state_rule(
            UiStyleRuleState::Always,
            CLASS_TOOL_ISLAND_PLACE,
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some([0, 0, 0, 0]),
                border_width: Some(0.0),
                radius: Some(ELECTRONICS_CORNER_RADIUS),
                ..UiStylePatch::default()
            },
        ),
    ]);
    rules.push(electronics_hover_rule(CLASS_BUTTON, *tokens));
    rules.push(electronics_focus_rule(CLASS_BUTTON, *tokens));
    rules.push(electronics_active_rule(CLASS_BUTTON, *tokens));

    // The tool in hand: the only saturated accent fill of the band. Hover keeps
    // the fill so a pointer crossing an engaged tool does not read as a state
    // change, and focus brightens it because the resting border already carries
    // the accent edge the shared focus ring would use.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Always,
        CLASS_TOOL_ACTIVE,
        UiStylePatch {
            fill: Some(tokens.accent),
            border: Some(tokens.accent_hot),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            radius: Some(ELECTRONICS_CORNER_RADIUS),
            text: Some(tokens.background),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        CLASS_TOOL_ACTIVE,
        UiStylePatch {
            fill: Some(tokens.accent),
            border: Some(tokens.focus),
            text: Some(tokens.background),
            ..UiStylePatch::default()
        },
    ));
    for state in [UiStyleRuleState::Focused, UiStyleRuleState::Active] {
        rules.push(electronics_state_rule(
            state,
            CLASS_TOOL_ACTIVE,
            UiStylePatch {
                fill: Some(tokens.accent_hot),
                border: Some(tokens.focus),
                ..UiStylePatch::default()
            },
        ));
    }

    // An engaged option, and the engaged document segment, use the selection
    // token: the same hue as the active tool but a pale wash with a neutral edge,
    // so the two states are told apart by geometry and not by hue alone. The
    // neutral resting border is also what keeps the shared focus ring visible on
    // top of it.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Always,
        CLASS_OPTION_ON,
        UiStylePatch {
            fill: Some(tokens.selection),
            border: Some(tokens.border),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            radius: Some(ELECTRONICS_CORNER_RADIUS),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    for (state, alpha) in [
        (UiStyleRuleState::Hovered, OPTION_HOVER_ALPHA),
        (UiStyleRuleState::Focused, OPTION_HOVER_ALPHA),
        (UiStyleRuleState::Active, OPTION_ACTIVE_ALPHA),
    ] {
        rules.push(electronics_state_rule(
            state,
            CLASS_OPTION_ON,
            UiStylePatch {
                fill: Some(with_alpha(tokens.accent, alpha)),
                border: Some(tokens.accent_hot),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        ));
    }
    rules.push(electronics_focus_rule(CLASS_OPTION_ON, *tokens));

    // A command whose prerequisite is missing stays visible and dimmed, keeps a
    // hover and a focus affordance so its reason can be read, and never reaches
    // the contrast of an available control.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Always,
        CLASS_UNAVAILABLE,
        UiStylePatch {
            fill: Some([0, 0, 0, 0]),
            border: Some([0, 0, 0, 0]),
            border_width: Some(0.0),
            text: Some(tokens.text_muted),
            opacity: Some(UNAVAILABLE_OPACITY),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        CLASS_UNAVAILABLE,
        UiStylePatch {
            fill: Some(tokens.surface_raised),
            opacity: Some(UNAVAILABLE_ATTENTION_OPACITY),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Focused,
        CLASS_UNAVAILABLE,
        UiStylePatch {
            border: Some(tokens.focus),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            opacity: Some(UNAVAILABLE_ATTENTION_OPACITY),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Active,
        CLASS_UNAVAILABLE,
        UiStylePatch {
            fill: Some(tokens.surface_raised),
            border: Some([0, 0, 0, 0]),
            border_width: Some(0.0),
            opacity: Some(UNAVAILABLE_ATTENTION_OPACITY),
            ..UiStylePatch::default()
        },
    ));

    // The destructive action keeps the danger token while it is available and the
    // shared warm focus ring for keyboard focus.
    rules.push(electronics_state_rule(
        UiStyleRuleState::Hovered,
        CLASS_DANGER,
        UiStylePatch {
            fill: Some(with_alpha(tokens.danger, DANGER_HOVER_ALPHA)),
            border: Some(tokens.danger),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_state_rule(
        UiStyleRuleState::Active,
        CLASS_DANGER,
        UiStylePatch {
            fill: Some(with_alpha(tokens.danger, DANGER_PRESS_ALPHA)),
            border: Some(tokens.danger),
            border_width: Some(ELECTRONICS_BORDER_WIDTH),
            text: Some(tokens.text),
            ..UiStylePatch::default()
        },
    ));
    rules.push(electronics_focus_rule(CLASS_DANGER, *tokens));
    UiStyleSheet { rules }
}
