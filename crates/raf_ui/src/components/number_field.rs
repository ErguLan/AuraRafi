//! Shared numeric row recipe: label, scrub field, stepper, slider and reset.
//!
//! The component is presentation-only. It emits semantic commands that hosts
//! translate into their own domain state, so the same feel can be reused by
//! Lighting, Project Settings and future inspectors.

use crate::controls::{UiRange, UiTextInput};
use crate::events::{UiAction, UiEventBinding, UiEventKind};
use crate::icons::{UiIcon, UiIconId, UiIconSize};
use crate::layout::{UiAlign, UiFlow, UiLayout, UiSizeMode};
use crate::node::{UiNode, UiNodeKind};
use crate::style::{StudioUiPalette, UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector};
use crate::text::{UiFontWeight, UiTextOverflow, UiTextRole, UiTextStyle};

#[derive(Debug, Clone)]
pub struct UiNumberFieldSpec {
    pub id: String,
    pub label_key: String,
    pub path: String,
    pub command_prefix: String,
    pub value_key: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub modified: bool,
    pub disabled: bool,
    pub hint_key: Option<String>,
    pub copy_text: Option<String>,
    pub show_slider: bool,
}

impl UiNumberFieldSpec {
    pub fn new(
        id: impl Into<String>,
        label_key: impl Into<String>,
        path: impl Into<String>,
        value_key: impl Into<String>,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
    ) -> Self {
        Self {
            id: id.into(),
            label_key: label_key.into(),
            path: path.into(),
            command_prefix: "lighting".to_string(),
            value_key: value_key.into(),
            value,
            min,
            max,
            step: step.max(f32::EPSILON),
            modified: false,
            disabled: false,
            hint_key: None,
            copy_text: None,
            show_slider: true,
        }
    }

    pub fn with_modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn with_hint_key(mut self, hint_key: impl Into<String>) -> Self {
        self.hint_key = Some(hint_key.into());
        self
    }

    pub fn with_copy_text(mut self, copy_text: impl Into<String>) -> Self {
        self.copy_text = Some(copy_text.into());
        self
    }

    pub fn with_slider(mut self, show_slider: bool) -> Self {
        self.show_slider = show_slider;
        self
    }

    fn command(&self, suffix: &str) -> String {
        format!("{}.{}:{}", self.command_prefix, suffix, self.path)
    }
}

