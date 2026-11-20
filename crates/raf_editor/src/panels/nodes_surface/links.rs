//! Node connector dots and wire interaction bindings.

use super::*;

pub(super) fn pin_button(
    palette: StudioUiPalette,
    node: &Node,
    pin: &NodePin,
    is_self_pending: bool,
    is_compatible: bool,
    is_dimmed: bool,
    language: raf_core::Language,
    zoom: f32,
) -> UiNode {
    let tokens = palette.tokens();
    let node_id = node.id.0;
    let pin_id = pin.id;
    let id = format!("nodes.pin.{node_id}.{pin_id}");
    let color = nodes_canvas::pin_color(pin.data_type, tokens.accent);
    let rect = nodes_canvas::pin_rect(node, pin, zoom);
    let label = nodes_catalog::localized_pin_label(node, &pin.name, language);
    let type_key = nodes_catalog::pin_type_key(pin.data_type);
    let type_label = raf_core::i18n::t(type_key, language);

    let tooltip = if is_self_pending {
        format!("{label} ({type_label}) · Active wire source")
    } else if is_compatible {
        format!("Connect to {label} ({type_label})")
    } else if is_dimmed {
        format!("{label} ({type_label}) · Incompatible")
    } else {
        format!("{label} · {type_label}")
    };

    let fill_color = if is_self_pending {
        tokens.accent_hot
    } else if is_compatible {
        [255, 255, 255, 255]
    } else if is_dimmed {
        [color[0], color[1], color[2], 70]
    } else {
        color
    };

    let border_color = if is_self_pending {
        [255, 255, 255, 255]
    } else if is_compatible {
        tokens.accent_hot
    } else {
        tokens.text
    };

    let dot = UiNode::new(id, UiNodeKind::Button)
        .with_class(if is_self_pending {
            "nodes-pin-dot-pending"
        } else if is_compatible {
            "nodes-pin-dot-compatible"
        } else {
            "nodes-pin-dot"
        })
        .with_layout(UiLayout::absolute(rect).with_z_index(nodes_canvas::Z_PIN_DOT))
        .with_style(UiStyle {
            fill: fill_color,
            border: border_color,
            text: tokens.text,
            border_width: if is_self_pending || is_compatible {
                3.0
            } else {
                2.0
            },
            radius: rect.width * 0.5,
            opacity: if is_dimmed { 0.35 } else { 1.0 },
        })
        .with_accessibility_role(UiAccessibilityRole::Button)
        .with_accessibility_label_key(
            nodes_catalog::pin_label_key(node, &pin.name)
                .unwrap_or_else(|| "nodes.pin.unknown".to_string()),
        )
        .with_accessibility_selected(is_self_pending)
        .with_tooltip_value(tooltip)
        .focusable()
        .with_event(UiEventBinding::command(
            UiEventKind::Click,
            format!("nodes.pin.{node_id}:{pin_id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragStart,
            format!(
                "nodes.wire.start.{node_id}:{pin_id}:{}",
                if matches!(pin.kind, PinKind::Output) {
                    "out"
                } else {
                    "in"
                }
            ),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragMove,
            format!("nodes.wire.move.{node_id}:{pin_id}"),
        ))
        .with_event(UiEventBinding::command(
            UiEventKind::DragEnd,
            format!("nodes.wire.end.{node_id}:{pin_id}"),
        ));

    dot
}
