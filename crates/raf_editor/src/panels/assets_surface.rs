//! Retained RafUI document for the editor Assets browser.
//!
//! The panel is declarative only: it renders rows, toolbars and overlay
//! documents from [`AssetsSurfaceParams`] and emits commands. Filesystem work,
//! focus and transient state live in the host and in the application boundary.

#[path = "assets_surface/catalog.rs"]
mod catalog;
#[path = "assets_surface/create.rs"]
mod create;
#[path = "assets_surface/menus.rs"]
mod menus;
#[path = "assets_surface/modals.rs"]
mod modals;
#[path = "assets_surface/preview.rs"]
mod preview;
#[path = "assets_surface/search.rs"]
mod search;
#[cfg(test)]
#[path = "assets_surface/tests.rs"]
mod tests;

pub use catalog::{
    asset_absolute_path, asset_display_name, asset_file_name, asset_rows_with_builtins,
    asset_visible_range, format_row_name, is_builtin_asset_row, visible_asset_rows,
    BUILTIN_ASSET_ROWS,
};
pub(crate) use catalog::asset_scroll_projection_changed;
pub use create::{
    build_assets_create_menu_surface, build_assets_file_popover_surface,
    build_assets_primitive_popover_surface, build_assets_script_popover_surface,
};
pub use menus::{
    assets_overlay_style_sheet, build_assets_backdrop_surface, build_assets_context_menu_surface,
};
pub use modals::{
    build_assets_delete_modal_surface, build_assets_open_modal_surface,
    build_assets_rename_modal_surface,
};

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiCompactMode, UiFlow, UiLayout, UiNode, UiNodeKind, UiOverflow, UiScrollAxis,
    UiSpacing, UiSurface,
};

use crate::panels::editor_bottom_dock_styles::bottom_style_sheet;

use self::catalog::{
    asset_card, asset_status_strip, asset_toolbar, asset_virtual_spacer_height,
    selection_action_bar, spacer,
};
use self::preview::{append_asset_actions, decorate_asset_card, find_node_mut};
use self::search::{asset_sidebar, empty_state};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetFilter {
    All,
    Images,
    Models,
    Audio,
    Scripts,
    Interfaces,
    Animations,
}

impl AssetFilter {
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::All => "app.assets_category_all",
            Self::Images => "app.images",
            Self::Models => "app.models",
            Self::Audio => "app.audio",
            Self::Scripts => "app.scripts_filter",
            Self::Interfaces => "ui.designer.title",
            Self::Animations => "animation.tab",
        }
    }

    pub const fn empty_title_key(self) -> &'static str {
        match self {
            Self::All => "app.no_assets",
            Self::Images => "app.assets_empty_images",
            Self::Models => "app.assets_empty_models",
            Self::Audio => "app.assets_empty_audio",
            Self::Scripts => "app.assets_empty_scripts",
            Self::Interfaces => "ui.designer.empty",
            Self::Animations => "animation.empty_library",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetSort {
    NameAsc,
    NameDesc,
}

impl AssetSort {
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::NameAsc => "app.assets_sort_asc",
            Self::NameDesc => "app.assets_sort_desc",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetViewMode {
    Grid,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetsStatusKind {
    Pending,
    Success,
    Error,
}

/// Transient, already translated feedback line rendered above the asset grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetsStatus {
    pub kind: AssetsStatusKind,
    pub text: String,
}

impl AssetsStatus {
    pub fn pending(text: impl Into<String>) -> Self {
        Self {
            kind: AssetsStatusKind::Pending,
            text: text.into(),
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            kind: AssetsStatusKind::Success,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            kind: AssetsStatusKind::Error,
            text: text.into(),
        }
    }
}

/// A filesystem operation the host is waiting to observe in the catalog.
///
/// The panel never claims success up front: the workbench compares the fresh
/// catalog against this expectation and only then publishes a Success or
/// Error status.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetsOperation {
    pub expected_row: String,
    pub expect_present: bool,
    pub started_seconds: f64,
    pub catalog_revision: u64,
    /// Locale key reported when the catalog confirms the change.
    pub success_key: &'static str,
    /// Locale key reported when the change never shows up.
    pub error_key: &'static str,
}

/// Normalized motion sample shared by every Assets overlay and the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetsMotion {
    /// 0.0 when overlays are closed, 1.0 when fully shown.
    pub overlay: f32,
    /// 0.0 -> 1.0 when the visible category changes.
    pub view: f32,
    /// 0.0 -> 1.0 while a just-changed row is highlighted.
    pub highlight: f32,
}

