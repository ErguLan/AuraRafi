//! Recent projects, empty states and the legacy recent-activity panel.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiImage, UiImageFit,
    UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode,
    UiSpacing, UiStyle, UiTextStyle,
};

use super::*;

/// Number of project rows visible before the list starts scrolling.
pub(crate) const HUB_LIST_VISIBLE_ROWS: usize = 7;
/// Vertical footprint of one project row in the Hub list.
pub(crate) const HUB_LIST_ROW_HEIGHT: f32 = 43.0;

/// Copy for the showpiece and the list when nothing is visible. The Hub must
/// not claim "no projects" while it is still scanning, nor when a search or
/// filter simply matched nothing.
pub(crate) fn hub_empty_copy(model: &HubSurfaceModel) -> (&'static str, Option<&'static str>) {
    if model.discovery_pending && model.projects.is_empty() {
        ("app.hub_loading_projects", None)
    } else if hub_is_filtering(model) {
        (
            "app.hub_no_results_title",
            Some("app.hub_no_results_subtitle"),
        )
    } else {
        ("app.hub_empty_title", Some("app.hub_empty_subtitle"))
    }
}

pub(crate) fn hub_recent_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = hub_tokens(palette);
    let mut header = UiNode::new("hub.recent-header", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 12.0,
            ..UiLayout::fixed(0.0, 34.0)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new("hub.recent-title", UiNodeKind::Label)
                .with_text_key("app.hub_nav_projects")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 22.0)
                }),
        );
    // While the first scan is still running the count would lie ("0 projects").
    if !(model.discovery_pending && model.projects.is_empty()) {
        header = header.with_child(
            UiNode::new("hub.recent-count", UiNodeKind::Label)
                .with_text_key("hub.results")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout {
                    width_mode: UiSizeMode::FitContent,
                    height_mode: UiSizeMode::Fixed,
                    basis: [0.0, 18.0],
                    ..UiLayout::default()
                }),
        );
    }
    let panel = UiNode::new("hub.recent", UiNodeKind::Panel)
        .with_class("hub-recent")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(16.0),
            gap: 0.0,
            ..UiLayout::fixed(0.0, hub_recent_height(model))
        })
        .with_child(header);

    if model.projects.is_empty() {
        let (title_key, subtitle_key) = hub_empty_copy(model);
        return panel.with_child(
            UiNode::new("hub.recent-empty", UiNodeKind::Label)
                .with_text_key(subtitle_key.unwrap_or(title_key))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 42.0)),
        );
    }

    let visible_rows = model.projects.len().min(HUB_LIST_VISIBLE_ROWS);
    let mut rows = UiNode::scroll_view("hub.recent-scroll", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 0.0,
            overflow: UiOverflow::ScrollY,
            padding: UiSpacing {
                right: 20.0,
                ..UiSpacing::ZERO
            },
            ..UiLayout::fixed(0.0, visible_rows as f32 * HUB_LIST_ROW_HEIGHT)
        })
        .with_style(UiStyle::transparent());

    for (index, project) in model.projects.iter().enumerate() {
        let row_id = format!("hub.recent.{index}");
        rows = rows.with_child(
            UiNode::new(row_id.clone(), UiNodeKind::Panel)
                .with_class("hub-recent-row")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 12.0,
                    padding: UiSpacing::xy(4.0, 0.0),
                    ..UiLayout::fixed(0.0, HUB_LIST_ROW_HEIGHT)
                })
                .with_style(UiStyle::transparent())
                .with_tooltip_value(project.path.display().to_string())
                .focusable()
                .with_event(project_action(project, "open"))
                .with_event(project_context_action(project))
                .with_event(project_hover_event(project, UiEventKind::HoverEnter))
                .with_event(project_hover_event(project, UiEventKind::HoverLeave))
                .with_child(
                    UiNode::image(
                        format!("{row_id}.icon"),
                        UiImage {
                            options: Default::default(),
                            source: UiImageSource::new(project_kind_icon_key(project)),
                            fit: UiImageFit::Contain,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(22.0, 22.0)),
                )
                .with_child(
                    UiNode::new(format!("{row_id}.name"), UiNodeKind::Label)
                        .with_text_key(format!("hub.recent.{index}.name"))
                        .with_text_style(UiTextStyle::button(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fixed(0.0, 22.0)
                        }),
                )
                .with_child(
                    UiNode::new(format!("{row_id}.kind"), UiNodeKind::Label)
                        .with_class("hub-recent-kind")
                        .with_text_key(format!("hub.recent.{index}.kind"))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(104.0, 24.0)),
                )
                .with_child(
                    UiNode::new(format!("{row_id}.meta"), UiNodeKind::Label)
                        .with_text_key(format!("hub.recent.{index}.meta"))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(160.0, 22.0)),
                )
                .with_child(hub_recent_menu_button(project, &row_id)),
        );
    }
    panel.with_child(rows)
}

