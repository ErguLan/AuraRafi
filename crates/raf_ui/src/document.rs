//! Serializable user-authored UI documents.
//!
//! A document deliberately starts with an empty root. The editor may provide
//! authoring tools, but it never injects a HUD, menu, or template into a game.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{UiNode, UiNodeKind, UiStyle};

pub const UI_DOCUMENT_VERSION: u32 = 1;

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

    pub fn add_node(&mut self, parent_id: &str, node: UiNode) -> Result<(), String> {
        if self.find_node(&node.id).is_some() {
            return Err("UI node id already exists.".to_string());
        }
        let Some(parent) = find_node_mut(&mut self.root, parent_id) else {
            return Err("UI parent node was not found.".to_string());
        };
        parent.children.push(node);
        Ok(())
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
}
