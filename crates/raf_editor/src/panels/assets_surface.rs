//! Retained RafUI document for the editor Assets browser.
//!
//! The panel is declarative only: it renders rows, toolbars and overlay
//! documents from [`AssetsSurfaceParams`] and emits commands. Filesystem work,
//! focus and transient state live in the host and in the application boundary.

use std::path::Path;

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAction, UiAlign, UiCompactMode, UiEventBinding,
    UiEventKind, UiFlow, UiFontWeight, UiIcon, UiIconId, UiIconSize, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiStylePatch, UiStyleRule,
    UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface, UiSurfaceMaterial, UiTextOverflow,
    UiTextRole, UiTextStyle,
};

use crate::panels::editor_bottom_dock_styles::bottom_style_sheet;
use crate::panels::primitive_create::{
    primitive_key, primitive_name, primitive_slug, CREATEABLE_PRIMITIVES,
};
use crate::script_support::is_script_file;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetFilter {
    All,
    Images,
    Models,
    Audio,
    Scripts,
}

impl AssetFilter {
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::All => "app.assets_category_all",
            Self::Images => "app.images",
            Self::Models => "app.models",
            Self::Audio => "app.audio",
            Self::Scripts => "app.scripts_filter",
        }
    }

    pub const fn empty_title_key(self) -> &'static str {
        match self {
            Self::All => "app.no_assets",
            Self::Images => "app.assets_empty_images",
            Self::Models => "app.assets_empty_models",
            Self::Audio => "app.assets_empty_audio",
            Self::Scripts => "app.assets_empty_scripts",
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

/// Virtual assets that are always available in a Game project. They are
/// authoring shortcuts, not files on disk; opening or dropping one still
/// executes the real scene-creation path in the application boundary.
pub const BUILTIN_ASSET_ROWS: [&str; 4] = [
    "builtin://primitive/cube",
    "builtin://primitive/sphere",
    "builtin://primitive/cylinder",
    "builtin://primitive/plane",
];

pub fn asset_rows_with_builtins(asset_rows: &[String]) -> Vec<String> {
    BUILTIN_ASSET_ROWS
        .iter()
        .map(|row| (*row).to_string())
        .chain(asset_rows.iter().cloned())
        .collect()
}

const ASSET_CARD_WIDTH: f32 = 116.0;
const ASSET_CARD_HEIGHT: f32 = 116.0;
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
pub const ASSETS_CREATE_MENU_HEIGHT: f32 = 104.0;
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
        grid = grid.with_child(asset_card(
            palette,
            source_index,
            row.as_str(),
            view,
            selected == Some(row.as_str()),
            highlight == Some(row.as_str()),
            motion,
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

    let root = UiNode::new("assets.root", UiNodeKind::Panel)
        .with_class("bottom-panel")
        .with_layout(UiLayout::fill(UiFlow::Row))
        .with_style(palette.panel_style())
        .with_child(asset_sidebar(palette, language, filter, script_extension, rows.len()))
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

    let mut surface = UiSurface::new("editor.bottom.assets", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn asset_toolbar(
    palette: StudioUiPalette,
    filter: AssetFilter,
    sort: AssetSort,
    view: AssetViewMode,
    create_open: bool,
    pending: bool,
    visible_count: usize,
    total_count: usize,
) -> UiNode {
    let tokens = palette.tokens();
    let toolbar = UiNode::new("assets.toolbar", UiNodeKind::Toolbar)
        .with_class("assets-toolbar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(10.0, 3.0),
            ..UiLayout::fixed(0.0, ASSET_TOOLBAR_HEIGHT)
                .with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("assets.create-trigger", UiNodeKind::Button)
                .with_class(if create_open {
                    "asset-action asset-create-trigger asset-create-trigger-open"
                } else {
                    "asset-action asset-create-trigger"
                })
                .with_text_key("app.assets_create")
                .with_accessibility_label_key("app.assets_create")
                .with_layout(UiLayout::fixed(84.0, 26.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.create-menu.toggle",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("enter".to_string()),
                    "assets.create-menu.toggle",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("space".to_string()),
                    "assets.create-menu.toggle",
                )),
        )
        .with_child(
            UiNode::new("assets.refresh", UiNodeKind::Button)
                .with_class(if pending {
                    "asset-action asset-action-busy"
                } else {
                    "asset-action"
                })
                .with_icon(UiIcon::new(UiIconId::Refresh).with_size(UiIconSize::Small))
                .with_tooltip_key("app.refresh_assets")
                .with_accessibility_label_key("app.refresh_assets")
                .with_layout(UiLayout::fixed(26.0, 26.0))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "assets.refresh"))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("enter".to_string()),
                    "assets.refresh",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("space".to_string()),
                    "assets.refresh",
                )),
        )
        .with_child(breadcrumb(palette, filter))
        .with_child(
            UiNode::new("assets.toolbar.spacer", UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("assets.count", UiNodeKind::Label)
                .with_class("asset-count")
                .with_text_value(format!("{visible_count} / {total_count}"))
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("assets.sort", UiNodeKind::Button)
                .with_class("asset-action")
                .with_text_key(sort.label_key())
                .with_accessibility_label_key(sort.label_key())
                .with_layout(UiLayout::fit_content().with_text_safe_area(true))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "assets.sort.toggle")),
        )
        .with_child(
            UiNode::new("assets.view", UiNodeKind::Button)
                .with_class("asset-action")
                .with_icon(UiIcon::new(if view == AssetViewMode::Grid {
                    UiIconId::Menu
                } else {
                    UiIconId::Grid
                }).with_size(UiIconSize::Small))
                .with_tooltip_key(if view == AssetViewMode::Grid {
                    "app.assets_view_list"
                } else {
                    "app.assets_view_grid"
                })
                .with_accessibility_label_key(if view == AssetViewMode::Grid {
                    "app.assets_view_list"
                } else {
                    "app.assets_view_grid"
                })
                .with_layout(UiLayout::fixed(26.0, 26.0))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, "assets.view.toggle")),
        )
        .with_child(
            UiNode::new("assets.open-folder", UiNodeKind::Button)
                .with_class("asset-action")
                .with_icon(UiIcon::new(UiIconId::Folder).with_size(UiIconSize::Small))
                .with_tooltip_key("app.open_folder")
                .with_accessibility_label_key("app.open_folder")
                .with_layout(UiLayout::fixed(26.0, 26.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.open-folder",
                )),
        );
    toolbar
}

fn breadcrumb(palette: StudioUiPalette, filter: AssetFilter) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("assets.breadcrumb", UiNodeKind::Panel)
        .with_class("asset-breadcrumb")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(4.0, 0.0),
            ..UiLayout::fit_content()
        })
        .with_child(
            UiNode::new("assets.breadcrumb.leaf", UiNodeKind::Label)
                .with_text_key(filter.label_key())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 12.0,
                    line_height_px: 16.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        )
}

fn asset_status_strip(
    palette: StudioUiPalette,
    language: raf_core::Language,
    status: Option<&AssetsStatus>,
    pending: bool,
    catalog_error: Option<&str>,
) -> UiNode {
    let mut strip = UiNode::new("assets.status", UiNodeKind::Panel)
        .with_class("assets-status")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 4.0),
            ..UiLayout::fixed(0.0, 0.0)
                .with_width_mode(UiSizeMode::Fill)
                .with_height_mode(UiSizeMode::FitContent)
        });

    if let Some(catalog_error) = catalog_error {
        strip = strip.with_child(status_line(
            palette,
            "assets.status.catalog-error",
            UiIconId::Error,
            AssetsStatusKind::Error,
            catalog_error.to_string(),
            Some("assets.refresh"),
            "app.refresh_assets",
        ));
    }
    if pending && catalog_error.is_none() {
        strip = strip.with_child(status_line(
            palette,
            "assets.status.pending",
            UiIconId::Refresh,
            AssetsStatusKind::Pending,
            raf_core::i18n::t("app.assets_scanning", language),
            None,
            "",
        ));
    }
    if let Some(status) = status {
        strip = strip.with_child(status_line(
            palette,
            "assets.status.value",
            match status.kind {
                AssetsStatusKind::Pending => UiIconId::Refresh,
                AssetsStatusKind::Success => UiIconId::Success,
                AssetsStatusKind::Error => UiIconId::Error,
            },
            status.kind,
            status.text.clone(),
            None,
            "",
        ));
    }
    strip
}

#[allow(clippy::too_many_arguments)]
fn status_line(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    kind: AssetsStatusKind,
    text: String,
    action: Option<&str>,
    action_label_key: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let class = match kind {
        AssetsStatusKind::Pending => "assets-status-line assets-status-pending",
        AssetsStatusKind::Success => "assets-status-line assets-status-success",
        AssetsStatusKind::Error => "assets-status-line assets-status-error",
    };
    let mut line = UiNode::new(id, UiNodeKind::Panel)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(8.0, 3.0),
            ..UiLayout::fit_content()
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_value(text)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fit_content()),
        );
    if let Some(action) = action {
        line = line.with_child(
            UiNode::new(format!("{id}.action"), UiNodeKind::Button)
                .with_class("assets-status-action")
                .with_text_key(action_label_key)
                .with_layout(UiLayout::fit_content().with_text_safe_area(true))
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, action)),
        );
    }
    line
}

