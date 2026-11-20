//! Overlay menus, context menu and shared overlay chrome for the Assets panel.
//!
//! Moved from the surface facade without behavior changes.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiIcon,
    UiIconId, UiIconSize, UiLayout, UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiStylePatch,
    UiStyleRule, UiStyleRuleState, UiStyleSelector, UiStyleSheet, UiSurface, UiSurfaceMaterial,
    UiTextOverflow, UiTextRole, UiTextStyle,
};

use crate::panels::editor_bottom_dock_styles::bottom_style_sheet;

use super::asset_file_name;

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

pub(super) fn overlay_menu_root(_palette: StudioUiPalette, id: &str, title_key: &str) -> UiNode {
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

pub(super) fn overlay_popover_root(palette: StudioUiPalette, id: &str, title_key: &str) -> UiNode {
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

pub(super) fn menu_item(
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
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            command.to_string(),
        ))
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

pub(super) fn overlay_surface(id: &str, palette: StudioUiPalette, root: UiNode) -> UiSurface {
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
