//! Retained RafUI document for the editor Assets browser.

use std::path::Path;

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiCompactMode, UiEventBinding, UiEventKind,
    UiFlow, UiFontWeight, UiIcon, UiIconId, UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind,
    UiOverflow, UiScrollAxis, UiSizeMode, UiSpacing, UiStyle, UiSurface, UiSurfaceMaterial,
    UiTextOverflow, UiTextRole, UiTextStyle,
};

use crate::panels::editor_bottom_dock_styles::bottom_style_sheet;
use crate::panels::primitive_create::{
    primitive_key, primitive_name, primitive_slug, CREATEABLE_PRIMITIVES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetFilter {
    All,
    Images,
    Models,
    Audio,
    Scripts,
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

pub fn build_assets_surface(
    palette: StudioUiPalette,
    asset_rows: &[String],
    query: &str,
    filter: AssetFilter,
    visible_range: Option<(usize, usize)>,
    script_menu_open: bool,
    script_name: &str,
    primitive_menu_open: bool,
    dragging_asset: Option<&str>,
    file_menu_open: bool,
    file_name: &str,
) -> UiSurface {
    let tokens = palette.tokens();
    if primitive_menu_open {
        let root = primitive_create_modal_root(palette);
        let mut surface = UiSurface::new("editor.bottom.assets", palette, root);
        surface.style_sheet = bottom_style_sheet(palette);
        return surface;
    }
    let filtered_rows = asset_rows
        .iter()
        .enumerate()
        .filter(|(_, row)| asset_matches(row.as_str(), query, filter))
        .collect::<Vec<_>>();
    let total_rows = filtered_rows.len();
    let (start, end) = visible_range
        .map(|(start, end)| {
            let start = start.min(total_rows);
            (start, end.min(total_rows).max(start))
        })
        .unwrap_or((0, total_rows));

    let mut grid = UiNode::scroll_view("assets.grid", UiScrollAxis::Vertical)
        .with_class("assets-grid")
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            grow: 1.0,
            gap: ASSET_CARD_GAP,
            padding: UiSpacing::xy(12.0, 12.0),
            overflow: UiOverflow::ScrollY,
            compact: UiCompactMode::Wrap,
            ..UiLayout::default()
        });
    if start > 0 {
        grid = grid.with_child(spacer(
            "assets.top-spacer",
            asset_virtual_spacer_height(start),
        ));
    }
    for (source_index, row) in filtered_rows.iter().skip(start).take(end - start) {
        grid = grid.with_child(asset_card(
            palette,
            *source_index,
            row.as_str(),
            dragging_asset == Some(row.as_str()),
        ));
    }
    if end < total_rows {
        grid = grid.with_child(spacer(
            "assets.bottom-spacer",
            asset_virtual_spacer_height(total_rows - end),
        ));
    }
    if filtered_rows.is_empty() {
        let mut empty = UiNode::new("assets.empty", UiNodeKind::Panel)
            .with_class("assets-empty")
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                grow: 1.0,
                align_items: UiAlign::Center,
                justify_content: UiJustify::Center,
                gap: 4.0,
                min_size: [0.0, 72.0],
                ..UiLayout::fill(UiFlow::Column)
            })
            .with_child(
                UiNode::new("assets.empty.title", UiNodeKind::Label)
                    .with_text_key(if asset_rows.is_empty() {
                        "app.no_assets"
                    } else {
                        "app.search_no_results"
                    })
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            );
        if asset_rows.is_empty() {
            empty = empty.with_child(
                UiNode::new("assets.empty.hint", UiNodeKind::Label)
                    .with_text_key("app.drag_drop_hint")
                    .with_layout(UiLayout::fit_content())
                    .with_text_style(UiTextStyle::body(tokens.text_muted)),
            );
        }
        grid = grid.with_child(empty);
    }

    let mut root = UiNode::new("assets.root", UiNodeKind::Panel)
        .with_class("bottom-panel")
        .with_layout(UiLayout::fill(UiFlow::Row))
        .with_style(palette.panel_style())
        .with_child(asset_sidebar(palette, filter))
        .with_child(
            UiNode::new("assets.content", UiNodeKind::Panel)
                .with_class("assets-content")
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(
                    UiNode::new("assets.toolbar", UiNodeKind::Toolbar)
                        .with_class("assets-toolbar")
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            align_items: UiAlign::Center,
                            gap: 6.0,
                            padding: UiSpacing::xy(10.0, 4.0),
                            ..UiLayout::fixed(0.0, 36.0)
                        })
                        .with_child(
                            UiNode::new("assets.create-script", UiNodeKind::Button)
                                .with_class("asset-action asset-create-script")
                                .with_icon(UiIcon::new(UiIconId::Node).with_size(UiIconSize::Small))
                                .with_text_key("app.create_script")
                                .with_accessibility_label_key("app.create_script")
                                .with_layout(UiLayout::fit_content().with_text_safe_area(true))
                                .focusable()
                                .with_event(UiEventBinding::command(
                                    UiEventKind::Click,
                                    "assets.create-script",
                                ))
                                .with_event(UiEventBinding::command(
                                    UiEventKind::KeyPress("enter".to_string()),
                                    "assets.create-script",
                                ))
                                .with_event(UiEventBinding::command(
                                    UiEventKind::KeyPress("space".to_string()),
                                    "assets.create-script",
                                )),
                        )
                        .with_child(
                            UiNode::new("assets.create-primitive", UiNodeKind::Button)
                                .with_class("asset-action asset-create-primitive")
                                .with_icon(UiIcon::new(UiIconId::Cube).with_size(UiIconSize::Small))
                                .with_text_key("app.create_primitive")
                                .with_accessibility_label_key("app.create_primitive")
                                .with_layout(UiLayout::fit_content().with_text_safe_area(true))
                                .focusable()
                                .with_event(UiEventBinding::command(
                                    UiEventKind::Click,
                                    "assets.create-primitive.toggle",
                                )),
                        )
                        .with_child(
                            UiNode::new("assets.refresh", UiNodeKind::Button)
                                .with_class("asset-action")
                                .with_icon(UiIcon::new(UiIconId::Undo).with_size(UiIconSize::Small))
                                .with_tooltip_key("app.refresh_assets")
                                .with_accessibility_label_key("app.refresh_assets")
                                .with_layout(UiLayout::fixed(28.0, 28.0))
                                .focusable()
                                .with_event(UiEventBinding::command(
                                    UiEventKind::Click,
                                    "assets.refresh",
                                ))
                                .with_event(UiEventBinding::command(
                                    UiEventKind::KeyPress("enter".to_string()),
                                    "assets.refresh",
                                ))
                                .with_event(UiEventBinding::command(
                                    UiEventKind::KeyPress("space".to_string()),
                                    "assets.refresh",
                                )),
                        ),
                )
                .with_child(grid),
        );

    if script_menu_open {
        root = root.with_child(script_template_popover(palette, script_name));
    }
    if file_menu_open {
        root = root.with_child(file_create_popover(palette, file_name));
    }

    let mut surface = UiSurface::new("editor.bottom.assets", palette, root);
    surface.style_sheet = bottom_style_sheet(palette);
    surface
}

