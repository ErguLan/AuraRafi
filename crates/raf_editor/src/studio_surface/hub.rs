//! Hub composition, top bar, create form and primary chrome actions.

use raf_core::project::EditorViewMode;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow, UiImage,
    UiImageFit, UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow,
    UiResponsiveRule, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiSurfaceMaterial, UiTextInput,
    UiTextRole, UiTextStyle,
};
use raf_ui::{UiFontWeight, UiIcon, UiIconId, UiIconSize};

use super::*;

pub(crate) fn hub_root_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let visible_projects = hub_visible_projects(model);
    let mut visible_model = model.clone();
    visible_model.projects = visible_projects.clone();
    visible_model.recent_activity = hub_visible_recent(model);
    visible_model.featured_project = hub_visible_featured(model, &visible_projects);
    let mut root = UiNode::new("hub.root", UiNodeKind::Root)
        .with_layout(UiLayout::fill(UiFlow::Column))
        .with_style(hub_root_style(palette))
        .with_child(hub_topbar_surface(palette, &visible_model))
        .with_child(hub_content_columns_surface(palette, &visible_model));

    if let (Some(project), Some(position)) =
        (model.context_project.as_ref(), model.context_menu_position)
    {
        let position = clamp_context_menu_position(position, model.window_size);
        root = root.with_child(hub_context_menu_surface(
            palette,
            project,
            position,
            model.context_confirm_forget,
        ));
    }

    if let Some(position) = model
        .get_in_menu_position
        .filter(|_| model.get_in_menu_open)
    {
        let position = clamp_menu_position(position, model.window_size, HUB_GET_IN_MENU_SIZE);
        root = root.with_child(hub_get_in_menu_surface(palette, position));
    }

    if let Some(adopt) = model.pending_adopt.as_ref() {
        root = root.with_child(hub_adopt_overlay(palette, model, adopt));
    }

    if let Some(error) = model.pending_open_error.as_ref() {
        root = root.with_child(hub_open_error_overlay(palette, model, error));
    }

    if model.opening_project.is_some() {
        root = root.with_child(hub_project_opening(palette, model));
    }
    root
}

pub(crate) fn hub_topbar_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new("hub.topbar", UiNodeKind::Toolbar)
        .with_class("hub-topbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(24.0, 0.0),
            gap: 26.0,
            responsive: vec![UiResponsiveRule {
                max_width: 760.0,
                flow: Some(UiFlow::RowWrap),
                basis: Some([0.0, 96.0]),
                padding: Some(UiSpacing::xy(14.0, 8.0)),
                gap: Some(10.0),
                compact: Some(UiCompactMode::Wrap),
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, 48.0)
        })
        .with_child(
            UiNode::new("hub.brand-group", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 10.0,
                    ..UiLayout::fixed(112.0, 48.0)
                })
                .with_style(UiStyle::transparent())
                .with_child(
                    UiNode::image(
                        "hub.brand-logo",
                        UiImage {
                            options: Default::default(),
                            source: UiImageSource::new("editor.hub.brand"),
                            fit: UiImageFit::Contain,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(30.0, 30.0)),
                )
                .with_child(
                    UiNode::new("hub.brand-name", UiNodeKind::Label)
                        .with_text_key("hub.brand")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 24.0)),
                ),
        )
        .with_child(hub_top_nav_button(
            "hub.projects",
            "app.hub_nav_projects",
            "hub.projects",
            true,
            palette,
        ))
        .with_child(hub_top_nav_button(
            "hub.agents",
            "agent_workspace.title",
            "hub.agents",
            false,
            palette,
        ))
        .with_child(hub_top_nav_button(
            "hub.settings",
            "app.settings_menu",
            "hub.settings",
            false,
            palette,
        ))
        .with_child(
            UiNode::text_input(
                "hub.search",
                UiTextInput {
                    value_key: "hub.search".to_string(),
                    placeholder_key: Some("app.hub_search_hint".to_string()),
                    submit_command: Some("hub.search.submit".to_string()),
                    ..UiTextInput::new("hub.search")
                },
            )
            .with_class("hub-input")
            .with_layout(UiLayout {
                min_size: [220.0, 34.0],
                max_size: [420.0, 34.0],
                ..UiLayout::fixed(300.0, 34.0)
            })
            .with_accessibility_label_key("app.hub_search_hint"),
        )
        .with_child(
            UiNode::new("hub.window-drag", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::default()
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::DragStart,
                    "window.drag",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::DoubleClick,
                    "window.maximize",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::ContextMenu,
                    "window.system-menu",
                )),
        )
        .with_child(
            UiNode::new("hub.new", UiNodeKind::Button)
                .with_class(if model.get_in_menu_open {
                    "hub-new-button hub-new-active"
                } else {
                    "hub-new-button"
                })
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    justify_content: UiJustify::Center,
                    gap: 6.0,
                    padding: UiSpacing::xy(12.0, 0.0),
                    ..UiLayout::fixed(104.0, 34.0)
                })
                .focusable()
                .with_tooltip_key("app.hub_get_in")
                .with_accessibility_label_key("app.hub_get_in")
                .with_event(UiEventBinding::command(UiEventKind::Click, "hub.new"))
                .with_child(
                    UiNode::new("hub.new.label", UiNodeKind::Label)
                        .with_text_key("app.hub_get_in")
                        .with_text_style(UiTextStyle::button(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 20.0)),
                )
                .with_child(
                    UiNode::new("hub.new.chevron", UiNodeKind::Label)
                        .with_icon(
                            UiIcon::new(UiIconId::ChevronDown)
                                .with_size(UiIconSize::Small)
                                .with_tint(tokens.text_muted),
                        )
                        .with_layout(UiLayout::fixed(14.0, 14.0)),
                ),
        )
        .with_child(hub_window_button(
            "hub.minimize",
            "window.minimize",
            "editor.hub.minimize",
        ))
        .with_child(hub_window_button(
            "hub.maximize",
            "window.maximize",
            "editor.hub.maximize",
        ))
        .with_child(hub_window_button(
            "hub.close",
            "window.close",
            "editor.hub.close",
        ))
}

