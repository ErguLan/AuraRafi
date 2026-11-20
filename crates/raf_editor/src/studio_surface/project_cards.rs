//! Hub project cards, showcase previews and the legacy project grid recipes.

use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiCompactMode, UiEventKind, UiFlow, UiImage, UiImageFit,
    UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiResponsiveRule, UiSpacing,
    UiStyle, UiTextStyle,
};

use super::*;

pub(crate) fn hub_showcase_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let mut row = UiNode::new("hub.showcase", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 14.0,
            compact: UiCompactMode::Stack,
            responsive: vec![UiResponsiveRule {
                max_width: 820.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 0.0]),
                padding: None,
                gap: Some(14.0),
                compact: Some(UiCompactMode::Stack),
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, 316.0)
        })
        .with_style(UiStyle::transparent());

    if let Some(featured) = model.featured_project.as_ref() {
        row = row.with_child(hub_showcase_featured(palette, featured));
    } else {
        row = row.with_child(hub_showcase_empty(palette, model));
    }

    let mut side = UiNode::new("hub.showcase-side", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 14.0,
            basis: [0.0, 0.0],
            min_size: [280.0, 0.0],
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent());
    for (index, project) in model.recent_activity.iter().skip(1).take(2).enumerate() {
        side = side.with_child(hub_showcase_small(palette, project, index));
    }
    row.with_child(side)
}

pub(crate) fn hub_showcase_featured(palette: StudioUiPalette, project: &HubSurfaceProject) -> UiNode {
    let tokens = hub_tokens(palette);
    let preview_key = match project.project_type {
        ProjectType::Game => "editor.hub.preview-game",
        ProjectType::Electronics => "editor.hub.preview-electronics",
    };
    UiNode::new("hub.showcase-featured", UiNodeKind::Panel)
        .with_class("hub-featured")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            min_size: [360.0, 0.0],
            ..UiLayout::fill(UiFlow::Column)
        })
        .focusable()
        .with_tooltip_value(project.path.display().to_string())
        .with_event(project_action(project, "open"))
        .with_event(project_context_action(project))
        .with_event(project_hover_event(project, UiEventKind::HoverEnter))
        .with_event(project_hover_event(project, UiEventKind::HoverLeave))
        .with_child(
            UiNode::image(
                "hub.showcase-featured-image",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(preview_key),
                    fit: UiImageFit::Cover,
                    tint: None,
                },
            )
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [0.0, 158.0],
                ..UiLayout::fill(UiFlow::None)
            }),
        )
        .with_child(
            UiNode::new("hub.showcase-featured-copy", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 3.0,
                    padding: UiSpacing::xy(14.0, 10.0),
                    ..UiLayout::fixed(0.0, 78.0)
                })
                .with_style(UiStyle::transparent())
                .with_child(
                    UiNode::new("hub.showcase-featured-kind", UiNodeKind::Label)
                        .with_text_key("hub.featured.kind")
                        .with_text_style(UiTextStyle::body(tokens.accent))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                )
                .with_child(
                    UiNode::new("hub.showcase-featured-name", UiNodeKind::Label)
                        .with_text_key("hub.featured.name")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 24.0)),
                )
                .with_child(
                    UiNode::new("hub.showcase-featured-meta", UiNodeKind::Label)
                        .with_text_key("hub.featured.meta")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                ),
        )
}

