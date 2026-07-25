//! Structured inspection data for RafUI Studio.

use serde::{Deserialize, Serialize};

use crate::{UiDocument, UiLayout, UiNode, UiNodeKind, UiStyle, UiTextStyle};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioNodePath {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiStudioPropertyGroup {
    Identity,
    Layout,
    Appearance,
    Typography,
    Interaction,
    Events,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStudioNodeInspection {
    pub path: UiStudioNodePath,
    pub id: String,
    pub kind: UiNodeKind,
    pub classes: Vec<String>,
    pub text_key: Option<String>,
    pub tooltip_key: Option<String>,
    pub accessibility_label_key: Option<String>,
    pub layout: UiLayout,
    pub style: UiStyle,
    pub text_style: Option<UiTextStyle>,
    pub interactive: bool,
    pub focusable: bool,
    pub disabled: bool,
    pub child_count: usize,
    pub event_count: usize,
    pub groups: Vec<UiStudioPropertyGroup>,
}

pub fn inspect_document(document: &UiDocument) -> Vec<UiStudioNodeInspection> {
    let mut result = Vec::new();
    let mut path = Vec::new();
    inspect_node(&document.root, &mut path, &mut result);
    result
}

pub fn inspect_node_by_id(document: &UiDocument, id: &str) -> Option<UiStudioNodeInspection> {
    inspect_document(document)
        .into_iter()
        .find(|inspection| inspection.id == id)
}

fn inspect_node(node: &UiNode, path: &mut Vec<String>, result: &mut Vec<UiStudioNodeInspection>) {
    path.push(node.id.clone());
    result.push(UiStudioNodeInspection {
        path: UiStudioNodePath { ids: path.clone() },
        id: node.id.clone(),
        kind: node.kind,
        classes: node.classes.clone(),
        text_key: node.text_key.clone(),
        tooltip_key: node.tooltip_key.clone(),
        accessibility_label_key: node.accessibility_label_key.clone(),
        layout: node.layout.clone(),
        style: node.style.clone(),
        text_style: node.text_style,
        interactive: node.interactive,
        focusable: node.focusable,
        disabled: node.disabled,
        child_count: node.children.len(),
        event_count: node.event_handlers.len(),
        groups: vec![
            UiStudioPropertyGroup::Identity,
            UiStudioPropertyGroup::Layout,
            UiStudioPropertyGroup::Appearance,
            UiStudioPropertyGroup::Typography,
            UiStudioPropertyGroup::Interaction,
            UiStudioPropertyGroup::Events,
        ],
    });
    for child in &node.children {
        inspect_node(child, path, result);
    }
    path.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiDocument, UiNode};

    #[test]
    fn inspector_keeps_stable_paths_and_groups() {
        let mut document = UiDocument::blank("studio");
        document
            .add_node(
                "root",
                UiNode::new("toolbar", UiNodeKind::Toolbar)
                    .with_child(UiNode::new("grid", UiNodeKind::Button)),
            )
            .unwrap();

        let grid = inspect_node_by_id(&document, "grid").unwrap();
        assert_eq!(grid.path.ids, ["root", "toolbar", "grid"]);
        assert!(grid.groups.contains(&UiStudioPropertyGroup::Layout));
    }
}
