//! Badge row actions of the retained Hierarchy.

use raf_render::api_graphic_basic::ui_surface::{UiIcon, UiIconSize};
use raf_ui::{UiEventBinding, UiEventKind, UiLayout, UiNode, UiNodeKind};

use super::super::hierarchy_model::details::badge_icon;
use super::super::hierarchy_model::HierarchyRow;

/// Component badges visible per row. Anything else folds into `+N`.
pub(super) const MAX_VISIBLE_BADGES: usize = 2;

/// Appends the leading badges and the overflow count of one row.
pub(super) fn append_badges(mut root: UiNode, row: &HierarchyRow, row_height: f32) -> UiNode {
    for (index, badge) in row.badges.iter().take(MAX_VISIBLE_BADGES).enumerate() {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.badge.{}.{index}", row.id.0),
                UiNodeKind::Button,
            )
            .with_class("hierarchy-row-action")
            .with_icon(UiIcon::new(badge_icon(badge)).with_size(UiIconSize::Small))
            .with_tooltip_key(badge.key)
            .with_tooltip_value(badge.count.to_string())
            .with_accessibility_label_key(badge.key)
            .with_layout(UiLayout::fixed(22.0, row_height - 2.0))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hierarchy.inspect:{}:{}", row.id.0, badge.section),
            )),
        );
    }
    if row.badges.len() > MAX_VISIBLE_BADGES {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.badges.more.{}", row.id.0),
                UiNodeKind::Button,
            )
            .with_text_value(format!("+{}", row.badges.len() - MAX_VISIBLE_BADGES))
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
