//! Live document overlay and optional isolated artboard, using the same RafUI nodes.
use super::super::interface_workspace::*;
use super::chrome::icon_button;
use super::{chrome::*, InterfaceSurfaceHost};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::*;
pub const INSERT_MENU_SIZE: [f32; 2] = [208.0, ROW_HEIGHT * 5.0 + GAP * 4.0 + INSET * 2.0];

pub fn insert_menu(palette: StudioUiPalette) -> UiSurface {
    let t = tokens(palette);
    let mut root = UiNode::new("ui.designer.insert.menu", UiNodeKind::Menu)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            padding: UiSpacing::same(INSET),
            gap: GAP,
            ..UiLayout::fixed(INSERT_MENU_SIZE[0], INSERT_MENU_SIZE[1])
        })
        .with_style(UiStyle {
            fill: t.surface,
            border: t.border,
            border_width: 1.0,
            radius: 6.0,
            ..UiStyle::transparent()
        });
    for (kind, icon) in [
        ("frame", UiIconId::Frame),
        ("label", UiIconId::File),
        ("image", UiIconId::Assets),
        ("button", UiIconId::Select),
        ("input", UiIconId::Pencil),
    ] {
        let mut item = button(
            format!("ui.designer.insert.{kind}"),
            &format!("ui.element.{kind}"),
            format!("ui.designer.add:{kind}"),
            Some(icon),
            false,
            t,
        );
        item.layout.width_mode = UiSizeMode::Fill;
        root.children.push(item);
    }
    surface("editor.interface.insert", palette, root)
}

pub fn build(palette: StudioUiPalette, host: &InterfaceSurfaceHost, size: [f32; 2]) -> UiSurface {
    let t = tokens(palette);
    let active = host.workspace.active();
    let isolated = host.workspace.view == InterfaceView::Canvas;
    let geometry = host.workspace.geometry(size, host.preview_size);
    let mut root = UiNode::new("ui.designer.workspace", UiNodeKind::Root)
        .with_layout(UiLayout::fixed(size[0], size[1]))
        .with_style(UiStyle::transparent());
    let mut stage = UiNode::scroll_view("ui.designer.canvas", UiScrollAxis::Both)
        .with_layout(UiLayout {
            overflow: UiOverflow::ScrollBoth,
            ..UiLayout::absolute(geometry.stage)
        })
        .with_style(UiStyle {
            fill: if isolated { t.background } else { [0; 4] },
            ..UiStyle::transparent()
        });
    stage.interactive = active;
    stage.focusable = active;
    let mut preview = host.document.root.clone();
    if isolated {
        // A neutral canvas is authoring chrome, never a saved background.
        stage.children.push(
            UiNode::new("ui.designer.artboard.background", UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(geometry.artboard))
                .with_style(UiStyle {
                    fill: t.surface_alt,
                    border: t.border,
                    border_width: 1.0,
                    ..UiStyle::transparent()
                }),
        );
    }
    let mut styles = raf_ui::authoring::namespace_subtree(
        &mut preview,
        PREVIEW_PREFIX,
        &host.document.style_sheet,
    );
    scale_nodes(&mut preview, geometry.scale);
    // The outer rectangle is already expressed in presentation pixels.
    preview.layout.rect = Some(geometry.artboard);
    preview.layout.anchored = None;
    preview.layout.position_mode = UiPositionMode::Absolute;
    preview.layout.basis = [geometry.artboard.width, geometry.artboard.height];
    preview.layout.width_mode = UiSizeMode::Fixed;
    preview.layout.height_mode = UiSizeMode::Fixed;
    preview.layout.overflow = UiOverflow::Clip;
    for rule in &mut styles {
        rule.patch.radius = rule.patch.radius.map(|v| v * geometry.scale);
        rule.patch.border_width = rule.patch.border_width.map(|v| v * geometry.scale);
    }
    prepare(&mut preview, active && !host.interact, &host.selected, true);
    stage.children.push(preview);
    if active && host.document.root.children.is_empty() {
        stage.children.push(
            UiNode::column("ui.designer.empty", [])
                .with_layout(UiLayout {
                    flow: UiFlow::Column,
                    gap: 8.0,
                    padding: UiSpacing::same(INSET),
                    ..UiLayout::absolute(UiRect::new(
                        24.0,
                        24.0,
                        (size[0] - 48.0).clamp(1.0, 380.0),
                        104.0,
                    ))
                })
                .with_style(UiStyle {
                    fill: t.surface,
                    radius: 6.0,
                    text: t.text,
                    ..UiStyle::transparent()
                })
                .with_child(label(
                    "ui.designer.empty.title",
                    "ui.workspace.empty_title",
                    t,
                ))
                .with_child(label(
                    "ui.designer.empty.detail",
                    "ui.workspace.empty_hint",
                    t,
                )),
        );
    }
    root.children.push(stage);
    if active {
        root.children.push(toolbar(host, size[0], t));
        let mut status = row("ui.designer.status");
        status.layout = UiLayout {
            flow: UiFlow::Row,
            gap: GAP,
            padding: UiSpacing::xy(INSET, 0.0),
            align_items: UiAlign::Center,
            ..UiLayout::absolute(UiRect::new(
                0.0,
                size[1] - STATUS_HEIGHT,
                size[0],
                STATUS_HEIGHT,
            ))
        };
        status.style = UiStyle {
            fill: t.surface,
            text: t.text,
            ..UiStyle::transparent()
        };
        status.children.push(label(
            "ui.designer.status.mode",
            if isolated {
                "ui.workspace.canvas_status"
            } else {
                "ui.workspace.live_status"
            },
            t,
        ));
        status.children.push(value(
            "ui.designer.status.size",
            format!(
                "{:.0} × {:.0}",
                geometry.logical_size[0], geometry.logical_size[1]
            ),
            t,
        ));
        if host.dirty {
            status
                .children
                .push(label("ui.designer.status.dirty", "ui.designer.unsaved", t));
        }
        root.children.push(status);
    }
    let mut surface = surface("editor.interface.canvas", palette, root);
    surface.style_sheet.rules.extend(styles);
    surface
}

