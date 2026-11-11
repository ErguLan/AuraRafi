//! Game-owned UI instances and presentation descriptors, independent of a renderer.
use crate::{UiDocument, UiDocumentUpdate, UiLayout, UiNode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_UI_INSTANCES: usize = 32;
pub const MAX_UI_NODES: usize = 2048;
pub const MAX_UI_CANVAS_PIXELS: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiPresentationKind {
    #[default]
    Screen,
    CameraOverlay,
    CameraPlane,
    Surface,
    Billboard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiMount {
    pub id: String,
    pub kind: UiPresentationKind,
    /// Stable entity UUID or an explicit scene reference. Empty means no target.
    pub target: String,
    pub offset: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub size_meters: [f32; 2],
    pub canvas_size: [u32; 2],
    pub max_distance: f32,
    pub interactive: bool,
    pub visible: bool,
    pub always_on_top: bool,
}
impl Default for UiMount {
    fn default() -> Self {
        Self {
            id: "main".into(),
            kind: UiPresentationKind::Screen,
            target: String::new(),
            offset: [0.0; 3],
            rotation_degrees: [0.0; 3],
            size_meters: [1.0, 0.6],
            canvas_size: [640, 384],
            max_distance: 100.0,
            interactive: true,
            visible: true,
            always_on_top: false,
        }
    }
}
impl UiMount {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 128
            || self.target.len() > 256
            || self
                .offset
                .iter()
                .chain(&self.rotation_degrees)
                .any(|v| !v.is_finite())
            || self
                .size_meters
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v > 1000.0)
            || self.canvas_size.iter().any(|v| *v == 0 || *v > 2048)
            || !self.max_distance.is_finite()
            || self.max_distance <= 0.0
        {
            return Err("ui.invalid_presentation".into());
        }
        Ok(())
    }
}

/// A value subscription updates authored properties only when its source changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiValueBinding {
    pub key: String,
    pub node: String,
    pub property: UiBoundProperty,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiBoundProperty {
    Text,
    Visible,
    Enabled,
}