fn empty_state(palette: StudioUiPalette, no_rows: bool, query: &str, filter: AssetFilter) -> UiNode {
    let tokens = palette.tokens();
    let title_key = if !query.trim().is_empty() {
        "app.search_no_results"
    } else {
        filter.empty_title_key()
    };
    let empty = UiNode::new("assets.empty", UiNodeKind::Panel)
        .with_class("assets-empty")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            min_size: [0.0, 96.0],
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_child(
            UiNode::new("assets.empty.icon", UiNodeKind::Label)
                .with_icon(UiIcon::new(if filter == AssetFilter::Scripts {
                    UiIconId::Script
                } else {
                    UiIconId::Assets
                })
                .with_size(UiIconSize::Custom(28))
                .with_tint(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("assets.empty.title", UiNodeKind::Label)
                .with_text_key(title_key)
                .with_layout(UiLayout::fit_content())
                .with_text_style(UiTextStyle::body(tokens.text)),
        );
    let hint_key = if !query.trim().is_empty() {
        "app.search_no_results_hint"
    } else if no_rows {
        "app.drag_drop_hint"
    } else {
        "app.assets_empty_hint"
    };
    empty.with_child(
        UiNode::new("assets.empty.hint", UiNodeKind::Label)
            .with_text_key(hint_key)
            .with_layout(UiLayout::fit_content())
            .with_text_style(UiTextStyle::body(tokens.text_muted)),
    )
}

fn asset_sidebar(
    palette: StudioUiPalette,
    language: raf_core::Language,
    active: AssetFilter,
    script_extension: &str,
    asset_count: usize,
) -> UiNode {
    let tokens = palette.tokens();
    let mut sidebar = UiNode::new("assets.sidebar", UiNodeKind::Panel)
        .with_class("assets-sidebar")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            basis: [ASSET_SIDEBAR_WIDTH, 0.0],
            width_mode: UiSizeMode::Fixed,
            height_mode: UiSizeMode::Fill,
            min_size: [ASSET_SIDEBAR_WIDTH, 0.0],
            gap: 2.0,
            padding: UiSpacing::xy(8.0, 10.0),
            ..UiLayout::default()
        })
        .with_style(UiStyle {
            fill: tokens.surface_alt,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 0.0,
            opacity: 1.0,
        })
        .with_child(
            UiNode::text_input(
                "assets.search",
                raf_ui::UiTextInput {
                    value_key: "assets.search".to_string(),
                    placeholder_key: Some("app.search".to_string()),
                    max_length: 256,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("asset-search")
            .with_icon(UiIcon::new(UiIconId::Search).with_size(UiIconSize::Small))
            .with_accessibility_label_key("app.search")
            .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("assets.sidebar.title", UiNodeKind::Label)
                .with_text_key("app.assets_category_title")
                .with_text_style(UiTextStyle {
                    role: UiTextRole::PanelTitle,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        );
    for (id, icon, label_key) in [
        ("all", UiIconId::Assets, "app.assets_category_all"),
        ("models", UiIconId::Folder, "app.models"),
        ("images", UiIconId::Shaded, "app.images"),
        ("audio", UiIconId::Play, "app.audio"),
        ("scripts", UiIconId::Script, "app.scripts_filter"),
    ] {
        sidebar = sidebar.with_child(asset_category_row(
            palette,
            id,
            icon,
            label_key,
            matches!(
                (id, active),
                ("all", AssetFilter::All)
                    | ("models", AssetFilter::Models)
                    | ("images", AssetFilter::Images)
                    | ("audio", AssetFilter::Audio)
                    | ("scripts", AssetFilter::Scripts)
            ),
        ));
    }
    if active == AssetFilter::Scripts {
        sidebar = sidebar.with_child(script_extension_chips(script_extension));
    }
    sidebar.with_child(
        UiNode::new("assets.sidebar.count", UiNodeKind::Label)
            .with_class("asset-sidebar-count")
            .with_text_value(format_count("app.assets_count", asset_count, language))
            .with_text_style(UiTextStyle::body(tokens.text_muted))
            .with_text_overflow(UiTextOverflow::Ellipsis)
            .with_layout(UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)),
    )
}

fn script_extension_chips(active_extension: &str) -> UiNode {
    let active = if active_extension.is_empty() {
        "all"
    } else {
        active_extension
    };
    let mut chips = UiNode::new("assets.script-languages", UiNodeKind::Panel)
        .with_class("asset-script-languages")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            compact: UiCompactMode::Wrap,
            gap: 4.0,
            padding: UiSpacing::xy(0.0, 2.0),
            // Fixed height on purpose: a wrapped row reports no intrinsic
            // height, which would let the footer count overlap the last chip.
            ..UiLayout::fixed(0.0, 52.0).with_width_mode(UiSizeMode::Fill)
        });
    for (extension, label_key) in [
        ("all", "app.assets_lang_all"),
        ("rs", "app.assets_lang_rust"),
        ("cpp", "app.assets_lang_cpp"),
        ("rhai", "app.assets_lang_rhai"),
        ("lua", "app.assets_lang_lua"),
        ("py", "app.assets_lang_python"),
    ] {
        chips = chips.with_child(
            UiNode::new(format!("assets.script-ext.{extension}"), UiNodeKind::Button)
                .with_class(if active == extension {
                    "asset-chip asset-chip-active"
                } else {
                    "asset-chip"
                })
                .with_text_key(label_key)
                .with_accessibility_label_key(label_key)
                .with_layout(UiLayout::fixed(84.0, 22.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("assets.script-ext:{extension}"),
                )),
        );
    }
    chips
}

fn asset_category_row(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    label_key: &str,
    active: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let command = format!("assets.filter.{id}");
    UiNode::new(format!("assets.sidebar.row.{id}"), UiNodeKind::Button)
        .with_class(if active {
            "asset-category asset-category-active"
        } else {
            "asset-category"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 4.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(
            UiIcon::new(icon)
                .with_size(UiIconSize::Small)
                .with_tint(if active { tokens.accent } else { tokens.text_muted }),
        )
        .with_text_key(label_key)
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_tooltip_key(label_key)
        .with_accessibility_label_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Tab)
        .with_accessibility_selected(active)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command.clone()))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command.clone(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command,
        ))
}

fn asset_card(
    palette: StudioUiPalette,
    index: usize,
    row: &str,
    view: AssetViewMode,
    selected: bool,
    highlighted: bool,
    motion: AssetsMotion,
) -> UiNode {
    let tokens = palette.tokens();
    let open_command = format!("assets.open:{row}");
    let select_command = format!("assets.select:{row}");
    let mut class = String::from("asset-card");
    if selected {
        class.push_str(" asset-card-selected");
    }
    if highlighted {
        class.push_str(" asset-card-highlight");
    }
    let name = asset_file_name(row);
    let (chip, folder) = asset_meta(row);

    let node = if view == AssetViewMode::List {
        list_row(
            palette,
            index,
            &class,
            row,
            &name,
            chip.as_deref(),
            folder.as_deref(),
        )
    } else {
        grid_card(
            palette,
            index,
            &class,
            row,
            &name,
            chip.as_deref(),
            folder.as_deref(),
        )
    };

    node.with_tooltip_value(format!("{row}"))
        .with_accessibility_label_key(name.clone())
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_selected(selected)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, select_command))
        .with_event(UiEventBinding::command(UiEventKind::DoubleClick, open_command.clone()))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            open_command.clone(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            open_command,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::ContextMenu,
            format!("assets.context-menu:{row}"),
        ))
        .with_style(UiStyle {
            fill: tokens.surface,
            border: if highlighted {
                tokens.accent
            } else {
                tokens.border
            },
            text: tokens.text,
            border_width: if highlighted {
                1.0 + (1.0 - motion.highlight).clamp(0.0, 1.0)
            } else {
                1.0
            },
            radius: 4.0,
            opacity: {
                let settle = motion.view.clamp(0.0, 1.0);
                let pulse = if highlighted {
                    0.85 + 0.15 * motion.highlight.clamp(0.0, 1.0)
                } else {
                    1.0
                };
                (settle * pulse).clamp(0.0, 1.0)
            },
        })
}