fn toolbar(host: &InterfaceSurfaceHost, width: f32, t: UiTokens) -> UiNode {
    let mut items = row("ui.designer.toolbar.items");
    items.layout.padding = UiSpacing::same(8.0);
    items.layout.gap = 4.0;
    items.children.push(icon_button(
        "ui.designer.scene",
        "ui.workspace.scene",
        "ui.designer.view:scene",
        UiIconId::ChevronLeft,
        t,
    ));
    items.children.push(button(
        "ui.designer.live",
        "ui.workspace.live",
        "ui.designer.view:live",
        None,
        host.workspace.view == InterfaceView::Live,
        t,
    ));
    items.children.push(button(
        "ui.designer.isolated",
        "ui.workspace.canvas",
        "ui.designer.view:canvas",
        None,
        host.workspace.view == InterfaceView::Canvas,
        t,
    ));
    let mut insert = button(
        "ui.designer.insert",
        "ui.workspace.insert",
        "ui.designer.insert.toggle",
        Some(UiIconId::Add),
        host.workspace.insert_open,
        t,
    );
    insert.event_handlers = vec![UiEventBinding {
        event: UiEventKind::Click,
        action: UiAction::OpenMenu {
            id: "ui.designer.insert.menu".into(),
        },
    }];
    items.children.push(insert);
    if host.editing_asset.is_some() {
        items.children.push(button(
            "ui.designer.place",
            "ui.workspace.place",
            "ui.designer.place",
            Some(UiIconId::Add),
            false,
            t,
        ));
    }
    for (id, key, icon) in [
        ("undo", "ui.designer.undo", UiIconId::Undo),
        ("redo", "ui.designer.redo", UiIconId::Redo),
        ("save", "ui.designer.save", UiIconId::Success),
        ("export", "ui.designer.export", UiIconId::ExternalLink),
    ] {
        let mut action = icon_button(
            format!("ui.designer.{id}"),
            key,
            format!("ui.designer.{id}"),
            icon,
            t,
        );
        action.disabled = match id {
            "undo" => !host.can_undo(),
            "redo" => !host.can_redo(),
            _ => false,
        };
        items.children.push(action);
    }
    let mut snap = icon_button(
        "ui.designer.snap",
        "ui.workspace.snap",
        "ui.designer.snap",
        UiIconId::Magnet,
        t,
    );
    if host.workspace.snap {
        snap.style.fill = t.selection;
    }
    items.children.push(snap);
    if host.workspace.view == InterfaceView::Canvas {
        for (id, key, icon) in [
            ("wide", "ui.designer.wide", UiIconId::View3d),
            ("narrow", "ui.designer.narrow", UiIconId::View2d),
            ("zoom-out", "ui.workspace.zoom_out", UiIconId::ZoomOut),
            ("fit", "ui.workspace.fit", UiIconId::Focus),
            ("zoom-in", "ui.workspace.zoom_in", UiIconId::ZoomIn),
        ] {
            items.children.push(icon_button(
                format!("ui.designer.{id}"),
                key,
                format!("ui.designer.{id}"),
                icon,
                t,
            ));
        }
    }
    UiNode::scroll_view("ui.designer.toolbar", UiScrollAxis::Horizontal)
        .with_layout(UiLayout::absolute(UiRect::new(
            0.0,
            0.0,
            width,
            TOOLBAR_HEIGHT,
        )))
        .with_style(UiStyle {
            fill: t.surface,
            border: t.border,
            border_width: 1.0,
            ..UiStyle::transparent()
        })
        .with_child(items)
}
fn prepare(node: &mut UiNode, editing: bool, selected: &str, is_root: bool) {
    let original = node
        .id
        .strip_prefix(PREVIEW_PREFIX)
        .unwrap_or(&node.id)
        .to_owned();
    let count = node.children.len();
    node.event_handlers.clear();
    node.interactive = editing;
    node.focusable = editing;
    if let UiControl::TextInput(input) = &mut node.control {
        input.submit_command = None;
    }
    if editing {
        if node.hidden && original == selected {
            node.hidden = false;
            node.style.opacity *= 0.3;
        }
        node.disabled = false;
        if !matches!(node.control, UiControl::Image(_)) {
            node.control = UiControl::None;
        }
        node.event_handlers = vec![
            UiEventBinding::command(
                UiEventKind::PointerDown(UiPointerButton::Primary),
                format!("ui.designer.select:{original}"),
            ),
            UiEventBinding::command(UiEventKind::DragStart, "ui.designer.move.start"),
            UiEventBinding::command(UiEventKind::DragMove, "ui.designer.move.drag"),
            UiEventBinding::command(UiEventKind::DragEnd, "ui.designer.move.end"),
        ];
        if original == selected && !is_root {
            add_handles(node, &original);
        }
    } else if !matches!(node.control, UiControl::Image(_)) {
        // Authoring preview cannot execute game callbacks or edit controls in the editor.
        node.control = UiControl::None;
    }
    for child in node.children.iter_mut().take(count) {
        prepare(child, editing, selected, false);
    }
}
fn add_handles(node: &mut UiNode, id: &str) {
    let outline = |suffix: &str, color: [u8; 4], width: f32| {
        UiNode::new(
            format!("ui.designer.outline.{id}.{suffix}"),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            anchored: Some(game_ui::UiAnchoredRect {
                anchor: [0.0; 2],
                pivot: [0.0; 2],
                relative_size: [1.0; 2],
                offset: [0.0; 2],
                size: [0.0; 2],
            }),
            z_index: 100,
            ..Default::default()
        })
        .with_style(UiStyle {
            border: color,
            border_width: width,
            ..UiStyle::transparent()
        })
    };
    node.children
        .push(outline("contrast", [25, 25, 25, 255], 3.0));
    node.children.push(outline("selection", [255; 4], 1.0));
    for (name, anchor) in [
        ("nw", [0.0, 0.0]),
        ("n", [0.5, 0.0]),
        ("ne", [1.0, 0.0]),
        ("w", [0.0, 0.5]),
        ("e", [1.0, 0.5]),
        ("sw", [0.0, 1.0]),
        ("s", [0.5, 1.0]),
        ("se", [1.0, 1.0]),
    ] {
        node.children.push(
            UiNode::new(
                format!("ui.designer.handle.{id}.{name}"),
                UiNodeKind::Button,
            )
            .with_layout(UiLayout {
                anchored: Some(game_ui::UiAnchoredRect {
                    anchor,
                    pivot: anchor,
                    relative_size: [0.0; 2],
                    offset: [0.0; 2],
                    size: [HANDLE_SIZE; 2],
                }),
                z_index: 101,
                ..Default::default()
            })
            .with_style(UiStyle {
                fill: [255; 4],
                border: [25, 25, 25, 255],
                border_width: 1.0,
                ..UiStyle::transparent()
            })
            .focusable()
            .with_event(UiEventBinding::command(
                UiEventKind::DragStart,
                format!("ui.designer.resize.start:{name}"),
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::DragMove,
                "ui.designer.move.drag",
            ))
            .with_event(UiEventBinding::command(
                UiEventKind::DragEnd,
                "ui.designer.move.end",
            )),
        );
    }
}
fn scale_nodes(node: &mut UiNode, scale: f32) {
    let layout = &mut node.layout;
    if let Some(r) = &mut layout.rect {
        r.x *= scale;
        r.y *= scale;
        r.width *= scale;
        r.height *= scale;
    }
    for v in layout
        .basis
        .iter_mut()
        .chain(layout.min_size.iter_mut())
        .chain(layout.max_size.iter_mut())
    {
        *v *= scale;
    }
    for v in [
        &mut layout.padding.left,
        &mut layout.padding.right,
        &mut layout.padding.top,
        &mut layout.padding.bottom,
        &mut layout.gap,
    ] {
        *v *= scale;
    }
    layout.grid.min_column_width *= scale;
    layout.grid.row_height *= scale;
    for rule in &mut layout.responsive {
        rule.max_width *= scale;
        if let Some(basis) = &mut rule.basis {
            for v in basis {
                *v *= scale;
            }
        }
        if let Some(padding) = &mut rule.padding {
            padding.left *= scale;
            padding.right *= scale;
            padding.top *= scale;
            padding.bottom *= scale;
        }
        if let Some(gap) = &mut rule.gap {
            *gap *= scale;
        }
    }
    if let Some(a) = &mut layout.anchored {
        for v in a.size.iter_mut().chain(a.offset.iter_mut()) {
            *v *= scale;
        }
    }
    node.style.radius *= scale;
    node.style.border_width *= scale;
    for edges in [&mut node.visual.corners, &mut node.visual.border_widths]
        .into_iter()
        .flatten()
    {
        for value in edges {
            *value *= scale;
        }
    }
    if let Some(shadow) = &mut node.visual.shadow {
        shadow.offset[0] *= scale;
        shadow.offset[1] *= scale;
        shadow.softness *= scale;
        shadow.spread *= scale;
    }
    if let Some(ring) = &mut node.visual.focus_ring {
        ring.width *= scale;
        ring.offset *= scale;
    }
    node.visual.content.gap *= scale;
    let text = node
        .text_style
        .get_or_insert(UiTextStyle::body(node.style.text));
    text.size_px *= scale;
    text.line_height_px *= scale;
    for child in &mut node.children {
        scale_nodes(child, scale);
    }
}
