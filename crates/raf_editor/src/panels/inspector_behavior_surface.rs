//! Parameter/behavior controls only. Mutation and validation live in commands::behavior.
use super::*;

pub(crate) fn scope_key(source: Option<usize>) -> String {
    source
        .map(|i| i.to_string())
        .unwrap_or_else(|| "shared".into())
}
pub(crate) fn parse_scope(source: &str) -> Option<Option<usize>> {
    if source == "shared" {
        Some(None)
    } else {
        source.parse().ok().map(Some)
    }
}
pub(crate) fn parameter_key(
    uuid: uuid::Uuid,
    source: Option<usize>,
    index: usize,
    field: &str,
) -> String {
    format!(
        "inspector.parameter.{uuid}.{}.{index}.{field}",
        scope_key(source)
    )
}
pub(crate) fn dropdown_from_trigger(id: &str) -> Option<InspectorDropdown> {
    if id == "inspector.behavior-source.trigger" {
        return Some(InspectorDropdown::BehaviorSource);
    }
    if id == "inspector.prefab-source.trigger" {
        return Some(InspectorDropdown::PrefabSource);
    }
    let raw = id
        .strip_prefix("inspector.parameter-ref.")?
        .strip_suffix(".trigger")?;
    let (source, index) = raw.split_once('.')?;
    Some(InspectorDropdown::Parameter {
        source: parse_scope(source)?,
        index: index.parse().ok()?,
    })
}

fn input(palette: StudioUiPalette, key: String, action: String, placeholder: &str) -> UiNode {
    UiNode::text_input(
        key.clone(),
        UiTextInput {
            value_key: key,
            placeholder_key: Some(placeholder.into()),
            max_length: 4096,
            multiline: false,
            password: false,
            submit_command: Some(action),
        },
    )
    .with_class("inspector-input")
    .with_accessibility_label_key(placeholder)
    .with_tooltip_key("behavior.commit_hint")
    .with_layout(UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill))
    .with_text_style(UiTextStyle::body(palette.tokens().text))
}

pub(super) fn parameters(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    id: SceneNodeId,
    source: Option<usize>,
    values: &[SceneVariable],
    open: Option<InspectorDropdown>,
) -> UiNode {
    let scope = scope_key(source);
    let uuid = scene.get(id).expect("selected node").uuid;
    let mut section = UiNode::new(format!("inspector.parameters.{scope}"), UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 5.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
    for (index, parameter) in values.iter().enumerate().take(256) {
        let action =
            |action: &str| format!("inspector.parameter.{action}:{}:{scope}:{index}", id.0);
        let mut row = UiNode::new(
            format!("inspector.parameter-row.{scope}.{index}"),
            UiNodeKind::Panel,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 3.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        });
        let header = UiNode::new(
            format!("inspector.parameter-header.{scope}.{index}"),
            UiNodeKind::Toolbar,
        )
        .with_layout(UiLayout {
            flow: UiFlow::Row,
            gap: 3.0,
            ..UiLayout::fixed(0.0, 28.0).with_width_mode(UiSizeMode::Fill)
        })
        .with_child(
            input(
                palette,
                parameter_key(uuid, source, index, "name"),
                action("rename"),
                "app.name",
            )
            .with_layout(UiLayout {
                grow: 1.0,
                min_size: [48.0, 28.0],
                ..UiLayout::fixed(0.0, 28.0)
            }),
        )
        .with_child(
            action_button(
                palette,
                &format!("inspector.parameter-type.{scope}.{index}"),
                "behavior.cycle_type",
                action("type"),
            )
            .with_text_key(match parameter.value {
                VariableValue::Bool(_) => "behavior.type_bool",
                VariableValue::Number(_) => "behavior.type_number",
                VariableValue::Text(_) => "behavior.type_text",
                VariableValue::Entity(_) => "behavior.type_entity",
            }),
        )
        .with_child(action_button(
            palette,
            &format!("inspector.parameter-remove.{scope}.{index}"),
            "app.remove",
            action("remove"),
        ));
        row = row.with_child(header);
        row = match parameter.value {
            VariableValue::Entity(current) => {
                let dropdown = InspectorDropdown::Parameter { source, index };
                // Closed selectors retain only their current label, not a per-field scene scan.
                let mut options = vec![UiSelectOption::new("none", "behavior.unassigned")];
                if open == Some(dropdown) {
                    options.extend(
                        scene
                            .iter()
                            .filter(|(id, _)| scene.is_valid_node(*id))
                            .take(256)
                            .map(|(id, node)| {
                                UiSelectOption::new(
                                    node.uuid.to_string(),
                                    format!(
                                        "{} [{}]",
                                        scene.node_path(id).unwrap_or_else(|| node.name.clone()),
                                        &node.uuid.to_string()[..8]
                                    ),
                                )
                            }),
                    );
                }
                let current_value = current
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "none".into());
                if current.is_some() && !options.iter().any(|option| option.value == current_value)
                {
                    let label = raf_script::references::entity(scene, &current_value)
                        .and_then(|id| scene.node_path(id))
                        .unwrap_or_else(|| "behavior.missing".into());
                    options.push(UiSelectOption::new(&current_value, label));
                }
                let selected = options
                    .iter()
                    .position(|p| p.value == current_value)
                    .unwrap_or(0);
                row.with_child(dropdown_field(
                    palette,
                    &format!("inspector.parameter-ref.{scope}.{index}"),
                    "behavior.entity_reference",
                    dropdown,
                    open,
                    options,
                    selected,
                ))
            }
            _ => row.with_child(input(
                palette,
                parameter_key(uuid, source, index, "value"),
                action("value"),
                "behavior.parameter_value",
            )),
        };
        section = section.with_child(row);
    }
    section.with_child(action_button(
        palette,
        &format!("inspector.parameter-add.{scope}"),
        "behavior.add_parameter",
        format!("inspector.parameter.add:{}:{scope}:0", id.0),
    ))
}

