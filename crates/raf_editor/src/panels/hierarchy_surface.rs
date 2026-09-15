//! Declarative RafUI document for the scene Hierarchy.
//!
//! The surface only describes presentation and semantic events. Scene
//! mutations stay in `hierarchy_surface_host.rs` and the editor application.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize, UiSurface,
};
use raf_ui::{
    UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiJustify, UiLayout,
    UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurfaceMaterial, UiTextInput,
    UiTextOverflow, UiTextRole, UiTextStyle,
};

use super::hierarchy_model::{HierarchyRow, HierarchyView};
use super::primitive_create::{primitive_key, primitive_slug, CREATEABLE_PRIMITIVES};
use crate::project_catalog::ProjectCatalogResults;
use raf_core::scene::Primitive;

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
    bookmark_filled: [bool; 3],
    search_results: Option<&ProjectCatalogResults>,
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
            gap: 1.0,
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
    let content = match active_tab {
        "assets" => assets_tab(palette),
        "world" => world_tab(palette),
        "bookmarks" => bookmarks_tab(palette, bookmark_filled),
        "search" => search_tab(palette, search_results),
        _ => tree,
    };
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
        .with_child(if active_tab == "hierarchy" {
            toolbar(palette)
        } else {
            tab_context_toolbar(palette, active_tab)
        })
        .with_child(if active_tab == "hierarchy" {
            summary(palette, view.total_rows, selected.len())
        } else {
            tab_summary(palette, active_tab)
        })
        .with_child(content);

    let root = if let Some((label, pointer)) = drag_ghost {
        root.with_child(drag_ghost_node(palette, label, pointer))
    } else {
        root
    };
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

fn empty_context_menu(
    palette: StudioUiPalette,
    position: Option<[f32; 2]>,
    surface_size: [f32; 2],
    can_paste: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let menu_rect = empty_context_menu_rect(position, can_paste, surface_size);
    let authored_rect = raf_ui::UiRect::new(
        menu_rect.x - HIERARCHY_ROOT_PADDING,
        menu_rect.y - HIERARCHY_ROOT_PADDING,
        menu_rect.width,
        menu_rect.height,
    );
    let mut menu = UiNode::new("hierarchy.empty-context-menu", UiNodeKind::Menu)
        .with_class("hierarchy-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CONTEXT_MENU_GAP,
            padding: UiSpacing::same(CONTEXT_MENU_PADDING),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::absolute(authored_rect).with_z_index(220)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "hierarchy.menu.close",
        ))
        .with_child(menu_title(
            palette,
            "hierarchy.empty-context-menu.title",
            "app.scene_actions",
            UiIconId::Scene,
        ))
        .with_child(menu_item(
            palette,
            "hierarchy.empty-context-menu.create-primitive",
            "app.create_primitive",
            UiIconId::Cube,
            "hierarchy.empty.create-primitive-menu",
            true,
        ));
    menu = menu.with_child(menu_item(
        palette,
        "hierarchy.empty-context-menu.entity",
        "app.create_entity",
        UiIconId::Node,
        "hierarchy.create-entity:root",
        false,
    ));
    menu = menu.with_child(menu_item(
        palette,
        "hierarchy.empty-context-menu.folder",
        "app.add_folder",
        UiIconId::Folder,
        "hierarchy.create-folder:root",
        false,
    ));
    if can_paste {
        menu = menu.with_child(menu_separator("hierarchy.empty-context-menu.sep", palette));
        menu = menu.with_child(menu_item(
            palette,
            "hierarchy.empty-context-menu.paste",
            "app.paste_menu",
            UiIconId::Add,
            "hierarchy.paste:root",
            false,
        ));
    }
    menu
}

fn empty_primitive_menu(
    palette: StudioUiPalette,
    position: Option<[f32; 2]>,
    surface_size: [f32; 2],
) -> UiNode {
    let tokens = palette.tokens();
    let rect = empty_primitive_menu_rect(position, surface_size);
    let authored_rect = raf_ui::UiRect::new(
        rect.x - HIERARCHY_ROOT_PADDING,
        rect.y - HIERARCHY_ROOT_PADDING,
        rect.width,
        rect.height,
    );
    let mut menu = UiNode::new("hierarchy.empty-primitive-menu", UiNodeKind::Menu)
        .with_class("hierarchy-context-menu hierarchy-primitive-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CONTEXT_MENU_GAP,
            padding: UiSpacing::same(CONTEXT_MENU_PADDING),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::absolute(authored_rect).with_z_index(230)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "hierarchy.menu.close",
        ))
        .with_child(menu_title(
            palette,
            "hierarchy.empty-primitive-menu.title",
            "app.create_primitive",
            UiIconId::Cube,
        ));
    for primitive in CREATEABLE_PRIMITIVES {
        menu = menu.with_child(primitive_menu_item(
            palette,
            primitive,
            format!(
                "hierarchy.create-primitive:root:{}",
                primitive_slug(primitive)
            ),
        ));
    }
    menu
}

fn menu_separator(id: &str, palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::xy(4.0, 2.0),
            ..UiLayout::fixed(0.0, 5.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new(format!("{id}.line"), UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill))
                .with_style(UiStyle {
                    fill: [tokens.border[0], tokens.border[1], tokens.border[2], 120],
                    border: [0, 0, 0, 0],
                    text: [0, 0, 0, 0],
                    border_width: 0.0,
                    radius: 0.0,
                    opacity: 0.7,
                }),
        )
}

fn menu_title(palette: StudioUiPalette, id: &str, text_key: &str, icon: UiIconId) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, CONTEXT_MENU_TITLE_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(tokens.accent),
        )
        .with_text_key(text_key)
        .with_text_style(UiTextStyle::panel_title(tokens.text))
}

fn menu_item(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    icon: UiIconId,
    command: impl Into<String>,
    arrow: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let mut item = UiNode::new(id, UiNodeKind::Button)
        .with_class("hierarchy-menu-button")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, CONTEXT_MENU_BUTTON_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(icon)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(text_key)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content().with_text_safe_area(true)
                }),
        )
        .with_accessibility_label_key(text_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command));
    if arrow {
        item = item.with_child(
            UiNode::new(format!("{id}.arrow"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::ChevronRight)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        );
    }
    item
}

