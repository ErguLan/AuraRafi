//! Catalog rows, metadata, icons, filters and sorting for the Assets panel.
//!
//! Moved from the surface facade without behavior changes.

use std::path::Path;

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAccessibilityRole, UiAlign, UiEventBinding, UiEventKind, UiFlow,
    UiFontWeight, UiIcon, UiIconId, UiIconSize, UiLayout, UiNode, UiNodeKind, UiSizeMode,
    UiSpacing, UiStyle, UiTextOverflow, UiTextRole, UiTextStyle,
};

use crate::panels::primitive_create::primitive_name;
use crate::script_support::is_script_file;

use super::{
    AssetFilter, AssetSort, AssetViewMode, AssetsMotion, AssetsStatus, AssetsStatusKind,
    ASSET_CARD_GAP, ASSET_CARD_HEIGHT, ASSET_CARD_PITCH, ASSET_CARD_WIDTH,
    ASSET_GRID_FALLBACK_WIDTH, ASSET_GRID_PADDING_X, ASSET_LIST_PITCH, ASSET_LIST_ROW_GAP,
    ASSET_LIST_ROW_HEIGHT, ASSET_SELECTION_BAR_HEIGHT, ASSET_TOOLBAR_HEIGHT,
};

/// Virtual assets that are always available in a Game project. They are
/// authoring shortcuts, not files on disk; opening or dropping one still
/// executes the real scene-creation path in the application boundary.
pub const BUILTIN_ASSET_ROWS: [&str; 17] = [
    "builtin://primitive/cube",
    "builtin://primitive/sphere",
    "builtin://primitive/cylinder",
    "builtin://primitive/capsule",
    "builtin://primitive/plane",
    "builtin://primitive/wedge",
    "builtin://primitive/torus",
    "builtin://primitive/prism",
    "builtin://primitive/sphere2d",
    "builtin://camera",
    "builtin://light/point",
    "builtin://light/spot",
    "builtin://interface/frame",
    "builtin://interface/label",
    "builtin://interface/image",
    "builtin://interface/button",
    "builtin://interface/input",
];

/// Virtual rows that can be inserted from the Assets panel.
pub const BUILTIN_LIGHT_ROWS: [&str; 2] = ["builtin://light/point", "builtin://light/spot"];

pub fn asset_rows_with_builtins(asset_rows: &[String]) -> Vec<String> {
    raf_core::scene::geometry::CREATABLE_PRIMITIVES
        .into_iter()
        .map(|shape| shape.descriptor().source.to_string())
        .chain(std::iter::once("builtin://camera".to_string()))
        .chain(BUILTIN_LIGHT_ROWS.iter().map(|row| row.to_string()))
        .chain(
            BUILTIN_ASSET_ROWS
                .iter()
                .filter(|s| s.starts_with("builtin://interface/"))
                .map(|s| s.to_string()),
        )
        .chain(asset_rows.iter().cloned())
        .collect()
}

pub fn is_builtin_light_row(row: &str) -> bool {
    BUILTIN_LIGHT_ROWS.contains(&row)
}

/// Command the host must execute to create this virtual row.
///
/// Lights are the only built-ins without an `assets.open:` entry in the host:
/// the card emits `hierarchy.create-light:point|spot` directly. The lead wires
/// those commands to the scene light creation path.
pub fn builtin_asset_command(row: &str) -> Option<String> {
    match row {
        "builtin://light/point" => Some("hierarchy.create-light:point".to_string()),
        "builtin://light/spot" => Some("hierarchy.create-light:spot".to_string()),
        _ => None,
    }
}

pub(super) fn asset_open_command(row: &str) -> String {
    if let Some(command) = builtin_asset_command(row) {
        return command;
    }
    let lower = row.to_ascii_lowercase();
    // Opening a model or an Assembly means placing it in the scene. Only
    // documents that have their own viewer (images, animations, interfaces)
    // fall through to `assets.open`.
    if lower.ends_with(".glb") || lower.ends_with(".gltf") || lower.ends_with(".prefab.ron") {
        format!("assets.insert:{row}")
    } else {
        format!("assets.open:{row}")
    }
}

pub(crate) fn asset_scroll_projection_changed(
    before: f32,
    after: f32,
    view: AssetViewMode,
) -> bool {
    let pitch = match view {
        AssetViewMode::Grid => ASSET_CARD_PITCH,
        AssetViewMode::List => ASSET_LIST_PITCH,
    }
    .max(1.0);
    (before.max(0.0) / pitch).floor() != (after.max(0.0) / pitch).floor()
}