pub(super) fn behaviors(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    id: SceneNodeId,
    assets: &[String],
    open: Option<InspectorDropdown>,
    expanded: Option<usize>,
) -> UiNode {
    let node = scene.get(id).expect("selected node");
    let mut section = UiNode::new("inspector.behaviors", UiNodeKind::Panel)
        .with_layout(UiLayout {
            flow: UiFlow::Column,
            gap: 6.0,
            ..UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)
        })
        .with_child(section_label(palette, "behavior.title"));
    for (index, source) in node.scripts.iter().enumerate().take(64) {
        section = section.with_child(
            UiNode::new(format!("inspector.behavior.{index}"), UiNodeKind::Label)
                .with_text_value(source)
                .with_text_style(UiTextStyle::button(palette.tokens().text))
                .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill))
                .with_text_overflow(UiTextOverflow::Ellipsis),
        );
        section = section.with_child(action_button(
            palette,
            &format!("inspector.behavior.parameters.{index}"),
            "behavior.parameters",
            format!("inspector.behavior.parameters:{index}"),
        ));
        if expanded == Some(index) {
            let values = node
                .behavior_parameters
                .get(source)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            section = section.with_child(parameters(palette, scene, id, Some(index), values, open));
        }
        section = section.with_child(action_button(
            palette,
            &format!("inspector.behavior.detach.{index}"),
            "behavior.detach",
            format!("inspector.behavior.detach:{}:{index}", id.0),
        ));
    }
    let mut options = vec![UiSelectOption::new("", "behavior.choose_source")];
    options.extend(
        assets
            .iter()
            .filter(|path| path.ends_with(".rhai") && !node.scripts.contains(path))
            .take(256)
            .map(|path| UiSelectOption::new(path, path)),
    );
    section = section.with_child(dropdown_field(
        palette,
        "inspector.behavior-source",
        "behavior.attach",
        InspectorDropdown::BehaviorSource,
        open,
        options,
        0,
    ));
    section = section
        .with_child(section_label(palette, "behavior.prefab_title"))
        .with_child(input(
            palette,
            format!("inspector.prefab.{}.file", node.uuid),
            format!("inspector.prefab.capture:{}", id.0),
            "behavior.prefab_file",
        ))
        .with_child(action_button(
            palette,
            "inspector.prefab.capture",
            "behavior.prefab_capture",
            format!("inspector.prefab.capture:{}", id.0),
        ));
    let mut options = vec![UiSelectOption::new("", "behavior.choose_prefab")];
    options.extend(
        assets
            .iter()
            .filter(|path| path.ends_with(".prefab.ron"))
            .take(256)
            .map(|path| UiSelectOption::new(path, path)),
    );
    section.with_child(dropdown_field(
        palette,
        "inspector.prefab-source",
        "behavior.prefab_instantiate",
        InspectorDropdown::PrefabSource,
        open,
        options,
        0,
    ))
}