fn primitive_menu_item(
    palette: StudioUiPalette,
    primitive: Primitive,
    command: impl Into<String>,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("hierarchy.primitive-option.{}", primitive_slug(primitive)),
        UiNodeKind::Button,
    )
    .with_class("hierarchy-menu-button hierarchy-primitive-option")
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 8.0,
        padding: UiSpacing::xy(8.0, 0.0),
        ..UiLayout::fixed(0.0, CONTEXT_MENU_BUTTON_HEIGHT).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(
            format!(
                "hierarchy.primitive-option.{}.icon",
                primitive_slug(primitive)
            ),
            UiNodeKind::Label,
        )
        .with_icon(
            UiIcon::new(match primitive {
                Primitive::Cube => UiIconId::Cube,
                Primitive::Sphere => UiIconId::Sphere,
                Primitive::Cylinder => UiIconId::Cylinder,
                Primitive::Plane => UiIconId::Plane,
                Primitive::Empty => UiIconId::Node,
            })
            .with_size(UiIconSize::Small)
            .with_tint(tokens.accent),
        )
        .with_layout(UiLayout::fixed(16.0, 16.0)),
    )
    .with_child(
        UiNode::new(
            format!(
                "hierarchy.primitive-option.{}.label",
                primitive_slug(primitive)
            ),
            UiNodeKind::Label,
        )
        .with_text_key(primitive_key(primitive))
        .with_text_style(UiTextStyle::button(tokens.text))
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fit_content().with_text_safe_area(true)
        }),
    )
    .with_accessibility_label_key(primitive_key(primitive))
    .focusable()
    .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

fn header(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.header", UiNodeKind::Toolbar)
        .with_class("hierarchy-header")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("hierarchy.header.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Folder).with_size(UiIconSize::Small))
                .with_layout(UiLayout::fixed(18.0, 22.0)),
        )
        .with_child(
            UiNode::new("hierarchy.header.title", UiNodeKind::Label)
                .with_text_key("app.hierarchy")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
}

fn tabs(palette: StudioUiPalette, compact_tabs: bool, active_tab: &str) -> UiNode {
    let tokens = palette.tokens();
    let mut row = UiNode::new("hierarchy.tabs", UiNodeKind::Toolbar)
        .with_class("hierarchy-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            padding: UiSpacing::xy(2.0, 2.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
        });
    for (id, key, icon) in [
        ("hierarchy", "app.hierarchy", UiIconId::Scene),
        ("assets", "app.assets", UiIconId::Assets),
        ("world", "app.world", UiIconId::View3d),
        ("bookmarks", "app.bookmarks", UiIconId::Folder),
        ("search", "app.search", UiIconId::Search),
    ] {
        let active = id == active_tab;
        let mut tab_text = UiTextStyle::button(if active {
            tokens.text
        } else {
            tokens.text_muted
        });
        tab_text.size_px = 9.0;
        tab_text.line_height_px = 13.0;
        let mut button = UiNode::new(format!("hierarchy.tab.{id}"), UiNodeKind::Button)
            .with_class(if active {
                "hierarchy-tab-active"
            } else {
                "hierarchy-tab"
            })
            .interactive()
            .with_layout(if compact_tabs {
                UiLayout::fixed(32.0, 22.0)
            } else {
                let width = match id {
                    "hierarchy" => 86.0,
                    "assets" => 70.0,
                    "world" => 68.0,
                    "bookmarks" => 102.0,
                    "search" => 78.0,
                    _ => 80.0,
                };
                UiLayout::fixed(width, 22.0).with_text_safe_area(true)
            })
            .with_icon(
                UiIcon::new(icon)
                    .with_size(UiIconSize::Custom(10))
                    .with_tint(if active {
                        tokens.accent
                    } else {
                        tokens.text_muted
                    }),
            )
            .with_text_style(tab_text)
            .with_tooltip_key(key)
            .with_accessibility_label_key(key)
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hierarchy.tab:{id}"),
            ));
        if !compact_tabs {
            button = button.with_text_key(key);
        }
        row = row.with_child(button);
    }
    row
}

fn tab_context_toolbar(palette: StudioUiPalette, active_tab: &str) -> UiNode {
    let tokens = palette.tokens();
    let (label, command) = match active_tab {
        "assets" => ("app.open_assets_panel", "hierarchy.open-bottom:assets"),
        "world" => ("app.world", "hierarchy.world.wip"),
        "bookmarks" => ("app.camera_bookmarks", "hierarchy.bookmarks.info"),
        "search" => ("app.search_everywhere", "hierarchy.search.open"),
        _ => ("app.hierarchy", "hierarchy.tab:hierarchy"),
    };
    UiNode::new("hierarchy.tab-context", UiNodeKind::Toolbar)
        .with_class("hierarchy-tab-context")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(6.0, 0.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("hierarchy.tab-context.label", UiNodeKind::Label)
                .with_text_key(label)
                .with_text_style(UiTextStyle::button(tokens.text_muted))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
        .with_child(
            UiNode::new("hierarchy.tab-context.action", UiNodeKind::Button)
                .with_class("hierarchy-tab-context-action")
                .with_text_key(if active_tab == "world" {
                    "app.work_in_progress"
                } else {
                    "app.open"
                })
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout::fixed(88.0, 24.0))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        )
}

fn tab_summary(palette: StudioUiPalette, active_tab: &str) -> UiNode {
    let tokens = palette.tokens();
    let key = match active_tab {
        "world" => "app.world_wip_hint",
        "bookmarks" => "app.bookmark_hint",
        "search" => "app.search_hint",
        "assets" => "app.assets_hint",
        _ => "app.hierarchy",
    };
    UiNode::new("hierarchy.tab-summary", UiNodeKind::Label)
        .with_text_key(key)
        .with_text_style(compact_body_style(tokens.text_muted))
        .with_layout(UiLayout::fit_content())
}

