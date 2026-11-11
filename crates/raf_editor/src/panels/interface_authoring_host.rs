//! Direct-manipulation transactions and view commands for the interface document.
use super::super::interface_workspace::{InterfaceGesture, InterfaceView, MIN_ELEMENT_SIZE};
use super::*;

impl InterfaceSurfaceHost {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn open_asset(&mut self, asset: &str) -> Result<(), String> {
        self.finish_gesture();
        if self.dirty {
            self.save()?;
        }
        let root = self.project_root.as_ref().ok_or("ui.project_required")?;
        let relative = PathBuf::from(format!("assets/{}", asset.trim_start_matches("assets/")));
        let path = crate::ui_document_store::document_path(root, &relative)?;
        let modified = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok());
        let document = crate::ui_document_store::load(&path)?;
        self.document = document;
        self.selected = self.document.root.id.clone();
        self.path = Some(path);
        self.modified = modified;
        self.editing_asset = Some(relative);
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        self.diagnostic = None;
        self.force_images = true;
        self.drag = None;
        self.workspace.view = InterfaceView::Live;
        self.note_changed();
        Ok(())
    }
    pub fn create_asset(&mut self, language: Language) -> Result<(), String> {
        self.finish_gesture();
        if self.dirty {
            self.save()?;
        }
        let root = self.project_root.as_ref().ok_or("ui.project_required")?;
        let document = UiDocument::blank(t("ui.designer.title", language));
        let relative = format!("assets/ui/{}.ui.ron", document.id.0);
        let path = crate::ui_document_store::document_path(root, Path::new(&relative))?;
        crate::ui_document_store::save(&path, &document)?;
        self.open_asset(&relative)
    }
    pub fn place_asset(&mut self, language: Language) -> Result<(), String> {
        let asset = self.editing_asset.clone().ok_or("ui.invalid_element")?;
        let (root, session) = self.session_scope.clone().ok_or("ui.project_required")?;
        self.save()?;
        self.editing_asset = None;
        self.sync(&root, &session);
        if let Some(error) = &self.diagnostic {
            return Err(error.clone());
        }
        if self.path.as_ref() != Some(&crate::ui_document_store::document_path(&root, &session)?) {
            return Err("ui.document_conflict".into());
        }
        self.selected = self.document.root.id.clone();
        self.insert_asset(asset.to_str().ok_or("ui.invalid_value")?, language)?;
        self.workspace.view = InterfaceView::Live;
        Ok(())
    }
    pub fn open_live(&mut self) {
        if !self.workspace.active() {
            self.workspace.view = InterfaceView::Live;
            self.note_changed();
        }
    }
    pub fn set_viewport(&mut self, size: [f32; 2]) {
        self.workspace.viewport = size;
    }
    pub fn parent_id(&self) -> Option<String> {
        find_parent(&self.document.root, &self.selected)
    }
    pub fn cancel_gesture(&mut self) -> bool {
        let Some(drag) = self.drag.take() else {
            return false;
        };
        self.document = drag.before;
        self.note_changed();
        true
    }
    pub fn finish_gesture(&mut self) {
        if let Some(drag) = self.drag.take() {
            if drag.before != self.document {
                self.record(drag.before);
            }
        }
    }
    pub fn gesture_active(&self) -> bool {
        self.drag.is_some()
    }

    pub(super) fn handle_authoring_command(
        &mut self,
        command: &str,
        pointer: Option<[f32; 2]>,
        _controls: &UiControlState,
    ) -> bool {
        let result = match command {
            "insert.toggle" => {
                self.workspace.insert_open = !self.workspace.insert_open;
                Ok(())
            }
            "insert.close" => {
                self.workspace.insert_open = false;
                Ok(())
            }
            "open" | "view:live" => {
                self.workspace.insert_open = false;
                self.cancel_gesture();
                self.workspace.view = InterfaceView::Live;
                Ok(())
            }
            "view:scene" => {
                self.workspace.insert_open = false;
                self.cancel_gesture();
                self.workspace.view = InterfaceView::Scene;
                self.editing_asset = None;
                Ok(())
            }
            "view:canvas" | "wide" | "narrow" => {
                self.workspace.insert_open = false;
                self.cancel_gesture();
                self.workspace.view = InterfaceView::Canvas;
                if command == "wide" {
                    self.preview_size = [1280, 720];
                }
                if command == "narrow" {
                    self.preview_size = [390, 844];
                }
                self.workspace.zoom = None;
                Ok(())
            }
            "fit" => {
                self.workspace.zoom = None;
                Ok(())
            }
            "zoom-in" | "zoom-out" => {
                let current = self
                    .workspace
                    .geometry(self.workspace.viewport, self.preview_size)
                    .scale;
                self.workspace.zoom = Some(
                    (current * if command == "zoom-in" { 1.2 } else { 1.0 / 1.2 }).clamp(0.1, 4.0),
                );
                Ok(())
            }
            "snap" => {
                self.workspace.snap = !self.workspace.snap;
                Ok(())
            }
            "move.start" | "resize.start" => {
                self.start_gesture(
                    pointer,
                    if command == "move.start" {
                        InterfaceGesture::Move
                    } else {
                        InterfaceGesture::Resize([1, 1])
                    },
                );
                Ok(())
            }
            "move.drag" => {
                self.update_gesture(pointer);
                Ok(())
            }
            "move.end" => {
                self.finish_gesture();
                Ok(())
            }
            "cancel" => {
                self.cancel_gesture();
                Ok(())
            }
            _ if command.starts_with("resize.start:") => {
                let direction = match command.trim_start_matches("resize.start:") {
                    "nw" => [-1, -1],
                    "n" => [0, -1],
                    "ne" => [1, -1],
                    "w" => [-1, 0],
                    "e" => [1, 0],
                    "sw" => [-1, 1],
                    "s" => [0, 1],
                    _ => [1, 1],
                };
                self.start_gesture(pointer, InterfaceGesture::Resize(direction));
                Ok(())
            }
            _ if command.starts_with("section:") => {
                let key = command.trim_start_matches("section:");
                if !self.workspace.expanded.remove(key) {
                    self.workspace.expanded.insert(key.into());
                }
                Ok(())
            }
            _ if command.starts_with("tree:") => {
                let key = command.trim_start_matches("tree:");
                if !self.workspace.collapsed_nodes.remove(key) {
                    self.workspace.collapsed_nodes.insert(key.into());
                }
                Ok(())
            }
            _ if command.starts_with("anchor:") => {
                self.anchor_preset(command.trim_start_matches("anchor:"))
            }
            _ if command.starts_with("color:") => {
                self.set_fill(command.trim_start_matches("color:"))
            }
            _ => return false,
        };
        self.diagnostic = result.err();
        self.note_changed();
        true
    }

    fn start_gesture(&mut self, pointer: Option<[f32; 2]>, gesture: InterfaceGesture) {
        if self.selected == self.document.root.id {
            return;
        }
        let Some(start) = pointer else {
            return;
        };
        let Some(node) = self.node() else {
            return;
        };
        let geometry = self
            .workspace
            .geometry(self.workspace.viewport, self.preview_size);
        let parent = self.workspace.parent_box.unwrap_or(UiRect::new(
            0.0,
            0.0,
            geometry.logical_size[0] * geometry.scale,
            geometry.logical_size[1] * geometry.scale,
        ));
        let parent_size = [
            parent.width / geometry.scale,
            parent.height / geometry.scale,
        ];
        let rect = self
            .workspace
            .selection_box
            .map(|r| {
                UiRect::new(
                    (r.x - parent.x) / geometry.scale,
                    (r.y - parent.y) / geometry.scale,
                    r.width / geometry.scale,
                    r.height / geometry.scale,
                )
            })
            .unwrap_or_else(|| {
                node.layout
                    .anchored
                    .map(|a| {
                        a.resolve(
                            UiRect::new(0.0, 0.0, parent_size[0], parent_size[1]),
                            &node.layout,
                        )
                    })
                    .or(node.layout.rect)
                    .unwrap_or(UiRect::new(
                        0.0,
                        0.0,
                        node.layout.basis[0],
                        node.layout.basis[1],
                    ))
            });
        self.drag = Some(InterfaceDrag {
            before: self.document.clone(),
            start,
            gesture,
            scale: geometry.scale,
            rect,
            parent_size,
        });
    }

    fn update_gesture(&mut self, pointer: Option<[f32; 2]>) {
        let (Some(drag), Some(pointer)) = (&self.drag, pointer) else {
            return;
        };
        let delta = [
            (pointer[0] - drag.start[0]) / drag.scale,
            (pointer[1] - drag.start[1]) / drag.scale,
        ];
        let mut rect = drag.rect;
        match drag.gesture {
            InterfaceGesture::Move => {
                rect.x += delta[0];
                rect.y += delta[1];
                if self.workspace.snap {
                    rect.x = snap_edge(rect.x, rect.width, drag.parent_size[0]);
                    rect.y = snap_edge(rect.y, rect.height, drag.parent_size[1]);
                }
            }
            InterfaceGesture::Resize(direction) => {
                if direction[0] != 0 {
                    rect.width =
                        (rect.width + delta[0] * direction[0] as f32).max(MIN_ELEMENT_SIZE);
                    if direction[0] < 0 {
                        rect.x = drag.rect.right() - rect.width;
                    }
                }
                if direction[1] != 0 {
                    rect.height =
                        (rect.height + delta[1] * direction[1] as f32).max(MIN_ELEMENT_SIZE);
                    if direction[1] < 0 {
                        rect.y = drag.rect.bottom() - rect.height;
                    }
                }
            }
        }
        let Some(node) = self.document.find_node_mut(&self.selected) else {
            return;
        };
        if let Some(mut anchor) = drag
            .before
            .find_node(&self.selected)
            .and_then(|n| n.layout.anchored)
        {
            anchor.size = [
                rect.width - drag.parent_size[0] * anchor.relative_size[0],
                rect.height - drag.parent_size[1] * anchor.relative_size[1],
            ];
            anchor.offset = [
                rect.x - drag.parent_size[0] * anchor.anchor[0] + rect.width * anchor.pivot[0],
                rect.y - drag.parent_size[1] * anchor.anchor[1] + rect.height * anchor.pivot[1],
            ];
            node.layout.anchored = Some(anchor);
        }
        node.layout.position_mode = UiPositionMode::Absolute;
        node.layout.rect = Some(rect);
    }

    fn anchor_preset(&mut self, preset: &str) -> Result<(), String> {
        let index: usize = preset.parse().map_err(|_| "ui.invalid_value")?;
        if index > 8 || self.selected == self.document.root.id {
            return Err("ui.invalid_element".into());
        }
        let node = self.node().ok_or("ui.element_missing")?;
        let geometry = self
            .workspace
            .geometry(self.workspace.viewport, self.preview_size);
        let parent_size = self
            .workspace
            .parent_box
            .map(|p| [p.width / geometry.scale, p.height / geometry.scale])
            .unwrap_or(geometry.logical_size);
        let mut layout = node.layout.clone();
        let resolved = self
            .workspace
            .selection_box
            .zip(self.workspace.parent_box)
            .map(|(r, p)| {
                UiRect::new(
                    (r.x - p.x) / geometry.scale,
                    (r.y - p.y) / geometry.scale,
                    r.width / geometry.scale,
                    r.height / geometry.scale,
                )
            })
            .or_else(|| {
                layout.anchored.map(|a| {
                    a.resolve(
                        UiRect::new(0.0, 0.0, parent_size[0], parent_size[1]),
                        &layout,
                    )
                })
            })
            .or(layout.rect)
            .unwrap_or(UiRect::new(0.0, 0.0, layout.basis[0], layout.basis[1]));
        let point = [(index % 3) as f32 * 0.5, (index / 3) as f32 * 0.5];
        let mut anchor = layout.anchored.unwrap_or(UiAnchoredRect {
            anchor: point,
            pivot: point,
            relative_size: [0.0; 2],
            offset: [0.0; 2],
            size: [resolved.width, resolved.height],
        });
        anchor.anchor = point;
        anchor.pivot = point;
        anchor.offset = [
            resolved.x - parent_size[0] * point[0] + resolved.width * point[0],
            resolved.y - parent_size[1] * point[1] + resolved.height * point[1],
        ];
        layout.anchored = Some(anchor);
        layout.rect = Some(resolved);
        self.apply(&[UiEdit::Patch {
            node: self.selected.clone(),
            patch: UiNodePatch {
                layout: Some(layout),
                ..Default::default()
            },
        }])?;
        Ok(())
    }
    fn set_fill(&mut self, value: &str) -> Result<(), String> {
        let mut style = self.node().ok_or("ui.element_missing")?.style.clone();
        style.fill = parse_color(value)?;
        self.apply(&[UiEdit::Patch {
            node: self.selected.clone(),
            patch: UiNodePatch {
                style: Some(style),
                ..Default::default()
            },
        }])?;
        Ok(())
    }
}
fn snap_edge(position: f32, extent: f32, parent: f32) -> f32 {
    [0.0, (parent - extent) * 0.5, parent - extent]
        .into_iter()
        .find(|candidate| (*candidate - position).abs() <= 5.0)
        .unwrap_or(position)
}
