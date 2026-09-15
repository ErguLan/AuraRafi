//! Retained RafUI documents for the editor bottom-dock chrome.
//!
//! This module owns only dock chrome: tab strips, splitters, drag previews and
//! tab context menus. Content panels live in their own focused surfaces.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiIcon, UiIconId, UiIconSize,
    UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiSizeMode, UiSpacing, UiStyle,
    UiStylePatch, UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface,
    UiSurfaceMaterial, UiTextStyle,
};

use crate::panels::editor_bottom_dock_styles::bottom_style_sheet;
use raf_ui::{DockTab, DockTabGroup};

/// Non-persistent visual state shown while a bottom tab is being dragged.
/// The host owns the gesture; the surface only renders the proposed slot.
#[derive(Debug, Clone, PartialEq)]
pub struct BottomTabDragPreview {
    pub source_group_id: String,
    pub source_tab_id: String,
    pub moving_tab: DockTab,
    pub target_group_id: String,
    pub insertion_index: usize,
    pub split_before: Option<bool>,
    pub pulse: f32,
    pub transition: f32,
    pub width: f32,
}

pub fn build_tab_strip_surface(
    palette: StudioUiPalette,
    group: &DockTabGroup,
    collapsed: bool,
    preview: Option<&BottomTabDragPreview>,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("bottom.tabs", UiNodeKind::Toolbar)
        .with_class("bottom-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            padding: UiSpacing::xy(5.0, 2.0),
            overflow: UiOverflow::ScrollX,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        });

    let same_target = preview.is_some_and(|preview| {
        preview.target_group_id == group.id && preview.split_before.is_none()
    });
    let mut inserted_preview = false;
    for (index, tab) in group.tabs.iter().enumerate() {
        if same_target
            && !inserted_preview
            && index
                >= preview
                    .expect("same target implies preview")
                    .insertion_index
        {
            root = root.with_child(drag_preview_tab(
                preview.expect("same target implies preview"),
            ));
            inserted_preview = true;
        }
        // Keep the source node in the retained document while dragging. RafUI
        // captures the pointer on that node, so removing it here would make a
        // later pointer release unable to dispatch DragEnd.
        let active = group.active_tab == tab.id;
        let mut button = tab_button(group, tab, active);
        if preview.is_some_and(|preview| {
            preview.source_group_id == group.id && preview.source_tab_id == tab.id
        }) {
            button = button.with_class("bottom-tab-dragging");
        }
        root = root.with_child(button);
    }
    if same_target && !inserted_preview {
        root = root.with_child(drag_preview_tab(
            preview.expect("same target implies preview"),
        ));
    }

    root = root.with_child(
        UiNode::new("bottom.collapse", UiNodeKind::Button)
            .with_class("bottom-collapse-button")
            .with_layout(UiLayout {
                grow: 1.0,
                justify_content: UiJustify::End,
                align_items: UiAlign::Center,
                ..UiLayout::fit_content()
            })
            .with_icon(UiIcon::new(if collapsed {
                UiIconId::ChevronRight
            } else {
                UiIconId::ChevronDown
            }))
            .with_tooltip_key("editor.downbar.toggle")
            .with_accessibility_label_key("editor.downbar.toggle")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "bottom.toggle-collapsed",
            )),
    );

    let mut surface = UiSurface::new(format!("editor.bottom.tabs.{}", group.id), palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn drag_preview_tab(preview: &BottomTabDragPreview) -> UiNode {
    let pulse = preview.pulse.clamp(0.0, 1.0);
    let transition = preview.transition.clamp(0.0, 1.0);
    let fill_alpha = (88.0 + pulse * 72.0) as u8;
    let border_alpha = (180.0 + pulse * 75.0) as u8;
    UiNode::new("bottom.tab.drag-preview", UiNodeKind::Panel)
        .with_class("bottom-tab-drag-preview")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(preview.width.max(0.0) * transition, 28.0)
        })
        .with_style(UiStyle {
            fill: [232, 133, 28, fill_alpha],
            border: [199, 92, 174, border_alpha],
            text: [255, 237, 205, 255],
            border_width: 1.0,
            radius: 2.0,
            opacity: 0.25 + transition * 0.75,
        })
        .with_icon(UiIcon::new(preview.moving_tab.icon).with_size(UiIconSize::Small))
        .with_text_key(preview.moving_tab.title_key.clone())
        .with_text_style(UiTextStyle::button([255, 237, 205, 255]))
}