fn asset_sidebar(palette: StudioUiPalette, active: AssetFilter) -> UiNode {
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
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [0.0, 28.0],
                ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
            }),
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
    sidebar = sidebar
        .with_child(asset_category_row(
            palette,
            "all",
            UiIconId::Assets,
            "app.assets_category_all",
            active == AssetFilter::All,
        ))
        .with_child(asset_category_row(
            palette,
            "models",
            UiIconId::Folder,
            "app.models",
            active == AssetFilter::Models,
        ))
        .with_child(asset_category_row(
            palette,
            "images",
            UiIconId::Shaded,
            "app.images",
            active == AssetFilter::Images,
        ))
        .with_child(asset_category_row(
            palette,
            "audio",
            UiIconId::Play,
            "app.audio",
            active == AssetFilter::Audio,
        ))
        .with_child(asset_category_row(
            palette,
            "scripts",
            UiIconId::Node,
            "app.scripts_filter",
            active == AssetFilter::Scripts,
        ));
    sidebar = sidebar.with_child(
        UiNode::new("assets.sidebar.spacer", UiNodeKind::Panel).with_layout(UiLayout {
            grow: 1.0,
            ..UiLayout::default()
        }),
    );
    sidebar
}

fn asset_category_row(
    palette: StudioUiPalette,
    id: &str,
    icon: UiIconId,
    label_key: &str,
    active: bool,
) -> UiNode {
    let tokens = palette.tokens();
    let is_active = active;
    let command = format!("assets.filter.{id}");
    UiNode::new(format!("assets.sidebar.row.{id}"), UiNodeKind::Button)
        .with_class(if is_active {
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
        .with_style(UiStyle {
            fill: if is_active {
                tokens.surface
            } else {
                [0, 0, 0, 0]
            },
            border: if is_active {
                tokens.focus
            } else {
                [0, 0, 0, 0]
            },
            text: if is_active {
                tokens.text
            } else {
                tokens.text_muted
            },
            border_width: if is_active { 1.0 } else { 0.0 },
            radius: if is_active { 3.0 } else { 0.0 },
            opacity: 1.0,
        })
        .with_icon(UiIcon::new(icon).with_size(UiIconSize::Small))
        .with_text_key(label_key)
        .with_text_style(UiTextStyle::body(if is_active {
            tokens.text
        } else {
            tokens.text_muted
        }))
        .with_text_overflow(UiTextOverflow::Ellipsis)
        .with_tooltip_key(label_key)
        .with_accessibility_label_key(label_key)
        .with_accessibility_role(UiAccessibilityRole::Tab)
        .with_accessibility_selected(is_active)
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

fn primitive_create_modal_root(palette: StudioUiPalette) -> UiNode {
    let tokens = palette.tokens();
    let mut popover = UiNode::new("assets.primitive-popover", UiNodeKind::Menu)
        .with_class("asset-primitive-popover")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::xy(10.0, 8.0),
            align_self: Some(UiAlign::Stretch),
            ..UiLayout::fixed(284.0, 0.0)
                .with_height_mode(UiSizeMode::FitContent)
                .with_z_index(40)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.accent,
            text: tokens.text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 1.0,
        })
        .with_accessibility_label_key("app.create_primitive")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.create-primitive.cancel",
        ));
    popover = popover.with_child(
        UiNode::new("assets.primitive-popover.title", UiNodeKind::Label)
            .with_text_key("app.create_primitive")
            .with_text_style(UiTextStyle::panel_title(tokens.text))
            .with_layout(UiLayout::fit_content()),
    );
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
    popover = popover.with_child(grid);
    popover.with_child(
        UiNode::new("assets.primitive-popover.cancel", UiNodeKind::Button)
            .with_class("asset-primitive-cancel")
            .with_text_key("app.cancel")
            .with_text_style(UiTextStyle::button(tokens.text_muted))
            .with_layout(UiLayout::fit_content().with_text_safe_area(true))
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                "assets.create-primitive.cancel",
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("enter".to_string()),
                "assets.create-primitive.cancel",
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("space".to_string()),
                "assets.create-primitive.cancel",
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("escape".to_string()),
                "assets.create-primitive.cancel",
            )),
    )
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

