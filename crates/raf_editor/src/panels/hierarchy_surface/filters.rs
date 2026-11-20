//! Retained Hierarchy chrome: header, tabs, filters, summaries and tab bodies.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiIcon, UiIconId, UiIconSize,
};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind, UiOverflow,
    UiSizeMode, UiSpacing, UiTextInput, UiTextStyle,
};

use crate::project_catalog::ProjectCatalogResults;

use super::icon_button;
use super::rows::compact_body_style;

pub(crate) fn header(palette: StudioUiPalette) -> UiNode {
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

pub(crate) fn tabs(palette: StudioUiPalette, compact_tabs: bool, active_tab: &str) -> UiNode {
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
        ("lighting", "lighting.title", UiIconId::Shaded),
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

pub(super) fn tab_context_toolbar(palette: StudioUiPalette, active_tab: &str) -> UiNode {
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

pub(super) fn tab_summary(palette: StudioUiPalette, active_tab: &str) -> UiNode {
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

pub(super) fn assets_tab(palette: StudioUiPalette) -> UiNode {
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

pub(super) fn world_tab(palette: StudioUiPalette) -> UiNode {
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

pub(super) fn bookmarks_tab(palette: StudioUiPalette, filled: [bool; 3]) -> UiNode {
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

pub(super) fn search_tab(palette: StudioUiPalette, results: Option<&ProjectCatalogResults>) -> UiNode {
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

pub(super) fn toolbar(palette: StudioUiPalette) -> UiNode {
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

pub(super) fn summary(palette: StudioUiPalette, row_count: usize, selected_count: usize) -> UiNode {
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