pub(crate) fn hub_recent_height(model: &HubSurfaceModel) -> f32 {
    let visible_rows = model.projects.len().min(HUB_LIST_VISIBLE_ROWS);
    (66.0 + visible_rows as f32 * HUB_LIST_ROW_HEIGHT).max(326.0)
}

pub(crate) fn hub_recent_menu_button(project: &HubSurfaceProject, row_id: &str) -> UiNode {
    UiNode::new(format!("{row_id}.menu"), UiNodeKind::Button)
        .with_class("hub-recent-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            z_index: 10,
            ..UiLayout::fixed(28.0, 28.0)
        })
        .focusable()
        .with_event(project_menu_action(project))
        .with_child(
            UiNode::image(
                format!("{row_id}.menu-icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new("editor.hub.more"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(14.0, 14.0)),
        )
}

#[allow(dead_code)]
pub(crate) fn hub_activity_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = palette.tokens();
    let item_height = 54.0;
    let header_height = 30.0;
    let activity_height = if model.recent_activity.is_empty() {
        header_height + 22.0 + 8.0 + 34.0 + 10.0
    } else {
        header_height
            + 12.0
            + model.recent_activity.len() as f32 * item_height
            + (model.recent_activity.len() as f32 - 1.0).max(0.0) * 6.0
    };
    let mut panel = UiNode::new("hub.activity", UiNodeKind::Panel)
        .with_class("hub-side-panel")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(16.0),
            gap: 8.0,
            ..UiLayout::fixed(0.0, activity_height)
        })
        .with_child(
            UiNode::new("hub.activity-header", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, header_height)
                })
                .with_style(UiStyle::transparent())
                .with_child(
                    UiNode::image(
                        "hub.activity-header-icon",
                        UiImage {
                            options: Default::default(),
                            source: UiImageSource::new("editor.hub.icon-pulse"),
                            fit: UiImageFit::Contain,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(16.0, 16.0)),
                )
                .with_child(
                    UiNode::new("hub.activity-title", UiNodeKind::Label)
                        .with_text_key("app.hub_recent_activity")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fixed(0.0, 22.0)
                        }),
                )
                .with_child(hub_view_all_link(
                    "hub.activity-view-all",
                    "app.hub_view_all",
                )),
        );

    if model.recent_activity.is_empty() {
        return panel.with_child(
            UiNode::new("hub.activity-empty", UiNodeKind::Label)
                .with_text_key("app.hub_empty_subtitle")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 34.0)),
        );
    }

    for (index, project) in model.recent_activity.iter().enumerate() {
        panel = panel.with_child(
            UiNode::new(format!("hub.activity.{index}"), UiNodeKind::Panel)
                .with_class("hub-activity-item")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 10.0,
                    padding: UiSpacing::xy(6.0, 0.0),
                    ..UiLayout::fixed(0.0, item_height)
                })
                .with_style(UiStyle::transparent())
                .focusable()
                .with_event(project_action(project, "open"))
                .with_event(project_context_action(project))
                .with_child(
                    UiNode::image(
                        format!("hub.activity.{index}.icon"),
                        UiImage {
                            options: Default::default(),
                            source: UiImageSource::new(project_kind_icon_key(project)),
                            fit: UiImageFit::Contain,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(22.0, 22.0)),
                )
                .with_child(
                    UiNode::new(format!("hub.activity.{index}.copy"), UiNodeKind::Panel)
                        .with_layout(UiLayout {
                            flow: UiFlow::Column,
                            grow: 1.0,
                            gap: 2.0,
                            ..UiLayout::default()
                        })
                        .with_style(UiStyle::transparent())
                        .with_child(
                            UiNode::new(format!("hub.activity.{index}.name"), UiNodeKind::Label)
                                .with_text_key(format!("hub.activity.{index}.name"))
                                .with_text_style(UiTextStyle::button(tokens.text))
                                .with_layout(UiLayout::fixed(0.0, 20.0)),
                        )
                        .with_child(
                            UiNode::new(format!("hub.activity.{index}.meta"), UiNodeKind::Label)
                                .with_text_key(format!("hub.activity.{index}.meta"))
                                .with_text_style(UiTextStyle::body(tokens.text_muted))
                                .with_layout(UiLayout::fixed(0.0, 18.0)),
                        ),
                ),
        );
    }
    panel
}

#[allow(dead_code)]
pub(crate) fn hub_view_all_link(id: &str, label_key: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hub-link-button")
        .with_layout(UiLayout::fixed(64.0, 22.0))
        .focusable()
        .with_accessibility_label_key(label_key)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button([224, 116, 24, 255]))
        .with_event(UiEventBinding::command(UiEventKind::Click, "hub.view-all"))
}