pub(crate) fn hub_showcase_small(
    palette: StudioUiPalette,
    project: &HubSurfaceProject,
    index: usize,
) -> UiNode {
    let tokens = hub_tokens(palette);
    let preview_key = match project.project_type {
        ProjectType::Game => "editor.hub.preview-game",
        ProjectType::Electronics => "editor.hub.preview-electronics",
    };
    let id = format!("hub.showcase-small.{index}");
    UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_class("hub-card")
        .with_class("hub-showcase-small")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 14.0,
            padding: UiSpacing::same(10.0),
            ..UiLayout::fill(UiFlow::Row)
        })
        .focusable()
        .with_tooltip_value(project.path.display().to_string())
        .with_event(project_action(project, "open"))
        .with_event(project_context_action(project))
        .with_event(project_hover_event(project, UiEventKind::HoverEnter))
        .with_event(project_hover_event(project, UiEventKind::HoverLeave))
        .with_child(
            UiNode::image(
                format!("{id}.image"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(preview_key),
                    fit: UiImageFit::Cover,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(340.0, 140.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.copy"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    justify_content: UiJustify::Center,
                    gap: 4.0,
                    grow: 1.0,
                    ..UiLayout::default()
                })
                .with_style(UiStyle::transparent())
                .with_child(
                    UiNode::new(format!("{id}.name"), UiNodeKind::Label)
                        .with_text_key(format!("hub.showcase.small.{index}.name"))
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 22.0)),
                )
                .with_child(
                    UiNode::new(format!("{id}.kind"), UiNodeKind::Label)
                        .with_text_key(format!("hub.showcase.small.{index}.kind"))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                )
                .with_child(
                    UiNode::new(format!("{id}.meta"), UiNodeKind::Label)
                        .with_text_key(format!("hub.showcase.small.{index}.meta"))
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                ),
        )
}

pub(crate) fn hub_showcase_empty(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = hub_tokens(palette);
    let (title_key, subtitle_key) = hub_empty_copy(model);
    let mut empty = UiNode::new("hub.showcase-empty", UiNodeKind::Panel)
        .with_class("hub-featured")
        .with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("hub.showcase-empty-title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 28.0)),
        );
    if let Some(subtitle_key) = subtitle_key {
        empty = empty.with_child(
            UiNode::new("hub.showcase-empty-subtitle", UiNodeKind::Label)
                .with_text_key(subtitle_key)
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 24.0)),
        );
    }
    empty
}

#[allow(dead_code)]
pub(crate) fn hub_featured_project(palette: StudioUiPalette, project: &HubSurfaceProject) -> UiNode {
    let tokens = palette.tokens();
    let preview_key = match project.project_type {
        ProjectType::Game => "editor.hub.preview-game",
        ProjectType::Electronics => "editor.hub.preview-electronics",
    };
    UiNode::new("hub.featured", UiNodeKind::Panel)
        .with_class("hub-featured")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            gap: 18.0,
            padding: UiSpacing::same(18.0),
            responsive: vec![UiResponsiveRule {
                max_width: 640.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 246.0]),
                padding: Some(UiSpacing::same(12.0)),
                gap: Some(10.0),
                compact: None,
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, 206.0)
        })
        .focusable()
        .with_event(project_action(project, "open"))
        .with_event(project_context_action(project))
        .with_child(
            UiNode::new("hub.featured-preview", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(294.0, 170.0))
                .with_style(project_thumbnail_style(palette, project.project_type))
                .with_child(
                    UiNode::image(
                        "hub.featured-preview-image",
                        UiImage {
                            options: Default::default(),
                            source: UiImageSource::new(preview_key),
                            fit: UiImageFit::Cover,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fill(UiFlow::None)),
                ),
        )
        .with_child(
            UiNode::new("hub.featured-copy", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    justify_content: UiJustify::Center,
                    gap: 8.0,
                    ..UiLayout::default()
                })
                .with_style(UiStyle::transparent())
                .with_child(
                    UiNode::new("hub.featured-kind", UiNodeKind::Label)
                        .with_text_key(match project.project_type {
                            ProjectType::Game => "app.hub_type_game_label",
                            ProjectType::Electronics => "app.hub_type_electronics_label",
                        })
                        .with_text_style(UiTextStyle::button(tokens.accent))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                )
                .with_child(
                    UiNode::new("hub.featured-name", UiNodeKind::Label)
                        .with_text_key("hub.featured.name")
                        .with_text_style(UiTextStyle {
                            size_px: 24.0,
                            line_height_px: 30.0,
                            ..UiTextStyle::panel_title(tokens.text)
                        })
                        .with_layout(UiLayout::fixed(0.0, 30.0)),
                )
                .with_child(
                    UiNode::new("hub.featured-meta", UiNodeKind::Label)
                        .with_text_key("hub.featured.meta")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                ),
        )
        .with_child(
            UiNode::new("hub.featured-actions", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    justify_content: UiJustify::End,
                    align_items: UiAlign::End,
                    ..UiLayout::fixed(140.0, 170.0)
                })
                .with_style(UiStyle::transparent())
                .with_child(hub_featured_continue_button(project)),
        )
}