fn grid_card(
    palette: StudioUiPalette,
    index: usize,
    class: &str,
    row: &str,
    name: &str,
    chip: Option<&str>,
    folder: Option<&str>,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(format!("assets.card.{index}"), UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Start,
            gap: 4.0,
            padding: UiSpacing::xy(8.0, 8.0),
            ..UiLayout::fixed(ASSET_CARD_WIDTH, ASSET_CARD_HEIGHT)
        })
        .with_child(
            UiNode::new(format!("assets.card.{index}.icon"), UiNodeKind::Label)
                .with_layout(UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill))
                .with_icon(
                    UiIcon::new(asset_icon(row))
                        .with_size(UiIconSize::Panel)
                        .with_tint(tokens.text),
                ),
        )
        .with_child(
            UiNode::new(format!("assets.card.{index}.label"), UiNodeKind::Label)
                .with_text_value(name.to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 11.0,
                    line_height_px: 15.0,
                    weight: UiFontWeight::Medium,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(ASSET_CARD_WIDTH - 16.0, 15.0)),
        )
        .with_child(asset_card_meta(
            palette,
            format!("assets.card.{index}.meta"),
            chip,
            folder,
            ASSET_CARD_WIDTH - 16.0,
        ))
        .with_child(card_actions_row(
            palette,
            &format!("assets.card.{index}.actions"),
            row,
        ))
}

fn list_row(
    palette: StudioUiPalette,
    index: usize,
    class: &str,
    row: &str,
    name: &str,
    chip: Option<&str>,
    folder: Option<&str>,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(format!("assets.card.{index}"), UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(10.0, 4.0),
            ..UiLayout::fixed(0.0, ASSET_LIST_ROW_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("assets.card.{index}.icon"), UiNodeKind::Label)
                .with_icon(UiIcon::new(asset_icon(row)).with_size(UiIconSize::Panel))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new(format!("assets.card.{index}.label"), UiNodeKind::Label)
                .with_text_value(name.to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 12.0,
                    line_height_px: 16.0,
                    weight: UiFontWeight::Medium,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(180.0, 16.0)),
        )
        .with_child(asset_card_meta(
            palette,
            format!("assets.card.{index}.meta"),
            chip,
            folder,
            220.0,
        ))
        .with_child(
            UiNode::new(format!("assets.card.{index}.spacer"), UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new(format!("assets.card.{index}.more"), UiNodeKind::Button)
                .with_class("asset-card-action")
                .with_layout(UiLayout::fixed(22.0, 22.0))
                .with_icon(
                    UiIcon::new(UiIconId::Menu)
                        .with_size(UiIconSize::Small)
                        .with_tint(tokens.text_muted),
                )
                .with_tooltip_key("app.assets_actions_more")
                .with_accessibility_label_key("app.assets_actions_more")
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    format!("assets.context-menu:{row}"),
                )),
        )
}

/// Per-card action row. The card is a button, so its own actions live in real
/// child nodes instead of hidden behind a right click.
fn card_actions_row(palette: StudioUiPalette, id: &str, row: &str) -> UiNode {
    let tokens = palette.tokens();
    let mut actions = UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 2.0,
            ..UiLayout::fixed(0.0, 20.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new(format!("{id}.spacer"), UiNodeKind::Panel)
                .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill)),
        );
    if !is_builtin_row(row) {
        for (suffix, icon, label_key, command) in [
            (
                "rename",
                UiIconId::Pencil,
                "app.assets_context_rename",
                format!("assets.rename.begin:{row}"),
            ),
            (
                "reveal",
                UiIconId::Folder,
                "app.assets_context_reveal",
                format!("assets.reveal:{row}"),
            ),
            (
                "more",
                UiIconId::Menu,
                "app.assets_actions_more",
                format!("assets.context-menu:{row}"),
            ),
        ] {
            actions = actions.with_child(
                UiNode::new(format!("{id}.{suffix}"), UiNodeKind::Button)
                    .with_class("asset-card-action")
                    .with_layout(UiLayout::fixed(20.0, 20.0))
                    .with_icon(
                        UiIcon::new(icon)
                            .with_size(UiIconSize::Small)
                            .with_tint(tokens.text_muted),
                    )
                    .with_tooltip_key(label_key)
                    .with_accessibility_label_key(label_key)
                    .focusable()
                    .with_event(UiEventBinding::command(UiEventKind::Click, command)),
            );
        }
    }
    actions
}

fn asset_card_meta(
    palette: StudioUiPalette,
    id: String,
    chip: Option<&str>,
    folder: Option<&str>,
    width: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let mut meta = UiNode::new(id.clone(), UiNodeKind::Panel)
        .with_class("asset-card-meta")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            ..UiLayout::fixed(width, 14.0)
        });
    if let Some(chip) = chip {
        meta = meta.with_child(
            UiNode::new(format!("{id}.chip"), UiNodeKind::Label)
                .with_class("asset-chip-value")
                .with_text_value(chip.to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 9.0,
                    line_height_px: 12.0,
                    weight: UiFontWeight::Bold,
                    color: tokens.accent,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        );
    }
    if let Some(folder) = folder {
        meta = meta.with_child(
            UiNode::new(format!("{id}.folder"), UiNodeKind::Label)
                .with_text_value(folder.to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 9.0,
                    line_height_px: 12.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed((width - 40.0).max(0.0), 12.0)),
        );
    }
    meta
}

/// Action bar for the selected row. It keeps the common file operations one
/// click away instead of hiding every one of them behind a context menu.
fn selection_action_bar(palette: StudioUiPalette, row: &str, highlight: Option<&str>) -> UiNode {
    let tokens = palette.tokens();
    if row.is_empty() {
        return UiNode::new("assets.selection", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(0.0, 0.0).with_width_mode(UiSizeMode::Fill));
    }
    let builtin = is_builtin_row(row);
    let mut actions: Vec<(UiIconId, &'static str, String)> = vec![(
        if builtin {
            UiIconId::Add
        } else {
            UiIconId::ExternalLink
        },
        if builtin {
            "app.assets_context_create"
        } else {
            "app.assets_context_open"
        },
        format!("assets.open:{row}"),
    )];
    if !builtin {
        actions.extend([
            (
                UiIconId::Pencil,
                "app.assets_context_rename",
                format!("assets.rename.begin:{row}"),
            ),
            (
                UiIconId::Copy,
                "app.assets_context_duplicate",
                format!("assets.duplicate:{row}"),
            ),
            (
                UiIconId::Folder,
                "app.assets_context_reveal",
                format!("assets.reveal:{row}"),
            ),
            (
                UiIconId::Trash,
                "app.assets_context_delete",
                format!("assets.delete.begin:{row}"),
            ),
        ]);
    }
    let mut bar = UiNode::new("assets.selection", UiNodeKind::Panel)
        .with_class("assets-selection-bar")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(10.0, 4.0),
            ..UiLayout::fixed(0.0, ASSET_SELECTION_BAR_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            UiNode::new("assets.selection.name", UiNodeKind::Label)
                .with_class("asset-selection-name")
                .with_text_value(asset_file_name(row))
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 12.0,
                    line_height_px: 16.0,
                    weight: UiFontWeight::Medium,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)
                }),
        );
    if highlight == Some(row) {
        bar = bar.with_child(
            UiNode::new("assets.selection.hint", UiNodeKind::Label)
                .with_text_key("app.assets_selection_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        );
    }
    for (index, (icon, label_key, command)) in actions.into_iter().enumerate() {
        bar = bar.with_child(
            UiNode::new(format!("assets.selection.action.{index}"), UiNodeKind::Button)
                .with_class("asset-card-action")
                .with_layout(UiLayout::fixed(24.0, 24.0))
                .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
                .with_tooltip_key(label_key)
                .with_accessibility_label_key(label_key)
                .focusable()
                .with_event(UiEventBinding::command(UiEventKind::Click, command)),
        );
    }
    bar
}

/// Extension chip for scripts and the parent folder for everything else.
/// Built-in rows have no file, so they get no secondary line at all.
fn asset_meta(row: &str) -> (Option<String>, Option<String>) {
    if row.starts_with("builtin://primitive/") {
        return (None, None);
    }
    let extension = Path::new(row)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    if extension.as_deref().is_some_and(|extension| is_script_file(&format!("asset.{extension}"))) {
        return (
            Some(
                extension
                    .as_deref()
                    .map(extension_label)
                    .unwrap_or_default(),
            ),
            None,
        );
    }
    let folder = Path::new(row)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|folder| !folder.is_empty());
    (None, folder)
}

