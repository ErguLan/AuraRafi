//! Retained native-studio surface blueprint.
//!
//! This is intentionally a renderer-independent editor shell definition. The
//! retained panels are composed by the native Winit host and ApiGraphicBasic.

use raf_core::config::Theme;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiCompactMode, UiEventBinding, UiEventKind, UiFlow,
    UiImage, UiImageFit, UiImageSource, UiJustify, UiLayout, UiNode, UiNodeKind, UiOverflow,
    UiResponsiveRule, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface, UiSurfaceMaterial, UiTextInput,
    UiTextRole, UiTextStyle, UiTokens,
};
use raf_ui::{UiFontWeight, UiIcon, UiIconId, UiIconSize};
use serde_json::json;
use std::path::PathBuf;

/// The visible filter is deliberately data, not an editor-widget detail. A
/// native window and future project surfaces can all build the same hub from
/// it.
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

/// A folder chosen through the native picker that is not a project yet. The
/// Hub asks for confirmation before adopting it as a project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubPendingAdopt {
    pub path: PathBuf,
    pub name: String,
    pub project_type: ProjectType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HubSurfaceModel {
    pub filter: HubSurfaceFilter,
    pub theme: Theme,
    pub search_query: String,
    pub total_projects: usize,
    pub game_projects: usize,
    pub electronics_projects: usize,
    pub projects: Vec<HubSurfaceProject>,
    pub featured_project: Option<HubSurfaceProject>,
    pub recent_activity: Vec<HubSurfaceProject>,
    pub context_project: Option<HubSurfaceProject>,
    pub context_menu_position: Option<[f32; 2]>,
    /// Logical window size supplied by the host. It lets the retained document
    /// keep the project context menu inside the viewport on every resize.
    pub window_size: [f32; 2],
    /// Transient pointer state supplied by the host. Keeping it in the model
    /// lets the retained document reveal card actions without a renderer-only
    /// hover shortcut.
    pub hovered_project_path: Option<PathBuf>,
    pub create_name: String,
    pub create_path: String,
    pub create_project_type: ProjectType,
    pub create_error: Option<HubProjectCreateError>,
    pub create_active: bool,
    pub create_type_menu_open: bool,
    pub create_with_agent: bool,
    /// True when `create_path` points at an existing project folder. The Hub
    /// reuses the create row as the "open existing project" entry point, so
    /// the primary action flips from Create to Open.
    pub create_path_is_project: bool,
    /// True while the project name field owns the keyboard. The host mirrors
    /// focus so the retained name pill can draw its focus ring.
    pub create_name_focused: bool,
    /// Transient hover state for the name pill. The host mirrors it so the
    /// whole pill (sparkle, padding and rounded corners) reacts as one unit.
    pub create_name_hovered: bool,
    /// Same contract for the location pill.
    pub create_path_hovered: bool,
    /// True while the host is scanning the filesystem in the background. The
    /// Hub shows a loading state instead of "No projects found" on first paint.
    pub discovery_pending: bool,
    /// Two-step confirmation for destructive context-menu rows. The first
    /// activation arms the row; the second one removes the project.
    pub context_confirm_forget: bool,
    /// "Get in" dropdown: new project, open folder, or open a `.ron` file.
    pub get_in_menu_open: bool,
    pub get_in_menu_position: Option<[f32; 2]>,
    /// Folder awaiting the "create a project here" confirmation.
    pub pending_adopt: Option<HubPendingAdopt>,
    /// Human-readable reason shown when a project cannot be opened.
    pub pending_open_error: Option<String>,
    pub opening_project: Option<String>,
    pub opening_error: Option<String>,
    pub opening_step: u8,
    pub opening_size: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubProjectCreateError {
    NameRequired,
    NameInvalid,
    LocationRequired,
    CreationFailed(String),
}

/// Fixed logical size of the project context menu. The builder and the host
/// share it so the menu can be clamped inside the window and dismissed by a
/// press outside the exact same rectangle.
pub(crate) const HUB_CONTEXT_MENU_SIZE: [f32; 2] = [236.0, 180.0];
/// Fixed logical size of the "Get in" dropdown.
pub(crate) const HUB_GET_IN_MENU_SIZE: [f32; 2] = [252.0, 112.0];
const HUB_CONTEXT_MENU_MARGIN: f32 = 8.0;

/// Clamps a menu anchor to the current logical window so the menu is never cut
/// off at the bottom or right edge.
pub(crate) fn clamp_menu_position(
    position: [f32; 2],
    window_size: [f32; 2],
    size: [f32; 2],
) -> [f32; 2] {
    let max_x = (window_size[0] - size[0] - HUB_CONTEXT_MENU_MARGIN).max(HUB_CONTEXT_MENU_MARGIN);
    let max_y = (window_size[1] - size[1] - HUB_CONTEXT_MENU_MARGIN).max(HUB_CONTEXT_MENU_MARGIN);
    [
        position[0].clamp(HUB_CONTEXT_MENU_MARGIN, max_x),
        position[1].clamp(HUB_CONTEXT_MENU_MARGIN, max_y),
    ]
}

/// Clamps the project context menu anchor.
pub(crate) fn clamp_context_menu_position(position: [f32; 2], window_size: [f32; 2]) -> [f32; 2] {
    clamp_menu_position(position, window_size, HUB_CONTEXT_MENU_SIZE)
}

fn hub_create_height(model: &HubSurfaceModel) -> f32 {
    if model.create_error.is_some() || model.create_path_is_project {
        102.0
    } else {
        52.0
    }
}

/// The Hub has a quieter neutral surface than the editor canvas. Keep this
/// adjustment local to the Hub so the rest of the engine retains its locked
/// industrial palette.
fn hub_tokens(palette: StudioUiPalette) -> UiTokens {
    let mut tokens = palette.tokens();
    if matches!(palette, StudioUiPalette::IndustrialDark) {
        tokens.background = [9, 9, 10, 255];
        tokens.surface = [14, 14, 15, 255];
        tokens.surface_alt = [20, 20, 21, 255];
        tokens.surface_raised = [26, 26, 27, 255];
        tokens.canvas = [11, 11, 12, 255];
        tokens.border = [48, 48, 50, 255];
        tokens.text_muted = [148, 148, 152, 255];
    }
    tokens
}

fn hub_root_style(palette: StudioUiPalette) -> UiStyle {
    let tokens = hub_tokens(palette);
    UiStyle {
        fill: tokens.background,
        border: tokens.border,
        text: tokens.text,
        border_width: 0.0,
        radius: 0.0,
        opacity: 1.0,
    }
}

impl Default for HubSurfaceModel {
    fn default() -> Self {
        Self {
            filter: HubSurfaceFilter::All,
            theme: Theme::Dark,
            search_query: String::new(),
            total_projects: 0,
            game_projects: 0,
            electronics_projects: 0,
            projects: Vec::new(),
            featured_project: None,
            recent_activity: Vec::new(),
            context_project: None,
            context_menu_position: None,
            window_size: [1280.0, 900.0],
            hovered_project_path: None,
            create_name: String::new(),
            create_path: String::new(),
            create_project_type: ProjectType::Game,
            create_error: None,
            create_active: false,
            create_type_menu_open: false,
            create_with_agent: false,
            create_path_is_project: false,
            create_name_focused: false,
            create_name_hovered: false,
            create_path_hovered: false,
            discovery_pending: false,
            context_confirm_forget: false,
            get_in_menu_open: false,
            get_in_menu_position: None,
            pending_adopt: None,
            pending_open_error: None,
            opening_project: None,
            opening_error: None,
            opening_step: 0,
            opening_size: [1280.0, 900.0],
        }
    }
}

fn hub_project_matches(model: &HubSurfaceModel, project: &HubSurfaceProject) -> bool {
    let type_matches = match model.filter {
        HubSurfaceFilter::All => true,
        HubSurfaceFilter::Game => project.project_type == ProjectType::Game,
        HubSurfaceFilter::Electronics => project.project_type == ProjectType::Electronics,
    };
    if !type_matches {
        return false;
    }

    let query = model.search_query.trim().to_ascii_lowercase();
    query.is_empty()
        || project.name.to_ascii_lowercase().contains(&query)
        || project
            .path
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains(&query)
}

pub(crate) fn hub_visible_projects(model: &HubSurfaceModel) -> Vec<HubSurfaceProject> {
    model
        .projects
        .iter()
        .filter(|project| hub_project_matches(model, project))
        .cloned()
        .collect()
}

pub(crate) fn hub_visible_recent(model: &HubSurfaceModel) -> Vec<HubSurfaceProject> {
    model
        .recent_activity
        .iter()
        .filter(|project| hub_project_matches(model, project))
        .cloned()
        .collect()
}

pub(crate) fn hub_visible_featured(
    model: &HubSurfaceModel,
    projects: &[HubSurfaceProject],
) -> Option<HubSurfaceProject> {
    model
        .featured_project
        .as_ref()
        .filter(|project| hub_project_matches(model, project))
        .cloned()
        .or_else(|| projects.first().cloned())
}

/// True when the Hub list is showing a filtered subset rather than the whole
/// library. The empty states use this to distinguish "no projects yet" from
/// "this search matched nothing".
pub(crate) fn hub_is_filtering(model: &HubSurfaceModel) -> bool {
    model.filter != HubSurfaceFilter::All || !model.search_query.trim().is_empty()
}

/// Number of project rows visible before the list starts scrolling.
const HUB_LIST_VISIBLE_ROWS: usize = 7;
/// Vertical footprint of one project row in the Hub list.
const HUB_LIST_ROW_HEIGHT: f32 = 43.0;

/// Copy for the showpiece and the list when nothing is visible. The Hub must
/// not claim "no projects" while it is still scanning, nor when a search or
/// filter simply matched nothing.
fn hub_empty_copy(model: &HubSurfaceModel) -> (&'static str, Option<&'static str>) {
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
    let tokens = hub_tokens(palette);
    // Soft warm outline for the primary project-name pill, matching the design
    // reference without competing with the orange primary action.
    let accent_line = [tokens.accent[0], tokens.accent[1], tokens.accent[2], 140];
    // Whole-pill hover wash. It keeps the rounded silhouette intact instead of
    // letting a child control paint a rectangular block.
    let raised_hover = [
        tokens.surface_raised[0].saturating_add(10),
        tokens.surface_raised[1].saturating_add(10),
        tokens.surface_raised[2].saturating_add(10),
        tokens.surface_raised[3],
    ];
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
                    border_width: Some(1.0),
                    fill: Some(tokens.surface_alt),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-card".to_string()),
                UiStylePatch {
                    fill: Some([82, 54, 24, 255]),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
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
                    border_width: Some(1.0),
                    fill: Some(tokens.surface_alt),
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
                UiStyleSelector::Class("hub-window-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.surface),
                    radius: Some(3.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Always),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-window-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-window-close".to_string()),
                UiStylePatch {
                    fill: Some(tokens.danger),
                    border: Some(tokens.danger),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-primary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some([168, 88, 15, 255]),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-primary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    border: Some([168, 88, 15, 255]),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-primary-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
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
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-menu-row".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-menu-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-menu-row-danger".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-menu-row-danger".to_string()),
                UiStylePatch {
                    fill: Some([180, 45, 45, 90]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-menu-divider".to_string()),
                UiStylePatch {
                    fill: Some(tokens.border),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-agent-toggle".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(10.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-agent-toggle".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-agent-toggle-on".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(10.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-agent-toggle-on".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
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
                UiStyleSelector::Class("hub-create-icon-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(5.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-icon-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-icon-button".to_string()),
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
                UiStyleSelector::Class("hub-top-nav".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    text: Some(tokens.text_muted),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-top-nav-active".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(0.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-top-nav".to_string()),
                UiStylePatch {
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-new-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    text: Some(tokens.text),
                    radius: Some(5.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-new-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-new-active".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-form".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-highlight".to_string()),
                UiStylePatch {
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    fill: Some(tokens.surface_raised),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-name-field".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(accent_line),
                    border_width: Some(1.0),
                    radius: Some(10.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-name-hovered".to_string()),
                UiStylePatch {
                    fill: Some(raised_hover),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-name-focused".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(2.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-name-input".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-name-input".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-name-input".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-location".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(10.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-location-hovered".to_string()),
                UiStylePatch {
                    fill: Some(raised_hover),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-path-input".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(0.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-path-input".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-path-input".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-location-button".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(8.0),
                    text: Some(tokens.text_muted),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-location-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-location-button".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-submit".to_string()),
                UiStylePatch {
                    radius: Some(8.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-picker".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(10.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-picker".to_string()),
                UiStylePatch {
                    border: Some(tokens.focus),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Focused),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-picker".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-menu".to_string()),
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
                UiStyleSelector::Class("hub-create-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.focus),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-option-selected".to_string()),
                UiStylePatch {
                    fill: Some(tokens.selection),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-recent".to_string()),
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
                UiStyleSelector::Class("hub-recent-row".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some(tokens.border),
                    border_width: Some(0.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-recent-row".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-recent-kind".to_string()),
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
                UiStyleSelector::Class("hub-recent-menu".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    ..UiStylePatch::default()
                },
            ),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-recent-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.border),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-primary-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    border: Some([255, 190, 96, 255]),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-new-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-create-icon-button".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    text: Some([18, 18, 20, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
            UiStyleRule::new(
                UiStyleSelector::Class("hub-recent-row".to_string()),
                UiStylePatch {
                    fill: Some([82, 54, 24, 255]),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(UiStyleRuleState::Active),
        ],
    };
    surface
}

// Legacy Hub recipes kept as migration reference. The mounted Hub is built
// from the native builders below; these helpers are intentionally not a
// second runtime path and can be retired after visual snapshot coverage.
#[allow(dead_code)]
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
    let tokens = hub_tokens(palette);
    UiStyle {
        fill: tokens.background,
        border: tokens.background,
        text: tokens.text,
        border_width: 0.0,
        radius: 0.0,
        opacity: 1.0,
    }
}

fn hub_topbar_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_header_surface(palette: StudioUiPalette) -> UiNode {
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

fn hub_content_columns_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_workspace_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_create_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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
                    ..UiLayout::fixed(0.0, 52.0)
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

    if model.create_error.is_some() {
        panel = panel.with_child(
            UiNode::new("hub.create-error", UiNodeKind::Label)
                .with_class("hub-create-error")
                .with_text_key("hub.create.error")
                .with_text_style(UiTextStyle::body(tokens.danger))
                .with_layout(UiLayout::fixed(0.0, 44.0)),
        );
    } else if model.create_path_is_project {
        panel = panel.with_child(
            UiNode::new("hub.create-open-hint", UiNodeKind::Label)
                .with_text_key("app.hub_open_existing")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fixed(0.0, 44.0)),
        );
    }
    panel
}

fn hub_create_name_input(_palette: StudioUiPalette, focused: bool, hovered: bool) -> UiNode {
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

fn hub_create_type_picker(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_create_type_option(
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

fn hub_create_location_input(
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

fn hub_agent_toggle(palette: StudioUiPalette, enabled: bool) -> UiNode {
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

fn hub_create_submit(palette: StudioUiPalette, opens_existing: bool) -> UiNode {
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

fn hub_showcase_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_showcase_featured(palette: StudioUiPalette, project: &HubSurfaceProject) -> UiNode {
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

fn hub_showcase_small(
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

fn hub_showcase_empty(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_recent_surface(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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

fn hub_recent_height(model: &HubSurfaceModel) -> f32 {
    let visible_rows = model.projects.len().min(HUB_LIST_VISIBLE_ROWS);
    (66.0 + visible_rows as f32 * HUB_LIST_ROW_HEIGHT).max(326.0)
}

fn hub_recent_menu_button(project: &HubSurfaceProject, row_id: &str) -> UiNode {
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
fn hub_side_column(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
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
fn hub_legacy_create_surface(palette: StudioUiPalette) -> UiNode {
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

#[allow(dead_code)]
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
    confirm_forget: bool,
) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new("hub.context-menu", UiNodeKind::Menu)
        .with_class("hub-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(
            UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::same(6.0),
                gap: 2.0,
                ..UiLayout::absolute(raf_render::api_graphic_basic::ui_surface::UiRect::new(
                    position[0],
                    position[1],
                    HUB_CONTEXT_MENU_SIZE[0],
                    HUB_CONTEXT_MENU_SIZE[1],
                ))
            }
            .with_z_index(100),
        )
        .with_child(
            UiNode::new("hub.context-title", UiNodeKind::Label)
                .with_text_key("hub.context.title")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    padding: UiSpacing::xy(10.0, 0.0),
                    ..UiLayout::fixed(0.0, 26.0)
                }),
        )
        .with_child(hub_menu_divider("hub.context-divider-top"))
        .with_child(hub_menu_row(
            palette,
            "hub.context-open",
            "app.hub_open",
            project,
            "open",
            "editor.hub.context-open",
        ))
        .with_child(hub_menu_row(
            palette,
            "hub.context-agent",
            "agent_workspace.work_with_ai",
            project,
            "agent",
            "editor.hub.agent",
        ))
        .with_child(hub_menu_row(
            palette,
            "hub.context-duplicate",
            "app.hub_duplicate",
            project,
            "duplicate",
            "editor.hub.context-duplicate",
        ))
        .with_child(hub_menu_divider("hub.context-divider-bottom"))
        .with_child(hub_menu_row(
            palette,
            "hub.context-forget",
            if confirm_forget {
                "app.hub_remove_confirm"
            } else {
                "app.hub_delete"
            },
            project,
            "forget",
            "editor.hub.context-forget",
        ))
}

/// One desktop-grade context-menu row: borderless, full-width, with a semantic
/// icon. Resting fill is transparent; hover illumination comes from the Hub
/// stylesheet so the menu reads as a menu rather than a stack of buttons.
fn hub_menu_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    project: &HubSurfaceProject,
    action: &str,
    icon_key: &str,
) -> UiNode {
    let destructive = action == "forget";
    let class = if destructive {
        "hub-menu-row-danger"
    } else {
        "hub-menu-row"
    };
    let text_color = if destructive {
        palette.tokens().danger
    } else {
        palette.tokens().text
    };
    UiNode::new(id, UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(10.0, 0.0),
            basis: [0.0, 32.0],
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            ..UiLayout::default()
        })
        .focusable()
        .with_event(project_action(project, action))
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
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(text_color))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 18.0)
                }),
        )
}

fn hub_menu_divider(id: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_class("hub-menu-divider")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            basis: [0.0, 1.0],
            width_mode: UiSizeMode::Fill,
            height_mode: UiSizeMode::Fixed,
            ..UiLayout::default()
        })
}

/// "Get in" dropdown: the single entry point for creating a project or opening
/// one that lives anywhere on disk.
fn hub_get_in_menu_surface(palette: StudioUiPalette, position: [f32; 2]) -> UiNode {
    UiNode::new("hub.getin-menu", UiNodeKind::Menu)
        .with_class("hub-context-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(
            UiLayout {
                flow: UiFlow::Column,
                padding: UiSpacing::same(6.0),
                gap: 2.0,
                ..UiLayout::absolute(raf_render::api_graphic_basic::ui_surface::UiRect::new(
                    position[0],
                    position[1],
                    HUB_GET_IN_MENU_SIZE[0],
                    HUB_GET_IN_MENU_SIZE[1],
                ))
            }
            .with_z_index(100),
        )
        .with_child(hub_menu_command_row(
            palette,
            "hub.getin.new",
            "app.hub_new_project",
            "hub.getin.new",
            "editor.hub.plus",
        ))
        .with_child(hub_menu_command_row(
            palette,
            "hub.getin.open-folder",
            "app.hub_open_folder",
            "hub.getin.open-folder",
            "editor.hub.folder",
        ))
        .with_child(hub_menu_command_row(
            palette,
            "hub.getin.open-file",
            "app.hub_open_ron",
            "hub.getin.open-file",
            "editor.hub.project-file",
        ))
}

fn hub_menu_command_row(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    icon_key: &str,
) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hub-menu-row")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(10.0, 0.0),
            basis: [0.0, 32.0],
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
            .with_layout(UiLayout::fixed(16.0, 16.0)),
        )
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button(tokens.text))
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 18.0)
                }),
        )
}

fn hub_dialog_overlay(model: &HubSurfaceModel, id: &str, card: UiNode) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(
            UiLayout {
                flow: UiFlow::Column,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                ..UiLayout::absolute(raf_ui::UiRect::new(
                    0.0,
                    0.0,
                    model.window_size[0],
                    model.window_size[1],
                ))
            }
            .with_z_index(110),
        )
        .with_style(UiStyle {
            fill: [0, 0, 0, 180],
            ..UiStyle::transparent()
        })
        .with_child(card)
}

