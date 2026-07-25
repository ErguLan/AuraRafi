//! Retained native-studio surface blueprint.
//!
//! This is intentionally a renderer-independent editor shell definition. The
//! remaining egui panels stay as adapters while the native WGPU host takes
//! ownership of concrete panel content one surface at a time.

use raf_core::config::Theme;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow,
    UiImage, UiImageFit, UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow,
    UiResponsiveRule, UiScrollAxis, UiSpacing, UiStyle, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface, UiTextInput, UiTextStyle,
};
use serde_json::json;
use std::path::PathBuf;

/// The visible filter is deliberately data, not an editor-widget detail. A
/// native window, the temporary eframe adapter, and future project surfaces
/// can all build the same hub from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubSurfaceFilter {
    All,
    Game,
    Electronics,
}

/// Project data consumed by the retained Hub document. It contains display
/// values only; loading, duplication, and persistence remain in the editor
/// application boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubSurfaceProject {
    pub name: String,
    pub path: PathBuf,
    pub project_type: ProjectType,
    pub last_opened_label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HubSurfaceModel {
    pub filter: HubSurfaceFilter,
    pub theme: Theme,
    pub total_projects: usize,
    pub game_projects: usize,
    pub electronics_projects: usize,
    pub projects: Vec<HubSurfaceProject>,
    pub featured_project: Option<HubSurfaceProject>,
    pub recent_activity: Vec<HubSurfaceProject>,
    pub context_project: Option<HubSurfaceProject>,
    pub context_menu_position: Option<[f32; 2]>,
    /// Transient pointer state supplied by the host. Keeping it in the model
    /// lets the retained document reveal card actions without a renderer-only
    /// hover shortcut.
    pub hovered_project_path: Option<PathBuf>,
}

impl Default for HubSurfaceModel {
    fn default() -> Self {
        Self {
            filter: HubSurfaceFilter::All,
            theme: Theme::Dark,
            total_projects: 0,
            game_projects: 0,
            electronics_projects: 0,
            projects: Vec::new(),
            featured_project: None,
            recent_activity: Vec::new(),
            context_project: None,
            context_menu_position: None,
            hovered_project_path: None,
        }
    }
}

/// Builds a valid empty retained Hub document. Hosts should use
/// [`build_hub_surface_with_model`] when project data is available.
pub fn build_hub_surface(palette: StudioUiPalette) -> UiSurface {
    build_hub_surface_with_model(palette, &HubSurfaceModel::default())
}