#[derive(Debug, Clone)]
pub struct UiGameInstance {
    pub document: UiDocument,
    pub mount: UiMount,
    pub revision: u64,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiElementRef {
    pub instance: String,
    pub node: String,
    pub generation: u64,
}

#[derive(Debug, Default)]
pub struct UiGameRuntime {
    instances: BTreeMap<String, UiGameInstance>,
    next_generation: u64,
    values: BTreeMap<String, String>,
}
impl UiGameRuntime {
    pub fn instances(&self) -> &BTreeMap<String, UiGameInstance> {
        &self.instances
    }
    pub fn mount(&mut self, document: &UiDocument, mount: UiMount) -> Result<(), String> {
        document.validate().map_err(|errors| errors.join("; "))?;
        mount.validate()?;
        if self.instances.contains_key(&mount.id) {
            return Err("ui.duplicate_instance".into());
        }
        if self.instances.len() >= MAX_UI_INSTANCES {
            return Err("ui.instance_budget".into());
        }
        self.check_canvas_budget(None, &mount)?;
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.instances.insert(
            mount.id.clone(),
            UiGameInstance {
                document: document.clone(),
                mount,
                revision: 1,
                generation: self.next_generation,
            },
        );
        Ok(())
    }
    pub fn mount_document(&mut self, document: &UiDocument) -> Result<(), String> {
        document.validate().map_err(|errors| errors.join("; "))?;
        let mounts = if document.mounts.is_empty() {
            let mut mount = UiMount::default();
            if document.space == crate::UiDocumentSpace::Camera {
                mount.kind = UiPresentationKind::CameraOverlay;
                mount.target = document
                    .camera_binding
                    .as_ref()
                    .map(|b| b.camera_key.clone())
                    .unwrap_or_default();
            } else if document.space == crate::UiDocumentSpace::World {
                return Err("ui.world_target_required".into());
            }
            vec![mount]
        } else {
            document.mounts.clone()
        };
        // Reject the complete set before installing any part of it.
        if mounts.len() + self.instances.len() > MAX_UI_INSTANCES {
            return Err("ui.instance_budget".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for mount in &mounts {
            mount.validate()?;
            if !ids.insert(&mount.id) || self.instances.contains_key(&mount.id) {
                return Err("ui.duplicate_instance".into());
            }
        }
        let pixels: u64 = self
            .instances
            .values()
            .map(|i| canvas_pixels(&i.mount))
            .sum::<u64>()
            + mounts.iter().map(canvas_pixels).sum::<u64>();
        if pixels > MAX_UI_CANVAS_PIXELS {
            return Err("ui.canvas_budget".into());
        }
        for mount in mounts {
            self.mount(document, mount)?;
        }
        Ok(())
    }
    pub fn element(&self, instance: &str, node: &str) -> Result<UiElementRef, String> {
        let item = self.instances.get(instance).ok_or("ui.instance_missing")?;
        item.document.find_node(node).ok_or("ui.element_missing")?;
        Ok(UiElementRef {
            instance: instance.into(),
            node: node.into(),
            generation: item.generation,
        })
    }
    pub fn update(
        &mut self,
        reference: &UiElementRef,
        update: UiDocumentUpdate,
    ) -> Result<(), String> {
        let item = self
            .instances
            .get_mut(&reference.instance)
            .ok_or("ui.instance_missing")?;
        if item.generation != reference.generation {
            return Err("ui.stale_reference".into());
        }
        let before = item
            .document
            .find_node(&reference.node)
            .cloned()
            .ok_or("ui.element_missing")?;
        item.document
            .update_node(&reference.node, update)
            .map_err(|e| e.to_string())?;
        if item.document.find_node(&reference.node) != Some(&before) {
            item.revision = item.revision.wrapping_add(1).max(1);
        }
        Ok(())
    }
    pub fn show(&mut self, instance: &str, visible: bool) -> Result<(), String> {
        let item = self
            .instances
            .get_mut(instance)
            .ok_or("ui.instance_missing")?;
        if item.mount.visible != visible {
            item.mount.visible = visible;
            item.revision = item.revision.wrapping_add(1).max(1);
        }
        Ok(())
    }
    pub fn close(&mut self, instance: &str) -> bool {
        self.instances.remove(instance).is_some()
    }
    pub fn clone_instance(&mut self, source: &str, id: &str) -> Result<(), String> {
        let item = self
            .instances
            .get(source)
            .ok_or("ui.instance_missing")?
            .clone();
        let mut mount = item.mount;
        mount.id = id.into();
        self.mount(&item.document, mount)
    }
    pub fn set_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        if key.is_empty()
            || key.len() > 128
            || value.len() > 4096
            || (!self.values.contains_key(key) && self.values.len() >= 256)
        {
            return Err("ui.value_budget".into());
        }
        if self.values.get(key).is_some_and(|old| old == value) {
            return Ok(());
        }
        if value != "true"
            && value != "false"
            && self.instances.values().any(|item| {
                item.document
                    .bindings
                    .iter()
                    .any(|b| b.key == key && b.property != UiBoundProperty::Text)
            })
        {
            return Err("ui.invalid_value".into());
        }
        self.values.insert(key.into(), value.into());
        for item in self.instances.values_mut() {
            let bindings = item
                .document
                .bindings
                .iter()
                .filter(|b| b.key == key)
                .cloned()
                .collect::<Vec<_>>();
            for binding in bindings {
                let update = match binding.property {
                    UiBoundProperty::Text => UiDocumentUpdate::TextValue(value.into()),
                    UiBoundProperty::Visible => UiDocumentUpdate::Visible(value == "true"),
                    UiBoundProperty::Enabled => UiDocumentUpdate::Enabled(value == "true"),
                };
                item.document
                    .update_node(&binding.node, update)
                    .map_err(|e| e.to_string())?;
                item.revision = item.revision.wrapping_add(1).max(1);
            }
        }
        Ok(())
    }
    pub fn set_mount(&mut self, instance: &str, mount: UiMount) -> Result<(), String> {
        mount.validate()?;
        if mount.id != instance {
            return Err("ui.instance_identity".into());
        }
        self.check_canvas_budget(Some(instance), &mount)?;
        let item = self
            .instances
            .get_mut(instance)
            .ok_or("ui.instance_missing")?;
        if item.mount != mount {
            item.mount = mount;
            item.revision = item.revision.wrapping_add(1).max(1);
        }
        Ok(())
    }
    fn check_canvas_budget(&self, replace: Option<&str>, mount: &UiMount) -> Result<(), String> {
        let pixels = canvas_pixels(mount)
            + self
                .instances
                .iter()
                .filter(|(id, _)| Some(id.as_str()) != replace)
                .map(|(_, i)| canvas_pixels(&i.mount))
                .sum::<u64>();
        if pixels > MAX_UI_CANVAS_PIXELS {
            Err("ui.canvas_budget".into())
        } else {
            Ok(())
        }
    }
    pub fn edit(
        &mut self,
        instance: &str,
        edits: &[crate::authoring::UiEdit],
    ) -> Result<(), String> {
        let item = self
            .instances
            .get_mut(instance)
            .ok_or("ui.instance_missing")?;
        if crate::authoring::apply_ui_edits(&mut item.document, edits)? {
            if edits.iter().any(|edit| {
                matches!(
                    edit,
                    crate::authoring::UiEdit::Remove { .. }
                        | crate::authoring::UiEdit::Rename { .. }
                )
            }) {
                // Removed/recreated IDs must not revive references held by scripts.
                self.next_generation = self.next_generation.wrapping_add(1).max(1);
                item.generation = self.next_generation;
            }
            item.revision = item.revision.wrapping_add(1).max(1);
        }
        Ok(())
    }
}
fn canvas_pixels(mount: &UiMount) -> u64 {
    if matches!(
        mount.kind,
        UiPresentationKind::Screen | UiPresentationKind::CameraOverlay
    ) {
        0
    } else {
        u64::from(mount.canvas_size[0]) * u64::from(mount.canvas_size[1])
    }
}

/// Serializable anchored placement in logical units. Offsets are additive to
/// normalized parent coordinates; layout remains independent from physical DPI.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiAnchoredRect {
    pub anchor: [f32; 2],
    pub pivot: [f32; 2],
    pub relative_size: [f32; 2],
    pub offset: [f32; 2],
    pub size: [f32; 2],
}
impl UiAnchoredRect {
    pub fn resolve(self, parent: crate::UiRect, layout: &UiLayout) -> crate::UiRect {
        let axis = |i: usize, extent: f32| {
            let value = (extent * self.relative_size[i] + self.size[i]).max(layout.min_size[i]);
            if layout.max_size[i] > 0.0 {
                value.min(layout.max_size[i])
            } else {
                value
            }
        };
        let size = [axis(0, parent.width), axis(1, parent.height)];
        crate::UiRect::new(
            parent.x + parent.width * self.anchor[0] + self.offset[0] - size[0] * self.pivot[0],
            parent.y + parent.height * self.anchor[1] + self.offset[1] - size[1] * self.pivot[1],
            size[0],
            size[1],
        )
    }
}

pub fn visit_ui_nodes(node: &UiNode, visitor: &mut impl FnMut(&UiNode)) {
    visitor(node);
    for child in &node.children {
        visit_ui_nodes(child, visitor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authoring::{apply_ui_edits, UiEdit, UiElementKind};
    #[test]
    fn game_ui_edits_roundtrip_are_atomic_and_references_do_not_revive() {
        let mut document = UiDocument::blank("HUD");
        apply_ui_edits(
            &mut document,
            &[UiEdit::Insert {
                parent: "root".into(),
                id: "score".into(),
                kind: UiElementKind::Label,
                rect: None,
                text: "0".into(),
            }],
        )
        .unwrap();
        let original = document.clone();
        assert!(apply_ui_edits(
            &mut document,
            &[
                UiEdit::Rename {
                    node: "score".into(),
                    id: "total".into()
                },
                UiEdit::Reparent {
                    node: "root".into(),
                    parent: "total".into()
                }
            ]
        )
        .is_err());
        assert_eq!(document, original);
        let serialized = serde_json::to_vec(&document).unwrap();
        let decoded: UiDocument = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(decoded, original);
        let mut runtime = UiGameRuntime::default();
        runtime.mount_document(&document).unwrap();
        let reference = runtime.element("main", "score").unwrap();
        runtime
            .update(&reference, UiDocumentUpdate::TextValue("5".into()))
            .unwrap();
        let revision = runtime.instances()["main"].revision;
        runtime
            .update(&reference, UiDocumentUpdate::TextValue("5".into()))
            .unwrap();
        assert_eq!(runtime.instances()["main"].revision, revision);
        runtime
            .edit(
                "main",
                &[
                    UiEdit::Remove {
                        node: "score".into(),
                    },
                    UiEdit::Insert {
                        parent: "root".into(),
                        id: "score".into(),
                        kind: UiElementKind::Label,
                        rect: None,
                        text: "new".into(),
                    },
                ],
            )
            .unwrap();
        assert!(runtime
            .update(&reference, UiDocumentUpdate::Visible(false))
            .is_err());
        assert_eq!(document, original);
    }
    #[test]
    fn game_ui_mount_set_is_atomic_and_canvas_residency_is_bounded() {
        let mut document = UiDocument::blank("World UI");
        document.mounts = vec![
            UiMount {
                kind: UiPresentationKind::Surface,
                target: "part".into(),
                canvas_size: [2048, 2048],
                ..Default::default()
            },
            UiMount {
                id: "second".into(),
                kind: UiPresentationKind::Surface,
                target: "part".into(),
                ..Default::default()
            },
        ];
        let mut runtime = UiGameRuntime::default();
        assert!(runtime.mount_document(&document).is_err());
        assert!(runtime.instances().is_empty());
        document.mounts.pop();
        runtime.mount_document(&document).unwrap();
        assert!(runtime.clone_instance("main", "copy").is_err());
        assert_eq!(runtime.instances().len(), 1);
        let anchor = UiAnchoredRect {
            anchor: [1.0, 0.0],
            pivot: [1.0, 0.0],
            relative_size: [0.5, 0.0],
            offset: [-12.0, 8.0],
            size: [0.0, 24.0],
        };
        assert_eq!(
            anchor.resolve(
                crate::UiRect::new(20.0, 30.0, 400.0, 200.0),
                &UiLayout::default()
            ),
            crate::UiRect::new(208.0, 38.0, 200.0, 24.0)
        );
    }
}
