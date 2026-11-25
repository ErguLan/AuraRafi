//! Behavior and variable section composition for the game Inspector.

use super::behavior_surface;
use super::sections::section_title;
use super::*;

/// Builds the variable and behavior children in document order.
pub(super) fn build_behavior_sections(
    palette: StudioUiPalette,
    scene: &SceneGraph,
    id: SceneNodeId,
    node: &raf_core::scene::SceneNode,
    project_assets: &[String],
    view: InspectorViewState,
) -> Vec<UiNode> {
    let mut children = Vec::new();
    children.push(section_title(
        palette,
        "app.variables",
        InspectorSection::Variables,
        view.variables,
    ));
    if view.variables {
        children.push(behavior_surface::parameters(
            palette,
            scene,
            id,
            None,
            &node.variables,
            view.dropdown,
        ));
    }
    children.push(behavior_surface::behaviors(
        palette,
        scene,
        id,
        project_assets,
        view.dropdown,
        view.behavior_parameters,
    ));
    children
}

pub(crate) fn variable_value_text(value: &VariableValue) -> String {
    match value {
        VariableValue::Bool(value) => value.to_string(),
        VariableValue::Number(value) => value.to_string(),
        VariableValue::Text(value) => value.clone(),
        VariableValue::Entity(value) => value.map(|id| id.to_string()).unwrap_or_default(),
    }
}