pub fn number_field(palette: StudioUiPalette, spec: &UiNumberFieldSpec) -> UiNode {
    let tokens = palette.tokens();
    let disabled = spec.disabled;
    let mut root = UiNode::new(spec.id.clone(), UiNodeKind::Panel)
        .with_class(if disabled {
            "ui-number-field ui-number-field-disabled"
        } else {
            "ui-number-field"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Stretch,
            gap: 2.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    let mut header =
        UiNode::new(format!("{}.header", spec.id), UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
        });
    header = header.with_child(
        UiNode::new(format!("{}.label", spec.id), UiNodeKind::Label)
            .with_text_key(spec.label_key.clone())
            .with_text_style(UiTextStyle::body(tokens.text))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
            }),
    );
    if let Some(copy_text) = &spec.copy_text {
        header = header.with_child(icon_action(
            format!("{}.copy", spec.id),
            UiIconId::Copy,
            "app.copy",
            UiAction::SetClipboard {
                text: copy_text.clone(),
            },
        ));
    }
    header = header.with_child(icon_command(
        format!("{}.paste", spec.id),
        UiIconId::File,
        "app.paste",
        spec.command("paste"),
        disabled,
    ));
    if spec.modified {
        header = header.with_child(icon_command(
            format!("{}.reset", spec.id),
            UiIconId::Refresh,
            "lighting.reset",
            spec.command("reset"),
            disabled,
        ));
    }
    root = root.with_child(header);

    let mut control =
        UiNode::new(format!("{}.control", spec.id), UiNodeKind::Panel).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
        });
    let mut scrub = UiNode::new(format!("{}.scrub", spec.id), UiNodeKind::Button)
        .with_class("ui-number-scrub")
        .with_layout(UiLayout::fixed(16.0, 24.0))
        .with_icon(
            UiIcon::new(UiIconId::Move)
                .with_size(UiIconSize::Custom(12))
                .with_tint(tokens.text_muted),
        )
        .with_tooltip_key("lighting.drag_value")
        .with_accessibility_label_key("lighting.drag_value")
        .focusable();
    if !disabled {
        scrub = scrub
            .with_event(UiEventBinding::command(
                UiEventKind::PointerDown(crate::UiPointerButton::Primary),
                spec.command("scrub.begin"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::DragMove,
                spec.command("scrub.move"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::PointerUp(crate::UiPointerButton::Primary),
                spec.command("scrub.end"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::DragEnd,
                spec.command("scrub.end"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::DoubleClick,
                spec.command("focus"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("ArrowUp".to_string()),
                format!("{}.step:{}:1", spec.command_prefix, spec.path),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("ArrowDown".to_string()),
                format!("{}.step:{}:-1", spec.command_prefix, spec.path),
            ));
    }
    control = control.with_child(scrub);
    control = control.with_child(step_button(spec, palette, "−", -1.0, disabled));
    let text_input = UiNode::text_input(
        spec.value_key.clone(),
        UiTextInput {
            value_key: spec.value_key.clone(),
            placeholder_key: None,
            max_length: 32,
            multiline: false,
            password: false,
            submit_command: Some(spec.command("commit")),
        },
    )
    .with_class("ui-number-input")
    .with_layout(UiLayout {
        grow: 1.0,
        min_size: [48.0, 24.0],
        ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_text_style(UiTextStyle {
        role: UiTextRole::Monospace,
        size_px: 12.0,
        line_height_px: 16.0,
        weight: UiFontWeight::Regular,
        color: tokens.text,
        inherit_color: false,
    })
    .with_accessibility_label_key(spec.label_key.clone())
    .disabled(disabled);
    control = control.with_child(text_input);
    control = control.with_child(step_button(spec, palette, "+", 1.0, disabled));
    root = root.with_child(control);

    if spec.show_slider {
        let range = UiNode::range(
            format!("{}.range", spec.id),
            UiRange::new(
                spec.value_key.clone(),
                spec.value,
                spec.min,
                spec.max,
                spec.step,
            ),
        )
        .with_class("ui-number-range")
        .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill))
        .disabled(disabled);
        let range = if disabled {
            range
        } else {
            range
                .with_event(UiEventBinding::command(
                    UiEventKind::PointerDown(crate::UiPointerButton::Primary),
                    spec.command("gesture.begin"),
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::PointerUp(crate::UiPointerButton::Primary),
                    spec.command("gesture.end"),
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::DragEnd,
                    spec.command("gesture.end"),
                ))
        };
        root = root.with_child(range);
    }
    if let Some(hint_key) = &spec.hint_key {
        root = root.with_child(
            UiNode::new(format!("{}.hint", spec.id), UiNodeKind::Label)
                .with_text_key(hint_key.clone())
                .with_text_style(UiTextStyle {
                    size_px: 10.0,
                    line_height_px: 13.0,
                    ..UiTextStyle::body(tokens.text_muted)
                })
                .with_text_overflow(UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    root
}

fn step_button(
    spec: &UiNumberFieldSpec,
    palette: StudioUiPalette,
    label: &str,
    direction: f32,
    disabled: bool,
) -> UiNode {
    let direction = if direction < 0.0 { "-1" } else { "1" };
    let color = if disabled {
        palette.tokens().text_muted
    } else {
        palette.tokens().text
    };
    let mut button = UiNode::new(format!("{}.step.{direction}", spec.id), UiNodeKind::Button)
        .with_class("ui-number-step")
        .with_text_value(label)
        .with_layout(UiLayout::fixed(22.0, 24.0))
        .with_text_style(UiTextStyle {
            role: UiTextRole::Monospace,
            size_px: 14.0,
            line_height_px: 18.0,
            weight: UiFontWeight::Bold,
            color,
            inherit_color: false,
        })
        .with_accessibility_label_key(if direction == "-1" {
            "lighting.decrease_value"
        } else {
            "lighting.increase_value"
        })
        .focusable()
        .disabled(disabled);
    if !disabled {
        button = button.with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("{}.step:{}:{direction}", spec.command_prefix, spec.path),
        ));
    }
    button
}

fn icon_command(
    id: impl Into<String>,
    icon: UiIconId,
    tooltip_key: &str,
    command: String,
    disabled: bool,
) -> UiNode {
    let mut button = UiNode::new(id, UiNodeKind::Button)
        .with_class("ui-field-icon")
        .with_layout(UiLayout::fixed(20.0, 20.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Custom(12)))
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .focusable()
        .disabled(disabled);
    if !disabled {
        button = button.with_event(UiEventBinding::command(UiEventKind::Click, command));
    }
    button
}

fn icon_action(
    id: impl Into<String>,
    icon: UiIconId,
    tooltip_key: &str,
    action: UiAction,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("ui-field-icon")
        .with_layout(UiLayout::fixed(20.0, 20.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Custom(12)))
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action,
        })
}

pub fn number_field_rules(palette: StudioUiPalette) -> Vec<UiStyleRule> {
    let tokens = palette.tokens();
    let chrome = tokens.chrome();
    vec![
        UiStyleRule::new(
            UiStyleSelector::Class("ui-number-field-disabled".to_string()),
            UiStylePatch {
                opacity: Some(0.58),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-number-input".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_alt),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(3.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-number-input".to_string()),
            UiStylePatch {
                border: Some(tokens.focus),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Focused),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-number-step".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(3.0),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-number-step".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.accent),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-number-scrub".to_string()),
            UiStylePatch {
                fill: Some([0, 0, 0, 0]),
                border: Some([0, 0, 0, 0]),
                border_width: Some(0.0),
                radius: Some(3.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-number-scrub".to_string()),
            UiStylePatch {
                fill: Some(chrome.divider),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-field-icon".to_string()),
            UiStylePatch {
                fill: Some([0, 0, 0, 0]),
                border: Some([0, 0, 0, 0]),
                border_width: Some(0.0),
                opacity: Some(0.68),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        UiStyleRule::new(
            UiStyleSelector::Class("ui-field-icon".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                border_width: Some(1.0),
                opacity: Some(1.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    ]
}
