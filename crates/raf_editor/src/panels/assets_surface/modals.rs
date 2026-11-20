//! Open, rename and delete modal documents for the Assets panel.
//!
//! Moved from the surface facade without behavior changes.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiIcon,
    UiIconId, UiIconSize, UiLayout, UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiSurface,
    UiSurfaceMaterial, UiTextOverflow, UiTextRole, UiTextStyle,
};

use super::catalog::{asset_file_name, asset_icon};
use super::menus::overlay_surface;
use super::{ASSETS_MODAL_TEXT_WIDTH, ASSETS_MODAL_WIDTH};

pub fn build_assets_open_modal_surface(
    palette: StudioUiPalette,
    row: &str,
    absolute_path: &str,
) -> UiSurface {
    let root = modal_root(palette, "assets.open-modal", "app.assets_open_title")
        .with_child(asset_file_header(
            palette,
            "assets.open-modal",
            row,
            absolute_path,
        ))
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
            Some((&format!("assets.rename.confirm:{row}"), "app.confirm")),
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
fn asset_file_header(palette: StudioUiPalette, id: &str, row: &str, absolute_path: &str) -> UiNode {
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
            ..UiLayout::fixed(ASSETS_MODAL_TEXT_WIDTH, 0.0).with_height_mode(UiSizeMode::FitContent)
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
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            command.to_string(),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::KeyPress("enter".to_string()),
            command.to_string(),
        ))
}

/// Rounded icon plate. Icons live in child nodes because a node level icon is
/// painted inside the node content box instead of the child flow.
fn icon_tile(palette: StudioUiPalette, id: &str, icon: UiIconId, size: UiIconSize) -> UiNode {
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
    let mut footer =
        UiNode::new(format!("{id}.footer"), UiNodeKind::Toolbar).with_layout(UiLayout {
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