fn extension_label(extension: &str) -> String {
    match extension {
        "rs" => "RS".to_string(),
        "cpp" | "cc" | "cxx" => "CPP".to_string(),
        "rhai" => "RHAI".to_string(),
        "js" => "JS".to_string(),
        "ts" => "TS".to_string(),
        "py" => "PY".to_string(),
        "lua" => "LUA".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

pub fn asset_file_name(row: &str) -> String {
    if let Some(slug) = row.strip_prefix("builtin://primitive/") {
        return crate::panels::primitive_create::parse_primitive_slug(slug)
            .map(|primitive| primitive_name(primitive).to_string())
            .unwrap_or_else(|| slug.to_string());
    }
    Path::new(row)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(row)
        .to_string()
}

pub fn asset_display_name(row: &str) -> String {
    row.strip_prefix("builtin://primitive/")
        .and_then(crate::panels::primitive_create::parse_primitive_slug)
        .map(|primitive| format!("Built-in / {}", primitive_name(primitive)))
        .unwrap_or_else(|| row.to_string())
}

fn sort_asset_rows(rows: &mut [&String], sort: AssetSort) {
    rows.sort_by(|left, right| {
        let (left_builtin, right_builtin) = (is_builtin_row(left), is_builtin_row(right));
        match (left_builtin, right_builtin) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => {
                let ordering = asset_sort_key(left).cmp(&asset_sort_key(right));
                match sort {
                    AssetSort::NameAsc => ordering,
                    AssetSort::NameDesc => ordering.reverse(),
                }
            }
        }
    });
}

fn asset_sort_key(row: &str) -> String {
    let name = asset_file_name(row);
    name.to_ascii_lowercase()
}

fn is_builtin_row(row: &str) -> bool {
    row.starts_with("builtin://")
}

fn asset_matches_script_extension(row: &str, filter: AssetFilter, extension: &str) -> bool {
    if filter != AssetFilter::Scripts || extension.is_empty() || is_builtin_row(row) {
        return true;
    }
    Path::new(row)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case(extension))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Overlay documents: create menu, popovers, modals and the context menu.
// ---------------------------------------------------------------------------

/// Click-through surface rendered behind a modal so the press resolves to the
/// modal dismiss command instead of a control underneath.
pub fn build_assets_backdrop_surface(palette: StudioUiPalette) -> UiSurface {
    let root = UiNode::new("assets.backdrop", UiNodeKind::Panel)
        .with_class("assets-backdrop")
        .with_layout(UiLayout::fill(UiFlow::None))
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            "assets.modal.close",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.modal.close",
        ));
    overlay_surface("editor.assets.backdrop", palette, root)
}

pub fn build_assets_create_menu_surface(palette: StudioUiPalette) -> UiSurface {
    let mut root = overlay_menu_root(palette, "assets.create-menu", "app.assets_create");
    for (id, icon, label_key, command) in [
        (
            "script",
            UiIconId::Script,
            "app.create_script",
            "assets.create-script",
        ),
        (
            "primitive",
            UiIconId::Cube,
            "app.create_primitive",
            "assets.create-primitive.toggle",
        ),
        (
            "file",
            UiIconId::File,
            "app.create_file",
            "assets.create-file",
        ),
    ] {
        root = root.with_child(menu_item(
            palette,
            &format!("assets.create-menu.{id}"),
            icon,
            label_key,
            None,
            command,
        ));
    }
    overlay_surface("editor.assets.create-menu", palette, root)
}

pub fn build_assets_script_popover_surface(palette: StudioUiPalette, script_name: &str) -> UiSurface {
    let tokens = palette.tokens();
    let root = overlay_popover_root(palette, "assets.script-popover", "app.create_script")
        .with_child(
            UiNode::text_input(
                "assets.script-name",
                raf_ui::UiTextInput {
                    value_key: "assets.script-name".to_string(),
                    placeholder_key: Some("app.script_name".to_string()),
                    max_length: 96,
                    multiline: false,
                    password: false,
                    submit_command: None,
                },
            )
            .with_class("asset-script-name")
            .with_accessibility_label_key("app.script_name")
            .with_layout(UiLayout {
                align_self: Some(UiAlign::Stretch),
                ..UiLayout::fixed(0.0, 28.0)
                    .with_width_mode(UiSizeMode::Fill)
                    .with_text_safe_area(true)
            })
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "assets.script.cancel",
            )),
        )
        .with_child(
            UiNode::new("assets.script-popover.hint", UiNodeKind::Label)
                .with_text_key("app.script_language_hint")
                .with_text_style(UiTextStyle::body(tokens.text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new("assets.script-popover.actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 5.0,
                    min_size: [0.0, 30.0],
                    ..UiLayout::fit_content()
                })
                .with_child(script_template_button(
                    "rust",
                    "app.script_language_rust",
                    "assets.script.create:rust",
                ))
                .with_child(script_template_button(
                    "rhai",
                    "app.script_rhai",
                    "assets.script.create:rhai",
                ))
                .with_child(script_template_button(
                    "cpp",
                    "app.script_cpp",
                    "assets.script.create:cpp",
                ))
                .with_child(
                    UiNode::new("assets.script.cancel", UiNodeKind::Button)
                        .with_class("asset-script-cancel")
                        .with_text_key("app.cancel")
                        .with_layout(UiLayout::fit_content().with_text_safe_area(true))
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "assets.script.cancel",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("escape".to_string()),
                            "assets.script.cancel",
                        )),
                ),
        )
        .with_child(
            UiNode::new("assets.script-popover.name-preview", UiNodeKind::Label)
                .with_text_value(script_name.to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 10.0,
                    line_height_px: 13.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fit_content()),
        );
    overlay_surface("editor.assets.script-popover", palette, root)
}

pub fn build_assets_file_popover_surface(palette: StudioUiPalette, file_name: &str) -> UiSurface {
    let root = overlay_popover_root(palette, "assets.file-popover", "app.create_file")
        .with_child(
            UiNode::text_input(
                "assets.file-name",
                raf_ui::UiTextInput {
                    value_key: "assets.file-name".to_string(),
                    placeholder_key: Some("app.file_name_placeholder".to_string()),
                    max_length: 128,
                    multiline: false,
                    password: false,
                    submit_command: Some("assets.file.create".to_string()),
                },
            )
            .with_class("asset-script-name")
            .with_accessibility_label_key("app.file_name_placeholder")
            .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill))
            .with_text_value(file_name.to_string())
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "assets.file.cancel",
            )),
        )
        .with_child(
            UiNode::new("assets.file-actions", UiNodeKind::Toolbar)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    justify_content: UiJustify::End,
                    gap: 5.0,
                    ..UiLayout::fit_content()
                })
                .with_child(file_action_button(
                    "file-cancel",
                    "app.cancel",
                    "assets.file.cancel",
                ))
                .with_child(file_action_button(
                    "file-create",
                    "app.create",
                    "assets.file.create",
                )),
        );
    overlay_surface("editor.assets.file-popover", palette, root)
}

pub fn build_assets_primitive_popover_surface(palette: StudioUiPalette) -> UiSurface {
    let tokens = palette.tokens();
    let mut root = overlay_popover_root(palette, "assets.primitive-popover", "app.create_primitive");
    let mut grid =
        UiNode::new("assets.primitive-popover.grid", UiNodeKind::Toolbar).with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    for primitive in &CREATEABLE_PRIMITIVES {
        grid = grid.with_child(asset_primitive_modal_option(palette, *primitive));
    }
    root = root.with_child(grid).with_child(
        UiNode::new("assets.primitive-popover.cancel", UiNodeKind::Button)
            .with_class("asset-primitive-cancel")
            .with_text_key("app.cancel")
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_layout(UiLayout::fit_content().with_text_safe_area(true))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "assets.create-primitive.cancel",
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "assets.create-primitive.cancel",
            )),
    );
    overlay_surface("editor.assets.primitive-popover", palette, root)
}

pub fn build_assets_context_menu_surface(
    palette: StudioUiPalette,
    row: &str,
    builtin: bool,
    absolute_path: &str,
) -> UiSurface {
    let builtin_label = asset_file_name(row);
    let mut root = overlay_menu_root(palette, "assets.context", &builtin_label);
    if builtin {
        root = root.with_child(menu_item(
            palette,
            "assets.context.open",
            UiIconId::Add,
            "app.assets_context_create",
            None,
            &format!("assets.open:{row}"),
        ));
    } else {
        root = root.with_child(menu_item(
            palette,
            "assets.context.open",
            UiIconId::ExternalLink,
            "app.assets_context_open",
            None,
            &format!("assets.open:{row}"),
        ));
        root = root.with_child(menu_item(
            palette,
            "assets.context.rename",
            UiIconId::Pencil,
            "app.assets_context_rename",
            Some("F2"),
            &format!("assets.rename.begin:{row}"),
        ));
        root = root.with_child(menu_item(
            palette,
            "assets.context.duplicate",
            UiIconId::Copy,
            "app.assets_context_duplicate",
            Some("Ctrl+D"),
            &format!("assets.duplicate:{row}"),
        ));
        root = root.with_child(menu_item_action(
            palette,
            "assets.context.copy-path",
            UiIconId::File,
            "app.assets_context_copy_path",
            None,
            UiAction::SetClipboard {
                text: absolute_path.to_string(),
            },
        ));
        root = root.with_child(menu_item(
            palette,
            "assets.context.reveal",
            UiIconId::Folder,
            "app.assets_context_reveal",
            None,
            &format!("assets.reveal:{row}"),
        ));
        root = root.with_child(menu_item(
            palette,
            "assets.context.open-with",
            UiIconId::Settings,
            "app.assets_context_open_with",
            None,
            &format!("assets.open.with:{row}"),
        ));
        root = root.with_child(menu_item(
            palette,
            "assets.context.delete",
            UiIconId::Trash,
            "app.assets_context_delete",
            Some("Del"),
            &format!("assets.delete.begin:{row}"),
        ));
    }
    root = root.with_child(menu_item(
        palette,
        "assets.context.cancel",
        UiIconId::Close,
        "app.cancel",
        None,
        "assets.modal.close",
    ));
    overlay_surface("editor.assets.context-menu", palette, root)
}