const ASSET_ROW_HEIGHT: f32 = 32.0;
const ASSET_ROW_GAP: f32 = 3.0;
const ASSET_ROW_ESTIMATE: f32 = ASSET_ROW_HEIGHT + ASSET_ROW_GAP;
const ASSET_CARD_WIDTH: f32 = 108.0;
const ASSET_CARD_HEIGHT: f32 = 110.0;
const ASSET_CARD_GAP: f32 = 6.0;
const ASSET_SIDEBAR_WIDTH: f32 = 184.0;

fn asset_virtual_spacer_height(row_count: usize) -> f32 {
    if row_count == 0 {
        0.0
    } else {
        row_count as f32 * ASSET_ROW_ESTIMATE - ASSET_ROW_GAP
    }
}

fn asset_card(palette: StudioUiPalette, index: usize, row: &str, dragging: bool) -> UiNode {
    let tokens = palette.tokens();
    let command = format!("assets.open:{row}");
    let drag_start = format!("assets.drag.start:{row}");
    let drag_end = format!("assets.drag.end:{row}");
    let class = if dragging {
        "asset-card asset-card-dragging"
    } else {
        "asset-card"
    };
    let display_name = asset_display_name(row);
    UiNode::new(format!("assets.card.{index}"), UiNodeKind::Button)
        .with_class(class)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            align_items: UiAlign::Center,
            justify_content: UiJustify::Center,
            gap: 6.0,
            padding: UiSpacing::xy(6.0, 10.0),
            ..UiLayout::fixed(ASSET_CARD_WIDTH, ASSET_CARD_HEIGHT)
        })
        .with_style(UiStyle {
            fill: tokens.surface,
            border: tokens.border,
            text: tokens.text,
            border_width: 1.0,
            radius: 4.0,
            opacity: 1.0,
        })
        .with_tooltip_key(display_name.clone())
        .with_accessibility_label_key(display_name.clone())
        .with_accessibility_role(UiAccessibilityRole::Button)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::DoubleClick,
            command.clone(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command.clone(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("space".to_string()),
            command,
        ))
        .with_event(UiEventBinding::command(UiEventKind::DragStart, drag_start))
        .with_event(UiEventBinding::command(UiEventKind::DragEnd, drag_end))
        .with_child(
            UiNode::new(format!("assets.card.{index}.icon"), UiNodeKind::Label)
                .with_icon(UiIcon::new(asset_icon(row)).with_size(UiIconSize::Panel))
                .with_layout(UiLayout::fit_content()),
        )
        .with_child(
            UiNode::new(format!("assets.card.{index}.label"), UiNodeKind::Label)
                .with_text_value(display_name)
                .with_text_style(UiTextStyle {
                    role: UiTextRole::Label,
                    size_px: 10.0,
                    line_height_px: 13.0,
                    weight: UiFontWeight::Regular,
                    color: tokens.text,
                    inherit_color: false,
                })
                .with_text_overflow(UiTextOverflow::Ellipsis)
                .with_layout(UiLayout::fixed(ASSET_CARD_WIDTH - 12.0, 26.0)),
        )
}