fn assets_tab(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.assets-tab", UiNodeKind::Panel)
        .with_class("hierarchy-tab-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::same(12.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("hierarchy.assets-tab.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(UiIconId::Assets).with_size(UiIconSize::Panel))
                .with_layout(UiLayout::fixed(24.0, 24.0)),
        )
        .with_child(
            UiNode::new("hierarchy.assets-tab.title", UiNodeKind::Label)
                .with_text_key("app.assets")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hierarchy.assets-tab.body", UiNodeKind::Label)
                .with_text_key("app.assets_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn world_tab(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.world-tab", UiNodeKind::Panel)
        .with_class("hierarchy-tab-card hierarchy-tab-card-wip")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::same(12.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("hierarchy.world-tab.title", UiNodeKind::Label)
                .with_text_key("app.world")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hierarchy.world-tab.badge", UiNodeKind::Label)
                .with_text_key("app.work_in_progress")
                .with_class("hierarchy-wip-badge")
                .with_text_style(UiTextStyle::button(tokens.accent))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hierarchy.world-tab.body", UiNodeKind::Label)
                .with_text_key("app.world_wip_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn bookmarks_tab(palette: StudioUiPalette, filled: [bool; 3]) -> UiNode {
    let tokens = palette.tokens();
    let mut list = UiNode::new("hierarchy.bookmarks-tab", UiNodeKind::Panel)
        .with_class("hierarchy-tab-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::same(8.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    for (index, is_filled) in filled.into_iter().enumerate() {
        let slot = index + 1;
        list = list.with_child(
            UiNode::new(format!("hierarchy.bookmark.{slot}"), UiNodeKind::Toolbar)
                .with_class("hierarchy-bookmark-row")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 6.0,
                    padding: UiSpacing::xy(6.0, 2.0),
                    ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new(
                        format!("hierarchy.bookmark.{slot}.label"),
                        UiNodeKind::Label,
                    )
                    .with_text_value(format!("{slot}"))
                    .with_text_style(UiTextStyle::button(tokens.text))
                    .with_layout(UiLayout::fixed(18.0, 22.0)),
                )
                .with_child(
                    UiNode::new(
                        format!("hierarchy.bookmark.{slot}.state"),
                        UiNodeKind::Label,
                    )
                    .with_text_key(if is_filled {
                        "app.bookmark_saved_state"
                    } else {
                        "app.bookmark_empty_state"
                    })
                    .with_text_style(UiTextStyle::body(tokens.text_muted))
                    .with_layout(UiLayout {
                        grow: 1.0,
                        ..UiLayout::fit_content()
                    }),
                )
                .with_child(
                    UiNode::new(
                        format!("hierarchy.bookmark.{slot}.restore"),
                        UiNodeKind::Button,
                    )
                    .with_class("hierarchy-bookmark-action")
                    .with_text_key("app.restore")
                    .with_layout(UiLayout::fixed(70.0, 24.0))
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("hierarchy.bookmark.restore:{index}"),
                    )),
                )
                .with_child(
                    UiNode::new(
                        format!("hierarchy.bookmark.{slot}.save"),
                        UiNodeKind::Button,
                    )
                    .with_class("hierarchy-bookmark-action hierarchy-bookmark-save")
                    .with_text_key("app.save")
                    .with_layout(UiLayout::fixed(58.0, 24.0))
                    .focusable()
                    .with_event(UiEventBinding::command(
                        UiEventKind::Click,
                        format!("hierarchy.bookmark.save:{index}"),
                    )),
                ),
        );
    }
    list
}

fn search_tab(palette: StudioUiPalette, results: Option<&ProjectCatalogResults>) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::new("hierarchy.search-tab", UiNodeKind::Panel)
        .with_class("hierarchy-tab-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::same(10.0),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::fill(UiFlow::Column).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::text_input(
                "hierarchy.search.global",
                UiTextInput {
                    value_key: "hierarchy.search.global".to_string(),
                    placeholder_key: Some("app.search_everywhere".to_string()),
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: Some("hierarchy.search.open".to_string()),
                },
            )
            .with_class("hierarchy-search-global")
            .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("hierarchy.search-tab.body", UiNodeKind::Label)
                .with_text_key("app.search_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hierarchy.search-tab.open", UiNodeKind::Button)
                .with_text_key("app.open_search")
                .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "hierarchy.search.open",
                )),
        );

    let Some(results) = results else {
        return panel;
    };
    let result_count = results.assets.len() + results.project_entries.len();
    panel = panel.with_child(
        UiNode::new("hierarchy.search-tab.results-heading", UiNodeKind::Label)
            .with_text_key("app.search_results")
            .with_text_style(UiTextStyle::button(tokens.text))
            .with_layout(UiLayout::fit_content()),
    );
    panel = panel.with_child(
        UiNode::new("hierarchy.search-tab.results-count", UiNodeKind::Label)
            .with_text_value(result_count.to_string())
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_layout(UiLayout::fit_content()),
    );
    for (index, entry) in results.project_entries.iter().enumerate() {
        panel = panel.with_child(search_result_row(
            palette,
            &format!("hierarchy.search.project.{index}"),
            &entry.label,
            entry.icon,
            entry.depth,
        ));
    }
    for (index, asset) in results.assets.iter().enumerate() {
        panel = panel.with_child(search_result_row(
            palette,
            &format!("hierarchy.search.asset.{index}"),
            asset,
            UiIconId::Assets,
            0,
        ));
    }
    panel
}

fn search_result_row(
    palette: StudioUiPalette,
    id: &str,
    label: &str,
    icon: UiIconId,
    depth: u8,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Toolbar)
        .with_class("hierarchy-search-result")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing {
                left: 6.0 + f32::from(depth) * 10.0,
                right: 6.0,
                top: 2.0,
                bottom: 2.0,
            },
            ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_value(label.to_string())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fit_content()
                }),
        )
}