pub fn build_assets_open_modal_surface(
    palette: StudioUiPalette,
    row: &str,
    absolute_path: &str,
) -> UiSurface {
    let root = modal_root(palette, "assets.open-modal", "app.assets_open_title")
        .with_child(asset_file_header(palette, "assets.open-modal", row, absolute_path))
        .with_child(modal_option(
            palette,
            "assets.open-modal.primary",
            UiIconId::Script,
            "app.assets_open_yoll",
            "app.assets_open_yoll_hint",
            true,
            &format!("assets.open.yoll:{row}"),
        ))
        .with_child(modal_option(
            palette,
            "assets.open-modal.editor",
            UiIconId::Pencil,
            "app.assets_open_editor",
            "app.assets_open_editor_hint",
            false,
            &format!("assets.open.editor:{row}"),
        ))
        .with_child(modal_option(
            palette,
            "assets.open-modal.manager",
            UiIconId::Folder,
            "app.assets_open_manager",
            "app.assets_open_manager_hint",
            false,
            &format!("assets.open.manager:{row}"),
        ))
        .with_child(modal_option(
            palette,
            "assets.open-modal.with",
            UiIconId::Settings,
            "app.assets_open_with",
            "app.assets_open_with_hint",
            false,
            &format!("assets.open.with:{row}"),
        ))
        .with_child(modal_footer(
            palette,
            "assets.open-modal",
            Some((absolute_path, "app.assets_context_copy_path")),
            None,
        ));
    overlay_surface("editor.assets.open-modal", palette, root)
}

pub fn build_assets_rename_modal_surface(palette: StudioUiPalette, row: &str) -> UiSurface {
    let root = modal_root(palette, "assets.rename-modal", "app.assets_rename_title")
        .with_child(
            UiNode::new("assets.rename-modal.path", UiNodeKind::Panel)
                .with_class("assets-modal-path")
                .with_layout(UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill))
                .with_child(
                    UiNode::new("assets.rename-modal.path.text", UiNodeKind::Label)
                        .with_text_value(format!("assets/{row}"))
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Monospace,
                            size_px: 10.0,
                            line_height_px: 13.0,
                            weight: UiFontWeight::Regular,
                            color: palette.tokens().text_muted,
                            inherit_color: false,
                        })
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout::fixed(0.0, 13.0).with_width_mode(UiSizeMode::Fill)),
                ),
        )
        .with_child(
            UiNode::text_input(
                "assets.rename-value",
                raf_ui::UiTextInput {
                    value_key: "assets.rename-value".to_string(),
                    placeholder_key: Some("app.assets_rename_placeholder".to_string()),
                    max_length: 96,
                    multiline: false,
                    password: false,
                    submit_command: Some("assets.rename.confirm".to_string()),
                },
            )
            .with_class("asset-script-name")
            .with_accessibility_label_key("app.assets_rename_placeholder")
            .with_layout(UiLayout::fixed(0.0, 32.0).with_width_mode(UiSizeMode::Fill))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "assets.rename.cancel",
            )),
        )
        .with_child(
            UiNode::new("assets.rename-modal.hint", UiNodeKind::Label)
                .with_text_key("app.assets_rename_hint")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(modal_footer(
            palette,
            "assets.rename-modal",
            None,
            Some((
                &format!("assets.rename.confirm:{row}"),
                "app.confirm",
            )),
        ));
    overlay_surface("editor.assets.rename-modal", palette, root)
}

pub fn build_assets_delete_modal_surface(palette: StudioUiPalette, row: &str) -> UiSurface {
    let root = modal_root(palette, "assets.delete-modal", "app.assets_delete_title")
        .with_child(
            UiNode::new("assets.delete-modal.message", UiNodeKind::Label)
                .with_text_value(format!("assets/{row}"))
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Monospace,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: UiFontWeight::Regular,
                    color: palette.tokens().text,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(0.0, 14.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new("assets.delete-modal.hint", UiNodeKind::Label)
                .with_text_key("app.assets_delete_hint")
                .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(modal_footer(
            palette,
            "assets.delete-modal",
            None,
            Some((
                &format!("assets.delete.confirm:{row}"),
                "app.assets_delete_confirm",
            )),
        ));
    overlay_surface("editor.assets.delete-modal", palette, root)
}

/// File identity block at the top of the open modal: icon tile, name and the
/// real path the action will use.
fn asset_file_header(
    palette: StudioUiPalette,
    id: &str,
    row: &str,
    absolute_path: &str,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(format!("{id}.file"), UiNodeKind::Panel)
        .with_class("assets-modal-file")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 10.0,
            padding: UiSpacing::xy(10.0, 10.0),
            ..UiLayout::fixed(0.0, 60.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(icon_tile(
            palette,
            &format!("{id}.file.icon"),
            asset_icon(row),
            UiIconSize::Custom(20),
        ))
        .with_child(
            UiNode::new(format!("{id}.file.text"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 3.0,
                    ..UiLayout::fixed(0.0, 0.0)
                        .with_width_mode(UiSizeMode::Fill)
                        .with_height_mode(UiSizeMode::FitContent)
                })
                .with_child(
                    UiNode::new(format!("{id}.file.name"), UiNodeKind::Label)
                        .with_text_value(asset_file_name(row))
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::PanelTitle,
                            size_px: 13.0,
                            line_height_px: 18.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.text,
                            inherit_color: false,
                        })
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)),
                )
                .with_child(
                    UiNode::new(format!("{id}.file.path"), UiNodeKind::Label)
                        .with_text_value(absolute_path.to_string())
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::Monospace,
                            size_px: 10.0,
                            line_height_px: 13.0,
                            weight: UiFontWeight::Regular,
                            color: tokens.text_muted,
                            inherit_color: false,
                        })
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout::fixed(0.0, 13.0).with_width_mode(UiSizeMode::Fill)),
                ),
        )
}

/// One action row of the open modal. Label and hint share a column so neither
/// one steals width from the other.
fn modal_option(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    label_key: &str,
    hint_key: &str,
    primary: bool,
    command: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let text_column = UiNode::new(format!("{id}.text"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            // Explicit width: a Fill child inside a row has no assigned
            // main-axis extent to resolve against and would collapse,
            // ellipsizing both the label and its hint.
            ..UiLayout::fixed(ASSETS_MODAL_TEXT_WIDTH, 0.0)
                .with_height_mode(UiSizeMode::FitContent)
        })
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Button,
                    size_px: 12.0,
                    line_height_px: 16.0,
                    weight: if primary {
                        UiFontWeight::Bold
                    } else {
                        UiFontWeight::Medium
                    },
                    color: if primary {
                        [255, 255, 255, 255]
                    } else {
                        tokens.text
                    },
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)),
        )
        .with_child(
            UiNode::new(format!("{id}.hint"), UiNodeKind::Label)
                .with_text_key(hint_key)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 10.0,
                    line_height_px: 14.0,
                    weight: UiFontWeight::Regular,
                    color: if primary {
                        [255, 236, 214, 255]
                    } else {
                        tokens.text_muted
                    },
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(0.0, 14.0).with_width_mode(UiSizeMode::Fill)),
        );
    let mut option = UiNode::new(id, UiNodeKind::Button)
        .with_class(if primary {
            "assets-modal-option assets-modal-option-primary"
        } else {
            "assets-modal-option"
        })
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 12.0,
            padding: UiSpacing::xy(10.0, 8.0),
            ..UiLayout::fixed(0.0, 58.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(icon_tile(
            palette,
            &format!("{id}.icon"),
            icon,
            UiIconSize::Custom(18),
        ))
        .with_child(text_column);
    if primary {
        option = option.with_child(
            UiNode::new(format!("{id}.badge"), UiNodeKind::Label)
                .with_class("assets-recommended-badge")
                .with_text_key("app.assets_recommended")
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 9.0,
                    line_height_px: 12.0,
                    weight: UiFontWeight::Bold,
                    color: [255, 255, 255, 255],
                    inherit_color: false,
                })
                .with_layout(UiLayout::fixed(96.0, 18.0)),
        );
    }
    option
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command.to_string()))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command.to_string(),
        ))
}

/// Rounded icon plate. Icons live in child nodes because a node level icon is
/// painted inside the node content box instead of the child flow.
fn icon_tile(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    size: UiIconSize,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Label)
        .with_class("assets-icon-tile")
        .with_layout(UiLayout::fixed(34.0, 34.0))
        .with_icon(UiIcon::new(icon).with_size(size).with_tint(tokens.accent))
}

