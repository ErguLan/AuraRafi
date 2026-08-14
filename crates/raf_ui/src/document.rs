//! Serializable user-authored UI documents.
//!
//! A document deliberately starts with an empty root. The editor may provide
//! authoring tools, but it never injects a HUD, menu, or template into a game.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{UiNode, UiNodeKind, UiStyle, UiTheme};

pub const UI_DOCUMENT_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UiDocumentId(pub Uuid);

impl UiDocumentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for UiDocumentId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiDocumentSpace {
    Screen,
    World,
    Camera,
}

impl Default for UiDocumentSpace {
    fn default() -> Self {
        Self::Screen
    }
}

/// Optional reference from a runtime camera to a UI document.
///
/// The camera references a document but never owns its nodes in the scene
/// hierarchy. That keeps one document reusable across cameras and sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiCameraBinding {
    pub camera_key: String,
    pub document_id: UiDocumentId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDocument {
    pub version: u32,
    pub id: UiDocumentId,
    pub name: String,
    #[serde(default)]
    pub space: UiDocumentSpace,
    #[serde(default)]
    pub camera_binding: Option<UiCameraBinding>,
    #[serde(default)]
    pub theme: UiTheme,
    pub root: UiNode,
}

impl UiDocument {
    pub fn blank(name: impl Into<String>) -> Self {
        Self {
            version: UI_DOCUMENT_VERSION,
            id: UiDocumentId::new(),
            name: name.into(),
            space: UiDocumentSpace::Screen,
            camera_binding: None,
            theme: UiTheme::raf_ui(),
            root: UiNode::new("root", UiNodeKind::Root).with_style(UiStyle::transparent()),
        }
    }

    pub fn bind_to_camera(&mut self, camera_key: impl Into<String>) {
        self.space = UiDocumentSpace::Camera;
        self.camera_binding = Some(UiCameraBinding {
            camera_key: camera_key.into(),
            document_id: self.id,
        });
    }

    pub fn clear_camera_binding(&mut self) {
        self.camera_binding = None;
        if self.space == UiDocumentSpace::Camera {
            self.space = UiDocumentSpace::Screen;
        }
    }

    pub fn find_node(&self, id: &str) -> Option<&UiNode> {
        find_node(&self.root, id)
    }

    /// Mutable counterpart used by controlled authoring tools such as RafUI
    /// authoring tools. Domain state still belongs to the application boundary; this
    /// only edits the retained UI document itself.
    pub fn find_node_mut(&mut self, id: &str) -> Option<&mut UiNode> {
        find_node_mut(&mut self.root, id)
    }

    pub fn add_node(&mut self, parent_id: &str, node: UiNode) -> Result<(), String> {
        if self.find_node(&node.id).is_some() {
            return Err("UI node id already exists.".to_string());
        }
        let mut ids = std::collections::BTreeSet::new();
        collect_ids(&node, &mut ids)?;
        if ids.iter().any(|id| self.find_node(id).is_some()) {
            return Err("UI subtree contains an id already used by the document.".to_string());
        }
        let Some(parent) = find_node_mut(&mut self.root, parent_id) else {
            return Err("UI parent node was not found.".to_string());
        };
        parent.children.push(node);
        Ok(())
    }

    /// Validates serialized documents before a host attempts layout or input.
    /// This keeps malformed authoring data from reaching renderer hot paths.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.version == 0 || self.version > UI_DOCUMENT_VERSION {
            errors.push(format!("Unsupported UI document version {}.", self.version));
        }
        if self.name.trim().is_empty() {
            errors.push("UI document name cannot be empty.".to_string());
        }
        if self.root.kind != UiNodeKind::Root {
            errors.push("UI document root must have kind Root.".to_string());
        }
        let mut ids = std::collections::BTreeSet::new();
        validate_node(&self.root, &mut ids, &mut errors);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Applies the lossless migrations known by the current document format.
    pub fn migrate(&mut self) -> Result<(), Vec<String>> {
        if self.version > UI_DOCUMENT_VERSION {
            return Err(vec![format!(
                "Cannot migrate future UI document version {}.",
                self.version
            )]);
        }
        self.version = UI_DOCUMENT_VERSION;
        self.validate()
    }

    pub fn remove_node(&mut self, id: &str) -> Option<UiNode> {
        if id == self.root.id {
            return None;
        }
        remove_node(&mut self.root, id)
    }
}

impl Default for UiDocument {
    fn default() -> Self {
        Self::blank("Untitled UI")
    }
}

fn find_node<'a>(node: &'a UiNode, id: &str) -> Option<&'a UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_node(child, id))
}

fn find_node_mut<'a>(node: &'a mut UiNode, id: &str) -> Option<&'a mut UiNode> {
    if node.id == id {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_node_mut(child, id) {
            return Some(found);
        }
    }
    None
}

fn remove_node(node: &mut UiNode, id: &str) -> Option<UiNode> {
    let index = node.children.iter().position(|child| child.id == id);
    if let Some(index) = index {
        return Some(node.children.remove(index));
    }
    for child in &mut node.children {
        if let Some(removed) = remove_node(child, id) {
            return Some(removed);
        }
    }
    None
}