pub(super) fn asset_toolbar(
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
            ..UiLayout::fixed(0.0, ASSET_TOOLBAR_HEIGHT).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            // Primary entry point for the declarative import modal. The host
            // listens for `assets.import-open`, opens the modal and focuses
            // `assets.import.name`. The actual file dialog/drop path remains
            // native because RafUI surfaces never touch the filesystem.
            UiNode::new("assets.import-trigger", UiNodeKind::Button)
                .with_class("asset-action asset-import-trigger")
                .with_text_key("electronics.footprint.import_file")
                .with_accessibility_label_key("electronics.footprint.import_file")
                .with_icon(UiIcon::new(UiIconId::Add).with_size(UiIconSize::Small))
                .with_layout(UiLayout::fixed(88.0, 26.0))
                .focusable()
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.import-open",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("enter".to_string()),
                    "assets.import-open",
                ))
                .with_event(UiEventBinding::command(
                    UiEventKind::KeyPress("space".to_string()),
                    "assets.import-open",
                )),
        )
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
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.sort.toggle",
                )),
        )
        .with_child(
            UiNode::new("assets.view", UiNodeKind::Button)
                .with_class("asset-action")
                .with_icon(
                    UiIcon::new(if view == AssetViewMode::Grid {
                        UiIconId::Menu
                    } else {
                        UiIconId::Grid
                    })
                    .with_size(UiIconSize::Small),
                )
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
                .with_event(UiEventBinding::command(
                    UiEventKind::Click,
                    "assets.view.toggle",
                )),
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

pub(super) fn asset_status_strip(
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

pub(super) fn asset_card(
    palette: StudioUiPalette,
    index: usize,
    row: &str,
    view: AssetViewMode,
    selected: bool,
    highlighted: bool,
    motion: AssetsMotion,
) -> UiNode {
    let tokens = palette.tokens();
    let open_command = asset_open_command(row);
    let select_command = format!("assets.select:{row}");
    let builtin_light = is_builtin_light_row(row);
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

    let mut node = node
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            format!("assets.drag-start:{row}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            "assets.drag-move",
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            "assets.drag-end",
        ))
        .with_tooltip_value(format!("{row}"))
        .with_accessibility_label_key(name.clone())
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_selected(selected)
        .focusable();
    if builtin_light {
        // `hierarchy.create-light:point|spot` is the documented integration
        // point. The Assets host must forward it to scene light creation; the
        // panel intentionally keeps no scene state.
        node = node
            .with_event(UiEventBinding::command(
                UiEventKind::Click,
                open_command.clone(),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("enter".to_string()),
                open_command.clone(),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("space".to_string()),
                open_command.clone(),
            ));
    } else {
        node = node
            .with_event(UiEventBinding::command(UiEventKind::Click, select_command))
            .with_event(UiEventBinding::command(
                UiEventKind::DoubleClick,
                open_command.clone(),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("enter".to_string()),
                open_command.clone(),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::KeyPress("space".to_string()),
                open_command,
            ));
    }
    node.with_event(UiEventBinding::command(
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
pub(super) fn selection_action_bar(
    palette: StudioUiPalette,
    row: &str,
    highlight: Option<&str>,
) -> UiNode {
    let tokens = palette.tokens();
    if row.is_empty() {
        return UiNode::new("assets.selection", UiNodeKind::Panel)
            .with_layout(UiLayout::fixed(0.0, 0.0).with_width_mode(UiSizeMode::Fill));
    }
    let builtin = is_builtin_row(row);
    let light = is_builtin_light_row(row);
    let open_command = asset_open_command(row);
    let placeable = open_command.starts_with("assets.insert:");
    let mut actions: Vec<(UiIconId, &'static str, String)> = vec![(
        if placeable || builtin {
            UiIconId::Add
        } else {
            UiIconId::ExternalLink
        },
        if light || placeable {
            "assets.insert"
        } else if builtin {
            "app.assets_context_create"
        } else {
            "app.assets_context_open"
        },
        open_command,
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
            UiNode::new(
                format!("assets.selection.action.{index}"),
                UiNodeKind::Button,
            )
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
pub(super) fn asset_meta(row: &str) -> (Option<String>, Option<String>) {
    if is_builtin_row(row) {
        return (None, None);
    }
    let extension = Path::new(row)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    if extension
        .as_deref()
        .is_some_and(|extension| is_script_file(&format!("asset.{extension}")))
    {
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
    if let Some(kind) = row.strip_prefix("builtin://interface/") {
        return format!("ui.element.{kind}");
    }
    if row == "builtin://camera" {
        return "Camera".into();
    }
    if row == "builtin://light/point" {
        return "Point Light".into();
    }
    if row == "builtin://light/spot" {
        return "Spot Light".into();
    }
    if let Some(slug) = row.strip_prefix("builtin://primitive/") {
        return crate::panels::primitive_create::parse_primitive_slug(slug)
            .map(|primitive| primitive_card_name(primitive).to_string())
            .unwrap_or_else(|| slug.to_string());
    }
    Path::new(row)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(row)
        .to_string()
}

/// Display name used on cards and in literals. `primitive_name` only knows the
/// original five shapes, so the newer primitives keep a readable literal until
/// their locale keys land.
pub fn primitive_card_name(primitive: raf_core::scene::Primitive) -> &'static str {
    match primitive {
        raf_core::scene::Primitive::Wedge => "Wedge",
        raf_core::scene::Primitive::Torus => "Torus",
        raf_core::scene::Primitive::Prism => "Prism",
        raf_core::scene::Primitive::Sphere2D => "Sphere 2D",
        other => primitive_name(other),
    }
}

pub fn asset_display_name(row: &str) -> String {
    if row == "builtin://camera" {
        return "Built-in / Camera".into();
    }
    if let Some(kind) = row.strip_prefix("builtin://light/") {
        return format!("Built-in / {} Light", title_case(kind));
    }
    row.strip_prefix("builtin://primitive/")
        .and_then(crate::panels::primitive_create::parse_primitive_slug)
        .map(|primitive| format!("Built-in / {}", primitive_card_name(primitive)))
        .unwrap_or_else(|| row.to_string())
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(super) fn sort_asset_rows(rows: &mut [&String], sort: AssetSort) {
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

pub(super) fn asset_matches_script_extension(
    row: &str,
    filter: AssetFilter,
    extension: &str,
) -> bool {
    if filter != AssetFilter::Scripts || extension.is_empty() || is_builtin_row(row) {
        return true;
    }
    Path::new(row)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case(extension))
        .unwrap_or(false)
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
        .filter(|row| asset_matches_script_extension(row.as_str(), filter, script_extension))
        .collect();
    sort_asset_rows(&mut filtered, sort);
    filtered
}

/// Cards that fit on one grid row for a real grid width.
///
/// `viewport_width` is the width of the scroll viewport, padding included.
/// The grid lays cards with wrap, so its usable content width excludes the
/// horizontal padding used by `build_assets_surface`.
pub fn asset_grid_columns(viewport_width: f32) -> usize {
    if !viewport_width.is_finite() {
        return 1;
    }
    let content = (viewport_width - ASSET_GRID_PADDING_X * 2.0).max(0.0);
    (((content + ASSET_CARD_GAP) / (ASSET_CARD_WIDTH + ASSET_CARD_GAP)).floor() as usize).max(1)
}

/// Number of visual rows a wrapped card collection occupies.
pub fn asset_grid_row_count(card_count: usize, columns: usize) -> usize {
    if card_count == 0 {
        return 0;
    }
    let columns = columns.max(1);
    (card_count + columns - 1) / columns
}

/// Visible row window for a vertical list of equally high rows.
///
/// Unlike the old card-based estimate, the scroll offset is clamped against
/// the whole row count so stale offsets from a taller panel do not materialize
/// a window beyond the end of the list.
pub fn asset_grid_row_range(
    total_rows: usize,
    scroll_offset: f32,
    viewport_height: f32,
    row_pitch: f32,
    overscan: usize,
) -> (usize, usize) {
    if total_rows == 0 || viewport_height <= 0.0 {
        return (0, 0);
    }
    let pitch = row_pitch.max(1.0);
    let max_scroll = (total_rows as f32 * pitch - viewport_height).max(0.0);
    let scroll = if scroll_offset.is_finite() {
        scroll_offset.clamp(0.0, max_scroll)
    } else {
        0.0
    };
    let range = raf_ui::UiVirtualRange::for_vertical_list(
        total_rows,
        scroll,
        viewport_height,
        pitch,
        overscan,
    );
    (range.start.min(total_rows), range.end.min(total_rows))
}

/// Window of cards to materialize for a real grid width.
///
/// Grid rows are computed from the card count and `panel_width`, then mapped
/// back to a row-aligned card range. The surface uses the same width for its
/// top and bottom spacers, so skipped content occupies exactly the rows it
/// should. Lists keep their existing one-card-per-row projection.
pub fn asset_visible_range_for_width(
    rows: &[String],
    query: &str,
    filter: AssetFilter,
    script_extension: &str,
    sort: AssetSort,
    scroll_offset: f32,
    viewport_height: f32,
    panel_width: f32,
    view: AssetViewMode,
) -> (usize, usize) {
    let count = visible_asset_rows(rows, query, filter, script_extension, sort).len();
    if count == 0 || viewport_height <= 0.0 {
        return (0, 0);
    }
    match view {
        AssetViewMode::List => {
            let capacity = (viewport_height / ASSET_LIST_PITCH.max(1.0)).ceil() as usize;
            if capacity >= count {
                return (0, count);
            }
            let (start, end) =
                asset_grid_row_range(count, scroll_offset, viewport_height, ASSET_LIST_PITCH, 4);
            (start.min(count), end.min(count))
        }
        AssetViewMode::Grid => {
            let columns = asset_grid_columns(panel_width);
            let total_rows = asset_grid_row_count(count, columns);
            let (start_row, end_row) = asset_grid_row_range(
                total_rows,
                scroll_offset,
                viewport_height,
                ASSET_CARD_PITCH,
                2,
            );
            (
                (start_row * columns).min(count),
                (end_row * columns).min(count),
            )
        }
    }
}

/// Backwards-compatible window for hosts that do not yet pass the real grid
/// width. The Assets workbench should call
/// [`asset_visible_range_for_width`] with `content_rect.width` (see the module
/// comment in `assets_surface.rs`).
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
    asset_visible_range_for_width(
        rows,
        query,
        filter,
        script_extension,
        sort,
        scroll_offset,
        viewport_height,
        ASSET_GRID_FALLBACK_WIDTH,
        view,
    )
}

fn asset_virtual_row_count(card_count: usize, panel_width: f32, view: AssetViewMode) -> usize {
    match view {
        AssetViewMode::Grid => asset_grid_row_count(card_count, asset_grid_columns(panel_width)),
        AssetViewMode::List => card_count,
    }
}

/// Height reserved above the first materialized card. Skipped rows always
/// start on a row boundary, so no trailing gap is subtracted here.
pub(super) fn asset_virtual_top_spacer_height(
    card_count: usize,
    panel_width: f32,
    view: AssetViewMode,
) -> f32 {
    if card_count == 0 {
        return 0.0;
    }
    let pitch = match view {
        AssetViewMode::Grid => ASSET_CARD_PITCH,
        AssetViewMode::List => ASSET_LIST_PITCH,
    };
    asset_virtual_row_count(card_count, panel_width, view) as f32 * pitch
}

/// Height reserved below the last materialized card. The final row does not
/// contribute its trailing gap to scrollable content.
pub(super) fn asset_virtual_bottom_spacer_height(
    card_count: usize,
    panel_width: f32,
    view: AssetViewMode,
) -> f32 {
    if card_count == 0 {
        return 0.0;
    }
    let (pitch, gap) = match view {
        AssetViewMode::Grid => (ASSET_CARD_PITCH, ASSET_CARD_GAP),
        AssetViewMode::List => (ASSET_LIST_PITCH, ASSET_LIST_ROW_GAP),
    };
    (asset_virtual_row_count(card_count, panel_width, view) as f32 * pitch - gap).max(0.0)
}

pub(super) fn asset_matches(row: &str, query: &str, filter: AssetFilter) -> bool {
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
        AssetFilter::Interfaces => {
            row.starts_with("builtin://interface/") || lower.ends_with(".ui.ron")
        }
        AssetFilter::Animations => raf_assets::animation_asset::is_animation_asset(row),
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
        AssetFilter::Models => {
            row.to_ascii_lowercase().ends_with(".prefab.ron")
                || matches!(
                    extension.as_deref(),
                    Some("obj") | Some("gltf") | Some("glb") | Some("fbx") | Some("stl")
                )
        }
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

pub(super) fn asset_icon(name: &str) -> UiIconId {
    if raf_assets::animation_asset::is_animation_asset(name) {
        return UiIconId::Play;
    }
    if name.starts_with("builtin://interface/") || name.to_ascii_lowercase().ends_with(".ui.ron") {
        return UiIconId::Frame;
    }
    if name == "builtin://camera" {
        return UiIconId::Camera;
    }
    if name.starts_with("builtin://light/") {
        return UiIconId::Sun;
    }
    if let Some(slug) = name.strip_prefix("builtin://primitive/") {
        return match crate::panels::primitive_create::parse_primitive_slug(slug) {
            Some(raf_core::scene::Primitive::Cube)
            | Some(raf_core::scene::Primitive::Wedge)
            | Some(raf_core::scene::Primitive::Prism) => UiIconId::Cube,
            Some(raf_core::scene::Primitive::Sphere)
            | Some(raf_core::scene::Primitive::Torus)
            | Some(raf_core::scene::Primitive::Sphere2D) => UiIconId::Sphere,
            Some(raf_core::scene::Primitive::Capsule) => UiIconId::Capsule,
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

pub(super) fn spacer(id: &str, height: f32) -> UiNode {
    UiNode::new(id, UiNodeKind::Panel)
        .with_layout(UiLayout::fixed(0.0, height.max(0.0)).with_width_mode(UiSizeMode::Fill))
}