fn hub_dialog_card(palette: StudioUiPalette, id: &str, width: f32) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 12.0,
            padding: UiSpacing::same(24.0),
            basis: [width, 0.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fixed)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            border_width: 1.0,
            radius: 10.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_role(raf_ui::UiAccessibilityRole::Dialog)
}

fn hub_dialog_primary_button(
    palette: StudioUiPalette,
    id: &str,
    label_key: &str,
    command: &str,
    width: f32,
) -> UiNode {
    let tokens = hub_tokens(palette);
    UiNode::new(id, UiNodeKind::Button)
        .with_class("hub-primary-button")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::fixed(width, 32.0)
        })
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::button([18, 18, 20, 255]))
                .with_layout(UiLayout::fit_content()),
        )
        .with_style(UiStyle {
            fill: tokens.accent,
            border: tokens.accent_hot,
            text: [18, 18, 20, 255],
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        })
}

fn hub_adopt_overlay(
    palette: StudioUiPalette,
    model: &HubSurfaceModel,
    adopt: &HubPendingAdopt,
) -> UiNode {
    let tokens = hub_tokens(palette);
    let card = hub_dialog_card(palette, "hub.adopt.card", 540.0)
        .with_accessibility_label_key("app.hub_adopt_title")
        .with_child(
            UiNode::new("hub.adopt.title", UiNodeKind::Label)
                .with_text_key("app.hub_adopt_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hub.adopt.path", UiNodeKind::Label)
                .with_text_value(adopt.path.display().to_string())
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("hub.adopt.type", UiNodeKind::Label)
                .with_text_key("hub.adopt.type")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hub.adopt.note", UiNodeKind::Label)
                .with_text_key("app.hub_adopt_note")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("hub.adopt.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::End,
                    gap: 8.0,
                    ..UiLayout::fit_content()
                })
                .with_child(hub_command_button(
                    "hub.adopt.cancel",
                    "app.cancel",
                    "hub.adopt.cancel",
                    120.0,
                    32.0,
                    "hub-project-menu",
                ))
                .with_child(hub_dialog_primary_button(
                    palette,
                    "hub.adopt.confirm",
                    "app.hub_adopt_confirm",
                    "hub.adopt.confirm",
                    170.0,
                )),
        );
    hub_dialog_overlay(model, "hub.adopt.overlay", card)
}