pub(crate) fn hub_header_surface(palette: StudioUiPalette) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new("hub.header", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 62.0)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new("hub.title", UiNodeKind::Label)
                .with_text_key("app.hub_start_building")
                .with_text_style(UiTextStyle {
                    size_px: 32.0,
                    line_height_px: 38.0,
                    weight: UiFontWeight::Regular,
                    ..UiTextStyle::panel_title(tokens.text)
                })
                .with_layout(UiLayout::fixed(0.0, 38.0)),
        )
        .with_child(
            UiNode::new("hub.subtitle", UiNodeKind::Label)
                .with_text_key("app.hub_start_building_detail")
                .with_text_style(UiTextStyle {
                    size_px: 14.0,
                    line_height_px: 18.0,
                    ..UiTextStyle::body(tokens.text_muted)
                })
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        )
}

pub(crate) fn hub_content_columns_surface(
    palette: StudioUiPalette,
    model: &HubSurfaceModel,
) -> UiNode {
    UiNode::new("hub.content-columns", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(hub_body_style(palette))
        .with_child(
            UiNode::scroll_view("hub.content", UiScrollAxis::Vertical)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    align_items: UiAlign::Center,
                    grow: 1.0,
                    min_size: [320.0, 0.0],
                    padding: UiSpacing::xy(28.0, 64.0),
                    overflow: UiOverflow::ScrollY,
                    responsive: vec![UiResponsiveRule {
                        max_width: 880.0,
                        flow: None,
                        basis: None,
                        padding: Some(UiSpacing::xy(18.0, 24.0)),
                        gap: Some(0.0),
                        compact: None,
                        grid_columns: None,
                    }],
                    ..UiLayout::fill(UiFlow::Column)
                })
                .with_style(hub_body_style(palette))
                .with_child(hub_workspace_surface(palette, model)),
        )
}

pub(crate) fn hub_workspace_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let create_height = hub_create_height(model);
    let content_height = 62.0 + create_height + 316.0 + hub_recent_height(model) + 60.0;
    let main_column = UiNode::new("hub.main-column", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [1280.0, content_height],
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            min_size: [320.0, 0.0],
            max_size: [1280.0, 0.0],
            gap: 20.0,
            align_items: UiAlign::Stretch,
            ..UiLayout::default()
        })
        .with_style(UiStyle::transparent())
        .with_child(hub_header_surface(palette))
        .with_child(hub_create_surface(palette, model))
        .with_child(hub_showcase_surface(palette, model))
        .with_child(hub_recent_surface(palette, model));

    main_column
}

