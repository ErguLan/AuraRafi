//! Native document authoring state. The surface only projects this controller.
use super::interface_workspace::{color_hex, parse_color, InterfaceDrag, InterfaceWorkspace};
use raf_core::{i18n::t, Language};
use raf_ui::authoring::{apply_ui_edits, UiEdit, UiNodePatch};
use raf_ui::game_ui::{UiAnchoredRect, UiPresentationKind};
use raf_ui::*;
use std::path::{Path, PathBuf};
#[path = "interface_authoring_host.rs"]
mod authoring;

pub struct InterfaceSurfaceHost {
    pub document: UiDocument,
    pub selected: String,
    pub preview_size: [u32; 2],
    pub interact: bool,
    pub dirty: bool,
    pub revision: u64,
    pub diagnostic: Option<String>,
    pub images_dirty: bool,
    pub project_root: Option<PathBuf>,
    pub workspace: InterfaceWorkspace,
    pub editing_asset: Option<PathBuf>,
    session_scope: Option<(PathBuf, PathBuf)>,
    path: Option<PathBuf>,
    undo: Vec<UiDocument>,
    redo: Vec<UiDocument>,
    drag: Option<InterfaceDrag>,
    seed_needed: bool,
    text_drafts: UiTextDraftBank,
    cancel_field: Option<String>,
    last_poll: std::time::Instant,
    modified: Option<std::time::SystemTime>,
    image_keys: std::collections::BTreeSet<String>,
    force_images: bool,
}
impl Default for InterfaceSurfaceHost {
    fn default() -> Self {
        Self {
            document: UiDocument::blank("Interface"),
            selected: "root".into(),
            preview_size: [1280, 720],
            interact: false,
            dirty: false,
            revision: 1,
            diagnostic: None,
            images_dirty: true,
            project_root: None,
            workspace: Default::default(),
            editing_asset: None,
            session_scope: None,
            path: None,
            undo: Vec::new(),
            redo: Vec::new(),
            drag: None,
            seed_needed: true,
            text_drafts: UiTextDraftBank::default(),
            cancel_field: None,
            last_poll: std::time::Instant::now(),
            modified: None,
            image_keys: Default::default(),
            force_images: true,
        }
    }
}
impl InterfaceSurfaceHost {
    pub fn sync(&mut self, root: &Path, relative: &Path) -> bool {
        let scope = (root.to_owned(), relative.to_owned());
        let scope_changed = self.session_scope.as_ref() != Some(&scope);
        let source = if scope_changed {
            relative
        } else {
            self.editing_asset.as_deref().unwrap_or(relative)
        };
        let Ok(path) = crate::ui_document_store::document_path(root, source) else {
            return false;
        };
        let switched = self.path.as_ref() != Some(&path);
        if !switched && self.last_poll.elapsed().as_millis() < 500 {
            return false;
        }
        self.last_poll = std::time::Instant::now();
        let modified = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok());
        if !switched && modified == self.modified {
            return false;
        }
        if self.dirty {
            if switched {
                if let Err(error) = self.save() {
                    self.diagnostic = Some(error);
                    return false;
                }
            } else {
                let changed = self.diagnostic.as_deref() != Some("ui.document_conflict");
                self.diagnostic = Some("ui.document_conflict".into());
                return changed;
            }
        }
        self.project_root = Some(root.into());
        if scope_changed {
            self.editing_asset = None;
            self.session_scope = Some(scope);
        }
        if switched {
            self.drag = None;
            self.workspace.selection_box = None;
            self.workspace.parent_box = None;
            self.force_images = true;
        }
        self.path = Some(path.clone());
        self.modified = modified;
        match if path.exists() {
            crate::ui_document_store::load(&path)
        } else {
            Ok(UiDocument::blank("Interface"))
        } {
            Ok(document) => {
                self.document = document;
                self.selected = self.document.root.id.clone();
                self.undo.clear();
                self.redo.clear();
                self.dirty = false;
                self.diagnostic = None;
            }
            Err(error) => {
                self.document = UiDocument::blank("Interface");
                self.selected = "root".into();
                self.undo.clear();
                self.redo.clear();
                self.dirty = false;
                self.diagnostic = Some(error);
            }
        }
        self.note_changed();
        true
    }
    pub fn save(&mut self) -> Result<(), String> {
        self.finish_gesture();
        let path = self.path.as_ref().ok_or("ui.project_required")?;
        if std::fs::metadata(path).ok().and_then(|m| m.modified().ok()) != self.modified {
            return Err("ui.document_conflict".into());
        }
        crate::ui_document_store::save(path, &self.document)?;
        self.modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        self.dirty = false;
        Ok(())
    }
    pub fn apply(&mut self, edits: &[UiEdit]) -> Result<bool, String> {
        let before = self.document.clone();
        if !apply_ui_edits(&mut self.document, edits)? {
            return Ok(false);
        }
        self.record(before);
        Ok(true)
    }
    fn record(&mut self, before: UiDocument) {
        self.undo.push(before);
        if self.undo.len() > 16 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.note_changed();
    }
    fn note_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
        self.seed_needed = true;
        self.images_dirty = true;
    }
    pub fn node(&self) -> Option<&UiNode> {
        self.document.find_node(&self.selected)
    }
    pub fn take_image_changes(&mut self) -> bool {
        if !self.images_dirty {
            return false;
        }
        self.images_dirty = false;
        let mut keys = std::collections::BTreeSet::new();
        game_ui::visit_ui_nodes(&self.document.root, &mut |n| {
            if let UiControl::Image(image) = &n.control {
                keys.insert(image.source.key.clone());
            }
        });
        if keys == self.image_keys && !self.force_images {
            return false;
        }
        self.image_keys = keys;
        self.force_images = false;
        true
    }
    pub fn seed(&mut self, controls: &mut UiControlState) {
        if !self.seed_needed {
            return;
        }
        let Some(node) = self.node() else {
            return;
        };
        let rect = node.layout.rect.unwrap_or(UiRect::new(
            0.0,
            0.0,
            node.layout.basis[0],
            node.layout.basis[1],
        ));
        let anchor = node.layout.anchored;
        let mount = self.document.mounts.first().cloned().unwrap_or_default();
        let bindings = [
            ("document_name", self.document.name.clone()),
            ("id", node.id.clone()),
            (
                "parent",
                find_parent(&self.document.root, &self.selected).unwrap_or_default(),
            ),
            (
                "text",
                node.text_value
                    .clone()
                    .or(node.text_key.clone())
                    .unwrap_or_default(),
            ),
            ("x", anchor.map_or(rect.x, |a| a.offset[0]).to_string()),
            ("y", anchor.map_or(rect.y, |a| a.offset[1]).to_string()),
            ("width", rect.width.to_string()),
            ("height", rect.height.to_string()),
            ("anchor_x", anchor.map_or(0.0, |p| p.anchor[0]).to_string()),
            ("anchor_y", anchor.map_or(0.0, |p| p.anchor[1]).to_string()),
            (
                "relative_width",
                anchor.map_or(0.0, |p| p.relative_size[0]).to_string(),
            ),
            (
                "relative_height",
                anchor.map_or(0.0, |p| p.relative_size[1]).to_string(),
            ),
            (
                "font_size",
                node.text_style.map_or(13.0, |s| s.size_px).to_string(),
            ),
            (
                "image",
                match &node.control {
                    UiControl::Image(i) => i.source.key.clone(),
                    _ => String::new(),
                },
            ),
            (
                "event",
                match &node.control {
                    UiControl::TextInput(input) => input.submit_command.clone(),
                    _ => None,
                }
                .or_else(|| {
                    node.event_handlers.first().and_then(|e| {
                        if let UiAction::Command { name } = &e.action {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default(),
            ),
            ("fill", color_hex(node.style.fill)),
            ("text_color", color_hex(node.style.text)),
            ("target", mount.target),
            ("offset", mount.offset.map(|v| v.to_string()).join(",")),
            (
                "rotation",
                mount.rotation_degrees.map(|v| v.to_string()).join(","),
            ),
            ("meters", mount.size_meters.map(|v| v.to_string()).join(",")),
            ("canvas", mount.canvas_size.map(|v| v.to_string()).join(",")),
        ].into_iter().map(|(key, value)| UiTextBinding::new(format!("ui.designer.field.{key}"), value, 4096));
        let scope = format!("{:?}:{}:{}", self.path, self.document.id.0, self.selected);
        self.text_drafts.sync(scope, bindings, controls);
        if let Some(key) = self.cancel_field.take() { self.text_drafts.cancel(&key, controls); }
        self.seed_needed = false;
    }
    pub fn add(&mut self, kind: &str, language: Language) -> Result<(), String> {
        self.workspace.insert_open = false;
        let mut id = kind.to_owned();
        let mut suffix = 2;
        while self.document.find_node(&id).is_some() {
            id = format!("{kind}-{suffix}");
            suffix += 1;
        }
        let node_kind = match kind {
            "label" => UiNodeKind::Label,
            "button" => UiNodeKind::Button,
            "image" => UiNodeKind::Image,
            "frame" => UiNodeKind::Panel,
            "input" => UiNodeKind::TextInput,
            _ => return Err("ui.invalid_element".into()),
        };
        let size = if kind == "frame" {
            [280.0, 180.0]
        } else if kind == "image" {
            [160.0; 2]
        } else {
            [180.0, 44.0]
        };
        let mut node = UiNode::new(&id, node_kind).with_layout(UiLayout::absolute(UiRect::new(
            24.0, 24.0, size[0], size[1],
        )));
        if kind == "image" {
            node.control = UiControl::Image(UiImage {
                source: UiImageSource::new(""),
                fit: UiImageFit::Contain,
                tint: None,
                options: Default::default(),
            });
        } else if kind == "input" {
            node.control = UiControl::TextInput(UiTextInput::new(id.clone()));
            node = node.focusable();
        } else if kind != "frame" {
            node.text_value = Some(t(&format!("ui.element.{kind}"), language));
        }
        if kind == "button" {
            node = node.focusable();
        }
        node.style = UiStyle {
            text: [32, 32, 32, 255],
            fill: if kind == "frame" || kind == "button" {
                [255; 4]
            } else if kind == "input" {
                [245, 245, 245, 255]
            } else {
                [0; 4]
            },
            radius: 4.0,
            ..UiStyle::transparent()
        };
        let parent = self
            .node()
            .filter(|n| matches!(n.kind, UiNodeKind::Root | UiNodeKind::Panel))
            .map(|n| n.id.clone())
            .unwrap_or_else(|| self.document.root.id.clone());
        self.apply(&[UiEdit::Add { parent, node }])?;
        self.selected = id;
        self.open_live();
        Ok(())
    }
    pub fn use_image(&mut self, asset: &str, language: Language) -> Result<(), String> {
        let image = UiImage {
            source: UiImageSource::new(format!("assets://{}", asset.trim_start_matches("assets/"))),
            fit: UiImageFit::Contain,
            tint: None,
            options: Default::default(),
        };
        if !self
            .node()
            .is_some_and(|node| matches!(node.control, UiControl::Image(_)))
        {
            self.add("image", language)?;
        }
        self.apply(&[UiEdit::Patch {
            node: self.selected.clone(),
            patch: UiNodePatch {
                image: Some(image),
                ..Default::default()
            },
        }])?;
        Ok(())
    }
    pub fn insert_asset(&mut self, asset: &str, language: Language) -> Result<(), String> {
        if let Some(kind) = asset.strip_prefix("builtin://interface/") {
            return self.add(kind, language);
        }
        let root = self.project_root.as_ref().ok_or("ui.project_required")?;
        let relative = format!("assets/{}", asset.trim_start_matches("assets/"));
        let path = crate::ui_document_store::document_path(root, Path::new(&relative))?;
        let document = crate::ui_document_store::load(&path)?;
        let prefix = format!("component-{}.", uuid::Uuid::new_v4().simple());
        let mut component = document.root.clone();
        let rules =
            raf_ui::authoring::namespace_subtree(&mut component, &prefix, &document.style_sheet);
        component.kind = UiNodeKind::Panel;
        if component.layout.rect.is_none() && component.layout.anchored.is_none() {
            component.layout.position_mode = UiPositionMode::Absolute;
            component.layout.anchored = Some(UiAnchoredRect {
                anchor: [0.0; 2],
                pivot: [0.0; 2],
                relative_size: [1.0; 2],
                offset: [0.0; 2],
                size: [0.0; 2],
            });
        }
        let parent = self.document.root.id.clone();
        let mut edits = vec![
            UiEdit::Add {
                parent,
                node: component,
            },
            UiEdit::Rules { rules },
        ];
        for mut binding in document.bindings {
            binding.node = format!("{prefix}{}", binding.node);
            edits.push(UiEdit::Bind { binding });
        }
        self.apply(&edits)?;
        self.selected = format!("{prefix}{}", document.root.id);
        Ok(())
    }
    pub fn handle(
        &mut self,
        action: &UiAction,
        controls: &UiControlState,
        pointer: Option<[f32; 2]>,
        target: Option<(String, f32)>,
        language: Language,
    ) -> bool {
        if matches!(action,UiAction::OpenMenu{id} if id=="ui.designer.insert.menu") {
            return self.handle(
                &UiAction::Command {
                    name: "ui.designer.insert.toggle".into(),
                },
                controls,
                pointer,
                target,
                language,
            );
        }
        let UiAction::Command { name } = action else {
            return false;
        };
        let Some(command) = name.strip_prefix("ui.designer.") else {
            return false;
        };
        if self.handle_authoring_command(command, pointer, controls) {
            return true;
        }
        let result: Result<(), String> = (|| {
            if let Some(kind) = command.strip_prefix("add:") {
                return self.add(kind, language);
            }
            if let Some(asset) = command.strip_prefix("asset-image:") {
                return self.use_image(asset, language);
            }
            if let Some(id) = command.strip_prefix("select:") {
                self.open_live();
                self.selected = id.into();
                self.note_changed();
                return Ok(());
            }
            if let Some(mode) = command.strip_prefix("mount:") {
                let mut mount = self.document.mounts.first().cloned().unwrap_or_default();
                mount.kind = match mode {
                    "screen" => UiPresentationKind::Screen,
                    "camera" => UiPresentationKind::CameraOverlay,
                    "plane" => UiPresentationKind::CameraPlane,
                    "surface" => UiPresentationKind::Surface,
                    "billboard" => UiPresentationKind::Billboard,
                    _ => return Err("ui.invalid_presentation".into()),
                };
                if let Some((target, front_offset)) = target {
                    mount.target = target;
                    if mount.kind == UiPresentationKind::Surface && mount.offset == [0.0; 3] {
                        mount.offset[2] = front_offset;
                    }
                }
                if mount.kind == UiPresentationKind::CameraPlane && mount.offset == [0.0; 3] {
                    mount.offset = [0.0, 0.0, -1.0];
                }
                self.apply(&[UiEdit::Mount { mount }])?;
                self.workspace.view = if matches!(mode, "plane" | "surface" | "billboard") {
                    super::interface_workspace::InterfaceView::Canvas
                } else {
                    super::interface_workspace::InterfaceView::Live
                };
                return Ok(());
            }
            if let Some(field) = command.strip_prefix("commit:") {
                return self.commit(field, controls);
            }
            if let Some(field) = command.strip_prefix("cancel-field:") {
                self.cancel_field = Some(format!("ui.designer.field.{field}"));
                self.seed_needed = true;
                return Ok(());
            }
            if let Some(flow) = command.strip_prefix("flow:") {
                let mut layout = self.node().ok_or("ui.element_missing")?.layout.clone();
                layout.flow = match flow {
                    "row" => UiFlow::Row,
                    "column" => UiFlow::Column,
                    "grid" => UiFlow::Grid,
                    _ => UiFlow::None,
                };
                if flow == "grid" {
                    layout.grid.columns = 2;
                }
                let mut edits = vec![UiEdit::Patch {
                    node: self.selected.clone(),
                    patch: UiNodePatch {
                        layout: Some(layout),
                        ..Default::default()
                    },
                }];
                if flow != "none" {
                    for child in &self.node().unwrap().children {
                        let mut layout = child.layout.clone();
                        layout.rect = None;
                        layout.anchored = None;
                        layout.position_mode = UiPositionMode::Flow;
                        layout.width_mode = UiSizeMode::Fixed;
                        layout.height_mode = UiSizeMode::Fixed;
                        layout.basis = [180.0, 44.0];
                        edits.push(UiEdit::Patch {
                            node: child.id.clone(),
                            patch: UiNodePatch {
                                layout: Some(layout),
                                ..Default::default()
                            },
                        });
                    }
                }
                self.apply(&edits)?;
                return Ok(());
            }
            match command {
                "place" => {
                    self.place_asset(language)?;
                }
                "save" => self.save()?,
                "export" => {
                    let root = self.project_root.as_ref().ok_or("ui.project_required")?;
                    let relative = format!("assets/ui/{}.ui.ron", self.document.id.0);
                    let path = crate::ui_document_store::document_path(root, Path::new(&relative))?;
                    if self.path.as_ref() == Some(&path) {
                        self.save()?;
                    } else {
                        crate::ui_document_store::save(&path, &self.document)?;
                    }
                }
                "undo" => {
                    if let Some(document) = self.undo.pop() {
                        self.redo
                            .push(std::mem::replace(&mut self.document, document));
                        self.dirty = true;
                        self.note_changed();
                    }
                }
                "redo" => {
                    if let Some(document) = self.redo.pop() {
                        self.undo
                            .push(std::mem::replace(&mut self.document, document));
                        self.dirty = true;
                        self.note_changed();
                    }
                }
                "delete" => {
                    self.apply(&[UiEdit::Remove {
                        node: self.selected.clone(),
                    }])?;
                    self.selected = self.document.root.id.clone();
                }
                "visible" => {
                    self.apply(&[UiEdit::Patch {
                        node: self.selected.clone(),
                        patch: UiNodePatch {
                            visible: Some(self.node().ok_or("ui.element_missing")?.hidden),
                            ..Default::default()
                        },
                    }])?;
                }
                "enabled" => {
                    self.apply(&[UiEdit::Patch {
                        node: self.selected.clone(),
                        patch: UiNodePatch {
                            enabled: Some(self.node().ok_or("ui.element_missing")?.disabled),
                            ..Default::default()
                        },
                    }])?;
                }
                "interact" => {
                    self.interact = !self.interact;
                    self.note_changed();
                }
                "wide" => {
                    self.preview_size = [640, 360];
                    self.note_changed();
                }
                "narrow" => {
                    self.preview_size = [360, 640];
                    self.note_changed();
                }
                _ => return Err("ui.unknown_command".into()),
            }
            Ok(())
        })();
        if self.document.find_node(&self.selected).is_none() {
            self.selected = self.document.root.id.clone();
            self.note_changed();
        }
        self.diagnostic = result.err();
        true
    }
    fn commit(&mut self, field: &str, controls: &UiControlState) -> Result<(), String> {
        let value = controls.text(&format!("ui.designer.field.{field}"));
        if field == "document_name" {
            let value = value.trim();
            if value.is_empty() || value.len() > 256 {
                return Err("ui.invalid_value".into());
            }
            let before = self.document.clone();
            self.document.name = value.into();
            if before != self.document {
                self.record(before);
            }
            return Ok(());
        }
        if field == "id" {
            let old = self.selected.clone();
            self.apply(&[UiEdit::Rename {
                node: old,
                id: value.into(),
            }])?;
            self.selected = value.into();
            return Ok(());
        }
        if field == "parent" {
            self.apply(&[UiEdit::Reparent {
                node: self.selected.clone(),
                parent: value.into(),
            }])?;
            return Ok(());
        }
        if field == "target" {
            let mut mount = self.document.mounts.first().cloned().unwrap_or_default();
            mount.target = value.into();
            self.apply(&[UiEdit::Mount { mount }])?;
            return Ok(());
        }
        if matches!(field, "offset" | "rotation" | "meters" | "canvas") {
            let values = value
                .split(',')
                .map(|s| s.trim().parse::<f32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "ui.invalid_value")?;
            let mut mount = self.document.mounts.first().cloned().unwrap_or_default();
            if values.len()
                != if matches!(field, "meters" | "canvas") {
                    2
                } else {
                    3
                }
                || values.iter().any(|v| !v.is_finite())
            {
                return Err("ui.invalid_value".into());
            }
            match field {
                "offset" => mount.offset.copy_from_slice(&values),
                "rotation" => mount.rotation_degrees.copy_from_slice(&values),
                "meters" => mount.size_meters.copy_from_slice(&values),
                _ => {
                    if values.iter().any(|v| *v < 1.0 || *v > 2048.0) {
                        return Err("ui.invalid_value".into());
                    }
                    mount.canvas_size = [values[0] as u32, values[1] as u32];
                }
            }
            self.apply(&[UiEdit::Mount { mount }])?;
            return Ok(());
        }
        let node = self.node().ok_or("ui.element_missing")?;
        let mut patch = UiNodePatch::default();
        match field {
            "text" => patch.text = Some(value.into()),
            "image" => {
                patch.image = Some(UiImage {
                    source: UiImageSource::new(value),
                    fit: UiImageFit::Contain,
                    tint: None,
                    options: Default::default(),
                })
            }
            "event" if matches!(node.control, UiControl::TextInput(_)) => {
                let UiControl::TextInput(mut input) = node.control.clone() else {
                    unreachable!()
                };
                input.submit_command = (!value.is_empty()).then(|| value.to_string());
                patch.control = Some(UiControl::TextInput(input));
            }
            "event" => {
                patch.events = Some(if value.is_empty() {
                    Vec::new()
                } else {
                    vec![UiEventBinding::command(UiEventKind::Click, value)]
                })
            }
            "fill" | "text_color" => {
                let mut style = node.style.clone();
                if field == "fill" {
                    style.fill = parse_color(value)?;
                } else {
                    style.text = parse_color(value)?;
                }
                patch.style = Some(style);
                if field == "text_color" {
                    if let Some(mut text_style) = node.text_style {
                        text_style.color = parse_color(value)?;
                        patch.text_style = Some(text_style);
                    }
                }
            }
            "font_size" => {
                let number = value
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or("ui.invalid_value")?;
                let mut style = node
                    .text_style
                    .unwrap_or(UiTextStyle::body(node.style.text));
                style.size_px = number.clamp(6.0, 160.0);
                style.line_height_px = style.size_px * 1.35;
                patch.text_style = Some(style);
            }
            _ => {
                let number = value
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or("ui.invalid_value")?;
                let mut layout = node.layout.clone();
                let mut rect = layout.rect.unwrap_or(UiRect::new(0.0, 0.0, 180.0, 44.0));
                match field {
                    "x" => {
                        rect.x = number;
                        if let Some(a) = &mut layout.anchored {
                            a.offset[0] = number;
                        }
                    }
                    "y" => {
                        rect.y = number;
                        if let Some(a) = &mut layout.anchored {
                            a.offset[1] = number;
                        }
                    }
                    "width" => {
                        rect.width = number.max(1.0);
                        if let Some(a) = &mut layout.anchored {
                            a.size[0] = number.max(1.0);
                        }
                    }
                    "height" => {
                        rect.height = number.max(1.0);
                        if let Some(a) = &mut layout.anchored {
                            a.size[1] = number.max(1.0);
                        }
                    }
                    "anchor_x" | "anchor_y" | "relative_width" | "relative_height" => {
                        let mut anchor = layout.anchored.unwrap_or(UiAnchoredRect {
                            anchor: [0.0; 2],
                            pivot: [0.0; 2],
                            relative_size: [0.0; 2],
                            offset: [rect.x, rect.y],
                            size: [rect.width, rect.height],
                        });
                        if field == "anchor_x" {
                            anchor.anchor[0] = number;
                            anchor.pivot[0] = number;
                        } else if field == "anchor_y" {
                            anchor.anchor[1] = number;
                            anchor.pivot[1] = number;
                        } else {
                            let axis = usize::from(field == "relative_height");
                            anchor.relative_size[axis] = number.max(0.0);
                            anchor.size[axis] = 0.0;
                        }
                        layout.anchored = Some(anchor);
                    }
                    _ => return Err("ui.invalid_field".into()),
                }
                layout.rect = Some(rect);
                patch.layout = Some(layout);
            }
        }
        self.apply(&[UiEdit::Patch {
            node: self.selected.clone(),
            patch,
        }])?;
        Ok(())
    }
}
fn find_parent(node: &UiNode, target: &str) -> Option<String> {
    if node.children.iter().any(|n| n.id == target) {
        return Some(node.id.clone());
    }
    node.children
        .iter()
        .find_map(|child| find_parent(child, target))
}