impl Default for AssetsMotion {
    fn default() -> Self {
        Self {
            overlay: 0.0,
            view: 1.0,
            highlight: 0.0,
        }
    }
}

/// All state the Assets panel needs to be rebuilt. Keeping the surface free of
/// reads from the host makes the panel reproducible from data alone.
pub struct AssetsSurfaceParams<'a> {
    pub palette: StudioUiPalette,
    /// Rows including the virtual built-in primitive rows.
    pub rows: &'a [String],
    pub language: raf_core::Language,
    pub query: &'a str,
    pub filter: AssetFilter,
    /// Lowercase extension without the dot; empty means every language.
    pub script_extension: &'a str,
    pub sort: AssetSort,
    pub view: AssetViewMode,
    pub selected: Option<&'a str>,
    pub highlight: Option<&'a str>,
    pub status: Option<&'a AssetsStatus>,
    pub pending: bool,
    pub catalog_error: Option<&'a str>,
    pub visible_range: Option<(usize, usize)>,
    pub create_open: bool,
    pub motion: AssetsMotion,
    pub visuals: Option<&'a std::collections::HashMap<String, AssetVisual>>,
}

pub struct AssetVisual {
    pub entry: raf_assets::authoring_catalog::AuthoringAsset,
    pub preview: crate::asset_previews::PreviewState,
    pub can_apply_texture: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssetsIntent {
    QueryChanged(String),
    FilterChanged(AssetFilter),
    OpenFolder,
    Refresh,
    OpenAsset(String),
    BeginDrag(String),
    DropAsset { path: String, position: [f32; 2] },
    CreateScript { language: String, name: String },
    CreateFile { name: String },
    CreatePrimitive(raf_core::scene::Primitive),
}

const ASSET_CARD_WIDTH: f32 = 116.0;
const ASSET_CARD_HEIGHT: f32 = 164.0;
const ASSET_CARD_GAP: f32 = 6.0;
const ASSET_LIST_ROW_HEIGHT: f32 = 34.0;
const ASSET_LIST_ROW_GAP: f32 = 2.0;
const ASSET_SIDEBAR_WIDTH: f32 = 184.0;
pub const ASSET_TOOLBAR_HEIGHT: f32 = 34.0;
pub const ASSET_SELECTION_BAR_HEIGHT: f32 = 34.0;
/// Smallest grid viewport the virtualization will trust. A very short panel
/// keeps one full screen of rows instead of collapsing to a sliver.
pub const ASSET_CARD_MIN_VIEWPORT: f32 = 96.0;

/// Window-level geometry for the Assets overlays. Heights are generous upper
/// bounds: the host wrapper clips, so a too small rect would cut a menu while
/// an extra few pixels only show transparent padding.
pub const ASSETS_MENU_WIDTH: f32 = 214.0;
pub const ASSETS_CREATE_MENU_HEIGHT: f32 = 188.0;
pub const ASSETS_CONTEXT_MENU_HEIGHT: f32 = 256.0;
pub const ASSETS_CONTEXT_MENU_BUILTIN_HEIGHT: f32 = 76.0;
pub const ASSETS_POPOVER_WIDTH: f32 = 300.0;
pub const ASSETS_SCRIPT_POPOVER_HEIGHT: f32 = 164.0;
pub const ASSETS_FILE_POPOVER_HEIGHT: f32 = 112.0;
pub const ASSETS_PRIMITIVE_POPOVER_HEIGHT: f32 = 148.0;
pub const ASSETS_MODAL_WIDTH: f32 = 420.0;
/// Width reserved for the label and hint column of a modal option row.
pub const ASSETS_MODAL_TEXT_WIDTH: f32 = 232.0;
pub const ASSETS_OPEN_MODAL_HEIGHT: f32 = 400.0;
pub const ASSETS_RENAME_MODAL_HEIGHT: f32 = 210.0;
pub const ASSETS_DELETE_MODAL_HEIGHT: f32 = 168.0;

/// Row pitch used by virtualization. Grid cards are much taller than list rows,
/// so the estimate must follow the active view mode.
const ASSET_CARD_PITCH: f32 = ASSET_CARD_HEIGHT + ASSET_CARD_GAP;
const ASSET_LIST_PITCH: f32 = ASSET_LIST_ROW_HEIGHT + ASSET_LIST_ROW_GAP;

pub fn build_assets_surface(params: AssetsSurfaceParams<'_>) -> UiSurface {
    let AssetsSurfaceParams {
        palette,
        rows,
        language,
        query,
        filter,
        script_extension,
        sort,
        view,
        selected,
        highlight,
        status,
        pending,
        catalog_error,
        visible_range,
        create_open,
        motion,
        visuals,
    } = params;

    let filtered_rows = visible_asset_rows(rows, query, filter, script_extension, sort);
    let total_rows = filtered_rows.len();
    let (start, end) = visible_range
        .map(|(start, end)| {
            let start = start.min(total_rows);
            (start, end.min(total_rows).max(start))
        })
        .unwrap_or((0, total_rows));

    let mut grid = UiNode::scroll_view("assets.grid", UiScrollAxis::Vertical)
        .with_class(if view == AssetViewMode::Grid {
            "assets-grid"
        } else {
            "assets-grid assets-grid-list"
        })
        .with_layout(UiLayout {
            flow: if view == AssetViewMode::Grid {
                UiFlow::Row
            } else {
                UiFlow::Column
            },
            grow: 1.0,
            gap: if view == AssetViewMode::Grid {
                ASSET_CARD_GAP
            } else {
                ASSET_LIST_ROW_GAP
            },
            padding: UiSpacing::xy(12.0, 12.0),
            overflow: if view == AssetViewMode::Grid {
                UiOverflow::ScrollY
            } else {
                UiOverflow::ScrollY
            },
            compact: if view == AssetViewMode::Grid {
                UiCompactMode::Wrap
            } else {
                UiCompactMode::None
            },
            ..UiLayout::default()
        });
    if start > 0 {
        grid = grid.with_child(spacer(
            "assets.top-spacer",
            asset_virtual_spacer_height(start, view),
        ));
    }
    for (source_index, row) in filtered_rows
        .iter()
        .enumerate()
        .skip(start)
        .take(end - start)
    {
        let card = asset_card(
            palette,
            source_index,
            row.as_str(),
            view,
            selected == Some(row.as_str()),
            highlight == Some(row.as_str()),
            motion,
        );
        grid = grid.with_child(decorate_asset_card(
            card,
            visuals.and_then(|v| v.get(row.as_str())),
            view,
            language,
            palette,
        ));
    }
    if end < total_rows {
        grid = grid.with_child(spacer(
            "assets.bottom-spacer",
            asset_virtual_spacer_height(total_rows - end, view),
        ));
    }
    if filtered_rows.is_empty() {
        grid = grid.with_child(empty_state(palette, rows.is_empty(), query, filter));
    }

    let mut root = UiNode::new("assets.root", UiNodeKind::Panel)
        .with_class("bottom-panel")
        .with_layout(UiLayout::fill(UiFlow::Row))
        .with_style(palette.panel_style())
        .with_child(asset_sidebar(
            palette,
            language,
            filter,
            script_extension,
            rows.len(),
        ))
        .with_child(
            UiNode::new("assets.content", UiNodeKind::Panel)
                .with_class("assets-content")
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(asset_toolbar(
                    palette,
                    filter,
                    sort,
                    view,
                    create_open,
                    pending,
                    total_rows,
                    rows.len(),
                ))
                .with_child(asset_status_strip(
                    palette,
                    language,
                    status,
                    pending,
                    catalog_error,
                ))
                .with_child(grid)
                .with_child(selection_action_bar(
                    palette,
                    selected.unwrap_or_default(),
                    highlight,
                )),
        );

    if let Some(visual) = selected.and_then(|row| visuals.and_then(|v| v.get(row))) {
        if let Some(bar) = find_node_mut(&mut root, "assets.selection") {
            append_asset_actions(bar, visual, palette);
        }
    }
    let mut surface = UiSurface::new("editor.bottom.assets", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}
