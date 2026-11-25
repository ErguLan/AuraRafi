//! Transform vector controls and reset actions.

use super::components::action_button;
use super::*;

pub(super) fn vector_row(
    palette: StudioUiPalette,
    label: &str,
    value: Vec3,
    display_unit: DisplayUnit,
) -> UiNode {
    let values = [value.x, value.y, value.z];
    let unit = match label {
        "position" => display_unit.distance_suffix(),
        "rotation" => "deg",
        "scale" => "x",
        "size" => "m",
        "velocity" => "m/s",
        "angular_velocity" => "rad/s",
        _ => "",
    };
    let label_line = UiNode::new(
        format!("inspector.vector.{label}.label-line"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        align_items: UiAlign::Center,
        gap: 4.0,
        ..UiLayout::fixed(0.0, 18.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_child(
        UiNode::new(format!("inspector.vector.{label}.label"), UiNodeKind::Label)
            .with_text_key(if label == "size" {
                "app.dimensions".to_string()
            } else {
                format!("app.{label}")
            })
            .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
            .with_layout(UiLayout::fit_content()),
    )
    .with_child(
        UiNode::new(format!("inspector.vector.{label}.unit"), UiNodeKind::Label)
            .with_text_value(format!("({unit})"))
            .with_text_style(UiTextStyle::body(palette.tokens().text_muted))
            .with_layout(UiLayout::fit_content()),
    );
    let row = UiNode::new(format!("inspector.vector.{label}"), UiNodeKind::Panel)
        .with_class("inspector-vector")
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            ..UiLayout::fixed(0.0, 58.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(label_line);
    let mut controls = UiNode::new(
        format!("inspector.vector.{label}.controls"),
        UiNodeKind::Toolbar,
    )
    .with_layout(UiLayout {
        flow: UiFlow::Row,
        gap: 3.0,
        ..UiLayout::fixed(0.0, 34.0).with_width_mode(UiSizeMode::Fill)
    });
    for (axis, current) in [("x", values[0]), ("y", values[1]), ("z", values[2])] {
        let axis_cell = UiNode::new(format!("inspector.{label}.{axis}.cell"), UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 2.0,
                grow: 1.0,
                min_size: [42.0, 32.0],
                ..UiLayout::fixed(0.0, 32.0)
            })
            .with_child(
                UiNode::new(
                    format!("inspector.{label}.{axis}.caption"),
                    UiNodeKind::Label,
                )
                .with_text_value(axis.to_ascii_uppercase())
                .with_text_style(UiTextStyle::button(axis_color(axis)))
                .with_layout(UiLayout::fit_content()),
            )
            .with_child(numeric_text_input(palette, label, axis, current));
        controls = controls.with_child(axis_cell);
    }
    row.with_child(controls)
}

fn axis_color(axis: &str) -> [u8; 4] {
    match axis {
        "x" => [255, 102, 102, 255],
        "y" => [92, 214, 128, 255],
        "z" => [92, 166, 255, 255],
        _ => [210, 214, 220, 255],
    }
}

fn numeric_text_input(palette: StudioUiPalette, label: &str, axis: &str, _current: f32) -> UiNode {
    let id = format!("inspector.{label}.{axis}.input");
    let value_key = numeric_text_key(label, axis);
    UiNode::text_input(
        id,
        UiTextInput {
            value_key,
            placeholder_key: Some("app.value".to_string()),
            max_length: 24,
            multiline: false,
            password: false,
            submit_command: Some("inspector.numeric.commit".to_string()),
        },
    )
    .with_class("inspector-number-input")
    .with_layout(UiLayout {
        min_size: [42.0, 24.0],
        ..UiLayout::fixed(0.0, 24.0).with_width_mode(UiSizeMode::Fill)
    })
    .with_text_style(UiTextStyle::body(palette.tokens().text))
}

fn numeric_text_key(label: &str, axis: &str) -> String {
    format!("inspector.{label}.{axis}.text")
}
pub(super) fn transform_actions(palette: StudioUiPalette, id: SceneNodeId) -> UiNode {
    UiNode::new("inspector.transform.actions", UiNodeKind::Toolbar)
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 6.0,
            ..UiLayout::fixed(0.0, 30.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(action_button(
            palette,
            "inspector.reset-transform",
            "app.reset_transform",
            format!("inspector.reset-transform:{id}", id = id.0),
        ))
        .with_child(action_button(
            palette,
            "inspector.reset-all",
            "app.reset_all",
            format!("inspector.reset-all:{id}", id = id.0),
        ))
}