fn toolbar(palette: StudioUiPalette) -> UiNode {
    UiNode::new("hierarchy.toolbar", UiNodeKind::Toolbar)
        .with_class("hierarchy-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 4.0,
            padding: UiSpacing::xy(2.0, 0.0),
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::text_input(
                "hierarchy.search",
                UiTextInput {
                    value_key: "hierarchy.search".to_string(),
                    placeholder_key: Some("app.search_entities".to_string()),
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("hierarchy-search")
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [72.0, 28.0],
                ..UiLayout::fixed(0.0, 28.0)
            }),
        )
        .with_child(icon_button(
            palette,
            "hierarchy.filter",
            UiIconId::Filter,
            "hierarchy.toggle-hidden",
            "app.hierarchy_filter",
        ))
        .with_child(icon_button(
            palette,
            "hierarchy.add",
            UiIconId::Add,
            "hierarchy.create-folder:root",
            "app.add_folder",
        ))
}

fn summary(palette: StudioUiPalette, row_count: usize, selected_count: usize) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.summary", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(2.0, 0.0),
            ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("hierarchy.summary.count", UiNodeKind::Label)
                .with_text_value(row_count.to_string())
                .with_text_style(compact_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hierarchy.summary.count-label", UiNodeKind::Label)
                .with_text_key(if row_count == 1 {
                    "app.item"
                } else {
                    "app.items"
                })
                .with_text_style(compact_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hierarchy.summary.selected-count", UiNodeKind::Label)
                .with_text_value(selected_count.to_string())
                .with_text_style(compact_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hierarchy.summary.selected", UiNodeKind::Label)
                .with_text_key("app.selected")
                .with_text_style(compact_body_style(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
}

fn row_node(
    palette: StudioUiPalette,
    row: &HierarchyRow,
    selected: &[raf_core::scene::SceneNodeId],
    renaming: Option<(raf_core::scene::SceneNodeId, &str)>,
    row_height: f32,
    indent_width: f32,
    show_icons: bool,
    show_visibility: bool,
    show_locked: bool,
    drop_target: Option<raf_core::scene::SceneNodeId>,
) -> UiNode {
    let is_selected = selected.contains(&row.id);
    let class = if drop_target == Some(row.id) {
        "hierarchy-row-drop-target"
    } else if is_selected {
        "hierarchy-row-selected"
    } else if !row.visible {
        "hierarchy-row-hidden"
    } else if row.locked {
        "hierarchy-row-locked"
    } else {
        "hierarchy-row"
    };
    let mut root = UiNode::new(format!("hierarchy.row.{}", row.id.0), UiNodeKind::Toolbar)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 3.0,
            padding: UiSpacing::xy(3.0, 1.0),
            ..UiLayout::fixed(0.0, row_height).with_width_mode(UiSizeMode::Fill)
        });

    root = root.with_child(
        UiNode::new(
            format!("hierarchy.row.{}.selection-edge", row.id.0),
            UiNodeKind::Panel,
        )
        .with_class(if is_selected {
            "hierarchy-selection-edge"
        } else {
            "hierarchy-selection-edge-idle"
        })
        .with_layout(UiLayout::fixed(2.0, row_height - 4.0)),
    );

    for level in 0..row.depth {
        root = root.with_child(indent_guide(
            palette,
            row.id,
            level,
            row_height,
            indent_width,
        ));
    }
    if row.has_children {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.row.{}.expand", row.id.0),
                UiNodeKind::Button,
            )
            .with_class("hierarchy-expand")
            .with_class("hierarchy-row-action")
            .with_class(if is_selected {
                "hierarchy-row-action-selected"
            } else {
                ""
            })
            .with_layout(UiLayout::fixed(18.0, row_height - 2.0))
            .with_icon(
                UiIcon::new(if row.expanded {
                    UiIconId::ChevronDown
                } else {
                    UiIconId::ChevronRight
                })
                .with_size(UiIconSize::Small),
            )
            .with_tooltip_key(if row.expanded {
                "app.collapse_menu"
            } else {
                "app.expand_menu"
            })
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hierarchy.expand:{}", row.id.0),
            )),
        );
    } else {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.row.{}.expand-spacer", row.id.0),
                UiNodeKind::Panel,
            )
            .with_layout(UiLayout::fixed(18.0, row_height)),
        );
    }

    if let Some((rename_id, _rename_value)) = renaming {
        if rename_id == row.id {
            root = root.with_child(
                UiNode::text_input(
                    format!("hierarchy.rename.control.{}", row.id.0),
                    UiTextInput {
                        value_key: format!("hierarchy.rename.{}", row.id.0),
                        placeholder_key: Some("app.rename".to_string()),
                        max_length: 256,
                        multiline: false,
                        password: false,
                        submit_command: Some(format!("hierarchy.rename.commit:{}", row.id.0)),
                    },
                )
                .with_class("hierarchy-rename-input")
                .with_layout(UiLayout {
                    grow: 1.0,
                    min_size: [72.0, row_height - 2.0],
                    overflow: UiOverflow::Clip,
                    ..UiLayout::fixed(0.0, row_height - 2.0)
                }),
            );
            return root;
        }
    }

    let mut main = UiNode::new(
        format!("hierarchy.row.{}.select", row.id.0),
        UiNodeKind::Button,
    )
    .with_class("hierarchy-row-label")
    .with_layout(UiLayout {
        grow: 1.0,
        min_size: [64.0, row_height - 2.0],
        padding: UiSpacing::xy(4.0, 0.0),
        overflow: UiOverflow::Clip,
        ..UiLayout::fixed(0.0, row_height - 2.0)
    })
    .with_text_value(row.name.clone())
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_tooltip_value(row.name.clone())
    .with_text_style(compact_body_style(if is_selected {
        [255, 247, 232, 255]
    } else {
        [220, 224, 230, 255]
    }))
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("hierarchy.select:{}", row.id.0),
    ))
    .with_event(UiEventBinding {
        event: UiEventKind::DoubleClick,
        action: UiAction::Command {
            name: format!("hierarchy.rename.begin:{}", row.id.0),
        },
    })
    .with_event(UiEventBinding {
        event: UiEventKind::ContextMenu,
        action: UiAction::OpenMenu {
            id: format!("hierarchy.menu:{}", row.id.0),
        },
    })
    .with_event(UiEventBinding::command(
        UiEventKind::DragStart,
        format!("hierarchy.drag.start:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragMove,
        format!("hierarchy.drag.move:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::DragEnd,
        format!("hierarchy.drag.end:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::PointerMove,
        format!("hierarchy.drag.over:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("f2".to_string()),
        format!("hierarchy.rename.begin:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("delete".to_string()),
        format!("hierarchy.delete:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("arrowup".to_string()),
        format!("hierarchy.navigate:up:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("arrowdown".to_string()),
        format!("hierarchy.navigate:down:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("home".to_string()),
        format!("hierarchy.navigate:home:{}", row.id.0),
    ))
    .with_event(UiEventBinding::command(
        UiEventKind::KeyPress("end".to_string()),
        format!("hierarchy.navigate:end:{}", row.id.0),
    ));
    if show_icons {
        let icon_tint = if is_selected {
            palette.tokens().accent_hot
        } else {
            palette.tokens().text_muted
        };
        main = main.with_icon(
            UiIcon::new(row_icon(row))
                .with_size(UiIconSize::Small)
                .with_tint(icon_tint),
        );
    }
    root = root.with_child(main);
    if show_visibility {
        root = root.with_child(
            icon_button(
                palette,
                &format!("hierarchy.row.{}.visibility", row.id.0),
                if row.visible {
                    UiIconId::Eye
                } else {
                    UiIconId::EyeOff
                },
                format!("hierarchy.visibility:{}", row.id.0),
                "app.visible",
            )
            .with_class("hierarchy-row-action")
            .with_class(if is_selected {
                "hierarchy-row-action-selected"
            } else {
                ""
            }),
        );
    }
    if show_locked {
        root = root.with_child(
            UiNode::new(
                format!("hierarchy.row.{}.lock", row.id.0),
                UiNodeKind::Button,
            )
            .with_class("hierarchy-icon-button")
            .with_class("hierarchy-row-action")
            .with_class(if is_selected {
                "hierarchy-row-action-selected"
            } else {
                ""
            })
            .with_layout(UiLayout::fixed(24.0, row_height - 2.0))
            .with_icon(
                UiIcon::new(if row.locked {
                    UiIconId::Lock
                } else {
                    UiIconId::Unlock
                })
                .with_size(UiIconSize::Small),
            )
            .with_tooltip_key("app.hierarchy_lock")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hierarchy.lock:{}", row.id.0),
            )),
        );
    }
    root.with_child(
        icon_button(
            palette,
            &format!("hierarchy.row.{}.menu", row.id.0),
            UiIconId::More,
            format!("hierarchy.menu:{}", row.id.0),
            "app.more_menu",
        )
        .with_class("hierarchy-row-action")
        .with_class(if is_selected {
            "hierarchy-row-action-selected"
        } else {
            ""
        }),
    )
}

fn compact_body_style(color: [u8; 4]) -> UiTextStyle {
    UiTextStyle {
        // Hierarchy labels are compact controls, not prose. Keeping them in
        // the single-line role prevents narrow panels from wrapping names
        // one character at a time.
        role: UiTextRole::Button,
        size_px: 12.0,
        line_height_px: 16.0,
        weight: UiFontWeight::Regular,
        color,
        inherit_color: false,
    }
}

fn row_icon(row: &HierarchyRow) -> UiIconId {
    if row.is_folder {
        return UiIconId::Folder;
    }
    match row.primitive {
        Primitive::Empty => UiIconId::Node,
        Primitive::Cube => UiIconId::Cube,
        Primitive::Sphere => UiIconId::Sphere,
        Primitive::Plane => UiIconId::Plane,
        Primitive::Cylinder => UiIconId::Cylinder,
    }
}

fn indent_guide(
    palette: StudioUiPalette,
    id: raf_core::scene::SceneNodeId,
    level: usize,
    row_height: f32,
    indent_width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!("hierarchy.row.{}.indent.{}", id.0, level),
        UiNodeKind::Panel,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Stretch,
        justify_content: UiJustify::End,
        ..UiLayout::fixed(indent_width.max(8.0), row_height)
    })
    .with_child(
        UiNode::new(
            format!("hierarchy.row.{}.indent.{}.line", id.0, level),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout::fixed(1.0, row_height))
        .with_style(UiStyle {
            fill: tokens.border,
            border: tokens.border,
            text: tokens.border,
            border_width: 0.0,
            radius: 0.0,
            opacity: 0.28,
        }),
    )
}

fn context_menu(
    palette: StudioUiPalette,
    target: raf_core::scene::SceneNodeId,
    is_folder: bool,
    label: Option<&str>,
    position: Option<[f32; 2]>,
    surface_size: [f32; 2],
    can_paste: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let id = target.0;
    let menu_rect = context_menu_rect(position, is_folder, surface_size);
    let authored_menu_rect = raf_ui::UiRect::new(
        menu_rect.x - HIERARCHY_ROOT_PADDING,
        menu_rect.y - HIERARCHY_ROOT_PADDING,
        menu_rect.width,
        menu_rect.height,
    );
    let title_icon = if is_folder {
        UiIconId::Folder
    } else {
        UiIconId::Node
    };
    let mut title_node = UiNode::new("hierarchy.context-menu.title", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 7.0,
            padding: UiSpacing::xy(6.0, 0.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, CONTEXT_MENU_TITLE_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(
            UiIcon::new(title_icon)
                .with_size(UiIconSize::Small)
                .with_tint(tokens.accent),
        );
    title_node = match label {
        Some(label) => title_node.with_text_value(label.to_string()),
        None => title_node.with_text_key("app.hierarchy"),
    };
    title_node = title_node.with_text_style(UiTextStyle::panel_title(tokens.text));

    let mut menu = UiNode::new("hierarchy.context-menu", UiNodeKind::Menu)
        .with_class("hierarchy-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CONTEXT_MENU_GAP,
            padding: UiSpacing::same(CONTEXT_MENU_PADDING),
            overflow: UiOverflow::ScrollY,
            ..UiLayout::absolute(authored_menu_rect).with_z_index(220)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 6.0,
            opacity: 1.0,
        })
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "hierarchy.menu.close",
        ))
        .with_accessibility_label_key("app.hierarchy")
        .with_child(title_node);

    // Group 1: Edit actions
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.rename",
        "app.rename",
        Some(UiIconId::Select),
        format!("hierarchy.rename.begin:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.duplicate",
        "app.duplicate_menu",
        Some(UiIconId::Add),
        format!("hierarchy.duplicate:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.copy",
        "app.copy_menu",
        None,
        format!("hierarchy.copy:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.paste",
        "app.paste_menu",
        None,
        format!("hierarchy.paste:{id}"),
        false,
        !can_paste,
    ));

    // Separator 1
    menu = menu.with_child(menu_separator("hierarchy.context-menu.sep1", palette));

    // Group 2: View and Navigation
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.focus",
        "app.focus_entity",
        Some(UiIconId::Focus),
        format!("hierarchy.focus:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.expand",
        "app.expand_menu",
        Some(UiIconId::ChevronDown),
        format!("hierarchy.expand-recursive:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.collapse",
        "app.collapse_menu",
        Some(UiIconId::ChevronRight),
        format!("hierarchy.collapse-recursive:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.select-children",
        "app.select_children_menu",
        Some(UiIconId::Select),
        format!("hierarchy.select-children:{id}"),
        false,
        false,
    ));

    // Separator 2
    menu = menu.with_child(menu_separator("hierarchy.context-menu.sep2", palette));

    // Group 3: Structure and Hierarchy
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.entity",
        "app.create_entity",
        Some(UiIconId::Entity),
        format!("hierarchy.create-entity:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.folder",
        "app.add_folder",
        Some(UiIconId::Folder),
        format!("hierarchy.create-folder:{id}"),
        false,
        false,
    ));
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.root",
        "app.move_to_root",
        Some(UiIconId::Scene),
        format!("hierarchy.reparent-root:{id}"),
        false,
        false,
    ));
    if is_folder {
        menu = menu.with_child(context_menu_item(
            palette,
            "hierarchy.context-menu.ungroup",
            "app.ungroup",
            Some(UiIconId::Folder),
            format!("hierarchy.ungroup:{id}"),
            false,
            false,
        ));
    }

    // Separator 3
    menu = menu.with_child(menu_separator("hierarchy.context-menu.sep3", palette));

    // Group 4: Destructive
    menu = menu.with_child(context_menu_item(
        palette,
        "hierarchy.context-menu.delete",
        "app.delete_menu",
        Some(UiIconId::Trash),
        format!("hierarchy.delete:{id}"),
        true,
        false,
    ));

    menu
}

fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}