pub(crate) fn hub_create_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = hub_tokens(palette);
    let mut panel = UiNode::new("hub.create-form", UiNodeKind::Panel)
        .with_class("hub-create-form")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            ..UiLayout::fixed(0.0, hub_create_height(model))
        })
        .with_child(
            UiNode::new("hub.create-row", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 12.0,
                    ..UiLayout::fixed(0.0, HUB_CREATE_ROW_HEIGHT)
                })
                .with_style(UiStyle::transparent())
                .with_child(hub_create_name_input(
                    palette,
                    model.create_name_focused,
                    model.create_name_hovered,
                ))
                .with_child(hub_create_type_picker(palette, model))
                .with_child(hub_create_location_input(
                    palette,
                    &model.create_path,
                    model.create_path_hovered,
                ))
                .with_child(hub_agent_toggle(palette, model.create_with_agent))
                .with_child(hub_create_submit(palette, model.create_path_is_project)),
        );
    if !model.create_path_is_project {
        panel = panel.with_child(hub_create_advanced_toggle(palette, model));
        if model.create_advanced_open {
            panel = panel.with_child(hub_create_advanced_panel(palette, model));
        }
    }

    if model.create_error.is_some() {
        panel = panel.with_child(
            UiNode::new("hub.create-error", UiNodeKind::Label)
                .with_class("hub-create-error")
                .with_text_key("hub.create.error")
                .with_text_style(UiTextStyle::body(tokens.danger))
                .with_layout(UiLayout::fixed(0.0, HUB_CREATE_STATUS_HEIGHT)),
        );
    } else if model.create_path_is_project {
        panel = panel.with_child(
            UiNode::new("hub.create-open-hint", UiNodeKind::Label)
                .with_text_key("app.hub_open_existing")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, HUB_CREATE_STATUS_HEIGHT)),
        );
    }
    panel
}

fn hub_create_advanced_toggle(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = palette.tokens();
    let open = model.create_advanced_open;
    UiNode::new("hub.create.advanced-toggle", UiNodeKind::Button)
        .with_class("hub-create-advanced-toggle")
        .with_layout(UiLayout::fixed(168.0, HUB_ADVANCED_TOGGLE_HEIGHT))
        .with_icon(
            UiIcon::new(if open {
                UiIconId::ChevronDown
            } else {
                UiIconId::ChevronRight
            })
            .with_size(UiIconSize::Small)
            .with_tint(tokens.accent),
        )
        .with_text_key("hub.create.advanced")
        .with_text_style(UiTextStyle {
            inherit_color: true,
            ..UiTextStyle::button(tokens.accent)
        })
        .with_accessibility_label_key("hub.create.advanced")
        .with_accessibility_role(raf_ui::UiAccessibilityRole::Button)
        .with_accessibility_expanded(open)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "hub.create.advanced-toggle",
        ))
}

