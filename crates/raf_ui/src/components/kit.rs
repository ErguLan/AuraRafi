//! Complete recipes sharing metrics, content grouping and state selectors.
use crate::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UiButtonVariant {
    #[default]
    Standard,
    Quiet,
    Accent,
    Danger,
}

/// An authoring context, not a widget runtime. Mount `style_sheet()` once;
/// recipes return ordinary nodes with scoped component classes.
#[derive(Debug, Clone)]
pub struct UiComponents {
    pub tokens: UiTokens,
    pub metrics: UiThemeMetrics,
}

impl UiComponents {
    pub fn new(tokens: UiTokens, metrics: UiThemeMetrics) -> Self {
        Self { tokens, metrics }
    }

    pub fn for_palette(palette: StudioUiPalette) -> Self {
        Self::new(palette.tokens(), UiTheme::raf_ui().metrics)
    }

    pub fn button(
        &self,
        id: impl Into<String>,
        label_key: impl Into<String>,
        action: UiAction,
        variant: UiButtonVariant,
    ) -> UiNode {
        let label = label_key.into();
        let t = self.tokens;
        let m = self.metrics.for_control(UiControlKind::Button);
        let (class, fill, border, ink) = match variant {
            UiButtonVariant::Standard => ("raf-button-standard", t.surface_alt, t.border, t.text),
            UiButtonVariant::Quiet => ("raf-button-quiet", [0; 4], [0; 4], t.text),
            UiButtonVariant::Accent => (
                "raf-button-accent",
                t.accent,
                t.accent,
                t.chrome().icon_on_accent,
            ),
            UiButtonVariant::Danger => ("raf-button-danger", [0; 4], t.danger, t.danger),
        };
        UiNode::new(id, UiNodeKind::Button)
            .with_class("raf-button")
            .with_class(class)
            .with_text_key(&label)
            .with_tooltip_key(&label)
            .with_accessibility_label_key(label)
            .with_accessibility_role(UiAccessibilityRole::Button)
            .with_text_style(UiTextStyle::button(ink).inherit_theme_color())
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(
                UiLayout::fit_content()
                    .fixed_height(m.height)
                    .padding_xy(m.horizontal_padding, 0.0),
            )
            .with_visual(UiVisual {
                content: UiContentLayout {
                    align: UiContentAlign::Center,
                    gap: m.icon_gap,
                    ..Default::default()
                },
                ..Default::default()
            })
            .with_style(UiStyle {
                fill,
                border,
                text: ink,
                border_width: 1.0,
                radius: m.corner_radius,
                opacity: 1.0,
            })
            .focusable()
            .with_event(UiEventBinding {
                event: UiEventKind::Click,
                action,
            })
    }

    pub fn icon_button(
        &self,
        id: impl Into<String>,
        label_key: impl Into<String>,
        icon: UiIconId,
        action: UiAction,
    ) -> UiNode {
        let m = self.metrics.for_control(UiControlKind::Button);
        let mut node = self
            .button(id, label_key, action, UiButtonVariant::Quiet)
            .with_icon(UiIcon::new(icon).with_size(UiIconSize::Toolbar))
            .fixed_width(m.height)
            .with_padding(UiSpacing::same(0.0));
        node.text_key = None;
        node
    }

    pub fn text_input(&self, id: impl Into<String>, input: UiTextInput) -> UiNode {
        let m = self.metrics.for_control(UiControlKind::TextInput);
        UiNode::text_input(id, input)
            .with_class("raf-field")
            .with_layout(
                UiLayout::fit_content()
                    .fill_width()
                    .fixed_height(m.height)
                    .padding_xy(m.horizontal_padding, 0.0),
            )
            .with_style(UiStyle {
                fill: self.tokens.surface_alt,
                border: self.tokens.border,
                text: self.tokens.text,
                border_width: 1.0,
                radius: m.corner_radius,
                opacity: 1.0,
            })
            .with_text_style(UiTextStyle::body(self.tokens.text).inherit_theme_color())
            .with_accessibility_role(UiAccessibilityRole::Textbox)
    }

    pub fn select(&self, id: impl Into<String>, select: UiSelect) -> UiNode {
        let id = id.into();
        let m = self.metrics.for_control(UiControlKind::Select);
        let label = select
            .selected_option()
            .map(|item| item.label_key.clone())
            .unwrap_or_else(|| "app.select_empty".into());
        let open = select.open;
        UiNode::select(&id, select)
            .with_class("raf-field")
            .with_accessibility_label_key(&label)
            .with_accessibility_expanded(open)
            .with_layout(
                UiLayout::row()
                    .fill_width()
                    .fixed_height(m.height)
                    .align(UiAlign::Center)
                    .gap(m.icon_gap)
                    .padding_xy(m.horizontal_padding, 0.0),
            )
            .with_style(UiStyle {
                fill: self.tokens.surface_alt,
                border: self.tokens.border,
                text: self.tokens.text,
                border_width: 1.0,
                radius: m.corner_radius,
                opacity: 1.0,
            })
            .with_children([
                UiNode::new(format!("{id}.value"), UiNodeKind::Label)
                    .with_text_key(label)
                    .with_text_overflow(UiTextOverflow::Ellipsis)
                    .with_text_style(UiTextStyle::label(self.tokens.text))
                    .with_layout(UiLayout::fit_content().fill_width().grow(1.0)),
                UiNode::new(format!("{id}.arrow"), UiNodeKind::Label)
                    .with_icon(
                        UiIcon::new(if open {
                            UiIconId::ChevronDown
                        } else {
                            UiIconId::ChevronRight
                        })
                        .with_size(UiIconSize::Small)
                        .with_tint(self.tokens.text_muted),
                    )
                    .with_layout(UiLayout::fixed(
                        f32::from(UiIconSize::Small.logical_pixels()),
                        f32::from(UiIconSize::Small.logical_pixels()),
                    )),
            ])
    }

    pub fn property_field(
        &self,
        id: impl Into<String>,
        label_key: impl Into<String>,
        mut control: UiNode,
    ) -> UiNode {
        let id = id.into();
        let label = label_key.into();
        if control.accessibility_label_key.is_none() {
            control.accessibility_label_key = Some(label.clone());
        }
        UiNode::column(
            &id,
            [
                UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                    .with_text_key(label)
                    .with_text_style(UiTextStyle::label(self.tokens.text_muted))
                    .with_layout(UiLayout::fit_content().fill_width())
                    .with_text_overflow(UiTextOverflow::Ellipsis),
                control.fill_width(),
            ],
        )
        .fit_content()
        .fill_width()
        .with_gap(self.metrics.spacing * 0.5)
    }

    pub fn toolbar(
        &self,
        id: impl Into<String>,
        children: impl IntoIterator<Item = UiNode>,
    ) -> UiNode {
        let mut node = UiNode::row(id, children)
            .fit_content()
            .fill_width()
            .with_alignment(UiAlign::Center)
            .with_gap(self.metrics.spacing)
            .with_compact(UiCompactMode::Wrap);
        node.kind = UiNodeKind::Toolbar;
        node
    }

    pub fn style_sheet(&self) -> UiStyleSheet {
        super::states::component_states(self.tokens)
    }
}