fn hub_open_error_overlay(
    palette: StudioUiPalette,
    model: &HubSurfaceModel,
    error: &str,
) -> UiNode {
    let tokens = hub_tokens(palette);
    let card = hub_dialog_card(palette, "hub.open-error.card", 520.0)
        .with_accessibility_label_key("app.hub_open_error_title")
        .with_child(
            UiNode::new("hub.open-error.title", UiNodeKind::Label)
                .with_text_key("app.hub_open_error_title")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hub.open-error.detail", UiNodeKind::Label)
                .with_text_value(error.to_string())
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("hub.open-error.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::End,
                    ..UiLayout::fit_content()
                })
                .with_child(hub_command_button(
                    "hub.open-error.close",
                    "app.hub_close",
                    "hub.open-error.close",
                    110.0,
                    32.0,
                    "hub-project-menu",
                )),
        );
    hub_dialog_overlay(model, "hub.open-error.overlay", card)
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn filter_class(target: HubSurfaceFilter, active: HubSurfaceFilter) -> &'static str {
    if target == active {
        "hub-filter-active"
    } else {
        "hub-button"
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn hub_project_card(
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

fn project_kind_icon_key(project: &HubSurfaceProject) -> &'static str {
    match project.project_type {
        ProjectType::Game => "editor.hub.kind-game",
        ProjectType::Electronics => "editor.hub.kind-electronics",
    }
}

#[allow(dead_code)]
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

fn project_hover_event(project: &HubSurfaceProject, event: UiEventKind) -> UiEventBinding {
    let action = match &event {
        UiEventKind::HoverEnter => "hover",
        UiEventKind::HoverLeave => "leave",
        _ => return project_menu_event(project, event),
    };
    UiEventBinding {
        event,
        action: UiAction::Custom {
            channel: "hub.project".to_string(),
            payload: json!({
                "action": action,
                "path": project.path.to_string_lossy(),
            }),
        },
    }
}

fn hub_project_opening(palette: StudioUiPalette, model: &HubSurfaceModel) -> UiNode {
    let tokens = hub_tokens(palette);
    let failed = model.opening_error.is_some();
    let mut card = UiNode::new("hub.open.card", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 14.0,
            padding: UiSpacing::same(24.0),
            basis: [440.0, 0.0],
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fixed)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            border_width: 1.0,
            radius: 10.0,
            ..UiStyle::transparent()
        })
        .with_accessibility_role(raf_ui::UiAccessibilityRole::Dialog)
        .with_accessibility_label_key(if failed {
            "agent_workspace.load_failed"
        } else {
            "agent_workspace.loading"
        });
    if !failed {
        card = card.with_child(crate::agent_workspace_surface::loading_ring(
            palette,
            model.opening_step,
        ));
    }
    card = card
        .with_child(
            UiNode::new("hub.open.title", UiNodeKind::Label)
                .with_text_key(if failed {
                    "agent_workspace.load_failed"
                } else {
                    "agent_workspace.loading"
                })
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("hub.open.project", UiNodeKind::Label)
                .with_text_value(model.opening_project.clone().unwrap_or_default())
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    if failed {
        card = card.with_child(
            UiNode::new("hub.open.error", UiNodeKind::Label)
                .with_text_key("hub.open.error")
                .with_text_overflow(raf_ui::UiTextOverflow::Wrap)
                .with_text_style(UiTextStyle::body(tokens.warning))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
        );
    }
    let mut actions = UiNode::new("hub.open.actions", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 8.0,
            ..UiLayout::fit_content()
        })
        .with_child(hub_command_button(
            "hub.open.cancel",
            "app.cancel",
            "hub.open.cancel",
            110.0,
            32.0,
            "hub-project-menu",
        ));
    if failed {
        actions = actions.with_child(hub_command_button(
            "hub.open.retry",
            "agent_workspace.retry",
            "hub.open.retry",
            110.0,
            32.0,
            "hub-secondary-button",
        ));
    }
    UiNode::new("hub.open.overlay", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            ..UiLayout::absolute(raf_ui::UiRect::new(
                0.0,
                0.0,
                model.opening_size[0],
                model.opening_size[1],
            ))
            .with_z_index(120)
        })
        .with_style(UiStyle {
            fill: [0, 0, 0, 180],
            ..UiStyle::transparent()
        })
        .with_child(card.with_child(actions))
}

