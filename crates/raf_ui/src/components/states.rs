//! Explicit component state rules. User sheets may append higher precedence.
use crate::*;

pub(super) fn component_states(t: UiTokens) -> UiStyleSheet {
    let mut sheet = UiStyleSheet::default();
    for class in ["raf-button", "raf-field"] {
        let selector = UiStyleSelector::Class(class.into());
        for (state, patch) in [
            (
                UiStyleRuleState::Hovered,
                UiStylePatch {
                    fill: Some(t.surface_raised),
                    ..Default::default()
                },
            ),
            (
                UiStyleRuleState::Selected,
                UiStylePatch {
                    fill: Some(t.selection),
                    border: Some(t.focus),
                    ..Default::default()
                },
            ),
            (
                UiStyleRuleState::Active,
                UiStylePatch {
                    fill: Some(t.selection),
                    ..Default::default()
                },
            ),
            (
                UiStyleRuleState::Focused,
                UiStylePatch {
                    border: Some(t.focus),
                    border_width: Some(1.0),
                    ..Default::default()
                },
            ),
            (
                UiStyleRuleState::Invalid,
                UiStylePatch {
                    border: Some(t.danger),
                    border_width: Some(1.0),
                    ..Default::default()
                },
            ),
            (
                UiStyleRuleState::Disabled,
                UiStylePatch {
                    opacity: Some(0.56),
                    ..Default::default()
                },
            ),
        ] {
            sheet
                .rules
                .push(UiStyleRule::new(selector.clone(), patch).when(state));
        }
    }
    // Filled actions retain their contrast pair while hovering or pressing.
    for state in [UiStyleRuleState::Hovered, UiStyleRuleState::Active] {
        sheet.rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class("raf-button-accent".into()),
                UiStylePatch {
                    fill: Some(t.accent_hot),
                    text: Some(t.chrome().icon_on_accent),
                    ..Default::default()
                },
            )
            .when(state),
        );
        sheet.rules.push(
            UiStyleRule::new(
                UiStyleSelector::Class("raf-button-danger".into()),
                UiStylePatch {
                    fill: Some(t.surface_raised),
                    text: Some(t.danger),
                    ..Default::default()
                },
            )
            .when(state),
        );
    }
    sheet
}