fn hub_create_advanced_panel(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = palette.tokens();
    let mut panel = UiNode::new("hub.create.advanced", UiNodeKind::Panel)
        .with_class("hub-create-advanced")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fixed(
                0.0,
                if model.create_project_type == ProjectType::Game {
                    HUB_ADVANCED_VIEW_HEIGHT
                } else {
                    HUB_ADVANCED_NOTE_HEIGHT
                },
            )
            .with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle::transparent());
    if model.create_project_type != ProjectType::Game {
        return panel.with_child(
            UiNode::new("hub.create.advanced.game-only", UiNodeKind::Label)
                .with_text_key("hub.create.advanced.game_only")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
        );
    }
    let mut row = UiNode::new("hub.create.advanced.view-row", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new("hub.create.advanced.view-label", UiNodeKind::Label)
                .with_text_key("hub.create.advanced.initial_view")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    let mut picker = UiNode::new("hub.create.advanced.view-picker", UiNodeKind::Panel)
        .with_class("hub-create-view-picker")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            padding: UiSpacing::xy(2.0, 0.0),
            ..UiLayout::fixed(100.0, 26.0)
        });
    for (mode, slug) in [
        (EditorViewMode::View2d, "2d"),
        (EditorViewMode::View3d, "3d"),
    ] {
        let selected = model.create_initial_view == mode;
        picker = picker.with_child(
            UiNode::new(
                format!("hub.create.advanced.view.{slug}"),
                UiNodeKind::Button,
            )
            .with_class(if selected {
                "hub-create-view-option-selected"
            } else {
                "hub-create-view-option"
            })
            .with_layout(UiLayout::fixed(47.0, 24.0))
            .with_text_key(if mode.is_2d() {
                "app.viewport_2d_short"
            } else {
                "app.viewport_3d_short"
            })
            .with_text_style(UiTextStyle::button(if selected {
                tokens.accent
            } else {
                tokens.text_muted
            }))
            .with_accessibility_label_key(if mode.is_2d() {
                "app.viewport_2d_short"
            } else {
                "app.viewport_3d_short"
            })
            .with_accessibility_role(raf_ui::UiAccessibilityRole::Button)
            .with_accessibility_selected(selected)
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                format!("hub.create.advanced.view.{slug}"),
            )),
        );
    }
    row = row.with_child(picker);
    panel.with_child(row).with_child(
        UiNode::new("hub.create.advanced.help", UiNodeKind::Label)
            .with_text_key("hub.create.advanced.view_help")
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(raf_ui::UiTextOverflow::Ellipsis)
            .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
    )
}

pub(crate) fn hub_create_name_input(
    _palette: StudioUiPalette,
    focused: bool,
    hovered: bool,
) -> UiNode {
    let mut field = UiNode::new("hub.create.name-field", UiNodeKind::Panel)
        .with_class("hub-create-name-field")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing {
                left: 14.0,
                right: 12.0,
                ..UiSpacing::ZERO
            },
            grow: 1.0,
            min_size: [180.0, 52.0],
            basis: [0.0, 52.0],
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            ..UiLayout::default()
        })
        .with_child(
            UiNode::image(
                "hub.create.name-sparkle",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new("editor.hub.sparkle"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
        .with_child(
            UiNode::text_input(
                "hub.create.name",
                UiTextInput {
                    value_key: "hub.create.name".to_string(),
                    placeholder_key: Some("app.hub_project_name_placeholder".to_string()),
                    ..UiTextInput::new("hub.create.name")
                },
            )
            .with_class("hub-name-input")
            .with_layout(UiLayout {
                grow: 1.0,
                basis: [0.0, 52.0],
                width_mode: UiSizeMode::Fill,
                height_mode: UiSizeMode::Fixed,
                ..UiLayout::default()
            })
            .with_accessibility_label_key("app.hub_project_name_placeholder"),
        );
    if hovered {
        field = field.with_class("hub-create-name-hovered");
    }
    if focused {
        field = field.with_class("hub-create-name-focused");
    }
    field
}

pub(crate) fn hub_create_type_picker(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = hub_tokens(palette);
    let icon_key = match model.create_project_type {
        ProjectType::Game => "editor.hub.kind-game",
        ProjectType::Electronics => "editor.hub.kind-electronics",
    };
    let mut picker = UiNode::new("hub.create.type-picker", UiNodeKind::Panel)
        .with_class("hub-create-picker")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 0.0),
            ..UiLayout::fixed(156.0, 52.0)
        })
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "hub.create.type-menu",
        ))
        .with_child(
            UiNode::image(
                "hub.create.type-icon",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(20.0, 20.0)),
        )
        .with_child(
            UiNode::new("hub.create.type-label", UiNodeKind::Label)
                .with_text_key("hub.create.type")
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 22.0)
                }),
        )
        .with_child(
            UiNode::new("hub.create.type-chevron", UiNodeKind::Label)
                .with_icon(
                    UiIcon::new(UiIconId::ChevronDown)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_layout(UiLayout::fixed(16.0, 16.0)),
        );

    if model.create_type_menu_open {
        let mut type_menu_layout = UiLayout::absolute(
            raf_render::api_graphic_basic::ui_surface::UiRect::new(0.0, 54.0, 156.0, 84.0),
        );
        type_menu_layout.flow = UiFlow::Column;
        type_menu_layout.gap = 0.0;
        type_menu_layout.padding = UiSpacing::same(0.0);
        type_menu_layout.z_index = 80;
        picker = picker.with_child(
            UiNode::new("hub.create.type-menu", UiNodeKind::Menu)
                .with_class("hub-create-menu")
                .with_material(UiSurfaceMaterial::TranslucentRaised)
                .with_layout(type_menu_layout)
                .with_child(hub_create_type_option(
                    palette,
                    "hub.create.type-game",
                    "app.hub_game_kind",
                    "editor.hub.kind-game",
                    "hub.create.game",
                    model.create_project_type == ProjectType::Game,
                ))
                .with_child(hub_create_type_option(
                    palette,
                    "hub.create.type-electronics",
                    "app.hub_electronics_kind",
                    "editor.hub.kind-electronics",
                    "hub.create.electronics",
                    model.create_project_type == ProjectType::Electronics,
                )),
        );
    }
    picker
}

