//! Badge row actions of the retained Hierarchy.

use raf_render::api_graphic_basic::ui_surface::{UiIcon, UiIconId, UiIconSize};
use raf_ui::{UiEventBinding, UiEventKind, UiLayout, UiNode, UiNodeKind};

use super::super::hierarchy_model::HierarchyRow;

/// Appends the two leading badges and the overflow count of one row.
pub(super) fn append_badges(mut root: UiNode, row: &HierarchyRow, row_height: f32) -> UiNode {
    for (index, badge) in row.badges.iter().take(2).enumerate() {
        let icon = match badge.section {
            "metadata" => UiIconId::Warning,
            "behavior" => UiIconId::Script,
            "appearance" => UiIconId::Assets,
            "audio" => UiIconId::Play,
            _ => UiIconId::Node,
        };
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.badge.{}.{index}", row.id.0),
                UiNodeKind::Button,
            )
            .with_class("hierarchy-row-action")
            .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
            .with_tooltip_key(badge.key)
            .with_accessibility_label_key(badge.key)
            .with_layout(UiLayout::fixed(22.0, row_height - 2.0))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hierarchy.inspect:{}:{}", row.id.0, badge.section),
            )),
        );
    }
    if row.badges.len() > 2 {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.badges.more.{}", row.id.0),
                UiNodeKind::Button,
            )
            .with_text_value(format!("+{}", row.badges.len() - 2))
            .with_tooltip_key("hierarchy.more_details")
            .with_layout(UiLayout::fixed(22.0, row_height - 2.0))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hierarchy.inspect:{}:components", row.id.0),
            )),
        );
    }
    root
}