const CONTEXT_MENU_WIDTH: f32 = 236.0;
const EMPTY_CONTEXT_MENU_WIDTH: f32 = 226.0;
const EMPTY_PRIMITIVE_MENU_WIDTH: f32 = 220.0;
const HIERARCHY_ROOT_PADDING: f32 = 4.0;
const CONTEXT_MENU_MARGIN: f32 = 8.0;
const CONTEXT_MENU_PADDING: f32 = 6.0;
const CONTEXT_MENU_GAP: f32 = 2.0;
const CONTEXT_MENU_TITLE_HEIGHT: f32 = 26.0;
const CONTEXT_MENU_BUTTON_HEIGHT: f32 = 26.0;
const CONTEXT_MENU_BASE_BUTTON_COUNT: usize = 12;
const CONTEXT_MENU_SEPARATOR_COUNT: usize = 3;
const CONTEXT_MENU_SEPARATOR_HEIGHT: f32 = 5.0;

pub(crate) fn empty_context_menu_rect(
    position: Option<[f32; 2]>,
    can_paste: bool,
    surface_size: [f32; 2],
) -> raf_ui::UiRect {
    let button_count = 3 + usize::from(can_paste);
    let separator_height = if can_paste {
        CONTEXT_MENU_SEPARATOR_HEIGHT + CONTEXT_MENU_GAP
    } else {
        0.0
    };
    let natural_height = CONTEXT_MENU_PADDING * 2.0
        + CONTEXT_MENU_TITLE_HEIGHT
        + button_count as f32 * (CONTEXT_MENU_BUTTON_HEIGHT + CONTEXT_MENU_GAP)
        + separator_height;
    let height = bounded_menu_height(natural_height, surface_size[1]);
    let point = position.unwrap_or([CONTEXT_MENU_MARGIN, 130.0]);
    let x = clamp_menu_axis(point[0], surface_size[0].max(0.0), EMPTY_CONTEXT_MENU_WIDTH);
    let y = clamp_menu_axis(point[1], surface_size[1].max(0.0), height);
    raf_ui::UiRect::new(x, y, EMPTY_CONTEXT_MENU_WIDTH, height)
}

