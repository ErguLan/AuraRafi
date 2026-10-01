//! Reusable UI primitives and atom components for editor surfaces.
//!
//! Centralizes common button, divider, and control patterns so surfaces do not
//! re-implement bespoke node hierarchies, layout boilerplate, and event bindings.

use raf_render::api_graphic_basic::ui_surface::{
    UiEventBinding, UiEventKind, UiIcon, UiIconId, UiIconSize, UiLayout, UiNode, UiNodeKind,
    UiSpacing,
};

/// High-frequency icon button primitive used across toolbars, headers, and inspectors.
#[derive(Debug, Clone)]
pub struct IconButton<'a> {
    pub id: String,
    pub icon: UiIconId,
    pub command: &'a str,
    pub tooltip_key: Option<&'a str>,
    pub size: f32,
    pub active: bool,
    pub disabled: bool,
    pub danger: bool,
    pub class: Option<&'a str>,
}

impl<'a> IconButton<'a> {
    pub fn new(id: impl Into<String>, icon: UiIconId, command: &'a str) -> Self {
        Self {
            id: id.into(),
            icon,
            command,
            tooltip_key: None,
            size: 26.0,
            active: false,
            disabled: false,
            danger: false,
            class: None,
        }
    }

    pub fn tooltip(mut self, key: &'a str) -> Self {
        self.tooltip_key = Some(key);
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size.max(16.0);
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    pub fn build(self) -> UiNode {
        let mut node = UiNode::new(self.id, UiNodeKind::Button)
            .with_icon(UiIcon::new(self.icon).with_size(UiIconSize::Small))
            .with_layout(
                UiLayout::fixed(self.size, self.size).padding(UiSpacing::xy(3.0, 3.0)),
            )
            .with_event(UiEventBinding::command(UiEventKind::Click, self.command));

        if let Some(class) = self.class {
            node = node.with_class(class);
        }
        if self.danger {
            node = node.with_class("danger-button");
        }
        if let Some(tooltip) = self.tooltip_key {
            node = node.with_tooltip_key(tooltip);
        }
        if self.active {
            node = node.with_class("active");
            node.accessibility_selected = Some(true);
        }
        if self.disabled {
            node = node.disabled(true);
        }
        node
    }
}

/// Sleek divider atom for toolbars and panels.
pub struct UiSeparator;

impl UiSeparator {
    pub fn vertical(id: impl Into<String>, height: f32) -> UiNode {
        UiNode::new(id.into(), UiNodeKind::Panel)
            .with_class("ui-separator-vertical")
            .with_layout(UiLayout::fixed(1.0, height.max(1.0)))
    }

    pub fn horizontal(id: impl Into<String>, width: f32) -> UiNode {
        UiNode::new(id.into(), UiNodeKind::Panel)
            .with_class("ui-separator-horizontal")
            .with_layout(UiLayout::fixed(width.max(1.0), 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_button_builds_expected_retained_node() {
        let btn = IconButton::new("test.btn", UiIconId::Select, "test.cmd")
            .tooltip("test.tooltip")
            .size(28.0)
            .active(true)
            .danger(true)
            .class("custom-class")
            .build();

        assert_eq!(btn.id, "test.btn");
        assert_eq!(btn.kind, UiNodeKind::Button);
        assert!(btn.classes.contains(&"custom-class".to_string()));
        assert!(btn.classes.contains(&"danger-button".to_string()));
        assert!(btn.classes.contains(&"active".to_string()));
        assert_eq!(btn.tooltip_key.as_deref(), Some("test.tooltip"));
        assert_eq!(btn.accessibility_selected, Some(true));
        assert_eq!(btn.layout.basis, [28.0, 28.0]);
    }

    #[test]
    fn separator_builds_expected_node() {
        let sep = UiSeparator::vertical("test.sep", 18.0);
        assert_eq!(sep.id, "test.sep");
        assert_eq!(sep.kind, UiNodeKind::Panel);
        assert_eq!(sep.layout.basis, [1.0, 18.0]);
    }
}