#[allow(dead_code)]
pub(crate) fn hub_featured_continue_button(project: &HubSurfaceProject) -> UiNode {
    UiNode::new("hub.featured-continue", UiNodeKind::Button)
        .with_class("hub-primary-button")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 8.0,
            padding: UiSpacing::xy(14.0, 0.0),
            ..UiLayout::fixed(140.0, 42.0)
        })
        .focusable()
        .with_accessibility_label_key("app.hub_continue")
        .with_event(project_action(project, "open"))
        .with_child(
            UiNode::new("hub.featured-continue-label", UiNodeKind::Label)
                .with_text_key("app.hub_continue")
                .with_text_style(UiTextStyle::button([18, 18, 20, 255]))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 20.0)
                }),
        )
        .with_child(
            UiNode::image(
                "hub.featured-continue-arrow",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new("editor.hub.icon-arrow"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
}

#[allow(dead_code)]
pub(crate) fn hub_filter_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    UiNode::new("hub.filters", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Wrap,
            gap: 8.0,
            align_items: UiAlign::Center,
            responsive: vec![UiResponsiveRule {
                max_width: 560.0,
                flow: Some(UiFlow::RowWrap),
                basis: Some([0.0, 78.0]),
                padding: None,
                gap: Some(6.0),
                compact: None,
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, 36.0)
        })
        .with_style(UiStyle::transparent())
        .with_child(hub_command_button(
            "hub.filter-all",
            "app.all",
            "hub.filter-all",
            56.0,
            32.0,
            filter_class(HubSurfaceFilter::All, model.filter),
        ))
        .with_child(hub_command_button(
            "hub.filter-games",
            "app.hub_game_kind",
            "hub.filter-games",
            78.0,
            32.0,
            filter_class(HubSurfaceFilter::Game, model.filter),
        ))
        .with_child(hub_command_button(
            "hub.filter-electronics",
            "app.hub_electronics_kind",
            "hub.filter-electronics",
            116.0,
            32.0,
            filter_class(HubSurfaceFilter::Electronics, model.filter),
        ))
        .with_child(
            UiNode::new("hub.results", UiNodeKind::Label)
                .with_text_key("hub.results")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fixed(150.0, 28.0)),
        )
}

#[allow(dead_code)]
pub(crate) fn filter_class(target: HubSurfaceFilter, active: HubSurfaceFilter) -> &'static str {
    if target == active {
        "hub-filter-active"
    } else {
        "hub-button"
    }
}

#[allow(dead_code)]
pub(crate) fn hub_grid_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let mut grid = UiNode::grid("hub.projects-grid")
        .with_layout(UiLayout {
            gap: 16.0,
            grid: raf_render::api_graphic_basic::ui_surface::UiGridLayout {
                columns: 0,
                min_column_width: 230.0,
                row_height: 220.0,
            },
            responsive: vec![UiResponsiveRule {
                max_width: 560.0,
                flow: None,
                basis: None,
                padding: None,
                gap: Some(10.0),
                compact: None,
                grid_columns: Some(1),
            }],
            ..UiLayout::fill(UiFlow::Grid)
        })
        .with_style(palette.canvas_style());
    if model.projects.is_empty() {
        grid.layout.grid.columns = 1;
        grid.layout.grid.row_height = 34.0;
        return grid
            .with_child(
                UiNode::new("hub.empty-title", UiNodeKind::Label)
                    .with_text_key("app.hub_empty_title")
                    .with_text_style(UiTextStyle::panel_title(palette.tokens().text))
                    .with_layout(UiLayout::fixed(0.0, 28.0)),
            )
            .with_child(
                UiNode::new("hub.empty-subtitle", UiNodeKind::Label)
                    .with_text_key("app.hub_empty_subtitle")
                    .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                    .with_layout(UiLayout::fixed(0.0, 28.0)),
            )
            .with_child(hub_command_button(
                "hub.empty-new-game",
                "app.hub_new_game_short",
                "hub.new-game",
                180.0,
                36.0,
                "hub-primary-button",
            ));
    }

    for (index, project) in model.projects.iter().enumerate() {
        let menu_visible = model.hovered_project_path.as_deref() == Some(project.path.as_path());
        grid = grid.with_child(hub_project_card(palette, project, index, menu_visible));
    }
    grid
}