pub(crate) fn empty_primitive_menu_rect(
    position: Option<[f32; 2]>,
    surface_size: [f32; 2],
) -> raf_ui::UiRect {
    let main = empty_context_menu_rect(position, false, surface_size);
    let right_x = main.right() + 6.0;
    let x = if right_x + EMPTY_PRIMITIVE_MENU_WIDTH + CONTEXT_MENU_MARGIN <= surface_size[0] {
        right_x
    } else {
        (main.x - EMPTY_PRIMITIVE_MENU_WIDTH - 6.0).max(CONTEXT_MENU_MARGIN)
    };
    let natural_height = CONTEXT_MENU_PADDING * 2.0
        + CONTEXT_MENU_TITLE_HEIGHT
        + CREATEABLE_PRIMITIVES.len() as f32 * (CONTEXT_MENU_BUTTON_HEIGHT + CONTEXT_MENU_GAP);
    let height = bounded_menu_height(natural_height, surface_size[1]);
    let y = clamp_menu_axis(main.y, surface_size[1].max(0.0), height);
    raf_ui::UiRect::new(x, y, EMPTY_PRIMITIVE_MENU_WIDTH, height)
}

pub(crate) fn context_menu_rect(
    position: Option<[f32; 2]>,
    is_folder: bool,
    surface_size: [f32; 2],
) -> raf_ui::UiRect {
    let button_count = CONTEXT_MENU_BASE_BUTTON_COUNT + if is_folder { 1 } else { 0 };
    let natural_height = CONTEXT_MENU_PADDING * 2.0
        + CONTEXT_MENU_TITLE_HEIGHT
        + button_count as f32 * (CONTEXT_MENU_BUTTON_HEIGHT + CONTEXT_MENU_GAP)
        + CONTEXT_MENU_SEPARATOR_COUNT as f32 * (CONTEXT_MENU_SEPARATOR_HEIGHT + CONTEXT_MENU_GAP);
    let viewport_width = surface_size[0].max(0.0);
    let viewport_height = surface_size[1].max(0.0);
    let height = bounded_menu_height(natural_height, viewport_height);
    let point = position.unwrap_or([CONTEXT_MENU_MARGIN, 130.0]);
    let x = clamp_menu_axis(point[0], viewport_width, CONTEXT_MENU_WIDTH);
    let y = clamp_menu_axis(point[1], viewport_height, height);
    raf_ui::UiRect::new(x, y, CONTEXT_MENU_WIDTH, height)
}

