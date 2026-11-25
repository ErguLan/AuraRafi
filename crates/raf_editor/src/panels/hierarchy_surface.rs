//! Declarative RafUI document for the scene Hierarchy.
//!
//! The surface only describes presentation and semantic events. Scene
//! mutations stay in `hierarchy_surface_host.rs` and the editor application.
//!
//! Tooltip presentation is owned by the retained Game workbench host: this
//! module only emits stable `tooltip_key`/`tooltip_value` pairs. If the host
//! disables retained tooltips, re-enable them in the workbench host pipeline;
//! the keys here must stay unchanged so localization keeps working.

mod badges;
mod context;
mod dnd;
mod filters;
mod rows;
#[cfg(test)]
mod tests;

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiPointerButton, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextStyle,
};

use super::hierarchy_model::HierarchyView;
use crate::project_catalog::ProjectCatalogResults;

use self::context::{context_menu, empty_context_menu, empty_primitive_menu};
use self::dnd::{box_selection_node, drag_ghost_node};
use self::filters::{summary, toolbar};
use self::rows::row_node;

pub(crate) use self::filters::{header, tabs};

const HIERARCHY_ROOT_PADDING: f32 = 4.0;

pub fn build_hierarchy_surface(
    palette: StudioUiPalette,
    view: &HierarchyView,
    selected: &[raf_core::scene::SceneNodeId],
    renaming: Option<(raf_core::scene::SceneNodeId, &str)>,
    menu_target: Option<(raf_core::scene::SceneNodeId, bool)>,
    empty_menu_open: bool,
    empty_primitive_menu_open: bool,
    menu_label: Option<&str>,
    menu_position: Option<[f32; 2]>,
    surface_size: [f32; 2],
    row_height: f32,
    indent_width: f32,
    show_icons: bool,
    show_visibility: bool,
    show_locked: bool,
    transition: f32,
    drag_ghost: Option<(&str, [f32; 2])>,
    drop_target: Option<raf_core::scene::SceneNodeId>,
    box_selection: Option<raf_ui::UiRect>,
    can_paste: bool,
    compact_tabs: bool,
    active_tab: &str,
    _bookmark_filled: [bool; 3],
    _search_results: Option<&ProjectCatalogResults>,
    show_script_indicators: bool,
) -> UiSurface {
    let tokens = palette.tokens();
    let _ = (
        menu_target,
        empty_menu_open,
        empty_primitive_menu_open,
        menu_label,
        menu_position,
        surface_size,
        can_paste,
    );
    let mut tree = UiNode::scroll_view("hierarchy.tree", UiScrollAxis::Vertical)
        .with_class("hierarchy-tree")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            padding: UiSpacing::xy(6.0, 4.0),
            gap: 0.0,
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_event(UiEventBinding::command(
            UiEventKind::PointerMove,
            "hierarchy.drag.over:root",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            "hierarchy.box-select.start",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            "hierarchy.box-select.move",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            "hierarchy.box-select.end",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::PointerDown(UiPointerButton::Primary),
            "hierarchy.box-select.press",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "hierarchy.clear-selection",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::ContextMenu,
            "hierarchy.empty-menu",
        ));

    if view.top_spacer > 0.0 {
        tree = tree.with_child(spacer("hierarchy.tree.top-spacer", view.top_spacer));
    }
    for row in &view.rows {
        tree = tree.with_child(row_node(
            palette,
            row,
            selected,
            renaming,
            row_height,
            indent_width,
            show_icons,
            show_visibility,
            show_locked,
            show_script_indicators,
            drop_target,
        ));
    }
    if view.bottom_spacer > 0.0 {
        tree = tree.with_child(spacer("hierarchy.tree.bottom-spacer", view.bottom_spacer));
    }
    if view.total_rows == 0 {
        tree = tree.with_child(
            UiNode::new("hierarchy.empty", UiNodeKind::Panel)
                .with_class("hierarchy-empty")
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::Center,
                    ..UiLayout::fixed(0.0, 74.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new("hierarchy.empty.label", UiNodeKind::Label)
                        .with_text_key("app.no_entities")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fit_content()),
                ),
        );
    }

    let mut root_style = palette.panel_style();
    root_style.opacity = (0.55 + transition.clamp(0.0, 1.0) * 0.45).clamp(0.0, 1.0);
    // Placeholder tabs were removed: the tree is the only Scene body, while
    // Lighting renders its own retained navigation surface.
    let content = tree;
    let root = UiNode::new("hierarchy.root", UiNodeKind::Root)
        .with_class("hierarchy-root")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            padding: UiSpacing::same(HIERARCHY_ROOT_PADDING),
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(root_style)
        .with_child(header(palette))
        .with_child(tabs(palette, compact_tabs, active_tab))
        .with_child(toolbar(palette))
        .with_child(summary(palette, view.entity_rows, selected.len()))
        .with_child(content);

    let root = if let Some((label, pointer)) = drag_ghost {
        root.with_child(drag_ghost_node(palette, label, pointer))
    } else {
        root
    };
    let box_selection = box_selection.or(view.box_selection);
    let root = if let Some(selection) = box_selection {
        root.with_child(box_selection_node(palette, selection))
    } else {
        root
    };

    let mut surface = UiSurface::new("editor.hierarchy", palette, root);
    surface.style_sheet = hierarchy_style_sheet(palette);
    surface
}