#[allow(dead_code)]
pub(crate) fn hub_project_card(
    palette: StudioUiPalette,
    project: &HubSurfaceProject,
    index: usize,
    menu_visible: bool,
) -> UiNode {
    let tokens = hub_tokens(palette);
    let card_id = format!("hub.project.{index}");
    let preview_key = match project.project_type {
        ProjectType::Game => "editor.hub.preview-game",
        ProjectType::Electronics => "editor.hub.preview-electronics",
    };
    UiNode::new(card_id.clone(), UiNodeKind::Panel)
        .with_class("hub-card")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(12.0),
            gap: 6.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .focusable()
        .with_event(project_action(project, "open"))
        .with_event(project_context_action(project))
        .with_child({
            let preview = UiNode::new(format!("{card_id}.preview"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::None,
                    ..UiLayout::fixed(0.0, 118.0)
                })
                .with_style(project_thumbnail_style(palette, project.project_type))
                .with_child(
                    UiNode::image(
                        format!("{card_id}.preview-image"),
                        UiImage {
                            options: Default::default(),
                            source: UiImageSource::new(preview_key),
                            fit: UiImageFit::Cover,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fill(UiFlow::None)),
                );

            if menu_visible {
                preview.with_child(
                    UiNode::new(format!("{card_id}.preview-actions"), UiNodeKind::Overlay)
                        .with_layout(
                            UiLayout {
                                flow: UiFlow::Row,
                                justify_content: UiJustify::End,
                                align_items: UiAlign::Start,
                                padding: UiSpacing::same(8.0),
                                ..UiLayout::fill(UiFlow::None)
                            }
                            .with_z_index(2),
                        )
                        .with_style(UiStyle::transparent())
                        .with_child(hub_project_menu_button(project, &card_id)),
                )
            } else {
                preview
            }
        })
        .with_child(
            UiNode::new(format!("{card_id}.title-row"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 4.0,
                    ..UiLayout::fixed(0.0, 20.0)
                })
                .with_style(UiStyle::transparent())
                .with_child(
                    UiNode::new(format!("{card_id}.name"), UiNodeKind::Label)
                        .with_text_key(format!("hub.project.{index}.name"))
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fixed(0.0, 20.0)
                        }),
                ),
        )
        .with_child(
            UiNode::new(format!("{card_id}.kind"), UiNodeKind::Label)
                .with_text_key(format!("hub.project.{index}.kind"))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("{card_id}.last-opened"), UiNodeKind::Label)
                .with_text_key(format!("hub.project.{index}.last-opened"))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 18.0)),
        )
}

pub(crate) fn project_kind_icon_key(project: &HubSurfaceProject) -> &'static str {
    match project.project_type {
        ProjectType::Game => "editor.hub.kind-game",
        ProjectType::Electronics => "editor.hub.kind-electronics",
    }
}

#[allow(dead_code)]
pub(crate) fn hub_project_menu_button(project: &HubSurfaceProject, card_id: &str) -> UiNode {
    UiNode::new(format!("{card_id}.menu"), UiNodeKind::Panel)
        .with_class("hub-project-menu")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            ..UiLayout::fixed(22.0, 20.0)
        })
        .focusable()
        .with_event(project_menu_action(project))
        .with_child(
            UiNode::image(
                format!("{card_id}.menu-icon"),
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
pub(crate) fn project_thumbnail_style(palette: StudioUiPalette, project_type: ProjectType) -> UiStyle {
    let mut style = palette.subtle_panel_style();
    style.radius = 5.0;
    style.fill = match palette {
        StudioUiPalette::IndustrialDark => [18, 23, 29, 255],
        StudioUiPalette::PaperLight => [239, 241, 244, 255],
    };
    style.border = match project_type {
        ProjectType::Game => palette.tokens().border,
        ProjectType::Electronics => palette.tokens().accent,
    };
    style
}
