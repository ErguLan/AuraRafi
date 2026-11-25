//! Retained Hierarchy chrome: header, tabs, filter toolbar and summary.

use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiIcon, UiIconId, UiIconSize};
use raf_ui::{
    UiAlign, UiEventBinding, UiEventKind, UiFlow, UiLayout, UiNode, UiNodeKind, UiOverflow,
    UiSizeMode, UiSpacing, UiTextInput, UiTextStyle,
};

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

/// Scene and Lighting are the only live tree modes. Asset browsing, World and
/// Bookmarks live in their own panels instead of placeholder tabs here.
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
    for (id, key, icon, width) in [
        ("hierarchy", "app.scene", UiIconId::Scene, 86.0),
        ("lighting", "lighting.title", UiIconId::Shaded, 96.0),
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
