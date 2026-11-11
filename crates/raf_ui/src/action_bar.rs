//! Declarative action bars with caller-measured text and explicit overflow.

use crate::controls::{UiSelect, UiSelectOption};
use crate::events::{UiAction, UiEventBinding, UiEventKind};
use crate::geometry::{UiRect, UiSpacing};
use crate::icons::{UiIcon, UiIconId, UiIconSize};
use crate::layout::{UiAlign, UiFlow, UiJustify, UiLayout};
use crate::node::{UiAccessibilityRole, UiNode, UiNodeKind};
use crate::style::{StudioUiPalette, UiControlKind, UiStyle, UiSurfaceMaterial, UiThemeMetrics};
use crate::text::{UiTextOverflow, UiTextStyle};
use crate::visual::{UiContentAlign, UiIconPlacement, UiVisual};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiActionPriority {
    /// Moves into the overflow menu before other items when space is limited.
    OverflowFirst,
    Secondary,
    Primary,
    /// Remains in the visible row even when the row reports that it cannot fit.
    AlwaysVisible,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiActionBarItem {
    pub id: String,
    pub label_key: String,
    /// Stable semantic group id used to separate related actions.
    pub group: Option<String>,
    /// Already localized text used only by the caller-provided measure callback.
    pub measured_label: String,
    pub icon: Option<UiIconId>,
    pub action: UiAction,
    pub priority: UiActionPriority,
    pub show_label: bool,
    pub active: bool,
    pub disabled: bool,
}

impl UiActionBarItem {
    pub fn new(
        id: impl Into<String>,
        label_key: impl Into<String>,
        measured_label: impl Into<String>,
        icon: Option<UiIconId>,
        action: UiAction,
    ) -> Self {
        Self {
            id: id.into(),
            label_key: label_key.into(),
            group: None,
            measured_label: measured_label.into(),
            icon,
            action,
            priority: UiActionPriority::Secondary,
            show_label: true,
            active: false,
            disabled: false,
        }
    }

    pub fn with_priority(mut self, priority: UiActionPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_icon_only(mut self) -> Self {
        self.show_label = false;
        self
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiActionBarPlan {
    pub visible_ids: Vec<String>,
    pub overflow_ids: Vec<String>,
    pub required_width: f32,
    pub available_width: f32,
    pub overflow_button_width: f32,
    pub overflow_menu_width: f32,
    pub overflow_menu_height: f32,
    pub item_gap: f32,
    pub group_separator_width: f32,
    pub group_separators_before: Vec<String>,
    pub overflow_separator: bool,
    pub fits: bool,
}

/// Plans visible and overflow actions. The callback receives each actual,
/// localized label string; no character-count estimate is used.
pub fn plan_action_bar(
    items: &[UiActionBarItem],
    available_width: f32,
    theme: UiThemeMetrics,
    mut measure_text: impl FnMut(&str) -> f32,
) -> UiActionBarPlan {
    let button = theme.for_control(UiControlKind::Button);
    let item_gap = theme.spacing.max(0.0);
    let outer_padding = UiSpacing::same(item_gap);
    let label_widths = items
        .iter()
        .map(|item| finite_non_negative(measure_text(&item.measured_label)))
        .collect::<Vec<_>>();
    let widths = items
        .iter()
        .zip(&label_widths)
        .map(|(item, label_width)| item_content_width(item, button, *label_width))
        .collect::<Vec<_>>();
    let menu_item_widths = items
        .iter()
        .zip(&label_widths)
        .map(|(item, label_width)| item_menu_width(item, button, *label_width))
        .collect::<Vec<_>>();
    let group_separator_width = 1.0;
    let more_width =
        f32::from(UiIconSize::Toolbar.logical_pixels()) + button.horizontal_padding.max(0.0) * 2.0;
    let mut visible = (0..items.len()).collect::<Vec<_>>();
    let mut overflow = Vec::new();
    let available_width = finite_non_negative(available_width);

    while row_width(
        &visible,
        items,
        &widths,
        item_gap,
        !overflow.is_empty(),
        more_width,
        group_separator_width,
    ) > available_width
    {
        let candidate = visible
            .iter()
            .copied()
            .filter(|index| items[*index].priority != UiActionPriority::AlwaysVisible)
            .min_by_key(|index| (items[*index].priority, *index));
        let Some(candidate) = candidate else {
            break;
        };
        visible.retain(|index| *index != candidate);
        overflow.push(candidate);
    }

    overflow.sort_unstable();
    let has_overflow = !overflow.is_empty();
    let overflow_separator = has_overflow
        && visible
            .last()
            .is_some_and(|index| items[*index].group.as_deref() != Some(OVERFLOW_GROUP));
    let required_width = row_width(
        &visible,
        items,
        &widths,
        item_gap,
        has_overflow,
        more_width,
        group_separator_width,
    );
    let overflow_menu_width = overflow
        .iter()
        .map(|index| menu_item_widths[*index])
        .fold(0.0_f32, f32::max)
        + outer_padding.left
        + outer_padding.right;
    let menu_group_separators = group_boundaries(&overflow, items).len();
    let overflow_menu_height = if has_overflow {
        outer_padding.top
            + outer_padding.bottom
            + overflow.len() as f32 * button.height.max(0.0)
            + overflow.len().saturating_sub(1) as f32 * item_gap
            + menu_group_separators as f32 * (group_separator_width + item_gap)
    } else {
        0.0
    };

    let group_separators_before = group_boundaries(&visible, items)
        .into_iter()
        .map(|index| items[index].id.clone())
        .collect();
    UiActionBarPlan {
        visible_ids: visible
            .iter()
            .map(|index| items[*index].id.clone())
            .collect(),
        overflow_ids: overflow
            .iter()
            .map(|index| items[*index].id.clone())
            .collect(),
        required_width,
        available_width,
        overflow_button_width: if has_overflow { more_width } else { 0.0 },
        overflow_menu_width,
        overflow_menu_height,
        item_gap,
        group_separator_width,
        group_separators_before,
        overflow_separator,
        fits: required_width <= available_width,
    }
}

/// Builds a toolbar from a previously measured plan. Every omitted action is
/// represented in the More menu; `fits == false` is exposed to the host rather
/// than silently dropping a pinned action.
pub fn action_bar_node(
    id: impl Into<String>,
    items: &[UiActionBarItem],
    plan: &UiActionBarPlan,
    overflow_open: bool,
    palette: StudioUiPalette,
    theme: UiThemeMetrics,
) -> UiNode {
    let id = id.into();
    let metrics = theme.for_control(UiControlKind::Button);
    let tokens = palette.tokens();
    let mut bar = UiNode::new(id.clone(), UiNodeKind::Toolbar)
        .with_class("action-bar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: plan.item_gap,
            ..UiLayout::fit_content().fixed_height(metrics.height)
        })
        .with_accessibility_role(UiAccessibilityRole::Generic);

    for item_id in &plan.visible_ids {
        if let Some(item) = items.iter().find(|item| item.id == *item_id) {
            if plan.group_separators_before.iter().any(|id| id == item_id) {
                bar = bar.with_child(action_bar_separator(
                    format!("{id}.group-separator.{}", item.id),
                    plan.group_separator_width,
                    metrics.height * 0.64,
                    tokens.chrome().divider,
                ));
            }
            bar = bar.with_child(action_bar_button(item, palette, theme));
        }
    }
    if !plan.overflow_ids.is_empty() {
        let overflow_items = plan
            .overflow_ids
            .iter()
            .filter_map(|item_id| items.iter().find(|item| item.id == *item_id).cloned())
            .collect::<Vec<_>>();
        if plan.overflow_separator {
            bar = bar.with_child(action_bar_separator(
                format!("{id}.group-separator.more"),
                plan.group_separator_width,
                metrics.height * 0.64,
                tokens.chrome().divider,
            ));
        }
        bar = bar.with_child(action_bar_overflow(
            &id,
            &overflow_items,
            overflow_open,
            palette,
            theme,
            plan,
        ));
    }
    bar
}

fn action_bar_button(
    item: &UiActionBarItem,
    palette: StudioUiPalette,
    theme: UiThemeMetrics,
) -> UiNode {
    let tokens = palette.tokens();
    let metrics = theme.for_control(UiControlKind::Button);
    let chrome = tokens.chrome();
    let icon_tint = if item.active {
        chrome.icon_accent
    } else {
        chrome.icon_muted
    };
    let mut button = UiNode::new(item.id.clone(), UiNodeKind::Button)
        .with_class(if item.active {
            "action-bar-button action-bar-button-active"
        } else {
            "action-bar-button"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            padding: UiSpacing::xy(metrics.horizontal_padding, 0.0),
            ..UiLayout::fit_content().fixed_height(metrics.height)
        })
        .with_visual(UiVisual {
            corners: Some([metrics.corner_radius; 4]),
            content: crate::visual::UiContentLayout {
                align: UiContentAlign::Center,
                icon_placement: UiIconPlacement::Start,
                gap: metrics.icon_gap,
            },
            ..UiVisual::default()
        })
        .with_style(UiStyle {
            fill: if item.active {
                tokens.selection
            } else {
                tokens.surface_alt
            },
            border: if item.active {
                tokens.accent
            } else {
                tokens.border
            },
            text: tokens.text,
            border_width: 1.0,
            radius: metrics.corner_radius,
            opacity: 1.0,
        })
        .with_tooltip_key(item.label_key.clone())
        .with_accessibility_label_key(item.label_key.clone())
        .with_accessibility_role(UiAccessibilityRole::Button);
    button.disabled = item.disabled;
    if !item.disabled {
        button = button.focusable().with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: item.action.clone(),
        });
    }
    if let Some(icon) = item.icon {
        button = button.with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Toolbar)
                .with_tint(icon_tint),
        );
    }
    if item.show_label {
        button = button
            .with_text_key(item.label_key.clone())
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis);
    }
    button
}