fn asset_display_name(row: &str) -> String {
    row.strip_prefix("builtin://primitive/")
        .and_then(crate::panels::primitive_create::parse_primitive_slug)
        .map(|primitive| format!("Built-in / {}", primitive_name(primitive)))
        .unwrap_or_else(|| row.to_string())
}

fn script_template_popover(palette: StudioUiPalette, _script_name: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("assets.script-popover", UiNodeKind::Menu)
        .with_class("asset-script-popover")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            padding: UiSpacing::xy(8.0, 6.0),
            align_self: Some(UiAlign::Stretch),
            ..UiLayout::fit_content()
                .with_width_mode(UiSizeMode::Fill)
                .with_z_index(20)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.accent,
            text: tokens.text,
            border_width: 1.0,
            radius: 3.0,
            opacity: 0.98,
        })
        .with_accessibility_label_key("app.create_script")
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.script.cancel",
        ))
        .with_child(
            UiNode::new("assets.script-popover.title", UiNodeKind::Label)
                .with_text_key("app.create_script")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
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
                            UiEventKind::KeyPress("enter".to_string()),
                            "assets.script.cancel",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("space".to_string()),
                            "assets.script.cancel",
                        ))
                        .with_event(UiEventBinding::command(
                            UiEventKind::KeyPress("escape".to_string()),
                            "assets.script.cancel",
                        )),
                ),
        )
}

fn file_create_popover(palette: StudioUiPalette, file_name: &str) -> UiNode {
    let tokens = palette.tokens();
    UiNode::new("assets.file-popover", UiNodeKind::Menu)
        .with_class("asset-file-popover")
        .with_material(UiSurfaceMaterial::TranslucentRaised)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            padding: UiSpacing::same(8.0),
            align_self: Some(UiAlign::Stretch),
            ..UiLayout::fit_content()
                .with_width_mode(UiSizeMode::Fill)
                .with_z_index(20)
        })
        .with_style(UiStyle {
            fill: tokens.surface_raised,
            border: tokens.accent,
            text: tokens.text,
            border_width: 1.0,
            radius: 5.0,
            opacity: 1.0,
        })
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("escape".to_string()),
            "assets.file.cancel",
        ))
        .with_child(
            UiNode::new("assets.file-title", UiNodeKind::Label)
                .with_text_key("app.create_file")
                .with_text_style(UiTextStyle::panel_title(tokens.text))
                .with_layout(UiLayout::fit_content()),
        )
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
        )
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

pub fn asset_visible_range(
    asset_rows: &[String],
    query: &str,
    filter: AssetFilter,
    scroll_offset: f32,
    viewport_height: f32,
) -> (usize, usize) {
    let count = asset_rows
        .iter()
        .filter(|row| asset_matches(row, query, filter))
        .count();
    let range = raf_ui::UiVirtualRange::for_vertical_list(
        count,
        scroll_offset,
        viewport_height,
        ASSET_ROW_ESTIMATE,
        4,
    );
    (range.start, range.end)
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
        Some("rhai") | Some("rs") | Some("lua") | Some("py") | Some("cpp") | Some("cc")
        | Some("cxx") | Some("js") | Some("ts") => UiIconId::Node,
        Some("pcb") => UiIconId::Pcb,
        Some("sch") | Some("kicad_sch") => UiIconId::Schematic,
        Some("png") | Some("jpg") | Some("jpeg") | Some("bmp") | Some("tga") | Some("webp")
        | Some("svg") | Some("obj") | Some("gltf") | Some("glb") | Some("fbx") | Some("stl")
        | Some("wav") | Some("mp3") | Some("ogg") | Some("flac") => UiIconId::Assets,
        _ => UiIconId::Assets,
    }
}

fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_icons_follow_known_project_file_types() {
        assert_eq!(asset_icon("main.scene"), UiIconId::Scene);
        assert_eq!(asset_icon("board.pcb"), UiIconId::Pcb);
        assert_eq!(asset_icon("config.ron"), UiIconId::Project);
        assert_eq!(asset_icon("Building_A.glb"), UiIconId::Assets);
    }

    #[test]
    fn game_assets_include_truthful_builtin_primitive_rows() {
        let rows = asset_rows_with_builtins(&["models/tree.glb".to_string()]);
        assert_eq!(rows.len(), BUILTIN_ASSET_ROWS.len() + 1);
        assert_eq!(asset_display_name(&rows[0]), "Built-in / Cube");
        assert_eq!(asset_icon(&rows[1]), UiIconId::Sphere);
        assert_eq!(rows.last().map(String::as_str), Some("models/tree.glb"));
    }
}