pub(crate) fn hub_create_type_option(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    icon_key: &str,
    command: &str,
    selected: bool,
) -> UiNode {
    let tokens = hub_tokens(palette);
    let mut option = UiNode::new(id, UiNodeKind::Button).with_class("hub-create-option");
    if selected {
        option = option.with_class("hub-create-option-selected");
    }
    option
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(10.0, 0.0),
            basis: [0.0, 42.0],
            grow: 1.0,
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            ..UiLayout::default()
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 20.0)
                }),
        )
}

pub(crate) fn hub_create_location_input(
    _palette: StudioUiPalette,
    current_path: &str,
    hovered: bool,
) -> UiNode {
    let mut input = UiNode::text_input(
        "hub.create.path",
        UiTextInput {
            value_key: "hub.create.path".to_string(),
            placeholder_key: Some("app.location".to_string()),
            ..UiTextInput::new("hub.create.path")
        },
    )
    .with_class("hub-path-input")
    .with_class("hub-create-path")
    .with_layout(UiLayout {
        grow: 1.0,
        min_size: [110.0, 52.0],
        ..UiLayout::fixed(0.0, 52.0)
    })
    .with_accessibility_label_key("app.location");
    if !current_path.is_empty() {
        input = input.with_tooltip_value(current_path.to_string());
    }
    let mut pill = UiNode::new("hub.create.location-row", UiNodeKind::Panel)
        .with_class("hub-create-location")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            padding: UiSpacing::xy(4.0, 0.0),
            ..UiLayout::fixed(200.0, 52.0)
        });
    if hovered {
        pill = pill.with_class("hub-create-location-hovered");
    }
    pill.with_child(input).with_child(
        UiNode::new("hub.create.choose-path", UiNodeKind::Button)
            .with_class("hub-location-button")
            .with_layout(UiLayout {
                flow: UiFlow::Row,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                ..UiLayout::fixed(40.0, 44.0)
            })
            .with_tooltip_key("app.choose_location")
            .with_accessibility_label_key("app.choose_location")
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "hub.create.choose-path",
            ))
            .with_child(
                UiNode::image(
                    "hub.create.choose-path-icon",
                    UiImage {
                        options: Default::default(),
                        source: UiImageSource::new("editor.hub.folder"),
                        fit: UiImageFit::Contain,
                        tint: None,
                    },
                )
                .with_layout(UiLayout::fixed(20.0, 20.0)),
            ),
    )
}