fn action_bar_overflow(
    group_id: &str,
    items: &[UiActionBarItem],
    open: bool,
    palette: StudioUiPalette,
    theme: UiThemeMetrics,
    plan: &UiActionBarPlan,
) -> UiNode {
    let tokens = palette.tokens();
    let metrics = theme.for_control(UiControlKind::Button);
    let trigger_id = format!("{group_id}.more.trigger");
    let popup_id = format!("{group_id}.more.menu");
    let selection_key = format!("{group_id}.more.selection");
    let options = items
        .iter()
        .map(|item| {
            let mut option = UiSelectOption::new(item.id.clone(), item.label_key.clone());
            option.disabled = item.disabled;
            option
        })
        .collect::<Vec<_>>();
    let mut select = UiSelect::new(selection_key.clone(), options, 0).with_popup_id(&popup_id);
    select.open = open;
    let trigger = UiNode::select(trigger_id.clone(), select)
        .with_class(if open {
            "action-bar-button action-bar-button-active"
        } else {
            "action-bar-button"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            padding: UiSpacing::xy(metrics.horizontal_padding, 0.0),
            ..UiLayout::fit_content().fixed_height(metrics.height)
        })
        .with_visual(UiVisual {
            corners: Some([metrics.corner_radius; 4]),
            content: crate::visual::UiContentLayout {
                align: UiContentAlign::Center,
                icon_placement: UiIconPlacement::Start,
                gap: metrics.icon_gap,
            },
            ..UiVisual::default()
        })
        .with_style(UiStyle {
            fill: if open {
                tokens.selection
            } else {
                tokens.surface_alt
            },
            border: if open { tokens.accent } else { tokens.border },
            text: tokens.text,
            border_width: 1.0,
            radius: metrics.corner_radius,
            opacity: 1.0,
        })
        .with_icon(
            UiIcon::new(UiIconId::More)
                .with_size(UiIconSize::Toolbar)
                .with_tint(tokens.chrome().icon_muted),
        )
        .with_tooltip_key("app.more_menu")
        .with_accessibility_label_key("app.more_menu")
        .with_accessibility_role(UiAccessibilityRole::Combobox)
        .with_accessibility_expanded(open)
        .focusable();
    let mut group = UiNode::new(format!("{group_id}.more"), UiNodeKind::Panel)
        .with_layout(UiLayout::fit_content())
        .with_style(UiStyle::transparent())
        .with_child(trigger);

    if open {
        let padding = theme.spacing.max(0.0);
        let mut menu = UiNode::new(popup_id.clone(), UiNodeKind::Menu)
            .with_class("action-bar-overflow-menu")
            .with_material(UiSurfaceMaterial::TranslucentRaised)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: plan.item_gap,
                padding: UiSpacing::same(padding),
                overflow: crate::layout::UiOverflow::Clip,
                ..UiLayout::absolute(UiRect::new(
                    0.0,
                    metrics.height,
                    plan.overflow_menu_width,
                    plan.overflow_menu_height,
                ))
                .with_z_index(180)
            })
            .with_style(UiStyle {
                fill: tokens.surface_raised,
                border: tokens.border,
                text: tokens.text,
                border_width: 1.0,
                radius: metrics.corner_radius,
                opacity: 1.0,
            })
            .with_accessibility_role(UiAccessibilityRole::Menu)
            .focusable()
            .with_event(UiEventBinding {
                event: UiEventKind::KeyPress("escape".to_string()),
                action: UiAction::SetSelectOpen {
                    id: popup_id.clone(),
                    open: false,
                },
            });
        let mut previous_group: Option<Option<&str>> = None;
        for item in items {
            if previous_group.is_some_and(|group| group != item.group.as_deref()) {
                menu = menu.with_child(action_bar_separator(
                    format!("{group_id}.more.separator.{}", item.id),
                    (plan.overflow_menu_width - padding * 2.0).max(0.0),
                    plan.group_separator_width,
                    tokens.chrome().divider,
                ));
            }
            let mut option = UiNode::new(
                format!("{group_id}.more.option.{}", item.id),
                UiNodeKind::Button,
            )
            .with_class("action-bar-overflow-option")
            .with_layout(UiLayout {
                padding: UiSpacing::xy(metrics.horizontal_padding, 0.0),
                ..UiLayout::fixed(
                    (plan.overflow_menu_width - padding * 2.0).max(0.0),
                    metrics.height,
                )
            })
            .with_visual(UiVisual {
                content: crate::visual::UiContentLayout {
                    align: UiContentAlign::Start,
                    icon_placement: UiIconPlacement::Start,
                    gap: metrics.icon_gap,
                },
                ..UiVisual::default()
            })
            .with_text_key(item.label_key.clone())
            .with_text_style(UiTextStyle::button(tokens.text))
            .with_tooltip_key(item.label_key.clone())
            .with_accessibility_label_key(item.label_key.clone())
            .with_accessibility_role(UiAccessibilityRole::MenuItem)
            .focusable()
            .with_event(UiEventBinding {
                event: UiEventKind::Click,
                action: UiAction::SetSelect {
                    key: selection_key.clone(),
                    value: item.id.clone(),
                    index: items
                        .iter()
                        .position(|candidate| candidate.id == item.id)
                        .unwrap_or(0),
                },
            })
            .with_event(UiEventBinding {
                event: UiEventKind::Click,
                action: UiAction::SetSelectOpen {
                    id: trigger_id.clone(),
                    open: false,
                },
            });
            option.disabled = item.disabled;
            if let Some(icon) = item.icon {
                option = option.with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.chrome().icon_muted),
                );
            }
            menu = menu.with_child(option);
            previous_group = Some(item.group.as_deref());
        }
        group = group.with_child(menu);
    }
    group
}