/// Builds the production Hub surface from current project metadata. The
/// document is still serializable and renderer-neutral: only its actions and
/// resource keys cross into the host boundary.
pub fn build_hub_surface_with_model(
    palette: StudioUiPalette,
    model: &HubSurfaceModel,
) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = UiNode::new("hub.root", UiNodeKind::Root)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Stack,
            responsive: vec![UiResponsiveRule {
                max_width: 760.0,
                flow: Some(UiFlow::Column),
                basis: None,
                padding: None,
                gap: Some(0.0),
                compact: None,
                grid_columns: None,
            }],
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(palette.root_style())
        .with_child(
            UiNode::new("hub.navigation", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    basis: [190.0, 0.0],
                    min_size: [176.0, 0.0],
                    padding: UiSpacing::same(16.0),
                    gap: 10.0,
                    overflow: UiOverflow::Clip,
                    responsive: vec![UiResponsiveRule {
                        max_width: 760.0,
                        flow: None,
                        basis: Some([0.0, 160.0]),
                        padding: Some(UiSpacing::same(12.0)),
                        gap: Some(6.0),
                        compact: None,
                        grid_columns: None,
                    }],
                    ..UiLayout::default()
                })
                .with_style(hub_sidebar_style(palette))
                .with_child(
                    UiNode::image(
                        "hub.brand",
                        UiImage {
                            source: UiImageSource::new("editor.hub.brand"),
                            fit: UiImageFit::Contain,
                            tint: None,
                        },
                    )
                    .with_layout(UiLayout::fixed(0.0, 54.0)),
                )
                .with_child(
                    UiNode::new("hub.brand-name", UiNodeKind::Label)
                        .with_text_key("hub.brand")
                        .with_text_style(UiTextStyle::panel_title(tokens.text))
                        .with_layout(UiLayout::fixed(0.0, 24.0)),
                )
                .with_child(hub_nav_button(
                    "hub.home",
                    "app.hub_nav_home",
                    "hub.home",
                    "editor.hub.icon-home",
                    158.0,
                    36.0,
                    "hub-nav-active",
                ))
                .with_child(hub_nav_button(
                    "hub.projects",
                    "app.hub_nav_projects",
                    "hub.projects",
                    "editor.hub.kind-game",
                    158.0,
                    36.0,
                    "hub-nav-button",
                ))
                .with_child(
                    UiNode::new("hub.nav-divider", UiNodeKind::Separator)
                        .with_class("hub-divider")
                        .with_layout(UiLayout::fixed(158.0, 1.0)),
                )
                .with_child(
                    UiNode::new("hub.version", UiNodeKind::Label)
                        .with_text_key("hub.version")
                        .with_text_style(UiTextStyle::body(tokens.text_muted))
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::default()
                        }),
                )
                .with_child(hub_nav_button(
                    "hub.settings",
                    "app.settings_menu",
                    "hub.settings",
                    "editor.hub.settings",
                    158.0,
                    36.0,
                    "hub-nav-button",
                ))
                .with_child(
                    UiNode::new("hub.theme-controls", UiNodeKind::Toolbar)
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            compact: UiCompactMode::Wrap,
                            gap: 6.0,
                            ..UiLayout::fixed(0.0, 38.0)
                        })
                        .with_style(UiStyle::transparent())
                        .with_child(hub_theme_icon_button(
                            "hub.theme-dark",
                            "app.hub_theme_dark",
                            "hub.theme-dark",
                            Theme::Dark,
                            model.theme,
                            "editor.hub.icon-moon",
                        ))
                        .with_child(hub_theme_icon_button(
                            "hub.theme-light",
                            "app.hub_theme_light",
                            "hub.theme-light",
                            Theme::Light,
                            model.theme,
                            "editor.hub.icon-sun",
                        )),
                ),
        )
        .with_child(
            UiNode::new("hub.body", UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::Column)
                })
                .with_style(hub_body_style(palette))
                .with_child(hub_topbar_surface())
                .with_child(hub_content_columns_surface(palette, model)),
        );

    if let (Some(project), Some(position)) =
        (model.context_project.as_ref(), model.context_menu_position)
    {
        root = root.with_child(hub_context_menu_surface(palette, project, position));
    }

    let mut surface = UiSurface::new("studio.hub", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("hub-card".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-card".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-project-menu".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-project-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-featured".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-featured".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-side-panel".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(7.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-quick-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-quick-action".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-topbar".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    fill: Some(tokens.surface_alt),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-primary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-danger-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.danger),
                    text: Some(tokens.danger),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-danger-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.danger),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-filter-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-nav-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-nav-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-nav-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.focus),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-nav-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(2.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-theme-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-theme-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-divider".to_string()),
                UiStylePatch {
                    fill: Some(tokens.border),
                    border: Some(tokens.border),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-theme-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-theme-active".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-context-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-input".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-input".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-link-button".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    text: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-link-button".to_string()),
                UiStylePatch {
                    text: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-activity-item".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-activity-item".to_string()),
                UiStylePatch {
                    fill: Some(match palette {
                        StudioUiPalette::IndustrialDark => [24, 28, 34, 220],
                        StudioUiPalette::PaperLight => [240, 240, 240, 220],
                    }),
                    border: Some(match palette {
                        StudioUiPalette::IndustrialDark => [40, 44, 50, 255],
                        StudioUiPalette::PaperLight => [220, 222, 226, 255],
                    }),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    };
    surface
}

fn hub_sidebar_style(palette: StudioUiPalette) -> UiStyle {
    let tokens = palette.tokens();
    UiStyle {
        fill: match palette {
            StudioUiPalette::IndustrialDark => [10, 13, 18, 255],
            StudioUiPalette::PaperLight => tokens.surface,
        },
        border: tokens.border,
        text: tokens.text,
        border_width: 1.0,
        radius: 0.0,
        opacity: 1.0,
    }
}

fn hub_body_style(palette: StudioUiPalette) -> UiStyle {
    let tokens = palette.tokens();
    UiStyle {
        fill: tokens.background,
        border: tokens.background,
        text: tokens.text,
        border_width: 0.0,
        radius: 0.0,
        opacity: 1.0,
    }
}

fn hub_topbar_surface() -> UiNode {
    UiNode::new("hub.topbar", UiNodeKind::Toolbar)
        .with_class("hub-topbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            justify_content: UiJustify::SpaceBetween,
            align_items: UiAlign::Center,
            compact: UiCompactMode::Stack,
            padding: UiSpacing::same(16.0),
            gap: 12.0,
            responsive: vec![UiResponsiveRule {
                max_width: 760.0,
                flow: Some(UiFlow::Column),
                basis: Some([0.0, 122.0]),
                padding: Some(UiSpacing::same(12.0)),
                gap: Some(8.0),
                compact: None,
                grid_columns: None,
            }],
            ..UiLayout::fixed(0.0, 72.0)
        })
        .with_child(
            UiNode::text_input(
                "hub.search",
                UiTextInput {
                    value_key: "hub.search".to_string(),
                    placeholder_key: Some("app.hub_search_hint".to_string()),
                    submit_command: Some("app.hub_search".to_string()),
                    ..UiTextInput::new("hub.search")
                },
            )
            .with_class("hub-input")
            .with_layout(UiLayout {
                min_size: [220.0, 38.0],
                max_size: [560.0, 38.0],
                grow: 1.0,
                ..UiLayout::fixed(520.0, 38.0)
            }),
        )
        .with_child(hub_icon_command_button(
            "hub.settings-top",
            "hub.settings",
            "editor.hub.settings",
        ))
}

