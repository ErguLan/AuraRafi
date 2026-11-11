//! Contextual element properties, mounted in the existing Properties inspector.
use super::{chrome::*, InterfaceSurfaceHost};
use raf_render::api_graphic_basic::ui_surface::UiSurface;
use raf_ui::{game_ui::UiPresentationKind, *};

fn field(key: &str, width: f32, t: UiTokens) -> UiNode {
    let id = format!("ui.designer.field.{key}");
    let caption = if matches!(
        key,
        "x" | "y" | "anchor_x" | "anchor_y" | "relative_width" | "relative_height"
    ) {
        format!("ui.workspace.{key}")
    } else {
        format!("ui.field.{key}")
    };
    let kit = components(t);
    let control = kit.text_input(
                &id,
                UiTextInput {
                    submit_command: Some(format!("ui.designer.commit:{key}")),
                    ..UiTextInput::new(&id)
                },
            )
            .with_class("interface.input")
            .with_event(UiEventBinding::command(UiEventKind::KeyPress("escape".into()),
                format!("ui.designer.cancel-field:{key}")));
    kit.property_field(format!("{id}.row"), caption, control).fixed_width(width)
}
fn pair(a: &str, b: &str, width: f32, t: UiTokens) -> UiNode {
    let half = ((width - GAP) * 0.5).max(1.0);
    row(format!("ui.designer.pair.{a}.{b}"))
        .with_child(field(a, half, t))
        .with_child(field(b, half, t))
}
fn section(
    key: &str,
    title: &str,
    children: Vec<UiNode>,
    host: &InterfaceSurfaceHost,
    width: f32,
    t: UiTokens,
) -> UiNode {
    let open = host.workspace.expanded.contains(key);
    let mut result = UiNode::column(format!("ui.designer.section.{key}"), [])
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: GAP,
            padding: UiSpacing::xy(0.0, 6.0),
            ..UiLayout::fit_content().fixed_width(width)
        })
        .with_child(button(
            format!("ui.designer.section.{key}.toggle"),
            title,
            format!("ui.designer.section:{key}"),
            Some(if open {
                UiIconId::ChevronDown
            } else {
                UiIconId::ChevronRight
            }),
            false,
            t,
        ));
    if open {
        result.children.extend(children);
    }
    result
}
pub fn content(palette: StudioUiPalette, host: &InterfaceSurfaceHost, width: f32) -> UiNode {
    let t = tokens(palette);
    let width = (width - INSET * 2.0).max(1.0);
    let mut body = UiNode::scroll_view("ui.designer.inspector", UiScrollAxis::Vertical)
        .with_layout(UiLayout {
            padding: UiSpacing::same(INSET),
            gap: GAP,
            ..UiLayout::fill(UiFlow::Column)
        })
        .with_style(UiStyle {
            fill: t.surface,
            ..UiStyle::transparent()
        });
    let Some(node) = host.node() else {
        return body;
    };
    let is_root = node.id == host.document.root.id;
    body.children.push(
        value(
            "ui.designer.selection.name",
            if is_root {
                &host.document.name
            } else {
                &node.id
            },
            t,
        )
        .with_icon(UiIcon::new(element_icon(node)))
        .with_text_style(UiTextStyle::panel_title(t.text)),
    );
    body.children.push(label(
        "ui.designer.selection.kind",
        if is_root {
            "ui.workspace.document"
        } else {
            "ui.workspace.element"
        },
        t,
    ));
    if is_root {
        body.children.push(field("document_name", width, t));
    } else {
        let mut content = Vec::new();
        if matches!(
            node.kind,
            UiNodeKind::Label | UiNodeKind::Button | UiNodeKind::TextInput
        ) {
            content.push(field("text", width, t));
            content.push(field("font_size", width, t));
            content.push(field("text_color", width, t));
        }
        if matches!(node.control, UiControl::Image(_)) {
            content.push(field("image", width, t));
        }
        if !content.is_empty() {
            body.children.push(section(
                "content",
                "ui.designer.content",
                content,
                host,
                width,
                t,
            ));
        }
        let mut layout = vec![
            pair("x", "y", width, t),
            pair("width", "height", width, t),
            label("ui.designer.anchor.label", "ui.workspace.anchor", t),
        ];
        let mut anchors = UiNode::column("ui.designer.anchors", []).with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 4.0,
            ..UiLayout::fit_content()
        });
        let current = node.layout.anchored.map(|a| a.anchor).unwrap_or([0.0; 2]);
        for y in 0..3 {
            let mut line = row(format!("ui.designer.anchors.{y}"));
            line.layout.gap = 4.0;
            for x in 0..3 {
                let index = y * 3 + x;
                let mut cell = button(
                    format!("ui.designer.anchor.{index}"),
                    &format!("ui.anchor.{index}"),
                    format!("ui.designer.anchor:{index}"),
                    Some(UiIconId::Focus),
                    current == [x as f32 * 0.5, y as f32 * 0.5],
                    t,
                );
                cell.text_key = None;
                cell.accessibility_label_key = Some(format!("ui.anchor.{index}"));
                cell.layout = UiLayout::fixed(28.0, 24.0);
                line.children.push(cell);
            }
            anchors.children.push(line);
        }
        layout.push(anchors);
        layout.push(hint(
            "ui.designer.anchor.hint",
            "ui.workspace.anchor_hint",
            width,
            t,
        ));
        body.children.push(section(
            "layout",
            "ui.workspace.geometry",
            layout,
            host,
            width,
            t,
        ));
        let mut colors = row("ui.designer.swatches");
        for (id, color, key) in [
            ("white", "#FFFFFFFF", "ui.color.white"),
            ("gray", "#D9D9D9FF", "ui.color.gray"),
            ("dark", "#242424FF", "ui.color.dark"),
            ("clear", "#FFFFFF00", "ui.color.clear"),
        ] {
            let mut swatch = button(
                format!("ui.designer.swatch.{id}"),
                key,
                format!("ui.designer.color:{color}"),
                None,
                false,
                t,
            );
            swatch.text_key = None;
            swatch.accessibility_label_key = Some(key.into());
            swatch.layout = UiLayout::fixed(28.0, 24.0);
            swatch.style.fill = super::super::interface_workspace::parse_color(color).unwrap();
            swatch.style.border = t.border;
            swatch.style.border_width = 1.0;
            colors.children.push(swatch);
        }
        let mut visible = button(
            "ui.designer.visible",
            "ui.workspace.visible",
            "ui.designer.visible",
            Some(if node.hidden {
                UiIconId::EyeOff
            } else {
                UiIconId::Eye
            }),
            !node.hidden,
            t,
        );
        visible.layout.width_mode = UiSizeMode::Fill;
        body.children.push(section(
            "appearance",
            "ui.workspace.appearance",
            vec![
                colors,
                field("fill", width, t),
                visible,
                button(
                    "ui.designer.enabled",
                    "ui.workspace.enabled",
                    "ui.designer.enabled",
                    None,
                    !node.disabled,
                    t,
                ),
            ],
            host,
            width,
            t,
        ));
    }
    body.children.push(presentation(host, width, t));
    if !is_root {
        let mut advanced = vec![
            field("id", width, t),
            field("parent", width, t),
            pair("anchor_x", "anchor_y", width, t),
            pair("relative_width", "relative_height", width, t),
        ];
        if matches!(node.kind, UiNodeKind::Button | UiNodeKind::TextInput) {
            advanced.push(field("event", width, t));
        }
        for flow in ["none", "row", "column", "grid"] {
            advanced.push(button(
                format!("ui.designer.flow.{flow}"),
                &format!("ui.flow.{flow}"),
                format!("ui.designer.flow:{flow}"),
                None,
                false,
                t,
            ));
        }
        body.children.push(section(
            "advanced",
            "ui.workspace.advanced",
            advanced,
            host,
            width,
            t,
        ));
        body.children.push(button(
            "ui.designer.delete",
            "ui.designer.delete",
            "ui.designer.delete",
            Some(UiIconId::Trash),
            false,
            t,
        ));
    }
    if let Some(error) = &host.diagnostic {
        body.children
            .push(label("ui.designer.error.detail", error, t));
    }
    body
}
fn presentation(host: &InterfaceSurfaceHost, width: f32, t: UiTokens) -> UiNode {
    let mount = host.document.mounts.first().cloned().unwrap_or_default();
    let mut items = Vec::new();
    for (id, kind, icon) in [
        ("screen", UiPresentationKind::Screen, UiIconId::View2d),
        (
            "camera",
            UiPresentationKind::CameraOverlay,
            UiIconId::Camera,
        ),
        ("plane", UiPresentationKind::CameraPlane, UiIconId::Plane),
        ("surface", UiPresentationKind::Surface, UiIconId::Cube),
        ("billboard", UiPresentationKind::Billboard, UiIconId::Frame),
    ] {
        items.push(button(
            format!("ui.designer.mode.{id}"),
            &format!("ui.presentation.{id}"),
            format!("ui.designer.mount:{id}"),
            Some(icon),
            mount.kind == kind,
            t,
        ));
    }
    if mount.kind != UiPresentationKind::Screen {
        items.push(hint(
            "ui.designer.target.hint",
            "ui.workspace.target_hint",
            width,
            t,
        ));
        items.push(field("target", width, t));
    }
    if matches!(
        mount.kind,
        UiPresentationKind::CameraPlane
            | UiPresentationKind::Surface
            | UiPresentationKind::Billboard
    ) {
        items.push(hint(
            "ui.designer.world.hint",
            "ui.workspace.world_hint",
            width,
            t,
        ));
        for key in ["offset", "rotation", "meters", "canvas"] {
            items.push(field(key, width, t));
        }
    }
    section(
        "presentation",
        "ui.designer.presentation",
        items,
        host,
        width,
        t,
    )
}
pub fn build(palette: StudioUiPalette, host: &InterfaceSurfaceHost, size: [f32; 2]) -> UiSurface {
    surface(
        "editor.interface.properties",
        palette,
        content(palette, host, size[0]),
    )
}