pub fn build_drop_preview_surface(
    palette: StudioUiPalette,
    pulse: f32,
    split_before: bool,
    transition: f32,
) -> UiSurface {
    let alpha = (44.0 + pulse.clamp(0.0, 1.0) * 36.0) as u8;
    let border_alpha = (170.0 + pulse.clamp(0.0, 1.0) * 85.0) as u8;
    let transition = transition.clamp(0.0, 1.0);
    let tokens = palette.tokens();
    let direction_key = if split_before {
        "editor.downbar.split_left"
    } else {
        "editor.downbar.split_right"
    };
    let root = UiNode::new("bottom.drop-preview", UiNodeKind::Panel)
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 8.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle {
            fill: [232, 133, 28, alpha],
            border: [199, 92, 174, border_alpha],
            text: tokens.text,
            border_width: 2.0,
            radius: 5.0,
            opacity: 0.2 + transition * 0.8,
        })
        .with_child(
            UiNode::new("bottom.drop-preview.title", UiNodeKind::Label)
                .with_text_key("editor.downbar.split_preview")
                .with_text_style(UiTextStyle::panel_title([255, 237, 205, 255]))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("bottom.drop-preview.direction", UiNodeKind::Label)
                .with_text_key(direction_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fit_content()),
        );
    UiSurface::new("editor.bottom.drop-preview", palette, root)
}

/// Transparent-but-visible grabber between two bottom-dock groups. The visual
/// divider stays narrow while the retained button owns a wider hit target so
/// resizing does not require pixel-perfect aim.
pub fn build_dock_splitter_surface(
    palette: StudioUiPalette,
    left_group_id: &str,
    right_group_id: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    let target = format!("{left_group_id}|{right_group_id}");
    let root = UiNode::new(format!("bottom.splitter.{target}"), UiNodeKind::Button)
        .with_class("bottom-dock-splitter")
        .with_layout(UiLayout::fill(UiFlow::None))
        .with_style(UiStyle {
            fill: [0, 0, 0, 0],
            border: tokens.border,
            text: tokens.text,
            border_width: 0.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_tooltip_key("editor.downbar.resize")
        .with_accessibility_label_key("editor.downbar.resize")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            format!("bottom.resize.start.{target}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            format!("bottom.resize.move.{target}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            format!("bottom.resize.end.{target}"),
        ));
    let mut surface = UiSurface::new(
        format!("editor.bottom.splitter.{left_group_id}.{right_group_id}"),
        palette,
        root,
    );
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-dock-splitter".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-dock-splitter".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    opacity: Some(0.78),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("bottom-dock-splitter".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    opacity: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
        ],
    };
    surface
}