fn hub_header_surface(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hub.header", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fixed(0.0, 62.0)
        })
        .with_style(UiStyle::transparent())
        .with_child(
            UiNode::new("hub.title", UiNodeKind::Label)
                .with_text_key("hub.welcome")
                .with_text_style(UiTextStyle {
                    size_px: 28.0,
                    line_height_px: 34.0,
                    ..UiTextStyle::panel_title(tokens.text)
                })
                .with_layout(UiLayout::fixed(0.0, 34.0)),
        )
        .with_child(
            UiNode::new("hub.subtitle", UiNodeKind::Label)
                .with_text_key("hub.welcome-detail")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        )
}

fn hub_content_columns_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    UiNode::new("hub.content-columns", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            grow: 1.0,
            compact: UiCompactMode::Stack,
            gap: 0.0,
            responsive: vec![UiResponsiveRule {
                // The body is narrower than the native window because the
                // navigation rail is a sibling. Keep the desktop rail until
                // the two columns genuinely no longer fit.
                max_width: 620.0,
                flow: Some(UiFlow::Column),
                basis: None,
                padding: None,
                gap: Some(0.0),
                compact: None,
                grid_columns: None,
            }],
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_style(hub_body_style(palette))
        .with_child(
            UiNode::scroll_view("hub.content", UiScrollAxis::Vertical)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    grow: 1.0,
                    min_size: [320.0, 0.0],
                    padding: UiSpacing::same(24.0),
                    overflow: UiOverflow::ScrollY,
                    responsive: vec![UiResponsiveRule {
                        max_width: 760.0,
                        flow: None,
                        basis: None,
                        padding: Some(UiSpacing::same(18.0)),
                        gap: None,
                        compact: None,
                        grid_columns: None,
                    }],
                    ..UiLayout::fill(UiFlow::Column)
                })
                .with_style(hub_body_style(palette))
                .with_child(hub_workspace_surface(palette, model)),
        )
        .with_child(
            UiNode::scroll_view("hub.side-scroll", UiScrollAxis::Vertical)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    basis: [280.0, 0.0],
                    min_size: [248.0, 0.0],
                    padding: UiSpacing::same(16.0),
                    overflow: UiOverflow::ScrollY,
                    responsive: vec![UiResponsiveRule {
                        max_width: 620.0,
                        flow: None,
                        basis: Some([0.0, 0.0]),
                        padding: Some(UiSpacing::same(18.0)),
                        gap: None,
                        compact: None,
                        grid_columns: None,
                    }],
                    ..UiLayout::default()
                })
                .with_style(hub_sidebar_style(palette))
                .with_child(hub_side_column(palette, model)),
        )
}