#[cfg(test)]
mod project_opening_tests {
    use super::*;
    #[test]
    fn agent_workspace_hub_loading_is_not_an_agents_surface() {
        let mut model = HubSurfaceModel {
            opening_project: Some("Game".into()),
            opening_size: [900.0, 700.0],
            ..Default::default()
        };
        for failed in [false, true] {
            model.opening_error = failed.then(|| "agent_workspace.project_busy".into());
            let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);
            let mut session =
                raf_render::api_graphic_basic::ui_surface::UiSurfaceSession::default();
            let frame =
                session.build_frame_with_resolved_text(&surface, 900, 700, [0, 0, 0, 255], |key| {
                    crate::native_studio::resolve_hub_text(key, &model, raf_core::Language::Spanish)
                });
            assert!(frame
                .layout_boxes
                .iter()
                .any(|item| item.id == "hub.open.overlay"));
            assert!(frame
                .layout_boxes
                .iter()
                .any(|item| item.id == "hub.open.cancel"));
            assert_eq!(
                frame
                    .layout_boxes
                    .iter()
                    .any(|item| item.id == "hub.open.retry"),
                failed
            );
            assert!(!frame
                .layout_boxes
                .iter()
                .any(|item| item.id == "agent.workspace.root"));
        }
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

fn hub_top_nav_button(
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
                    options: Default::default(),
                    source: UiImageSource::new(image_key),
                    fit: UiImageFit::Contain,
                    tint: None,
                },
            )
            .with_layout(UiLayout::fixed(18.0, 18.0)),
        )
}