fn modal_root(palette: StudioUiPalette, id: &str, title_key: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Menu)
        .with_class("assets-modal")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 8.0,
            padding: UiSpacing::xy(12.0, 10.0),
            ..UiLayout::fixed(ASSETS_MODAL_WIDTH, 0.0).with_height_mode(UiSizeMode::FitContent)
        })
        .with_accessibility_label_key(title_key)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.modal.close",
        ))
        .with_child(
            UiNode::new(format!("{id}.header"), UiNodeKind::Panel)
                .with_layout(UiLayout {
                    flow: UiFlow::Row,
                    align_items: UiAlign::Center,
                    gap: 8.0,
                    ..UiLayout::fixed(0.0, 26.0).with_width_mode(UiSizeMode::Fill)
                })
                .with_child(
                    UiNode::new(format!("{id}.title"), UiNodeKind::Label)
                        .with_text_key(title_key)
                        .with_text_style(UiTextStyle {
                            role: UiTextRole::PanelTitle,
                            size_px: 13.0,
                            line_height_px: 18.0,
                            weight: UiFontWeight::Bold,
                            color: tokens.text,
                            inherit_color: false,
                        })
                        .with_text_overflow(UiTextOverflow::Ellipsis)
                        .with_layout(UiLayout {
                            grow: 1.0,
                            ..UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)
                        }),
                )
                .with_child(
                    UiNode::new(format!("{id}.close"), UiNodeKind::Button)
                        .with_class("asset-action")
                        .with_layout(UiLayout::fixed(22.0, 22.0))
                        .with_icon(
                            UiIcon::new(UiIconId::Close)
                                .with_size(UiIconSize::Small)
                                .with_tint(tokens.text_muted),
                        )
                        .with_tooltip_key("app.close")
                        .with_accessibility_label_key("app.close")
                        .focusable()
                        .with_event(UiEventBinding::command(
                            UiEventKind::Click,
                            "assets.modal.close",
                        )),
                ),
        )
}

/// Footer with an optional leading clipboard action and an optional confirm
/// button, plus the dismiss action every modal needs.
fn modal_footer(
    _palette: StudioUiPalette,
    id: &str,
    clipboard: Option<(&str, &str)>,
    primary: Option<(&str, &str)>,
) -> UiNode {
    let mut footer = UiNode::new(format!("{id}.footer"), UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 6.0,
            padding: UiSpacing::xy(0.0, 4.0),
            ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
        });
    if let Some((text, label_key)) = clipboard {
        footer = footer.with_child(
            UiNode::new(format!("{id}.clipboard"), UiNodeKind::Button)
                .with_class("asset-script-cancel")
                .with_text_key(label_key)
                .with_layout(UiLayout::fixed(124.0, 26.0))
                .focusable()
                .with_event(UiEventBinding {
                    event: UiEventKind::Click,
                    action: UiAction::SetClipboard {
                        text: text.to_string(),
                    },
                }),
        );
    }
    footer = footer.with_child(
        UiNode::new(format!("{id}.spacer"), UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill)),
    );
    if let Some((command, label_key)) = primary {
        footer = footer.with_child(
            UiNode::new(format!("{id}.confirm"), UiNodeKind::Button)
                .with_class("assets-modal-confirm")
                .with_text_key(label_key)
                .with_layout(UiLayout::fixed(96.0, 26.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    command.to_string(),
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("enter".to_string()),
                    command.to_string(),
                )),
        );
    }
    footer.with_child(
        UiNode::new(format!("{id}.secondary"), UiNodeKind::Button)
            .with_class("asset-script-cancel")
            .with_text_key("app.cancel")
            .with_layout(UiLayout::fixed(78.0, 26.0))
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "assets.modal.close",
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "assets.modal.close",
            )),
    )
}
fn overlay_menu_root(_palette: StudioUiPalette, id: &str, title_key: &str) -> UiNode {
    UiNode::new(id, UiNodeKind::Menu)
        .with_class("assets-menu")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 2.0,
            padding: UiSpacing::same(6.0),
            ..UiLayout::fixed(214.0, 0.0).with_height_mode(UiSizeMode::FitContent)
        })
        .with_accessibility_label_key(title_key)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.modal.close",
        ))
}

fn overlay_popover_root(palette: StudioUiPalette, id: &str, title_key: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(id, UiNodeKind::Menu)
        .with_class("assets-menu assets-popover")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::xy(10.0, 8.0),
            ..UiLayout::fixed(300.0, 0.0).with_height_mode(UiSizeMode::FitContent)
        })
        .with_child(
            UiNode::new(format!("{id}.title"), UiNodeKind::Label)
                .with_text_key(title_key)
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
        .with_accessibility_label_key(title_key)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.modal.close",
        ))
}

fn menu_item(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    label_key: &str,
    shortcut: Option<&str>,
    command: &str,
) -> UiNode {
    let tokens = palette.tokens();
    let mut item = UiNode::new(id, UiNodeKind::Button)
        .with_class("assets-menu-item")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 5.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)
                }),
        );
    if let Some(shortcut) = shortcut {
        item = item.with_child(
            UiNode::new(format!("{id}.shortcut"), UiNodeKind::Label)
                .with_text_value(shortcut.to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 10.0,
                    line_height_px: 13.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        );
    }
    item.focusable()
        .with_accessibility_label_key(label_key)
        .with_event(UiEventBinding::command(UiEventKind::Click, command.to_string()))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command.to_string(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command.to_string(),
        ))
}

/// Menu row bound to a retained action instead of a panel command.
fn menu_item_action(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    label_key: &str,
    shortcut: Option<&str>,
    action: UiAction,
) -> UiNode {
    let tokens = palette.tokens();
    let mut item = UiNode::new(id, UiNodeKind::Button)
        .with_class("assets-menu-item")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            align_items: UiAlign::Center,
            gap: 8.0,
            padding: UiSpacing::xy(8.0, 5.0),
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_child(
            UiNode::new(format!("{id}.label"), UiNodeKind::Label)
                .with_text_key(label_key)
                .with_text_style(UiTextStyle::body(tokens.text))
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout {
                    grow: 1.0,
                    ..UiLayout::fixed(0.0, 16.0).with_width_mode(UiSizeMode::Fill)
                }),
        )
        .with_accessibility_label_key(label_key)
        .focusable()
        .with_event(UiEventBinding {
            event: UiEventKind::Click,
            action,
        });
    if let Some(shortcut) = shortcut {
        item = item.with_child(
            UiNode::new(format!("{id}.shortcut"), UiNodeKind::Label)
                .with_text_value(shortcut.to_string())
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 10.0,
                    line_height_px: 13.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text_muted,
                    inherit_color: false,
                })
                .with_layout(UiLayout::fit_content()),
        );
    }
    item
}

/// Absolute path for a catalog row, or the row itself when the project root is
/// unknown or the row is a virtual built-in primitive.
pub fn asset_absolute_path(project_root: Option<&Path>, row: &str) -> String {
    if is_builtin_asset_row(row) {
        return row.to_string();
    }
    match project_root {
        Some(root) => root.join("assets").join(row).to_string_lossy().to_string(),
        None => format!("assets/{row}"),
    }
}

/// True for the virtual authoring shortcuts that have no file on disk.
pub fn is_builtin_asset_row(row: &str) -> bool {
    is_builtin_row(row)
}

fn overlay_surface(id: &str, palette: StudioUiPalette, root: UiNode) -> UiSurface {
    let mut surface = UiSurface::new(id, palette, root);
    surface.style_sheet = assets_overlay_style_sheet(palette);
    surface
}