fn hub_workspace_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let mut main_column = UiNode::new("hub.main-column", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            min_size: [300.0, 0.0],
            gap: 14.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent())
        .with_child(hub_header_surface(palette));

    if let Some(project) = model.featured_project.as_ref() {
        main_column = main_column.with_child(hub_featured_project(palette, project));
    }

    let main_column = main_column
        .with_child(
            UiNode::new("hub.recent-projects-label", UiNodeKind::Label)
                .with_text_key("app.hub_recent_projects")
                .with_text_style(UiTextStyle::panel_title(palette.tokens().text))
                .with_layout(UiLayout::fixed(0.0, 22.0)),
        )
        .with_child(hub_filter_surface(palette, model))
        .with_child(hub_grid_surface(palette, model));

    main_column
}

fn hub_featured_project(palette: StudioUiPalette, project: &HubSurfaceProject) -> UiNode {
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

fn hub_featured_continue_button(project: &HubSurfaceProject) -> UiNode {
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
                    source: UiImageSource::new("editor.hub.icon-arrow"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
}

fn hub_side_column(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    UiNode::new("hub.side-column", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            grow: 1.0,
            gap: 14.0,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle::transparent())
        .with_child(hub_create_surface(palette))
        .with_child(hub_activity_surface(palette, model))
}

fn hub_create_surface(palette: StudioUiPalette) -> UiNode {
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

fn hub_activity_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_view_all_link(id: &str, label_key: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hub-link-button")
        .with_layout(UiLayout::fixed(64.0, 22.0))
        .focusable()
        .with_accessibility_label_key(label_key)
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::button([224, 116, 24, 255]))
        .with_event(UiEventBinding::command(UiEventKind::Click, "hub.view-all"))
}

fn hub_context_menu_surface(
    palette: StudioUiPalette,
    project: &HubSurfaceProject,
    position: [f32; 2],
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("hub.context-menu", UiNodeKind::Menu)
        .with_class("hub-context-menu")
        .with_layout(
            UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::same(12.0),
                gap: 6.0,
                ..UiLayout::absolute(raf_render::api_graphic_basic::ui_surface::UiRect::new(
                    position[0],
                    position[1],
                    224.0,
                    178.0,
                ))
            }
            .with_z_index(100),
        )
        .with_child(
            UiNode::new("hub.context-title", UiNodeKind::Label)
                .with_text_key("hub.context.title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        )
        .with_child(hub_project_command_button(
            "hub.context-open",
            "app.hub_open",
            project,
            "open",
            198.0,
            32.0,
            "hub-button",
        ))
        .with_child(hub_project_command_button(
            "hub.context-duplicate",
            "app.hub_duplicate",
            project,
            "duplicate",
            198.0,
            32.0,
            "hub-button",
        ))
        .with_child(hub_project_command_button(
            "hub.context-forget",
            "app.hub_delete",
            project,
            "forget",
            198.0,
            32.0,
            "hub-danger-button",
        ))
        .with_child(hub_command_button(
            "hub.context-close",
            "app.cancel",
            "hub.context-close",
            198.0,
            32.0,
            "hub-button",
        ))
}

fn hub_filter_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_grid_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_project_card(
    palette: StudioUiPalette,
    project: &HubSurfaceProject,
    index: usize,
    menu_visible: bool,
) -> UiNode {
    let tokens = palette.tokens();
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

fn project_kind_icon_key(project: &HubSurfaceProject) -> &'static str {
    match project.project_type {
        ProjectType::Game => "editor.hub.kind-game",
        ProjectType::Electronics => "editor.hub.kind-electronics",
    }
}

fn hub_project_menu_button(project: &HubSurfaceProject, card_id: &str) -> UiNode {
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
                    source: UiImageSource::new("editor.hub.more"),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(14.0, 14.0)),
        )
}

fn project_thumbnail_style(palette: StudioUiPalette, project_type: ProjectType) -> UiStyle {
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

fn project_action(project: &HubSurfaceProject, action: &str) -> UiEventBinding {
    UiEventBinding {
        event: UiEventKind::Click,
        action: UiAction::Custom {
            channel: "hub.project".to_string(),
            payload: json!({
                "action": action,
                "path": project.path.to_string_lossy(),
            }),
        },
    }
}

fn project_context_action(project: &HubSurfaceProject) -> UiEventBinding {
    project_menu_event(project, UiEventKind::ContextMenu)
}

fn project_menu_action(project: &HubSurfaceProject) -> UiEventBinding {
    project_menu_event(project, UiEventKind::Click)
}

fn project_menu_event(project: &HubSurfaceProject, event: UiEventKind) -> UiEventBinding {
    UiEventBinding {
        event,
        action: UiAction::Custom {
            channel: "hub.project".to_string(),
            payload: json!({
                "action": "menu",
                "path": project.path.to_string_lossy(),
            }),
        },
    }
}

fn hub_command_button(
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

fn hub_quick_action_button(
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

fn hub_project_command_button(
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
fn hub_theme_button(id: &str, label_key: &str, theme: Theme, selected: Theme) -> UiNode {
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

fn hub_theme_icon_button(
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
                    source: UiImageSource::new(icon_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
}

fn hub_nav_button(
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

#[allow(dead_code)]
fn hub_icon_command_button(id: &str, command: &str, image_key: &str) -> UiNode {
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
                    source: UiImageSource::new(image_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
}

fn filter_class(target: HubSurfaceFilter, active: HubSurfaceFilter) -> &'static str {
    if target == active {
        "hub-filter-active"
    } else {
        "hub-button"
    }
}

pub fn build_studio_surface(palette: StudioUiPalette) -> UiSurface {
    let tokens = palette.tokens();
    let control_hover = match palette {
        StudioUiPalette::IndustrialDark => [44, 44, 46, 255],
        StudioUiPalette::PaperLight => [224, 224, 226, 255],
    };
    let root = UiNode::new("studio.root", UiNodeKind::Root)
        .with_class("studio-root")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(toolbar())
        .with_child(workspace())
        .with_child(bottom_dock());

    let mut surface = UiSurface::new("studio.editor", palette, root);
    surface.style_sheet = UiStyleSheet {
        rules: vec![
            UiStyleRule::new(
                UiStyleSelector::Class("studio-root".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("toolbar".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("dock-panel".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("canvas".to_string()),
                UiStylePatch {
                    fill: Some(tokens.background),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("toolbar-button".to_string()),
                UiStylePatch {
                    fill: Some(control_hover),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("toolbar-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("accent-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some(match palette {
                        StudioUiPalette::IndustrialDark => [18, 18, 20, 255],
                        StudioUiPalette::PaperLight => [255, 255, 255, 255],
                    }),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("accent-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("dock-tab".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(2.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("dock-tab".to_string()),
                UiStylePatch {
                    fill: Some(control_hover),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
        ],
    };
    surface
}

fn toolbar() -> UiNode {
    UiNode::new("studio.toolbar", UiNodeKind::Toolbar)
        .with_class("toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            padding: UiSpacing::same(8.0),
            gap: 6.0,
            ..UiLayout::fixed(0.0, 36.0)
        })
        .with_child(command_button(
            "studio.file",
            "app.file",
            "editor.menu.file",
            "toolbar-button",
        ))
        .with_child(command_button(
            "studio.edit",
            "app.studio_edit",
            "editor.menu.edit",
            "toolbar-button",
        ))
        .with_child(command_button(
            "studio.view",
            "app.studio_view",
            "editor.menu.view",
            "toolbar-button",
        ))
        .with_child(command_button(
            "studio.project",
            "app.studio_project",
            "editor.menu.project",
            "toolbar-button",
        ))
        .with_child(command_button(
            "studio.run",
            "app.studio_run",
            "editor.runtime.prepared",
            "accent-button",
        ))
}

fn workspace() -> UiNode {
    UiNode::new("studio.workspace", UiNodeKind::DockArea)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 1.0,
            ..UiLayout::fill(UiFlow::Row)
        })
        .with_child(dock_panel(
            "studio.hierarchy",
            "app.hierarchy",
            UiLayout::fixed(230.0, 0.0),
        ))
        .with_child(
            UiNode::new("studio.viewport", UiNodeKind::Canvas)
                .with_class("canvas")
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::None)
                })
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "viewport.focus",
                )),
        )
        .with_child(dock_panel(
            "studio.inspector",
            "app.properties",
            UiLayout::fixed(300.0, 0.0),
        ))
}

fn bottom_dock() -> UiNode {
    UiNode::new("studio.bottom", UiNodeKind::Panel)
        .with_class("dock-panel")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(6.0),
            gap: 4.0,
            ..UiLayout::fixed(0.0, 190.0)
        })
        .with_child(
            UiNode::new("studio.bottom-tabs", UiNodeKind::Toolbar)
                .with_class("toolbar")
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    gap: 4.0,
                    ..UiLayout::fixed(0.0, 28.0)
                })
                .with_child(command_button(
                    "studio.console",
                    "app.studio_console",
                    "bottom.console",
                    "dock-tab",
                ))
                .with_child(command_button(
                    "studio.assets",
                    "app.studio_assets",
                    "bottom.assets",
                    "dock-tab",
                ))
                .with_child(command_button(
                    "studio.node-editor",
                    "app.studio_node_editor",
                    "bottom.nodes",
                    "dock-tab",
                ))
                .with_child(command_button(
                    "studio.agent",
                    "app.agent_tab",
                    "bottom.agent",
                    "dock-tab",
                )),
        )
        .with_child(
            UiNode::new("studio.bottom-content", UiNodeKind::Panel)
                .with_class("canvas")
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fill(UiFlow::None)
                }),
        )
}

fn dock_panel(id: &str, title_key: &str, layout: UiLayout) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("dock-panel")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(8.0),
            gap: 6.0,
            ..layout
        })
        .with_child(
            UiNode::new(format!("{id}.title"), UiNodeKind::Label)
                .with_text_key(title_key)
                .with_layout(UiLayout::fixed(0.0, 20.0)),
        )
}

