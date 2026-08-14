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
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiTextInput, UiTextRole,
    UiTextStyle,
};

use super::hierarchy_model::{HierarchyRow, HierarchyView};
use raf_core::scene::Primitive;

pub fn build_hierarchy_surface(
    palette: StudioUiPalette,
    view: &HierarchyView,
    selected: &[raf_core::scene::SceneNodeId],
    renaming: Option<(raf_core::scene::SceneNodeId, &str)>,
    menu_target: Option<(raf_core::scene::SceneNodeId, bool)>,
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
    box_selection: Option<raf_ui::UiRect>,
    can_paste: bool,
    compact_tabs: bool,
    active_tab: &str,
    bookmark_filled: [bool; 3],
) -> UiSurface {
    let tokens = palette.tokens();
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
        "search" => search_tab(palette),
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

    let root = if let Some((target, is_folder)) = menu_target {
        root.with_child(context_menu(
            palette,
            target,
            is_folder,
            menu_label,
            menu_position,
            surface_size,
            can_paste,
        ))
    } else {
        root
    };
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
        .with_child(icon_button(
            palette,
            "hierarchy.header.close",
            UiIconId::Close,
            "hierarchy.panel.toggle",
            "ui.close",
        ))
}

fn tabs(palette: StudioUiPalette, compact_tabs: bool, active_tab: &str) -> UiNode {
    let tokens = palette.tokens();
    let mut row = UiNode::new("hierarchy.tabs", UiNodeKind::Toolbar)
        .with_class("hierarchy-tabs")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 0.0,
            padding: UiSpacing::xy(2.0, 0.0),
            overflow: UiOverflow::Clip,
            ..UiLayout::fixed(0.0, 25.0).with_width_mode(UiSizeMode::Fill)
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
                UiLayout::fixed(32.0, 25.0)
            } else {
                let width = match id {
                    "hierarchy" => 94.0,
                    "assets" => 74.0,
                    "world" => 72.0,
                    "bookmarks" => 108.0,
                    "search" => 84.0,
                    _ => 80.0,
                };
                UiLayout::fixed(width, 25.0).with_text_safe_area(true)
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

fn search_tab(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hierarchy.search-tab", UiNodeKind::Panel)
        .with_class("hierarchy-tab-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::same(10.0),
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
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
) -> UiNode {
    let is_selected = selected.contains(&row.id);
    let class = if is_selected {
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

    if let Some((rename_id, rename_value)) = renaming {
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
            root = root.with_child(small_button(
                "hierarchy.rename.ok",
                "app.ok",
                format!("hierarchy.rename.commit:{}", row.id.0),
            ));
            root = root.with_child(small_button(
                "hierarchy.rename.cancel",
                "app.cancel",
                "hierarchy.rename.cancel".to_string(),
            ));
            let _ = rename_value;
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
            opacity: 0.72,
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
    let title = match label {
        Some(label) => UiNode::new("hierarchy.context-menu.title", UiNodeKind::Label)
            .with_text_value(label.to_string()),
        None => UiNode::new("hierarchy.context-menu.title", UiNodeKind::Label)
            .with_text_key("app.hierarchy"),
    };
    let mut menu = UiNode::new("hierarchy.context-menu", UiNodeKind::Menu)
        .with_class("hierarchy-context-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: CONTEXT_MENU_GAP,
            padding: UiSpacing::same(CONTEXT_MENU_PADDING),
            overflow: UiOverflow::Clip,
            ..UiLayout::absolute(authored_menu_rect).with_z_index(220)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.accent,
            text: tokens.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        })
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "hierarchy.menu.close",
        ))
        .with_accessibility_label_key("app.hierarchy")
        .with_child(
            title
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    overflow: UiOverflow::Clip,
                    ..UiLayout::fixed(0.0, CONTEXT_MENU_TITLE_HEIGHT)
                        .with_width_mode(UiSizeMode::Fill)
                }),
        );
    for (suffix, label, command) in [
        (
            "rename",
            "app.rename",
            format!("hierarchy.rename.begin:{id}"),
        ),
        (
            "duplicate",
            "app.duplicate_menu",
            format!("hierarchy.duplicate:{id}"),
        ),
        ("copy", "app.copy_menu", format!("hierarchy.copy:{id}")),
        (
            "expand",
            "app.expand_menu",
            format!("hierarchy.expand-recursive:{id}"),
        ),
        (
            "collapse",
            "app.collapse_menu",
            format!("hierarchy.collapse-recursive:{id}"),
        ),
        (
            "select-children",
            "app.select_children_menu",
            format!("hierarchy.select-children:{id}"),
        ),
        (
            "folder",
            "app.add_folder",
            format!("hierarchy.create-folder:{id}"),
        ),
        (
            "entity",
            "app.create_entity",
            format!("hierarchy.create-entity:{id}"),
        ),
        ("focus", "app.focus_entity", format!("hierarchy.focus:{id}")),
        (
            "root",
            "app.move_to_root",
            format!("hierarchy.reparent-root:{id}"),
        ),
        (
            "delete",
            "app.delete_menu",
            format!("hierarchy.delete:{id}"),
        ),
    ] {
        menu = menu.with_child(small_button(
            &format!("hierarchy.context-menu.{suffix}"),
            label,
            command,
        ));
    }
    menu = menu.with_child(
        small_button(
            "hierarchy.context-menu.paste",
            "app.paste_menu",
            format!("hierarchy.paste:{id}"),
        )
        .disabled(!can_paste),
    );
    if is_folder {
        menu = menu.with_child(small_button(
            "hierarchy.context-menu.ungroup",
            "app.ungroup",
            format!("hierarchy.ungroup:{id}"),
        ));
    }
    menu.with_child(small_button(
        "hierarchy.context-menu.close",
        "app.cancel",
        "hierarchy.menu.close",
    ))
}

fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}

const CONTEXT_MENU_WIDTH: f32 = 244.0;
const HIERARCHY_ROOT_PADDING: f32 = 4.0;
const CONTEXT_MENU_MARGIN: f32 = 8.0;
const CONTEXT_MENU_PADDING: f32 = 6.0;
const CONTEXT_MENU_GAP: f32 = 2.0;
const CONTEXT_MENU_TITLE_HEIGHT: f32 = 24.0;
const CONTEXT_MENU_BUTTON_HEIGHT: f32 = 27.0;
const CONTEXT_MENU_BASE_BUTTON_COUNT: usize = 13;

pub(crate) fn context_menu_rect(
    position: Option<[f32; 2]>,
    is_folder: bool,
    surface_size: [f32; 2],
) -> raf_ui::UiRect {
    let button_count = CONTEXT_MENU_BASE_BUTTON_COUNT + if is_folder { 1 } else { 0 };
    let height = CONTEXT_MENU_PADDING * 2.0
        + CONTEXT_MENU_TITLE_HEIGHT
        + button_count as f32 * (CONTEXT_MENU_BUTTON_HEIGHT + CONTEXT_MENU_GAP);
    let viewport_width = surface_size[0].max(0.0);
    let viewport_height = surface_size[1].max(0.0);
    let point = position.unwrap_or([CONTEXT_MENU_MARGIN, 130.0]);
    let x = clamp_menu_axis(point[0], viewport_width, CONTEXT_MENU_WIDTH);
    let y = clamp_menu_axis(point[1], viewport_height, height);
    raf_ui::UiRect::new(x, y, CONTEXT_MENU_WIDTH, height)
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

fn small_button(id: &str, text_key: &str, command: impl Into<String>) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hierarchy-menu-button")
        .with_layout(UiLayout {
            align_self: Some(UiAlign::Stretch),
            padding: UiSpacing::xy(8.0, 0.0),
            ..UiLayout::fixed(0.0, 27.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_text_key(text_key)
        .with_accessibility_label_key(text_key)
        .with_text_style(UiTextStyle::button([222, 226, 232, 255]))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
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
        class_rule(
            "hierarchy-tabs",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
        class_rule(
            "hierarchy-toolbar",
            tokens.surface,
            tokens.border,
            tokens.text,
        ),
        class_rule("hierarchy-row", tokens.surface, tokens.border, tokens.text),
        class_rule(
            "hierarchy-row-selected",
            selection_fill,
            tokens.border,
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
        class_rule(
            "hierarchy-row-hidden",
            tokens.surface,
            tokens.border,
            tokens.text_muted,
        ),
        class_rule(
            "hierarchy-row-locked",
            tokens.surface,
            tokens.border,
            tokens.text_muted,
        ),
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
        class_rule(
            "hierarchy-row-action",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text_muted,
        ),
        class_rule(
            "hierarchy-row-action-selected",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            [255, 247, 232, 255],
        ),
        class_rule(
            "hierarchy-expand",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text_muted,
        ),
        class_rule(
            "hierarchy-row-label",
            [0, 0, 0, 0],
            [0, 0, 0, 0],
            tokens.text,
        ),
        class_rule(
            "hierarchy-tab",
            tokens.surface_alt,
            tokens.border,
            tokens.text_muted,
        ),
        class_rule(
            "hierarchy-tab-active",
            tokens.surface_raised,
            tokens.accent,
            tokens.text,
        ),
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
            tokens.accent,
            tokens.text,
        ),
        class_rule(
            "hierarchy-menu-button",
            tokens.surface_alt,
            tokens.border,
            tokens.text,
        ),
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
    ];
    rules.push(
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-row-label".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(2.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
    );
    rules.extend([
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-row-label".to_string()),
            UiStylePatch {
                border: Some(tokens.focus),
                border_width: Some(1.0),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Focused),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-icon-button".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.focus),
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
                border: Some(tokens.focus),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-tab".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.border),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-menu-button".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface_raised),
                border: Some(tokens.focus),
                text: Some(tokens.text),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Hovered),
        UiStyleRule::new(
            UiStyleSelector::Class("hierarchy-menu-button".to_string()),
            UiStylePatch {
                fill: Some(tokens.surface),
                border: Some(tokens.border),
                text: Some(tokens.text_muted),
                opacity: Some(0.48),
                ..UiStylePatch::default()
            },
        )
        .when(UiStyleRuleState::Disabled),
    ]);
    UiStyleSheet { rules }
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
            false,
            false,
            "hierarchy",
            [false; 3],
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
}