/// Builds only the context menus in a full-screen overlay surface. Keeping
/// this out of the side-panel surface lets menus cross the Hierarchy boundary
/// just like native editor context menus do.
pub fn build_hierarchy_context_overlay_surface(
    palette: StudioUiPalette,
    menu_target: Option<(raf_core::scene::SceneNodeId, bool)>,
    empty_menu_open: bool,
    empty_primitive_menu_open: bool,
    menu_label: Option<&str>,
    menu_position: Option<[f32; 2]>,
    surface_size: [f32; 2],
    can_paste: bool,
) -> UiSurface {
    let mut root = UiNode::new("hierarchy.context-overlay.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            padding: UiSpacing::same(HIERARCHY_ROOT_PADDING),
            ..UiLayout::fill(UiFlow::None)
        })
        .with_style(UiStyle::transparent());
    if let Some((target, is_folder)) = menu_target {
        root = root.with_child(context_menu(
            palette,
            target,
            is_folder,
            menu_label,
            menu_position,
            surface_size,
            can_paste,
        ));
    } else if empty_menu_open {
        root = root.with_child(empty_context_menu(
            palette,
            menu_position,
            surface_size,
            can_paste,
        ));
        if empty_primitive_menu_open {
            root = root.with_child(empty_primitive_menu(palette, menu_position, surface_size));
        }
    }
    let mut surface = UiSurface::new("editor.hierarchy.context-overlay", palette, root);
    surface.style_sheet = hierarchy_style_sheet(palette);
    surface
}

fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}