fn hub_window_button(id: &str, command: &str, image_key: &str) -> UiNode {
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
    fn rafui_regression_hub_type_picker_uses_a_registered_semantic_chevron() {
        let picker =
            hub_create_type_picker(StudioUiPalette::IndustrialDark, &HubSurfaceModel::default());
        let chevron = picker.find("hub.create.type-chevron").unwrap();
        assert_eq!(
            chevron.icon.map(|icon| icon.id),
            Some(UiIconId::ChevronDown)
        );
        assert_eq!(chevron.control, raf_ui::UiControl::None);
    }

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
    fn hub_surface_uses_creation_and_recent_controls() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let frame = surface.build_frame(520, 720, [8, 8, 8, 255]);

        assert!(frame
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "hub.create-form"));
        assert!(frame
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "hub.recent"));
    }

    #[test]
    fn hub_places_creation_before_showcase_and_recent_content() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let frame = surface.build_frame(1_000, 760, [8, 11, 15, 255]);

        let header = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.header")
            .expect("workspace header");
        let create = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create-form")
            .expect("create panel");
        let showcase = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.showcase")
            .expect("showcase panel");
        let recent = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.recent")
            .expect("recent panel");

        assert!(create.rect.y >= header.rect.bottom());
        assert!(showcase.rect.y >= create.rect.bottom());
        assert!(recent.rect.y >= showcase.rect.bottom());
    }

    #[test]
    fn hub_centers_workspace_at_its_maximum_content_width() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let frame = surface.build_frame(1_920, 1_080, [8, 11, 15, 255]);
        let workspace = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.main-column")
            .expect("workspace column");

        assert_eq!(workspace.rect.width, 1_280.0);
        assert_eq!(workspace.rect.x, 320.0);
    }

    #[test]
    fn hub_project_cards_expose_a_visible_hover_state() {
        let project = HubSurfaceProject {
            name: "Demo".to_string(),
            path: PathBuf::from("C:/Projects/Demo"),
            project_type: ProjectType::Game,
            last_opened_label: "14/07/2026".to_string(),
        };
        let electronics = HubSurfaceProject {
            name: "Board".to_string(),
            path: PathBuf::from("C:/Projects/Board"),
            project_type: ProjectType::Electronics,
            last_opened_label: "13/07/2026".to_string(),
        };
        let model = HubSurfaceModel {
            projects: vec![project.clone(), electronics.clone()],
            featured_project: Some(project.clone()),
            recent_activity: vec![project, electronics.clone(), electronics],
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);
        let mut session = UiSurfaceSession::default();
        let frame = session.build_frame(&surface, 1_300, 900, [8, 11, 15, 255]);
        let small = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.showcase-small.0")
            .expect("showcase card");

        let _ = session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some([
                    small.rect.x + small.rect.width * 0.5,
                    small.rect.y + small.rect.height * 0.5,
                ]),
                ..UiInputState::default()
            },
        );
        assert_eq!(
            session.interaction.focus.hovered.as_deref(),
            Some("hub.showcase-small.0")
        );
        let hovered_frame = session.build_frame(&surface, 1_300, 900, [8, 11, 15, 255]);
        let hovered = hovered_frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.showcase-small.0")
            .expect("hovered showcase card");

        assert_eq!(
            hovered.style.border,
            hub_tokens(StudioUiPalette::IndustrialDark).accent
        );
        assert_eq!(hovered.style.border_width, 1.0);
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
            .find(|layout| layout.id == "hub.recent.0.menu")
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
            Some("hub.recent.0.menu")
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
    fn hub_keeps_recent_project_menu_available_in_each_row() {
        let project = HubSurfaceProject {
            name: "Demo".to_string(),
            path: PathBuf::from("C:/Projects/Demo"),
            project_type: ProjectType::Game,
            last_opened_label: "14/07/2026".to_string(),
        };
        let model = HubSurfaceModel {
            projects: vec![project.clone()],
            ..HubSurfaceModel::default()
        };

        let frame = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model)
            .build_frame(1_300, 900, [8, 11, 15, 255]);

        assert!(frame
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "hub.recent.0.menu"));
    }

    #[test]
    fn hub_uses_top_navigation_instead_of_the_legacy_rail() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let topbar = surface
            .root
            .children
            .iter()
            .find(|child| child.id == "hub.topbar")
            .expect("hub topbar");

        assert_eq!(topbar.kind, UiNodeKind::Toolbar);
        assert!(!matches!(
            topbar.control,
            raf_render::api_graphic_basic::ui_surface::UiControl::ScrollView { .. }
        ));
        assert!(!surface
            .root
            .children
            .iter()
            .any(|child| child.id == "hub.navigation"));
    }

    #[test]
    fn hub_create_type_menu_has_no_dead_space() {
        let model = HubSurfaceModel {
            create_type_menu_open: true,
            ..HubSurfaceModel::default()
        };
        let frame = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model)
            .build_frame(1_300, 900, [8, 11, 15, 255]);

        let menu = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create.type-menu")
            .expect("type menu box");
        assert_eq!(menu.rect.height, 84.0);
        assert_eq!(menu.rect.width, 156.0);

        let game = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create.type-game")
            .expect("game option box");
        let electronics = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create.type-electronics")
            .expect("electronics option box");
        assert_eq!(game.rect.height, 42.0);
        assert_eq!(electronics.rect.height, 42.0);
        assert!(game.rect.width > 120.0);
        assert!(electronics.rect.width > 120.0);
    }

    #[test]
    fn hub_window_buttons_use_compact_centered_hit_areas() {
        let frame = build_hub_surface(StudioUiPalette::IndustrialDark).build_frame(
            1_300,
            900,
            [8, 11, 15, 255],
        );

        for id in ["hub.minimize", "hub.maximize", "hub.close"] {
            let button = frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == id)
                .unwrap_or_else(|| panic!("{id} box"));
            assert_eq!(button.rect.width, 32.0);
            assert_eq!(button.rect.height, 32.0);
        }
    }

    #[test]
    fn hub_create_row_keeps_aligned_controls() {
        let frame = build_hub_surface(StudioUiPalette::IndustrialDark).build_frame(
            1_300,
            900,
            [8, 11, 15, 255],
        );

        let picker = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create.type-picker")
            .expect("type picker box");
        let submit = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create.submit")
            .expect("submit box");
        let folder = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create.choose-path")
            .expect("folder button box");

        assert_eq!(picker.rect.height, 52.0);
        assert_eq!(submit.rect.height, 52.0);
        assert_eq!(folder.rect.height, 44.0);
        assert_eq!(folder.rect.width, 40.0);
    }

    #[test]
    fn hub_create_row_is_a_set_of_separate_pills() {
        let frame = build_hub_surface(StudioUiPalette::IndustrialDark).build_frame(
            1_300,
            900,
            [8, 11, 15, 255],
        );
        let rect = |id: &str| {
            frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == id)
                .unwrap_or_else(|| panic!("{id} box"))
                .rect
        };

        let name = rect("hub.create.name");
        let name_field = rect("hub.create.name-field");
        let picker = rect("hub.create.type-picker");
        let location = rect("hub.create.location-row");
        let agent = rect("hub.create.agent");
        let submit = rect("hub.create.submit");

        assert!(picker.x >= name.right() + 10.0);
        assert!((picker.x - name_field.right() - 12.0).abs() < 0.6);
        assert!(name_field.width > 420.0);
        assert!(name.width > 350.0);
        assert!(location.x >= picker.right() + 10.0);
        assert!(agent.x >= location.right() + 10.0);
        assert!(submit.x >= agent.right() + 10.0);
        assert_eq!(location.width, 200.0);
        assert_eq!(submit.width, 132.0);
    }

    #[test]
    fn hub_create_name_field_uses_the_ai_sparkle() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let serialized = format!("{:?}", surface.root);

        assert!(serialized.contains("hub.create.name-sparkle"));
        assert!(serialized.contains("editor.hub.sparkle"));
    }

    #[test]
    fn hub_create_submit_uses_the_crisp_arrow_asset() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let serialized = format!("{:?}", surface.root);
        assert!(
            !serialized.contains("hub.create.submit-icon"),
            "the stale low-resolution arrow node must not remain"
        );
        assert!(
            !serialized.contains("editor.hub.arrow-right"),
            "stale arrow key must not remain in the hub document"
        );
        assert!(serialized.contains("hub.create.submit-arrow"));
        assert!(serialized.contains("editor.hub.submit-arrow"));
    }

    #[test]
    fn hub_create_submit_label_is_bold_for_hierarchy() {
        use raf_render::api_graphic_basic::ui_surface::{UiStyleRuleState, UiStyleSelector};
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let rule = surface
            .style_sheet
            .rules
            .iter()
            .find(|rule| {
                matches!(
                    &rule.selector,
                    UiStyleSelector::Class(name) if name == "hub-primary-button"
                ) && matches!(rule.state, UiStyleRuleState::Always)
            })
            .expect("primary button rule");
        assert_eq!(rule.patch.border, Some([168, 88, 15, 255]));
    }

    #[test]
    fn hub_primary_button_uses_dark_ink_for_contrast() {
        use raf_render::api_graphic_basic::ui_surface::{UiStyleRuleState, UiStyleSelector};
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let rule = surface
            .style_sheet
            .rules
            .iter()
            .find(|rule| {
                matches!(
                    &rule.selector,
                    UiStyleSelector::Class(name) if name == "hub-primary-button"
                ) && matches!(rule.state, UiStyleRuleState::Always)
            })
            .expect("primary button rule");

        assert_eq!(rule.patch.text, Some([18, 18, 20, 255]));
        assert_eq!(rule.patch.radius, Some(4.0));
    }

    fn hub_test_project(name: &str) -> HubSurfaceProject {
        HubSurfaceProject {
            name: name.to_string(),
            path: PathBuf::from(format!("C:/Projects/{name}")),
            project_type: ProjectType::Game,
            last_opened_label: "14/07/2026".to_string(),
        }
    }

    #[test]
    fn hub_search_without_matches_reports_no_results_instead_of_missing_projects() {
        let project = hub_test_project("Demo");
        let model = HubSurfaceModel {
            projects: vec![project.clone()],
            featured_project: Some(project),
            search_query: "zzz".to_string(),
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

        let empty_title = surface
            .root
            .find("hub.showcase-empty-title")
            .expect("showcase empty title");
        assert_eq!(
            empty_title.text_key.as_deref(),
            Some("app.hub_no_results_title")
        );
        let list_empty = surface
            .root
            .find("hub.recent-empty")
            .expect("list empty copy");
        assert_eq!(
            list_empty.text_key.as_deref(),
            Some("app.hub_no_results_subtitle")
        );
    }

    #[test]
    fn hub_loading_state_waits_for_discovery_before_reporting_empty() {
        let model = HubSurfaceModel {
            discovery_pending: true,
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

        let empty = surface
            .root
            .find("hub.recent-empty")
            .expect("list loading copy");
        assert_eq!(empty.text_key.as_deref(), Some("app.hub_loading_projects"));
    }

    #[test]
    fn hub_create_row_offers_open_when_the_path_is_a_project() {
        let model = HubSurfaceModel {
            create_path_is_project: true,
            create_path: "C:/Projects/Demo".to_string(),
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

        let label = surface
            .root
            .find("hub.create.submit-label")
            .expect("submit label");
        assert_eq!(label.text_key.as_deref(), Some("app.hub_open"));
        assert!(surface.root.find("hub.create-open-hint").is_some());

        let frame = surface.build_frame(1_300, 900, [8, 11, 15, 255]);
        let form = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.create-form")
            .expect("create form");
        assert_eq!(form.rect.height, 102.0);
    }

    #[test]
    fn hub_context_menu_confirms_destructive_removal_before_acting() {
        let project = hub_test_project("Demo");
        let model = HubSurfaceModel {
            projects: vec![project.clone()],
            context_project: Some(project),
            context_menu_position: Some([12.0, 12.0]),
            context_confirm_forget: true,
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

        let label = surface
            .root
            .find("hub.context-forget.label")
            .expect("forget row label");
        assert_eq!(label.text_key.as_deref(), Some("app.hub_remove_confirm"));
    }

    #[test]
    fn hub_list_bounds_its_height_and_scrolls_large_libraries() {
        let projects: Vec<_> = (0..20)
            .map(|index| hub_test_project(&format!("Project {index}")))
            .collect();
        let model = HubSurfaceModel {
            projects,
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);
        let frame = surface.build_frame(1_300, 900, [8, 11, 15, 255]);

        let panel = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "hub.recent")
            .expect("projects panel");
        assert_eq!(
            panel.rect.height,
            66.0 + HUB_LIST_VISIBLE_ROWS as f32 * 43.0
        );
        assert!(surface.root.find("hub.recent-scroll").is_some());
        assert!(frame
            .layout_boxes
            .iter()
            .any(|layout| layout.id == "__rafui.scrollbar.hub.recent-scroll.track"));
    }

    #[test]
    fn hub_top_action_is_the_get_in_menu_trigger() {
        let surface = build_hub_surface(StudioUiPalette::IndustrialDark);
        let trigger = surface.root.find("hub.new").expect("get in trigger");
        let label = trigger.find("hub.new.label").expect("trigger label");

        assert_eq!(label.text_key.as_deref(), Some("app.hub_get_in"));
        assert!(trigger.find("hub.new.chevron").is_some());
    }

    #[test]
    fn hub_get_in_menu_offers_create_and_open_entries() {
        let model = HubSurfaceModel {
            get_in_menu_open: true,
            get_in_menu_position: Some([40.0, 40.0]),
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

        for id in [
            "hub.getin.new",
            "hub.getin.open-folder",
            "hub.getin.open-file",
        ] {
            let row = surface.root.find(id).unwrap_or_else(|| panic!("{id} row"));
            assert_eq!(row.kind, UiNodeKind::Button);
        }
    }

    #[test]
    fn hub_adopt_overlay_confirms_before_creating_a_project() {
        let model = HubSurfaceModel {
            pending_adopt: Some(HubPendingAdopt {
                path: PathBuf::from("C:/Projects/Imported"),
                name: "Imported".to_string(),
                project_type: ProjectType::Game,
            }),
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

        assert!(surface.root.find("hub.adopt.overlay").is_some());
        assert_eq!(
            surface.root.find("hub.adopt.confirm").map(|node| node.kind),
            Some(UiNodeKind::Button)
        );
        assert_eq!(
            surface.root.find("hub.adopt.cancel").map(|node| node.kind),
            Some(UiNodeKind::Button)
        );
        let path = surface.root.find("hub.adopt.path").expect("adopt path");
        assert_eq!(path.text_value.as_deref(), Some("C:/Projects/Imported"));
        let kind = surface.root.find("hub.adopt.type").expect("adopt kind");
        assert_eq!(kind.text_key.as_deref(), Some("hub.adopt.type"));
    }

    #[test]
    fn hub_open_error_overlay_keeps_the_reason_readable() {
        let detail = "Rafi could not read this project: bad ron";
        let model = HubSurfaceModel {
            pending_open_error: Some(detail.to_string()),
            ..HubSurfaceModel::default()
        };
        let surface = build_hub_surface_with_model(StudioUiPalette::IndustrialDark, &model);

        assert!(surface.root.find("hub.open-error.overlay").is_some());
        let reason = surface
            .root
            .find("hub.open-error.detail")
            .expect("error detail");
        assert_eq!(reason.text_value.as_deref(), Some(detail));
        assert!(surface.root.find("hub.open-error.close").is_some());
    }
}