pub(crate) fn hub_agent_toggle(palette: StudioUiPalette, enabled: bool) -> UiNode {
    let tokens = hub_tokens(palette);
    let mut toggle = UiNode::new("hub.create.agent", UiNodeKind::Button)
        .with_class("hub-agent-toggle")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(14.0, 0.0),
            ..UiLayout::fixed(152.0, 52.0)
        })
        .focusable()
        .with_accessibility_checked(enabled)
        .with_accessibility_label_key("agent_workspace.create_with_agent")
        .with_tooltip_key("agent_workspace.create_with_agent")
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "hub.create.agent-toggle",
        ))
        .with_child(
            UiNode::new("hub.create.agent-switch", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(34.0, 18.0))
                .with_style(UiStyle {
                    fill: if enabled {
                        tokens.accent_hot
                    } else {
                        [58, 58, 62, 255]
                    },
                    border: if enabled {
                        tokens.accent
                    } else {
                        [82, 82, 88, 255]
                    },
                    text: tokens.text_muted,
                    border_width: 1.0,
                    radius: 9.0,
                    opacity: 1.0,
                })
                .with_child(
                    UiNode::new("hub.create.agent-knob", UiNodeKind::Panel)
                        .with_layout(UiLayout::absolute(
                            raf_render::api_graphic_basic::ui_surface::UiRect::new(
                                if enabled { 18.0 } else { 2.0 },
                                2.0,
                                14.0,
                                14.0,
                            ),
                        ))
                        .with_style(UiStyle {
                            fill: if enabled {
                                [245, 245, 247, 255]
                            } else {
                                [198, 198, 204, 255]
                            },
                            border: if enabled {
                                [245, 245, 247, 255]
                            } else {
                                [198, 198, 204, 255]
                            },
                            text: tokens.text_muted,
                            border_width: 0.0,
                            radius: 7.0,
                            opacity: 1.0,
                        }),
                ),
        )
        .with_child(
            UiNode::new("hub.create.agent-label", UiNodeKind::Label)
                .with_text_key("app.hub_agent")
                .with_text_style(UiTextStyle::body(if enabled {
                    tokens.text
                } else {
                    tokens.text_muted
                }))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 20.0)
                }),
        );
    if enabled {
        toggle = toggle.with_class("hub-agent-toggle-on");
    }
    toggle
}

pub(crate) fn hub_create_submit(palette: StudioUiPalette, opens_existing: bool) -> UiNode {
    let tokens = hub_tokens(palette);
    let width = if opens_existing { 150.0 } else { 132.0 };
    UiNode::new("hub.create.submit", UiNodeKind::Button)
        .with_class("hub-primary-button")
        .with_class("hub-create-submit")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            padding: UiSpacing::xy(12.0, 0.0),
            ..UiLayout::fixed(width, 52.0)
        })
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "hub.create.submit",
        ))
        .with_child(
            UiNode::new("hub.create.submit-label", UiNodeKind::Label)
                .with_text_key(if opens_existing {
                    "app.hub_open"
                } else {
                    "app.hub_create"
                })
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Button,
                    size_px: 13.0,
                    line_height_px: 18.0,
                    weight: UiFontWeight::Bold,
                    color: [18, 18, 20, 255],
                    inherit_color: false,
                })
                .with_layout(UiLayout {
                    width_mode: UiSizeMode::FitContent,
                    height_mode: UiSizeMode::Fixed,
                    basis: [0.0, 18.0],
                    ..UiLayout::default()
                }),
        )
        .with_child(
            UiNode::image(
                "hub.create.submit-arrow",
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new("editor.hub.submit-arrow"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(15.0, 15.0)),
        )
        .with_style(UiStyle {
            fill: tokens.accent,
            border: tokens.accent_hot,
            text: [18, 18, 20, 255],
            border_width: 1.0,
            radius: 8.0,
            opacity: 1.0,
        })
}

#[allow(dead_code)]
pub(crate) fn hub_side_column(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    UiNode::new("hub.side-column", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 14.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent())
        .with_child(hub_legacy_create_surface(palette))
        .with_child(hub_activity_surface(palette, model))
}

#[allow(dead_code)]
pub(crate) fn hub_legacy_create_surface(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hub.create", UiNodeKind::Panel)
        .with_class("hub-side-panel")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(16.0),
            gap: 10.0,
            ..UiLayout::fixed(0.0, 158.0)
        })
        .with_child(
            UiNode::new("hub.create-title", UiNodeKind::Label)
                .with_text_key("app.hub_quick_actions")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 22.0)),
        )
        .with_child(hub_quick_action_button(
            palette,
            "hub.new-game",
            "app.hub_new_game_short",
            "hub.new-game",
            "editor.hub.kind-game",
        ))
        .with_child(hub_quick_action_button(
            palette,
            "hub.new-electronics",
            "app.hub_new_electronics_short",
            "hub.new-electronics",
            "editor.hub.kind-electronics",
        ))
}

pub(crate) fn hub_command_button(
    id: &str,
    label_key: &str,
    command: &str,
    width: f32,
    height: f32,
    class: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button([255, 255, 255, 255]))
        .with_class(class)
        .with_layout(UiLayout::fixed(width, height))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
}