fn bounded_menu_height(natural_height: f32, viewport_height: f32) -> f32 {
    let available = (viewport_height.max(0.0) - CONTEXT_MENU_MARGIN * 2.0).max(1.0);
    natural_height.min(available).max(1.0)
}

fn clamp_menu_axis(value: f32, viewport: f32, extent: f32) -> f32 {
    let max = (viewport - extent - CONTEXT_MENU_MARGIN).max(0.0);
    if max >= CONTEXT_MENU_MARGIN {
        value.clamp(CONTEXT_MENU_MARGIN, max)
    } else {
        value.clamp(0.0, max)
    }
}

fn drag_ghost_node(palette: StudioUiPalette, label: &str, pointer: [f32; 2]) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.drag-ghost", UiNodeKind::FloatingPanel)
        .with_class("hierarchy-drag-ghost")
        .with_layout(
            UiLayout::absolute(raf_ui::UiRect::new(
                (pointer[0] + 12.0).max(6.0),
                (pointer[1] + 12.0).max(6.0),
                188.0,
                28.0,
            ))
            .with_z_index(260),
        )
        .with_text_value(label.to_string())
        .with_text_style(UiTextStyle::body(tokens.text))
}

fn box_selection_node(palette: StudioUiPalette, selection: raf_ui::UiRect) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.box-selection", UiNodeKind::FloatingPanel)
        .with_class("hierarchy-box-selection")
        .with_layout(UiLayout::absolute(selection).with_z_index(240))
        .with_style(UiStyle {
            fill: [232, 133, 28, 24],
            border: tokens.accent,
            text: tokens.accent,
            border_width: 1.0,
            radius: 2.0,
            opacity: 0.72,
        })
}