fn item_content_width(
    item: &UiActionBarItem,
    metrics: crate::style::UiResolvedComponentMetrics,
    label_width: f32,
) -> f32 {
    let icon_width = item
        .icon
        .map(|_| f32::from(UiIconSize::Toolbar.logical_pixels()))
        .unwrap_or(0.0);
    let text_width = if item.show_label { label_width } else { 0.0 };
    let content_gap = if icon_width > 0.0 && text_width > 0.0 {
        metrics.icon_gap.max(0.0)
    } else {
        0.0
    };
    icon_width + content_gap + text_width + metrics.horizontal_padding.max(0.0) * 2.0
}

fn item_menu_width(
    item: &UiActionBarItem,
    metrics: crate::style::UiResolvedComponentMetrics,
    label_width: f32,
) -> f32 {
    let icon_width = item
        .icon
        .map(|_| f32::from(UiIconSize::Small.logical_pixels()))
        .unwrap_or(0.0);
    let content_gap = if icon_width > 0.0 && label_width > 0.0 {
        metrics.icon_gap.max(0.0)
    } else {
        0.0
    };
    icon_width + content_gap + label_width + metrics.horizontal_padding.max(0.0) * 2.0
}

fn row_width(
    visible: &[usize],
    items: &[UiActionBarItem],
    widths: &[f32],
    gap: f32,
    has_overflow: bool,
    overflow_button_width: f32,
    group_separator_width: f32,
) -> f32 {
    let item_count = visible.len() + usize::from(has_overflow);
    let content_width = visible.iter().map(|index| widths[*index]).sum::<f32>()
        + if has_overflow {
            overflow_button_width
        } else {
            0.0
        };
    let boundaries = group_boundaries(visible, items).len()
        + usize::from(
            has_overflow
                && visible
                    .last()
                    .is_some_and(|index| items[*index].group.as_deref() != Some(OVERFLOW_GROUP)),
        );
    content_width
        + item_count.saturating_sub(1) as f32 * gap
        + boundaries as f32 * (group_separator_width + gap)
}