pub fn build_tab_context_menu_surface(
    palette: StudioUiPalette,
    group_id: &str,
    tab_id: &str,
    can_split: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("bottom.tab-context", UiNodeKind::Menu)
        .with_class("bottom-tab-context")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::same(6.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_accessibility_label_key("app.more_menu")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "bottom.context.close",
        ))
        .with_child(
            tab_context_button(
                "bottom.tab-context.split-left",
                "editor.downbar.split_left",
                UiIconId::ChevronLeft,
                format!("bottom.context.split-left.{group_id}|{tab_id}"),
            )
            .disabled(!can_split),
        )
        .with_child(
            tab_context_button(
                "bottom.tab-context.split-right",
                "editor.downbar.split_right",
                UiIconId::ChevronRight,
                format!("bottom.context.split-right.{group_id}|{tab_id}"),
            )
            .disabled(!can_split),
        )
        .with_child(tab_context_button(
            "bottom.tab-context.reset",
            "app.reset_panels",
            UiIconId::Rotate,
            "bottom.context.reset",
        ));
    if !can_split {
        root = root.with_child(
            UiNode::new("bottom.tab-context.limit", UiNodeKind::Label)
                .with_text_key("editor.downbar.group_limit")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout {
                    padding: UiSpacing::xy(8.0, 2.0),
                    ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
                }),
        );
    }
    let mut surface = UiSurface::new("editor-bottom-tab-context", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn tab_context_button(
    id: &str,
    label_key: &str,
    icon: UiIconId,
    command: impl Into<String>,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("bottom-tab-context-action")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button([237, 239, 242, 255]).inherit_theme_color())
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn tab_button(group: &DockTabGroup, tab: &DockTab, active: bool) -> UiNode {
    let class = if active {
        "bottom-tab bottom-tab-active"
    } else {
        "bottom-tab"
    };
    let width = match tab.id.as_str() {
        "project-settings" => 104.0,
        "nodes" => 92.0,
        "console" => 96.0,
        "assets" => 86.0,
        "drc" => 86.0,
        "simulation" => 112.0,
        _ => 96.0,
    };
    UiNode::new(
        format!("bottom.tab.{}|{}", group.id, tab.id),
        UiNodeKind::Button,
    )
    .with_class(class)
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 5.0,
        padding: UiSpacing::xy(8.0, 0.0),
        ..UiLayout::fixed(width, 28.0).with_text_safe_area(true)
    })
    .with_icon(UiIcon::new(tab.icon).with_size(UiIconSize::Small))
    .with_text_key(tab.title_key.clone())
    .with_text_style(UiTextStyle::button([237, 239, 242, 255]).inherit_theme_color())
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("bottom.tab.{}|{}", group.id, tab.id),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragStart,
        format!("bottom.drag.start.{}|{}", group.id, tab.id),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragMove,
        format!("bottom.drag.move.{}|{}", group.id, tab.id),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragEnd,
        format!("bottom.drag.end.{}|{}", group.id, tab.id),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::ContextMenu,
        format!("bottom.context.open.{}|{}", group.id, tab.id),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child<'a>(node: &'a UiNode, id: &str) -> &'a UiNode {
        if node.id == id {
            return node;
        }
        node.children
            .iter()
            .find_map(|candidate| find_node(candidate, id))
            .unwrap_or_else(|| panic!("missing node {id}"))
    }

    fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|candidate| find_node(candidate, id))
    }

    #[test]
    fn bottom_tabs_expose_a_retained_context_menu_contract() {
        let group = DockTabGroup::new(
            "workspace",
            vec![DockTab::new(
                "console",
                "app.studio_console",
                UiIconId::Console,
            )],
        );
        let tabs = build_tab_strip_surface(StudioUiPalette::IndustrialDark, &group, false, None);
        let console_tab = child(&tabs.root, "bottom.tab.workspace|console");
        assert!(console_tab.event_handlers.iter().any(|binding| {
            binding.event == UiEventKind::ContextMenu
                && matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name }
                        if name == "bottom.context.open.workspace|console"
                )
        }));

        let menu = build_tab_context_menu_surface(
            StudioUiPalette::IndustrialDark,
            "workspace",
            "console",
            true,
        );
        for (node_id, expected_command) in [
            (
                "bottom.tab-context.split-left",
                "bottom.context.split-left.workspace|console",
            ),
            (
                "bottom.tab-context.split-right",
                "bottom.context.split-right.workspace|console",
            ),
            ("bottom.tab-context.reset", "bottom.context.reset"),
        ] {
            let action = child(&menu.root, node_id);
            assert!(action.event_handlers.iter().any(|binding| {
                matches!(
                    &binding.action,
                    raf_ui::UiAction::Command { name } if name == expected_command
                )
            }));
        }
    }

    #[test]
    fn bottom_splitter_exposes_a_wide_resize_contract() {
        let surface = build_dock_splitter_surface(
            StudioUiPalette::IndustrialDark,
            "native.primary",
            "native.group.2",
        );
        assert_eq!(surface.root.layout.width_mode, UiSizeMode::Fill);
        assert_eq!(surface.root.layout.height_mode, UiSizeMode::Fill);
        assert_eq!(
            surface.root.tooltip_key.as_deref(),
            Some("editor.downbar.resize")
        );
        for (event, expected) in [
            (
                UiEventKind::DragStart,
                "bottom.resize.start.native.primary|native.group.2",
            ),
            (
                UiEventKind::DragMove,
                "bottom.resize.move.native.primary|native.group.2",
            ),
            (
                UiEventKind::DragEnd,
                "bottom.resize.end.native.primary|native.group.2",
            ),
        ] {
            assert!(surface.root.event_handlers.iter().any(|binding| {
                binding.event == event
                    && matches!(
                        &binding.action,
                        raf_ui::UiAction::Command { name } if name == expected
                    )
            }));
        }
    }

    #[test]
    fn bottom_tabs_use_stable_text_safe_widths() {
        let group = DockTabGroup::new(
            "main",
            vec![DockTab::new(
                "project-settings",
                "app.studio_project",
                UiIconId::Settings,
            )],
        );
        let surface = build_tab_strip_surface(StudioUiPalette::IndustrialDark, &group, false, None);
        let tab = child(&surface.root, "bottom.tab.main|project-settings");

        assert_eq!(tab.layout.basis, [104.0, 28.0]);
        assert_eq!(tab.text_key.as_deref(), Some("app.studio_project"));
    }
}
