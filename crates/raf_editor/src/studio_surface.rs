//! Retained native-studio surface blueprint.
//!
//! This is intentionally a renderer-independent editor shell definition. The
//! retained panels are composed by the native Winit host and ApiGraphicBasic.

mod actions;
mod hub;
mod layout;
mod overlays;
mod project_cards;
mod recent;
mod styles;
#[cfg(test)]
mod tests;

use raf_core::config::Theme;
use raf_core::project::{EditorViewMode, ProjectType};
use raf_render::api_graphic_basic::ui_surface::{StudioUiPalette, UiSurface};
use std::path::PathBuf;

pub(crate) use actions::*;
pub(crate) use hub::*;
pub(crate) use layout::*;
pub(crate) use overlays::*;
pub(crate) use project_cards::*;
pub(crate) use recent::*;
pub(crate) use styles::*;

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
    pub create_advanced_open: bool,
    pub create_initial_view: EditorViewMode,
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
            create_advanced_open: false,
            create_initial_view: EditorViewMode::View3d,
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
    let mut surface = UiSurface::new("studio.hub", palette, hub_root_surface(palette, model));
    surface.style_sheet = hub_style_sheet(palette);
    surface
}

pub fn build_studio_surface(palette: StudioUiPalette) -> UiSurface {
    let mut surface = UiSurface::new("studio.editor", palette, studio_root());
    surface.style_sheet = studio_style_sheet(palette);
    surface
}