fn collect_ids(node: &UiNode, ids: &mut std::collections::BTreeSet<String>) -> Result<(), String> {
    if node.id.trim().is_empty() {
        return Err("UI node id cannot be empty.".to_string());
    }
    if !ids.insert(node.id.clone()) {
        return Err(format!(
            "UI subtree contains duplicate node id '{}'.",
            node.id
        ));
    }
    for child in &node.children {
        collect_ids(child, ids)?;
    }
    Ok(())
}

fn validate_node(
    node: &UiNode,
    ids: &mut std::collections::BTreeSet<String>,
    errors: &mut Vec<String>,
) {
    if node.id.trim().is_empty() {
        errors.push("UI node id cannot be empty.".to_string());
    } else if !ids.insert(node.id.clone()) {
        errors.push(format!("Duplicate UI node id '{}'.", node.id));
    }
    if !node.layout.gap.is_finite() || node.layout.gap < 0.0 {
        errors.push(format!("Node '{}' has an invalid layout gap.", node.id));
    }
    for dimension in node
        .layout
        .basis
        .iter()
        .chain(node.layout.min_size.iter())
        .chain(node.layout.max_size.iter())
    {
        if !dimension.is_finite() || *dimension < 0.0 {
            errors.push(format!(
                "Node '{}' has an invalid layout dimension.",
                node.id
            ));
            break;
        }
    }
    for padding in [
        node.layout.padding.left,
        node.layout.padding.right,
        node.layout.padding.top,
        node.layout.padding.bottom,
    ] {
        if !padding.is_finite() || padding < 0.0 {
            errors.push(format!("Node '{}' has invalid layout padding.", node.id));
            break;
        }
    }
    if !node.layout.grow.is_finite() || node.layout.grow < 0.0 {
        errors.push(format!(
            "Node '{}' has an invalid layout grow value.",
            node.id
        ));
    }
    if !node.layout.grid.min_column_width.is_finite()
        || node.layout.grid.min_column_width < 0.0
        || !node.layout.grid.row_height.is_finite()
        || node.layout.grid.row_height < 0.0
    {
        errors.push(format!("Node '{}' has invalid grid metrics.", node.id));
    }
    if let Some(rect) = node.layout.rect {
        if [rect.x, rect.y, rect.width, rect.height]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            errors.push(format!("Node '{}' has an invalid explicit rect.", node.id));
        }
    }
    if let Some(text_style) = node.text_style {
        if !text_style.size_px.is_finite()
            || !text_style.line_height_px.is_finite()
            || text_style.size_px <= 0.0
            || text_style.line_height_px <= 0.0
        {
            errors.push(format!("Node '{}' has invalid text metrics.", node.id));
        }
    }
    if node.kind == UiNodeKind::TextInput && node.control.text_input().is_none() {
        errors.push(format!(
            "TextInput '{}' is missing UiTextInput control data.",
            node.id
        ));
    }
    if node.kind == UiNodeKind::ScrollView && node.control.scroll_axis().is_none() {
        errors.push(format!(
            "ScrollView '{}' is missing scroll axis data.",
            node.id
        ));
    }
    for child in &node.children {
        validate_node(child, ids, errors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_document_has_no_user_nodes() {
        let document = UiDocument::blank("Interface");
        assert!(document.root.children.is_empty());
        assert_eq!(document.space, UiDocumentSpace::Screen);
    }

    #[test]
    fn camera_binding_references_the_document_without_reparenting_nodes() {
        let mut document = UiDocument::blank("Interface");
        document.bind_to_camera("player.camera");

        assert_eq!(document.space, UiDocumentSpace::Camera);
        assert_eq!(document.root.children.len(), 0);
        assert_eq!(
            document
                .camera_binding
                .as_ref()
                .map(|binding| binding.document_id),
            Some(document.id)
        );
    }

    #[test]
    fn document_nodes_are_added_and_removed_by_stable_id() {
        let mut document = UiDocument::blank("Interface");
        document
            .add_node("root", UiNode::new("start", UiNodeKind::Button))
            .unwrap();
        assert!(document.find_node("start").is_some());
        assert!(document.remove_node("start").is_some());
        assert!(document.find_node("start").is_none());
    }

    #[test]
    fn validation_rejects_duplicate_ids_and_bad_root() {
        let mut document = UiDocument::blank("Interface");
        document.root.kind = UiNodeKind::Panel;
        document
            .root
            .children
            .push(UiNode::new("same", UiNodeKind::Label));
        document
            .root
            .children
            .push(UiNode::new("same", UiNodeKind::Label));
        let errors = document.validate().unwrap_err();
        assert!(errors.iter().any(|error| error.contains("root")));
        assert!(errors.iter().any(|error| error.contains("Duplicate")));
    }

    #[test]
    fn validation_rejects_non_finite_layout_values() {
        let mut document = UiDocument::blank("Interface");
        document.root.layout.gap = f32::NAN;
        document.root.layout.grid.row_height = f32::INFINITY;
        let errors = document.validate().unwrap_err();
        assert!(errors.iter().any(|error| error.contains("gap")));
        assert!(errors.iter().any(|error| error.contains("grid")));
    }
}