const OVERFLOW_GROUP: &str = "__rafui_overflow_menu__";

fn group_boundaries(visible: &[usize], items: &[UiActionBarItem]) -> Vec<usize> {
    visible
        .windows(2)
        .filter_map(|pair| (items[pair[0]].group != items[pair[1]].group).then_some(pair[1]))
        .collect()
}

fn action_bar_separator(id: impl Into<String>, width: f32, height: f32, color: [u8; 4]) -> UiNode {
    UiNode::new(id, UiNodeKind::Separator)
        .with_class("action-bar-group-separator")
        .with_layout(UiLayout::fixed(width.max(0.0), height.max(0.0)))
        .with_style(UiStyle {
            fill: color,
            border: color,
            text: color,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, group: &str, priority: UiActionPriority) -> UiActionBarItem {
        UiActionBarItem::new(
            id,
            format!("label.{id}"),
            "measured label",
            None,
            UiAction::Command {
                name: format!("action.{id}"),
            },
        )
        .with_group(group)
        .with_priority(priority)
    }

    #[test]
    fn group_separators_are_measured_and_overflow_keeps_every_action() {
        let items = [
            item("polygons", "overlays", UiActionPriority::OverflowFirst),
            item("grid", "overlays", UiActionPriority::Secondary),
            item("labels", "annotations", UiActionPriority::Primary),
        ];

        let plan = plan_action_bar(&items, 100.0, UiThemeMetrics::default(), |_| 10.0);

        assert_eq!(plan.visible_ids, ["labels"]);
        assert_eq!(plan.overflow_ids, ["polygons", "grid"]);
        assert!(plan.overflow_separator);
        assert!(plan.fits);
        assert_eq!(plan.overflow_menu_height, 80.0);
        assert_eq!(
            plan.visible_ids.len() + plan.overflow_ids.len(),
            items.len(),
            "overflow must not discard any action"
        );
    }
}
