//! Create menu, script/file popovers and primitive chooser for the Assets panel.
//!
//! Moved from the surface facade without behavior changes.

use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAlign, UiEventBinding, UiEventKind, UiFlow, UiFontWeight, UiIcon, UiIconId,
    UiIconSize, UiJustify, UiLayout, UiNode, UiNodeKind, UiSizeMode, UiSpacing, UiSurface,
    UiTextOverflow, UiTextRole, UiTextStyle,
};

use crate::panels::primitive_create::{primitive_key, primitive_slug, CREATEABLE_PRIMITIVES};

use super::menus::{menu_item, overlay_menu_root, overlay_popover_root, overlay_surface};

pub fn build_assets_create_menu_surface(palette: StudioUiPalette) -> UiSurface {
    let mut root = overlay_menu_root(palette, "assets.create-menu", "app.assets_create");
    for (id, icon, label_key, command) in [
        ("interface",UiIconId::Frame,"ui.designer.title","assets.create-interface"),
        ("animation",UiIconId::Play,"animation.tab","animations.open"),
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
        (
            "generate",
            UiIconId::File,
            "app.generate_local_icon",
            "assets.generate-local-icon",
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

pub fn build_assets_script_popover_surface(
    palette: StudioUiPalette,
    script_name: &str,
) -> UiSurface {
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
    let mut root =
        overlay_popover_root(palette, "assets.primitive-popover", "app.create_primitive");
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
            raf_core::scene::Primitive::Capsule => UiIconId::Capsule,
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