#[allow(dead_code)]
pub(crate) fn hub_quick_action_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    icon_key: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("hub-quick-action")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(12.0, 0.0),
            ..UiLayout::fixed(0.0, 42.0)
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(20.0, 20.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(palette.tokens().text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 20.0)
                }),
        )
}

#[allow(dead_code)]
pub(crate) fn hub_project_command_button(
    id: &str,
    label_key: &str,
    project: &HubSurfaceProject,
    action: &str,
    width: f32,
    height: f32,
    class: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button([255, 255, 255, 255]))
        .with_class(class)
        .with_layout(UiLayout::fixed(width, height))
        .focusable()
        .with_event(project_action(project, action))
}

#[allow(dead_code)]
pub(crate) fn hub_theme_button(id: &str, label_key: &str, theme: Theme, selected: Theme) -> UiNode {
    let class = if theme == selected {
        "hub-theme-active"
    } else {
        "hub-theme-button"
    };
    let command = match theme {
        Theme::Dark => "hub.theme-dark",
        Theme::Light => "hub.theme-light",
        Theme::System => "hub.theme-system",
    };
    let width = match theme {
        Theme::Dark => 38.0,
        Theme::Light => 42.0,
        Theme::System => 58.0,
    };
    hub_command_button(id, label_key, command, width, 32.0, class)
}

#[allow(dead_code)]
pub(crate) fn hub_theme_icon_button(
    id: &str,
    label_key: &str,
    command: &str,
    theme: Theme,
    selected: Theme,
    icon_key: &str,
) -> UiNode {
    let class = if theme == selected {
        "hub-theme-active"
    } else {
        "hub-theme-button"
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            ..UiLayout::fixed(38.0, 32.0)
        })
        .focusable()
        .with_accessibility_label_key(label_key)
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
}

#[allow(dead_code)]
pub(crate) fn hub_nav_button(
    id: &str,
    label_key: &str,
    command: &str,
    icon_key: &str,
    width: f32,
    height: f32,
    class: &str,
) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(12.0, 0.0),
            ..UiLayout::fixed(width, height)
        })
        .focusable()
        .with_accessibility_label_key(label_key)
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button([245, 245, 245, 255]))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 20.0)
                }),
        )
}

pub(crate) fn hub_top_nav_button(
    id: &str,
    label_key: &str,
    command: &str,
    active: bool,
    palette: StudioUiPalette,
) -> UiNode {
    let tokens = hub_tokens(palette);
    let mut node = UiNode::new(id, UiNodeKind::Button)
        .with_class(if active {
            "hub-top-nav hub-top-nav-active"
        } else {
            "hub-top-nav"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            padding: UiSpacing::xy(4.0, 0.0),
            ..UiLayout::fixed(70.0, 48.0)
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 24.0)),
        );
    if active {
        node = node.with_child(
            UiNode::new(format!("{id}.active-indicator"), UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(28.0, 2.0))
                .with_style(UiStyle {
                    fill: tokens.accent,
                    border: tokens.accent,
                    text: tokens.accent,
                    border_width: 0.0,
                    radius: 0.0,
                    opacity: 1.0,
                }),
        );
    }
    node
}

#[allow(dead_code)]
pub(crate) fn hub_icon_command_button(id: &str, command: &str, image_key: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hub-button")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            justify_content: UiJustify::Center,
            align_items: UiAlign::Center,
            ..UiLayout::fixed(42.0, 38.0)
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(image_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
}

pub(crate) fn hub_window_button(id: &str, command: &str, image_key: &str) -> UiNode {
    let tooltip_key = match command {
        "window.minimize" => "app.window.minimize",
        "window.maximize" => "app.window.maximize",
        "window.close" => "app.window.close",
        _ => "app.window.close",
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(if command == "window.close" {
            "hub-window-button hub-window-close"
        } else {
            "hub-window-button"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::fixed(32.0, 32.0)
        })
        .with_tooltip_key(tooltip_key)
        .with_accessibility_label_key(tooltip_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::image(
                format!("{id}.icon"),
                UiImage {
                    options: Default::default(),
                    source: UiImageSource::new(image_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(14.0, 14.0)),
        )
}