/// Style sheet shared by every Assets overlay document. Overlays reuse the
/// panel sheet for shared controls (text inputs, action buttons) and layer the
/// menu, modal and backdrop rules on top.
pub fn assets_overlay_style_sheet(palette: StudioUiPalette) -> UiStyleSheet {
    let tokens = palette.tokens();
    let always = UiStyleRuleState::Always;
    let hovered = UiStyleRuleState::Hovered;
    let mut rules = bottom_style_sheet(palette).rules;
    rules.extend([
            UiStyleRule::new(
                UiStyleSelector::Class("assets-backdrop".to_string()),
                UiStylePatch {
                    fill: Some([6, 10, 16, 140]),
                    ..UiStylePatch::default()
                },
            )
            .when(always),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-menu".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(6.0),
                    ..UiStylePatch::default()
                },
            )
            .when(always),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-menu-item".to_string()),
                UiStylePatch {
                    fill: Some([0, 0, 0, 0]),
                    border: Some([0, 0, 0, 0]),
                    border_width: Some(0.0),
                    radius: Some(4.0),
                    text: Some(tokens.text),
                    ..UiStylePatch::default()
                },
            )
            .when(always),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-menu-item".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.accent),
                    border_width: Some(1.0),
                    ..UiStylePatch::default()
                },
            )
            .when(hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-modal".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_raised),
                    border: Some(tokens.accent),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(8.0),
                    ..UiStylePatch::default()
                },
            )
            .when(always),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-modal-file".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(always),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-modal-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface),
                    border: Some(tokens.border),
                    text: Some(tokens.text),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    ..UiStylePatch::default()
                },
            )
            .when(always),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-modal-option".to_string()),
                UiStylePatch {
                    fill: Some(tokens.surface_alt),
                    border: Some(tokens.accent),
                    ..UiStylePatch::default()
                },
            )
            .when(hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-modal-option-primary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(4.0),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(always),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-modal-option-primary".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    border: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(hovered),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-modal-confirm".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent),
                    border: Some(tokens.accent_hot),
                    border_width: Some(1.0),
                    radius: Some(3.0),
                    text: Some([255, 255, 255, 255]),
                    ..UiStylePatch::default()
                },
            )
            .when(always),
            UiStyleRule::new(
                UiStyleSelector::Class("assets-modal-confirm".to_string()),
                UiStylePatch {
                    fill: Some(tokens.accent_hot),
                    ..UiStylePatch::default()
                },
            )
            .when(hovered),
    ]);
    UiStyleSheet { rules }
}

fn asset_primitive_modal_option(
    palette: StudioUiPalette,
    primitive: raf_core::scene::Primitive,
) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new(
        format!(
            "assets.primitive-popover.option.{}",
            primitive_slug(primitive)
        ),
        UiNodeKind::Button,
    )
    .with_class("asset-primitive-popover-option")
    .with_layout(UiLayout {
        flow: UiFlow::Column,
        align_items: UiAlign::Center,
        justify_content: UiJustify::Center,
        gap: 4.0,
        padding: UiSpacing::xy(4.0, 6.0),
        ..UiLayout::fixed(56.0, 56.0)
    })
    .with_icon(
        UiIcon::new(match primitive {
            raf_core::scene::Primitive::Cube => UiIconId::Cube,
            raf_core::scene::Primitive::Sphere => UiIconId::Sphere,
            raf_core::scene::Primitive::Cylinder => UiIconId::Cylinder,
            raf_core::scene::Primitive::Plane => UiIconId::Plane,
            raf_core::scene::Primitive::Empty => UiIconId::Node,
        })
        .with_size(UiIconSize::Panel)
        .with_tint(tokens.accent),
    )
    .with_text_key(primitive_key(primitive))
    .with_text_style(UiTextStyle {
        role: UiTextRole::Label,
        size_px: 9.0,
        line_height_px: 11.0,
        weight: UiFontWeight::Regular,
        color: tokens.text,
        inherit_color: false,
    })
    .with_text_overflow(UiTextOverflow::Ellipsis)
    .with_accessibility_label_key(primitive_key(primitive))
    .focusable()
    .with_event(UiEventBinding::command(
        UiEventKind::Click,
        format!("assets.create-primitive:{}", primitive_slug(primitive)),
    ))
}

fn file_action_button(id: &str, label_key: &str, command: &str) -> UiNode {
    UiNode::new(format!("assets.file.{id}"), UiNodeKind::Button)
        .with_class("asset-file-action")
        .with_text_key(label_key)
        .with_layout(UiLayout::fit_content().with_text_safe_area(true))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.file.cancel",
        ))
}

fn script_template_button(id: &str, label_key: &str, command: &str) -> UiNode {
    UiNode::new(format!("assets.script.{id}"), UiNodeKind::Button)
        .with_class("asset-script-template")
        .with_text_key(label_key)
        .with_layout(UiLayout::fit_content().with_text_safe_area(true))
        .focusable()
        .with_event(UiEventBinding::command(UiEventKind::Click, command))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command,
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.script.cancel",
        ))
}

// ---------------------------------------------------------------------------
// Querying helpers
// ---------------------------------------------------------------------------

/// Translates `key` and substitutes its `{count}` placeholder.
fn format_count(key: &str, count: usize, language: raf_core::Language) -> String {
    raf_core::i18n::t(key, language).replace("{count}", &count.to_string())
}

/// Translates `key` and substitutes its `{name}` placeholder with the display
/// name of `row`.
pub fn format_row_name(key: &str, row: &str, language: raf_core::Language) -> String {
    raf_core::i18n::t(key, language).replace("{name}", &asset_file_name(row))
}

/// Rows the panel actually shows, in the order it shows them. Virtualization
/// must reuse this exact pipeline so a window index points at the same row the
/// full rebuild would have painted.
pub fn visible_asset_rows<'a>(
    rows: &'a [String],
    query: &str,
    filter: AssetFilter,
    script_extension: &str,
    sort: AssetSort,
) -> Vec<&'a String> {
    let mut filtered: Vec<&String> = rows
        .iter()
        .filter(|row| asset_matches(row.as_str(), query, filter))
        .filter(|row| {
            asset_matches_script_extension(row.as_str(), filter, script_extension)
        })
        .collect();
    sort_asset_rows(&mut filtered, sort);
    filtered
}

/// Window of rows to materialize for the current scroll offset.
pub fn asset_visible_range(
    rows: &[String],
    query: &str,
    filter: AssetFilter,
    script_extension: &str,
    sort: AssetSort,
    scroll_offset: f32,
    viewport_height: f32,
    view: AssetViewMode,
) -> (usize, usize) {
    let count = visible_asset_rows(rows, query, filter, script_extension, sort).len();
    if count == 0 || viewport_height <= 0.0 {
        return (0, 0);
    }
    let pitch = match view {
        AssetViewMode::Grid => ASSET_CARD_PITCH,
        AssetViewMode::List => ASSET_LIST_PITCH,
    };
    let capacity = (viewport_height / pitch.max(1.0)).ceil() as usize;
    if capacity >= count {
        return (0, count);
    }
    let range = raf_ui::UiVirtualRange::for_vertical_list(
        count,
        scroll_offset.max(0.0),
        viewport_height,
        pitch,
        4,
    );
    (range.start.min(count), range.end.min(count))
}

fn asset_virtual_spacer_height(row_count: usize, view: AssetViewMode) -> f32 {
    if row_count == 0 {
        return 0.0;
    }
    let pitch = match view {
        AssetViewMode::Grid => ASSET_CARD_PITCH,
        AssetViewMode::List => ASSET_LIST_PITCH,
    };
    row_count as f32 * pitch - ASSET_CARD_GAP
}

fn asset_matches(row: &str, query: &str, filter: AssetFilter) -> bool {
    let lower = row.to_lowercase();
    let query = query.trim().to_lowercase();
    let query_matches = query.is_empty() || lower.contains(&query);
    if !query_matches {
        return false;
    }
    let extension = Path::new(row)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    match filter {
        AssetFilter::All => true,
        AssetFilter::Images => matches!(
            extension.as_deref(),
            Some("png")
                | Some("jpg")
                | Some("jpeg")
                | Some("bmp")
                | Some("tga")
                | Some("webp")
                | Some("svg")
        ),
        AssetFilter::Models => matches!(
            extension.as_deref(),
            Some("obj") | Some("gltf") | Some("glb") | Some("fbx") | Some("stl")
        ),
        AssetFilter::Audio => matches!(
            extension.as_deref(),
            Some("wav") | Some("mp3") | Some("ogg") | Some("flac")
        ),
        AssetFilter::Scripts => matches!(
            extension.as_deref(),
            Some("rhai")
                | Some("rs")
                | Some("lua")
                | Some("py")
                | Some("cpp")
                | Some("cc")
                | Some("cxx")
                | Some("js")
                | Some("ts")
        ),
    }
}

fn asset_icon(name: &str) -> UiIconId {
    if let Some(slug) = name.strip_prefix("builtin://primitive/") {
        return match crate::panels::primitive_create::parse_primitive_slug(slug) {
            Some(raf_core::scene::Primitive::Cube) => UiIconId::Cube,
            Some(raf_core::scene::Primitive::Sphere) => UiIconId::Sphere,
            Some(raf_core::scene::Primitive::Cylinder) => UiIconId::Cylinder,
            Some(raf_core::scene::Primitive::Plane) => UiIconId::Plane,
            _ => UiIconId::Assets,
        };
    }
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("ron") | Some("toml") | Some("json") => UiIconId::Project,
        Some("scene") | Some("world") => UiIconId::Scene,
        Some(extension) if is_script_file(&format!("asset.{extension}")) => UiIconId::Script,
        Some("pcb") => UiIconId::Pcb,
        Some("sch") | Some("kicad_sch") => UiIconId::Schematic,
        Some("png") | Some("jpg") | Some("jpeg") | Some("bmp") | Some("tga") | Some("webp")
        | Some("svg") | Some("obj") | Some("gltf") | Some("glb") | Some("fbx") | Some("stl")
        | Some("wav") | Some("mp3") | Some("ogg") | Some("flac") => UiIconId::Assets,
        _ => UiIconId::File,
    }
}

fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params<'a>(palette: StudioUiPalette, rows: &'a [String]) -> AssetsSurfaceParams<'a> {
        AssetsSurfaceParams {
            palette,
            rows,
            language: raf_core::Language::English,
            query: "",
            filter: AssetFilter::All,
            script_extension: "",
            sort: AssetSort::NameAsc,
            view: AssetViewMode::Grid,
            selected: None,
            highlight: None,
            status: None,
            pending: false,
            catalog_error: None,
            visible_range: None,
            create_open: false,
            motion: AssetsMotion::default(),
        }
    }

    #[test]
    fn asset_icons_follow_known_project_file_types() {
        assert_eq!(asset_icon("main.scene"), UiIconId::Scene);
        assert_eq!(asset_icon("board.pcb"), UiIconId::Pcb);
        assert_eq!(asset_icon("config.ron"), UiIconId::Project);
        assert_eq!(asset_icon("Building_A.glb"), UiIconId::Assets);
        assert_eq!(asset_icon("scripts/player.rs"), UiIconId::Script);
        assert_eq!(asset_icon("readme.unknown"), UiIconId::File);
    }

    #[test]
    fn game_assets_include_truthful_builtin_primitive_rows() {
        let rows = asset_rows_with_builtins(&["models/tree.glb".to_string()]);
        assert_eq!(rows.len(), BUILTIN_ASSET_ROWS.len() + 1);
        assert_eq!(asset_display_name(&rows[0]), "Built-in / Cube");
        assert_eq!(asset_icon(&rows[1]), UiIconId::Sphere);
        assert_eq!(rows.last().map(String::as_str), Some("models/tree.glb"));
    }

    #[test]
    fn builtin_rows_stay_above_files_and_sorting_is_reversible() {
        let owned = vec![
            "scripts/zeta.rs".to_string(),
            "models/tree.glb".to_string(),
            "builtin://primitive/cube".to_string(),
            "scripts/alpha.rs".to_string(),
        ];
        let mut rows: Vec<&String> = owned.iter().collect();
        sort_asset_rows(&mut rows, AssetSort::NameAsc);
        assert_eq!(rows[0], "builtin://primitive/cube");
        assert_eq!(rows[1], "scripts/alpha.rs");
        assert_eq!(rows[2], "models/tree.glb");
        assert_eq!(rows[3], "scripts/zeta.rs");

        sort_asset_rows(&mut rows, AssetSort::NameDesc);
        assert_eq!(rows[0], "builtin://primitive/cube");
        assert_eq!(rows[1], "scripts/zeta.rs");
        assert_eq!(rows.last().map(|row| row.as_str()), Some("scripts/alpha.rs"));
    }

    #[test]
    fn script_language_filter_narrows_rows_only_in_the_scripts_category() {
        let rows = asset_rows_with_builtins(&[
            "scripts/player.rs".to_string(),
            "scripts/level.cpp".to_string(),
        ]);
        let visible = |filter, extension: &str| {
            rows.iter()
                .filter(|row| {
                    asset_matches(row, "", filter)
                        && asset_matches_script_extension(row, filter, extension)
                })
                .count()
        };
        assert_eq!(visible(AssetFilter::Scripts, "rs"), 1);
        assert_eq!(visible(AssetFilter::Scripts, ""), 2);
        // The chip only narrows the Scripts category, never the full listing.
        assert_eq!(visible(AssetFilter::All, "rs"), rows.len());
    }

    #[test]
    fn card_meta_shows_a_language_chip_for_scripts_and_a_folder_for_media() {
        assert_eq!(
            asset_meta("scripts/player.rs"),
            (Some("RS".to_string()), None)
        );
        assert_eq!(asset_meta("models/tree.glb"), (None, Some("models".to_string())));
        // Built-in rows are authoring shortcuts, so they show no file meta.
        assert_eq!(asset_meta("builtin://primitive/cube"), (None, None));
    }

    #[test]
    fn the_visible_range_materializes_every_row_that_fits() {
        let rows: Vec<String> = (0..40)
            .map(|index| format!("models/tree{index}.glb"))
            .collect();
        let (start, end) = asset_visible_range(
            &rows,
            "",
            AssetFilter::Models,
            "",
            AssetSort::NameAsc,
            0.0,
            ASSET_CARD_PITCH * 40.0,
            AssetViewMode::Grid,
        );
        assert_eq!((start, end), (0, 40));

        // A short viewport only builds the window plus the overscan.
        let (start, end) = asset_visible_range(
            &rows,
            "",
            AssetFilter::Models,
            "",
            AssetSort::NameAsc,
            0.0,
            ASSET_CARD_PITCH * 3.0,
            AssetViewMode::Grid,
        );
        assert_eq!(start, 0);
        assert!(end < 40);
        assert!(end - start >= 3);
    }

    #[test]
    fn the_visible_range_follows_the_scroll_offset() {
        let rows: Vec<String> = (0..40)
            .map(|index| format!("models/tree{index:02}.glb"))
            .collect();
        let viewport = ASSET_CARD_PITCH * 3.0;
        let (_, end) = asset_visible_range(
            &rows,
            "",
            AssetFilter::Models,
            "",
            AssetSort::NameAsc,
            0.0,
            viewport,
            AssetViewMode::Grid,
        );
        let (start, next_end) = asset_visible_range(
            &rows,
            "",
            AssetFilter::Models,
            "",
            AssetSort::NameAsc,
            ASSET_CARD_PITCH * 10.0,
            viewport,
            AssetViewMode::Grid,
        );
        assert!(start > 0);
        assert!(next_end > end);
        assert!(start < 40);
    }

    #[test]
    fn the_visible_range_uses_the_same_filter_and_order_as_the_panel() {
        let rows = asset_rows_with_builtins(&[
            "scripts/player.rs".to_string(),
            "scripts/level.cpp".to_string(),
            "models/tree.glb".to_string(),
        ]);
        // The Scripts category only matches real script files, so the two
        // built-in primitives stay out of the window the panel builds.
        let visible = visible_asset_rows(&rows, "", AssetFilter::Scripts, "", AssetSort::NameAsc);
        assert_eq!(visible.len(), 2);
        let (start, end) = asset_visible_range(
            &rows,
            "",
            AssetFilter::Scripts,
            "",
            AssetSort::NameAsc,
            0.0,
            ASSET_CARD_PITCH * 10.0,
            AssetViewMode::Grid,
        );
        assert_eq!((start, end), (0, visible.len()));
    }

    #[test]
    fn grid_cards_are_much_taller_than_list_rows_so_the_pitch_follows_the_view() {
        assert!(ASSET_CARD_PITCH > ASSET_LIST_PITCH * 2.0);
        assert_eq!(
            asset_virtual_spacer_height(2, AssetViewMode::List),
            2.0 * ASSET_LIST_PITCH - ASSET_CARD_GAP
        );
    }

    #[test]
    fn panel_and_overlays_build_from_data_only() {
        let rows = vec!["scripts/player.rs".to_string()];
        let palette = StudioUiPalette::IndustrialDark;
        let surface = build_assets_surface(params(palette, &rows));
        assert_eq!(surface.id, "editor.bottom.assets");

        let backdrop = build_assets_backdrop_surface(palette);
        assert_eq!(backdrop.id, "editor.assets.backdrop");
        assert!(build_assets_open_modal_surface(palette, "scripts/player.rs", "assets/scripts/player.rs")
            .style_sheet
            .rules
            .iter()
            .any(|rule| rule.selector == UiStyleSelector::Class("assets-modal".to_string())));
        assert!(build_assets_context_menu_surface(palette, "scripts/player.rs", false, "assets/scripts/player.rs")
            .root
            .find("assets.context.delete")
            .is_some());
    }

    #[test]
    fn the_open_modal_exposes_the_four_documented_ways_to_open_a_row() {
        let palette = StudioUiPalette::IndustrialDark;
        let modal = build_assets_open_modal_surface(palette, "scripts/player.rs", "assets/scripts/player.rs");
        for id in [
            "assets.open-modal.primary",
            "assets.open-modal.editor",
            "assets.open-modal.manager",
            "assets.open-modal.with",
        ] {
            assert!(modal.root.find(id).is_some(), "missing {id}");
        }
    }

    #[test]
    fn an_absolute_path_is_built_from_the_project_root_and_falls_back_to_the_row() {
        assert_eq!(
            asset_absolute_path(Some(Path::new("C:/project")), "scripts/player.rs"),
            std::path::Path::new("C:/project")
                .join("assets")
                .join("scripts/player.rs")
                .to_string_lossy()
        );
        assert_eq!(
            asset_absolute_path(None, "scripts/player.rs"),
            "assets/scripts/player.rs"
        );
        assert_eq!(
            asset_absolute_path(Some(Path::new("C:/project")), "builtin://primitive/cube"),
            "builtin://primitive/cube"
        );
    }
}