fn context_menu_item(
    palette: StudioUiPalette,
    id: &str,
    text_key: &str,
    icon: Option<UiIconId>,
    command: impl Into<String>,
    danger: bool,
    disabled: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let class = if danger {
        "hierarchy-menu-button hierarchy-menu-delete"
    } else {
        "hierarchy-menu-button"
    };
    let text_color = if disabled {
        tokens.text_muted
    } else if danger {
        [240, 160, 160, 255]
    } else {
        [222, 226, 232, 255]
    };
    let mut item = UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, CONTEXT_MENU_BUTTON_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_accessibility_label_key(text_key)
        .focusable()
        .disabled(disabled)
        .with_event(UiEventBinding::command(UiEventKind::Click, command));

    if let Some(icon_id) = icon {
        let icon_tint = if disabled {
            tokens.text_muted
        } else if danger {
            [220, 110, 110, 255]
        } else {
            tokens.text_muted
        };
        item = item.with_child(
            UiNode::new(format!("{id}.icon"), UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(icon_id)
                        .with_size(UiIconSize::Small)
                        .with_tint(icon_tint),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        );
    } else {
        item = item.with_child(
            UiNode::new(format!("{id}.icon-spacer"), UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        );
    }

    item = item.with_child(
        UiNode::new(format!("{id}.label"), UiNodeKind::Label)
            .with_text_key(text_key)
            .with_text_style(UiTextStyle::button(text_color))
            .with_layout(UiLayout {
                grow: 1.0,
                ..UiLayout::fit_content().with_text_safe_area(true)
            }),
    );

    item
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

fn hierarchy_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let selection_fill = match palette {
        StudioUiPalette::IndustrialDark => [116, 67, 24, 62],
        StudioUiPalette::PaperLight => [224, 116, 24, 42],
    };
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
                radius: Some(4.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        flat_class_rule("hierarchy-toolbar", tokens.surface, tokens.text),
        flat_class_rule("hierarchy-row", [0, 0, 0, 0], tokens.text),
        flat_class_rule("hierarchy-row-selected", selection_fill, tokens.text),
        class_rule(
            "hierarchy-row-drop-target",
            [116, 67, 24, 90],
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
        flat_class_rule(
            "hierarchy-row-action-selected",
            [0, 0, 0, 0],
            [255, 247, 232, 255],
        ),
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
                radius: Some(3.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Always),
        class_rule(
            "hierarchy-tab-context",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "hierarchy-tab-context-action",
            tokens.surface_raised,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "hierarchy-tab-card",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "hierarchy-tab-card-wip",
            tokens.surface_raised,
            tokens.accent,
            tokens.text,
        ),
        class_rule(
            "hierarchy-wip-badge",
            [0, 0, 0, 0],
            tokens.accent,
            tokens.accent,
        ),
        class_rule(
            "hierarchy-bookmark-row",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "hierarchy-bookmark-action",
            tokens.surface_raised,
            tokens.border,
            tokens.text_muted,
        ),
        class_rule(
            "hierarchy-bookmark-save",
            tokens.accent,
            tokens.accent,
            tokens.text,
        ),
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
            "hierarchy-search-result",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
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
                radius: Some(3.0),
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
                radius: Some(3.0),
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
                radius: Some(3.0),
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
                radius: Some(3.0),
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
                radius: Some(3.0),
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
                radius: Some(3.0),
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
                radius: Some(3.0),
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
                fill: Some([180, 45, 45, 90]),
                border: Some([0, 0, 0, 0]),
                text: Some([255, 210, 210, 255]),
                radius: Some(3.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    ]);
    UiStyleSheet { rules }
}

fn flat_class_rule(class: &str, fill: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some([0, 0, 0, 0]),
            text: Some(text),
            border_width: Some(0.0),
            radius: Some(3.0),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}

fn class_rule(class: &str, fill: [u8; 4], border: [u8; 4], text: [u8; 4]) -> UiStyleRule {
    UiStyleRule::new(
        UiStyleSelector::Class(class.to_string()),
        UiStylePatch {
            fill: Some(fill),
            border: Some(border),
            text: Some(text),
            border_width: Some(1.0),
            radius: Some(3.0),
            ..UiStylePatch::default()
        },
    )
    .when(UiStyleRuleState::Always)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(primitive: Primitive, is_folder: bool) -> HierarchyRow {
        HierarchyRow {
            id: raf_core::scene::SceneNodeId(0),
            depth: 0,
            name: "node".to_string(),
            primitive,
            is_folder,
            visible: true,
            locked: false,
            has_children: false,
            expanded: false,
        }
    }

    fn find_node<'a>(node: &'a raf_ui::UiNode, id: &str) -> Option<&'a raf_ui::UiNode> {
        if node.id == id {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_node(child, id))
    }

    #[test]
    fn hierarchy_rows_use_semantic_primitive_icons() {
        assert_eq!(row_icon(&row(Primitive::Cube, false)), UiIconId::Cube);
        assert_eq!(row_icon(&row(Primitive::Sphere, false)), UiIconId::Sphere);
        assert_eq!(row_icon(&row(Primitive::Plane, false)), UiIconId::Plane);
        assert_eq!(
            row_icon(&row(Primitive::Cylinder, false)),
            UiIconId::Cylinder
        );
        assert_eq!(row_icon(&row(Primitive::Empty, false)), UiIconId::Node);
        assert_eq!(row_icon(&row(Primitive::Cube, true)), UiIconId::Folder);
    }

    #[test]
    fn hierarchy_chrome_does_not_introduce_project_type_tabs_or_painted_drag_slots() {
        let view = HierarchyView {
            total_rows: 1,
            visible_start: 0,
            top_spacer: 0.0,
            bottom_spacer: 0.0,
            rows: vec![row(Primitive::Cube, false)],
        };
        let surface = build_hierarchy_surface(
            StudioUiPalette::IndustrialDark,
            &view,
            &[],
            None,
            None,
            false,
            false,
            None,
            None,
            [460.0, 400.0],
            26.0,
            14.0,
            true,
            true,
            true,
            1.0,
            None,
            None,
            None,
            false,
            false,
            "hierarchy",
            [false; 3],
            None,
        );
        let serialized = format!("{surface:?}");
        assert!(!serialized.contains("Games"));
        assert!(!serialized.contains("Electronics"));
        assert!(!serialized.contains("hierarchy.toolbar.accent"));
        assert!(!serialized.contains("hierarchy-drop-zone"));

        for (id, key) in [
            ("hierarchy", "app.hierarchy"),
            ("assets", "app.assets"),
            ("world", "app.world"),
            ("bookmarks", "app.bookmarks"),
            ("search", "app.search"),
        ] {
            let tab = find_node(&surface.root, &format!("hierarchy.tab.{id}"))
                .expect("hierarchy tab should be retained");
            assert!(tab.interactive, "{id} must receive hover input");
            assert_eq!(tab.tooltip_key.as_deref(), Some(key));
        }
    }

    #[test]
    fn regular_hierarchy_tabs_keep_all_labels_inside_the_panel() {
        let row = tabs(StudioUiPalette::IndustrialDark, false, "hierarchy");
        let total_width: f32 = row.children.iter().map(|tab| tab.layout.basis[0]).sum();

        assert_eq!(row.children.len(), 5);
        assert!(row.children.iter().all(|tab| tab.text_key.is_some()));
        assert!(total_width + row.layout.padding.left + row.layout.padding.right <= 414.0);
    }

    #[test]
    fn hierarchy_row_primary_target_opens_its_context_menu() {
        let row = row_node(
            StudioUiPalette::IndustrialDark,
            &row(Primitive::Cube, false),
            &[],
            None,
            26.0,
            14.0,
            true,
            true,
            true,
            None,
        );
        let target = find_node(&row, "hierarchy.row.0.select").expect("row target");

        assert!(target.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::OpenMenu { id } if id == "hierarchy.menu:0"
        )));
        assert_eq!(target.tooltip_value.as_deref(), Some("node"));
    }

    #[test]
    fn hierarchy_context_menu_is_bounded_inside_small_viewports() {
        let rect = context_menu_rect(Some([12.0, 190.0]), true, [240.0, 200.0]);

        assert!(rect.height < 200.0);
        assert!(rect.bottom() <= 192.0);
    }

    #[test]
    fn empty_context_menu_exposes_primitive_disclosure_and_options() {
        let surface = build_hierarchy_context_overlay_surface(
            StudioUiPalette::IndustrialDark,
            None,
            true,
            true,
            None,
            Some([80.0, 80.0]),
            [900.0, 600.0],
            false,
        );
        let trigger = find_node(
            &surface.root,
            "hierarchy.empty-context-menu.create-primitive",
        )
        .expect("primitive trigger");
        assert!(trigger.event_handlers.iter().any(|binding| matches!(
            &binding.action,
            raf_ui::UiAction::Command { name }
                if name == "hierarchy.empty.create-primitive-menu"
        )));
        let arrow = find_node(
            &surface.root,
            "hierarchy.empty-context-menu.create-primitive.arrow",
        )
        .expect("primitive disclosure arrow");
        assert_eq!(
            arrow.icon.as_ref().map(|icon| icon.id),
            Some(UiIconId::ChevronRight)
        );

        for slug in ["cube", "sphere", "cylinder", "plane"] {
            assert!(
                find_node(&surface.root, &format!("hierarchy.primitive-option.{slug}")).is_some()
            );
        }
    }
}