fn command_button(id: &str, text_key: &str, command: &str, class_name: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class_name)
        .with_text_key(text_key)
        .with_layout(UiLayout::fixed(82.0, 24.0))
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action: UiAction::Command {
                name: command.to_string(),
            },
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_render::api_graphic_basic::ui_surface::{UiInputState, UiSurfaceSession};

    #[test]
    fn studio_surface_has_fixed_docks_and_a_growing_canvas() {
        let surface = build_studio_surface(StudioUiPalette::IndustrialDark);
        let frame = surface.build_frame(1440, 900, [8, 8, 8, 255]);

        let viewport = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "studio.viewport")
            .expect("viewport box");
        let hierarchy = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "studio.hierarchy")
            .expect("hierarchy box");
        let inspector = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "studio.inspector")
            .expect("inspector box");

        assert_eq!(hierarchy.rect.width, 230.0);
        assert_eq!(inspector.rect.width, 300.0);
        assert!(viewport.rect.width > 700.0);
        assert!(frame
            .hit_regions
            .iter()
            .any(|region| region.id == "studio.run"));
    }

    #[test]
    fn hub_surface_uses_retained_search_and_responsive_grid_controls() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let frame = surface.build_frame(520, 720, [8, 8, 8, 255]);

        assert!(frame.layout_boxes.iter().any(
            |layout| layout.id == "hub.search" && matches!(layout.kind, UiNodeKind::TextInput)
        ));
        assert!(frame
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "hub.projects-grid"
                && matches!(layout.kind, UiNodeKind::Grid)));
    }

    #[test]
    fn hub_places_global_search_above_the_workspace_and_keeps_the_side_column_at_desktop_width() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        // The content area is narrower than the window once the 236px
        // navigation rail is accounted for. Cover a common compact desktop
        // width so the fixed right rail cannot silently stack below the fold.
        let frame = surface.build_frame(1_000, 760, [8, 11, 15, 255]);

        let search = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.search")
            .expect("global search box");
        let header = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.header")
            .expect("workspace header");
        let main = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.main-column")
            .expect("main column");
        let side = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.side-column")
            .expect("side column");
        let create = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create")
            .expect("create panel");
        let activity = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.activity")
            .expect("activity panel");

        assert!(search.rect.bottom() <= header.rect.y);
        assert!(side.rect.x >= main.rect.right());
        assert!(create.rect.height >= 158.0);
        assert!(activity.rect.y >= create.rect.bottom());
    }

    #[test]
    fn project_menu_button_dispatches_the_retained_context_action() {
        let project = HubSurfaceProject {
            name: "Demo".to_string(),
            path: PathBuf::from("C:/Projects/Demo"),
            project_type: ProjectType::Game,
            last_opened_label: "14/07/2026".to_string(),
        };
        let model = HubSurfaceModel {
            projects: vec![project.clone()],
            featured_project: Some(project),
            hovered_project_path: Some(PathBuf::from("C:/Projects/Demo")),
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);
        let mut session = UiSurfaceSession::default();
        let frame = session.build_frame(&surface, 1_300, 900, [8, 11, 15, 255]);
        let menu = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.project.0.menu")
            .expect("project menu box");
        let pointer_position = Some([
            menu.rect.x + menu.rect.width * 0.5,
            menu.rect.y + menu.rect.height * 0.5,
        ]);

        let _ = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position,
                pointer_down: true,
                ..UiInputState::default()
            },
        );
        assert_eq!(
            session.interaction.focus.active.as_deref(),
            Some("hub.project.0.menu")
        );
        let actions = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position,
                pointer_down: false,
                ..UiInputState::default()
            },
        );

        assert!(actions.iter().any(|dispatched| {
            matches!(
                &dispatched.action,
                UiAction::Custom { channel, payload }
                    if channel == "hub.project"
                        && payload.get("action").and_then(|value| value.as_str()) == Some("menu")
            )
        }));
    }

    #[test]
    fn hub_reveals_project_menu_only_for_the_hovered_card() {
        let project = HubSurfaceProject {
            name: "Demo".to_string(),
            path: PathBuf::from("C:/Projects/Demo"),
            project_type: ProjectType::Game,
            last_opened_label: "14/07/2026".to_string(),
        };
        let hidden_model = HubSurfaceModel {
            projects: vec![project.clone()],
            ..HubSurfaceModel::default()
        };
        let visible_model = HubSurfaceModel {
            projects: vec![project.clone()],
            hovered_project_path: Some(project.path),
            ..HubSurfaceModel::default()
        };

        let hidden = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &hidden_model)
            .build_frame(1_300, 900, [8, 11, 15, 255]);
        let visible = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &visible_model)
            .build_frame(1_300, 900, [8, 11, 15, 255]);

        assert!(!hidden
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "hub.project.0.menu"));
        assert!(visible
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "hub.project.0.menu"));
    }

    #[test]
    fn hub_navigation_is_a_fixed_panel_not_a_scroll_view() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let navigation = surface
            .root
            .children
            .iter()
            .find(|child| child.id == "hub.navigation")
            .expect("hub navigation");

        assert_eq!(navigation.kind, UiNodeKind::Panel);
        assert!(!matches!(
            navigation.control,
            raf_render::api_graphic_basic::ui_surface::UiControl::ScrollView { .. }
        ));
    }
}