fn icon_button(
    _palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    command: impl Into<String>,
    tooltip: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hierarchy-icon-button")
        .with_layout(UiLayout::fixed(24.0, 26.0))
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_tooltip_key(tooltip)
        .with_accessibility_label_key(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

pub(crate) fn hierarchy_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let radius = raf_ui::UiTheme::raf_ui().metrics.corner_radius;
    let mut rules = vec![
        class_rule(
            "hierarchy-root",
            tokens.background,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "hierarchy-header",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-tabs".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_alt),
                border: Some(tokens.border),
                text: Some(tokens.text),
                border_width: Some(1.0),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        flat_class_rule("hierarchy-toolbar", tokens.surface, tokens.text),
        flat_class_rule("hierarchy-row", [0, 0, 0, 0], tokens.text),
        flat_class_rule("hierarchy-row-selected", tokens.selection, tokens.text),
        class_rule(
            "hierarchy-row-drop-target",
            tokens.selection,
            tokens.accent_hot,
            tokens.text,
        ),
        class_rule(
            "hierarchy-selection-edge",
            tokens.accent,
            tokens.accent,
            tokens.accent,
        ),
        class_rule(
            "hierarchy-selection-edge-idle",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            [0, 0, 0, 0],
        ),
        flat_class_rule("hierarchy-row-hidden", [0, 0, 0, 0], tokens.text_muted),
        flat_class_rule("hierarchy-row-locked", [0, 0, 0, 0], tokens.text_muted),
        class_rule(
            "hierarchy-search",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "hierarchy-icon-button",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        flat_class_rule("hierarchy-row-action", [0, 0, 0, 0], tokens.text_muted),
        flat_class_rule("hierarchy-row-action-selected", [0, 0, 0, 0], tokens.text),
        flat_class_rule("hierarchy-expand", [0, 0, 0, 0], tokens.text_muted),
        flat_class_rule("hierarchy-row-label", [0, 0, 0, 0], tokens.text),
        flat_class_rule("hierarchy-tab", [0, 0, 0, 0], tokens.text_muted),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-tab-active".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                text: Some(tokens.text),
                border_width: Some(1.0),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        class_rule(
            "hierarchy-context-menu",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        flat_class_rule("hierarchy-menu-button", [0, 0, 0, 0], tokens.text),
        class_rule(
            "hierarchy-rename-input",
            tokens.surface_alt,
            tokens.accent,
            tokens.text,
        ),
        class_rule(
            "hierarchy-empty",
            tokens.surface,
            tokens.border,
            tokens.text_muted,
        ),
        class_rule(
            "hierarchy-drag-ghost",
            tokens.surface_raised,
            tokens.accent,
            tokens.text,
        ),
        class_rule(
            "hierarchy-box-selection",
            tokens.selection,
            tokens.accent,
            tokens.accent,
        ),
    ];
    rules.extend([
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-row".to_string()),
            UiStylePatch {
                fill: Some(match palette {
                    StudioUiPalette::IndustrialDark => [255, 255, 255, 12],
                    StudioUiPalette::PaperLight => [0, 0, 0, 10],
                }),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-row-hidden".to_string()),
            UiStylePatch {
                fill: Some(match palette {
                    StudioUiPalette::IndustrialDark => [255, 255, 255, 8],
                    StudioUiPalette::PaperLight => [0, 0, 0, 6],
                }),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-row-locked".to_string()),
            UiStylePatch {
                fill: Some(match palette {
                    StudioUiPalette::IndustrialDark => [255, 255, 255, 8],
                    StudioUiPalette::PaperLight => [0, 0, 0, 6],
                }),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-icon-button".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-icon-button".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.focus),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Focused),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-expand".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-row-action".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-tab".to_string()),
            UiStylePatch {
                fill: Some(match palette {
                    StudioUiPalette::IndustrialDark => [255, 255, 255, 12],
                    StudioUiPalette::PaperLight => [0, 0, 0, 8],
                }),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-menu-button".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-menu-button".to_string()),
            UiStylePatch {
                fill: Some([0, 0, 0, 0]),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text_muted),
                opacity: Some(0.45),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Disabled),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-menu-delete".to_string()),
            UiStylePatch {
                fill: Some(tokens.danger),
                border: Some([0, 0, 0, 0]),
                text: Some(tokens.text),
                radius: Some(radius),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    ]);
    UiStyleSheet { rules }
}

fn flat_class_rule(class: &str, fill: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    let radius = raf_ui::UiTheme::raf_ui().metrics.corner_radius;
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some([0, 0, 0, 0]),
            text: Some(text),
            border_width: Some(0.0),
            radius: Some(radius),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}

fn class_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    let radius = raf_ui::UiTheme::raf_ui().metrics.corner_radius;
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(1.0),
            radius: Some(radius),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}
