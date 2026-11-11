//! Serializable user-authored UI documents.
//!
//! A document deliberately starts with an empty root. The editor may provide
//! authoring tools, but it never injects a HUD, menu, or template into a game.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::controls::{UiControl, UiImage, UiImageOptions};
use crate::style::{UiStyle, UiStyleSheet, UiTheme};
use crate::{UiNode, UiNodeKind};

pub const UI_DOCUMENT_VERSION: u32 = 4;
pub const MAX_NESTED_ROUNDED_CLIPS: usize = 8;

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

/// Typed changes that a retained authoring host can apply to document data.
#[derive(Debug, Clone, PartialEq)]
pub enum UiDocumentUpdate {
    TextKey(String),
    TextValue(String),
    Visible(bool),
    Enabled(bool),
    Image(UiImage),
    ImageOptions(UiImageOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiDocumentUpdateError {
    NodeNotFound(String),
    NotImage(String),
    InvalidImageOptions(String),
}

impl std::fmt::Display for UiDocumentUpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodeNotFound(id) => write!(formatter, "UI node '{id}' was not found."),
            Self::NotImage(id) => write!(formatter, "UI node '{id}' does not contain image data."),
            Self::InvalidImageOptions(id) => {
                write!(formatter, "UI node '{id}' has invalid image options.")
            }
        }
    }
}

impl std::error::Error for UiDocumentUpdateError {}

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
    #[serde(default)]
    pub style_sheet: UiStyleSheet,
    pub root: UiNode,
    #[serde(default)]
    pub mounts: Vec<crate::game_ui::UiMount>,
    #[serde(default)]
    pub bindings: Vec<crate::game_ui::UiValueBinding>,
}

impl UiDocument {
    pub fn blank(name: impl Into<String>) -> Self {
        Self {
            version: UI_DOCUMENT_VERSION,
            id: UiDocumentId::new(),
            name: name.into(),
            space: UiDocumentSpace::Screen,
            camera_binding: None,
            theme: UiTheme::game_ui(),
            style_sheet: UiStyleSheet::default(),
            root: UiNode::new("root", UiNodeKind::Root).with_style(UiStyle::transparent()),
            mounts: Vec::new(),
            bindings: Vec::new(),
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

    pub fn update_node(
        &mut self,
        id: &str,
        update: UiDocumentUpdate,
    ) -> Result<(), UiDocumentUpdateError> {
        if self.find_node(id).is_none() {
            return Err(UiDocumentUpdateError::NodeNotFound(id.to_string()));
        }
        match &update {
            UiDocumentUpdate::Image(image) if !image_options_are_valid(&image.options) => {
                return Err(UiDocumentUpdateError::InvalidImageOptions(id.to_string()));
            }
            UiDocumentUpdate::ImageOptions(options) if !image_options_are_valid(options) => {
                return Err(UiDocumentUpdateError::InvalidImageOptions(id.to_string()));
            }
            _ => {}
        }
        let Some(node) = self.find_node_mut(id) else {
            return Err(UiDocumentUpdateError::NodeNotFound(id.to_string()));
        };
        match update {
            UiDocumentUpdate::TextKey(key) => {
                node.text_key = Some(key);
                node.text_value = None;
            }
            UiDocumentUpdate::TextValue(value) => {
                node.text_value = Some(value);
                node.text_key = None;
            }
            UiDocumentUpdate::Visible(visible) => node.hidden = !visible,
            UiDocumentUpdate::Enabled(enabled) => node.disabled = !enabled,
            UiDocumentUpdate::Image(image) => {
                node.kind = UiNodeKind::Image;
                node.control = UiControl::Image(image);
            }
            UiDocumentUpdate::ImageOptions(options) => {
                let UiControl::Image(image) = &mut node.control else {
                    return Err(UiDocumentUpdateError::NotImage(id.to_string()));
                };
                image.options = options;
            }
        }
        Ok(())
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
        validate_theme_metrics(&self.theme, &mut errors);
        validate_style_sheet(&self.style_sheet, &mut errors);
        validate_node(&self.root, &mut ids, &mut errors, 0);
        if ids.len() > crate::game_ui::MAX_UI_NODES { errors.push("ui.node_budget".into()); }
        if self.mounts.len() > crate::game_ui::MAX_UI_INSTANCES || self.bindings.len() > 256 {
            errors.push("ui.instance_budget".into());
        }
        let mut mounts = std::collections::BTreeSet::new();
        for mount in &self.mounts {
            if let Err(error) = mount.validate() { errors.push(error); }
            if !mounts.insert(&mount.id) { errors.push("ui.duplicate_instance".into()); }
        }
        for binding in &self.bindings {
            if !ids.contains(&binding.node) || binding.key.is_empty() || binding.key.len() > 128 {
                errors.push("ui.invalid_value_binding".into());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Applies the lossless migrations known by the current document format.
    pub fn migrate(&mut self) -> Result<(), Vec<String>> {
        let mut migrated = self.clone();
        match migrated.version {
            UI_DOCUMENT_VERSION => {}
            1 => {
                // Version 1 documents omitted optional presentation context.
                // Their serde defaults remain Screen, no camera binding, and
                // the built-in RafUI theme before the v2-to-v3 step.
                migrated.version = 2;
                migrate_v2_to_v3(&mut migrated);
            }
            2 => {
                // Version 3 adds optional presentation data. Missing fields
                // retain hidden=false, UiVisual::default(), and an empty
                // UiStyleSheet; existing v2 theme and node styles are kept.
                migrate_v2_to_v3(&mut migrated);
            }
            3 => { migrated.version = UI_DOCUMENT_VERSION; }
            version => {
                return Err(vec![if version > UI_DOCUMENT_VERSION {
                    format!("Cannot migrate future UI document version {version}.")
                } else {
                    format!("No safe migration is registered for UI document version {version}.")
                }]);
            }
        }
        migrated.validate()?;
        *self = migrated;
        Ok(())
    }

    pub fn remove_node(&mut self, id: &str) -> Option<UiNode> {
        if id == self.root.id {
            return None;
        }
        remove_node(&mut self.root, id)
    }
}

fn migrate_v2_to_v3(document: &mut UiDocument) {
    document.version = UI_DOCUMENT_VERSION;
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
    rounded_clip_depth: usize,
) {
    if let Some(placement) = node.layout.anchored {
        if placement.anchor.iter().chain(&placement.pivot).chain(&placement.relative_size)
            .chain(&placement.offset).chain(&placement.size).any(|v| !v.is_finite())
            || placement.relative_size.iter().chain(&placement.size).any(|v| *v < 0.0) {
            errors.push(format!("Invalid anchored placement: {}", node.id));
        }
    }
    if node.id.trim().is_empty() {
        errors.push("UI node id cannot be empty.".to_string());
    } else if !ids.insert(node.id.clone()) {
        errors.push(format!("Duplicate UI node id '{}'.", node.id));
    }
    if !node.layout.gap.is_finite() || node.layout.gap < 0.0 {
        errors.push(format!("Node '{}' has an invalid layout gap.", node.id));
    }
    let next_rounded_clip_depth = rounded_clip_depth + usize::from(has_rounded_clip(node));
    if next_rounded_clip_depth > MAX_NESTED_ROUNDED_CLIPS
        && rounded_clip_depth <= MAX_NESTED_ROUNDED_CLIPS
    {
        errors.push(format!(
            "Node '{}' exceeds the maximum nested rounded clip depth of {}.",
            node.id, MAX_NESTED_ROUNDED_CLIPS
        ));
    }
    validate_node_style(node, errors);
    validate_node_visual(node, errors);
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
    if let UiControl::Image(image) = &node.control {
        if !image_options_are_valid(&image.options) {
            errors.push(format!(
                "Image node '{}' has invalid image options.",
                node.id
            ));
        }
    }
    for child in &node.children {
        validate_node(child, ids, errors, next_rounded_clip_depth);
    }
}

fn image_options_are_valid(options: &UiImageOptions) -> bool {
    let focal_point_valid = options
        .focal_point
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value));
    let source_rect_valid = options.source_rect.map_or(true, |rect| {
        [rect.x, rect.y, rect.width, rect.height]
            .iter()
            .all(|value| value.is_finite())
            && rect.x >= 0.0
            && rect.y >= 0.0
            && rect.width > 0.0
            && rect.height > 0.0
            && rect.x + rect.width <= 1.0
            && rect.y + rect.height <= 1.0
    });
    let nine_slice_valid = options.nine_slice.map_or(true, |insets| {
        [insets.top, insets.right, insets.bottom, insets.left]
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
    });
    focal_point_valid && source_rect_valid && nine_slice_valid
}

fn has_rounded_clip(node: &UiNode) -> bool {
    // Count declared masks conservatively: a stylesheet may introduce a
    // radius later without changing the node's authored base style.
    node.visual.clip_children
}

fn validate_node_style(node: &UiNode, errors: &mut Vec<String>) {
    let style = &node.style;
    if !style.border_width.is_finite()
        || style.border_width < 0.0
        || !style.radius.is_finite()
        || style.radius < 0.0
        || !style.opacity.is_finite()
        || !(0.0..=1.0).contains(&style.opacity)
    {
        errors.push(format!("Node '{}' has invalid style metrics.", node.id));
    }
}

fn validate_node_visual(node: &UiNode, errors: &mut Vec<String>) {
    let visual = &node.visual;
    let invalid_corners = visual.corners.map_or(false, |corners| {
        corners
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    });
    let invalid_borders = visual.border_widths.map_or(false, |widths| {
        widths
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    });
    let invalid_gradient = visual.gradient.map_or(false, |gradient| {
        gradient.direction.iter().any(|value| !value.is_finite())
    });
    let invalid_shadow = visual.shadow.map_or(false, |shadow| {
        shadow.offset.iter().any(|value| !value.is_finite())
            || !shadow.softness.is_finite()
            || shadow.softness < 0.0
            || !shadow.spread.is_finite()
            || shadow.spread < 0.0
    });
    let invalid_focus_ring = visual.focus_ring.map_or(false, |ring| {
        !ring.width.is_finite() || ring.width < 0.0 || !ring.offset.is_finite()
    });
    let invalid_content = !visual.content.gap.is_finite() || visual.content.gap < 0.0;

    if invalid_corners
        || invalid_borders
        || invalid_gradient
        || invalid_shadow
        || invalid_focus_ring
        || invalid_content
    {
        errors.push(format!("Node '{}' has invalid visual metrics.", node.id));
    }
}

fn validate_style_sheet(style_sheet: &UiStyleSheet, errors: &mut Vec<String>) {
    if style_sheet.rules.len()>4096 {errors.push("ui.style_budget".into());return;}
    for (index, rule) in style_sheet.rules.iter().enumerate() {
        let patch = &rule.patch;
        let invalid = patch
            .border_width
            .map_or(false, |value| !value.is_finite() || value < 0.0)
            || patch
                .radius
                .map_or(false, |value| !value.is_finite() || value < 0.0)
            || patch.opacity.map_or(false, |value| {
                !value.is_finite() || !(0.0..=1.0).contains(&value)
            });
        if invalid {
            errors.push(format!(
                "UI stylesheet rule {index} has invalid style metrics."
            ));
        }
    }
}

fn validate_theme_metrics(theme: &UiTheme, errors: &mut Vec<String>) {
    let metrics = &theme.metrics;
    let component_overrides = [
        metrics.components.button,
        metrics.components.image,
        metrics.components.text_input,
        metrics.components.toggle,
        metrics.components.range,
        metrics.components.color_picker,
        metrics.components.select,
        metrics.components.scroll_view,
        metrics.components.grid,
        metrics.components.skeleton,
    ];
    let invalid_component_metric = component_overrides.iter().any(|overrides| {
        [
            overrides.height,
            overrides.horizontal_padding,
            overrides.icon_gap,
            overrides.corner_radius,
        ]
        .iter()
        .flatten()
        .any(|value| !value.is_finite() || *value < 0.0)
    });
    if !metrics.scale.is_finite()
        || metrics.scale <= 0.0
        || !metrics.control_height.is_finite()
        || metrics.control_height < 0.0
        || !metrics.corner_radius.is_finite()
        || metrics.corner_radius < 0.0
        || !metrics.spacing.is_finite()
        || metrics.spacing < 0.0
        || invalid_component_metric
    {
        errors.push("UI theme has invalid metrics.".to_string());
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

    #[test]
    fn version_two_documents_load_and_migrate_with_default_presentation_data() {
        let mut document = UiDocument::blank("Legacy");
        document.version = 2;
        document.root.text_value = Some("Keep me".to_string());

        let mut serialized = serde_json::to_value(document).unwrap();
        let object = serialized.as_object_mut().unwrap();
        object.remove("style_sheet");
        object.insert("version".to_string(), serde_json::json!(2));
        strip_version_three_node_fields(object.get_mut("root").unwrap());

        let mut loaded: UiDocument = serde_json::from_value(serialized).unwrap();
        assert!(loaded.style_sheet.rules.is_empty());
        assert!(!loaded.root.hidden);
        assert_eq!(loaded.root.visual, crate::visual::UiVisual::default());
        assert_eq!(loaded.root.text_value.as_deref(), Some("Keep me"));

        loaded.migrate().unwrap();
        assert_eq!(loaded.version, UI_DOCUMENT_VERSION);
        assert_eq!(loaded.root.text_value.as_deref(), Some("Keep me"));
    }

    #[test]
    fn version_one_documents_keep_additive_serde_defaults_and_migrate() {
        let mut document = UiDocument::blank("Legacy v1");
        document.version = 1;
        document.root.text_value = Some("Keep me too".to_string());

        let mut serialized = serde_json::to_value(document).unwrap();
        let object = serialized.as_object_mut().unwrap();
        object.remove("space");
        object.remove("camera_binding");
        object.remove("theme");
        object.remove("style_sheet");
        object.insert("version".to_string(), serde_json::json!(1));
        strip_version_three_node_fields(object.get_mut("root").unwrap());

        let mut loaded: UiDocument = serde_json::from_value(serialized).unwrap();
        assert_eq!(loaded.space, UiDocumentSpace::Screen);
        assert!(loaded.camera_binding.is_none());
        assert_eq!(loaded.theme.name, "RafUI");
        assert!(loaded.style_sheet.rules.is_empty());
        assert_eq!(loaded.root.visual, crate::visual::UiVisual::default());
        assert!(!loaded.root.hidden);
        assert_eq!(loaded.root.text_value.as_deref(), Some("Keep me too"));

        loaded.migrate().unwrap();
        assert_eq!(loaded.version, UI_DOCUMENT_VERSION);
        assert_eq!(loaded.root.text_value.as_deref(), Some("Keep me too"));
    }

    fn strip_version_three_node_fields(node: &mut serde_json::Value) {
        let Some(object) = node.as_object_mut() else {
            return;
        };
        object.remove("hidden");
        object.remove("visual");
        if let Some(children) = object
            .get_mut("children")
            .and_then(|value| value.as_array_mut())
        {
            for child in children {
                strip_version_three_node_fields(child);
            }
        }
    }

    #[test]
    fn typed_document_updates_change_only_the_requested_node_data() {
        let mut document = UiDocument::blank("Interface");
        document
            .add_node("root", UiNode::new("label", UiNodeKind::Label))
            .unwrap();
        document
            .update_node("label", UiDocumentUpdate::TextKey("ui.title".to_string()))
            .unwrap();
        document
            .update_node("label", UiDocumentUpdate::TextValue("Title".to_string()))
            .unwrap();
        document
            .update_node("label", UiDocumentUpdate::Visible(false))
            .unwrap();
        document
            .update_node("label", UiDocumentUpdate::Enabled(false))
            .unwrap();
        document
            .update_node(
                "label",
                UiDocumentUpdate::Image(UiImage {
                    source: crate::controls::UiImageSource::new("asset.logo"),
                    fit: Default::default(),
                    tint: None,
                    options: UiImageOptions::default(),
                }),
            )
            .unwrap();

        let node = document.find_node("label").unwrap();
        assert_eq!(node.kind, UiNodeKind::Image);
        assert_eq!(node.text_key, None);
        assert_eq!(node.text_value.as_deref(), Some("Title"));
        assert!(node.hidden);
        assert!(node.disabled);
        assert!(matches!(node.control, UiControl::Image(_)));
    }

    #[test]
    fn image_options_updates_validate_normalized_focal_points() {
        let mut document = UiDocument::blank("Images");
        document
            .add_node(
                "root",
                UiNode::image(
                    "logo",
                    UiImage {
                        source: crate::controls::UiImageSource::new("asset.logo"),
                        fit: Default::default(),
                        tint: None,
                        options: UiImageOptions::default(),
                    },
                ),
            )
            .unwrap();

        let mut options = UiImageOptions::default();
        options.focal_point = [0.25, 0.75];
        document
            .update_node("logo", UiDocumentUpdate::ImageOptions(options))
            .unwrap();
        assert!(matches!(
            &document.find_node("logo").unwrap().control,
            UiControl::Image(image) if image.options.focal_point == [0.25, 0.75]
        ));

        let mut invalid = UiImageOptions::default();
        invalid.focal_point = [f32::NAN, 1.01];
        assert_eq!(
            document.update_node("logo", UiDocumentUpdate::ImageOptions(invalid)),
            Err(UiDocumentUpdateError::InvalidImageOptions(
                "logo".to_string()
            ))
        );
        assert!(document.validate().is_ok());
    }

    #[test]
    fn validation_rejects_non_finite_visual_metrics_and_excess_rounded_clip_depth() {
        let mut document = UiDocument::blank("Clipped");
        document.root.visual.clip_children = true;
        document.root.visual.corners = Some([4.0; 4]);
        let mut parent = &mut document.root;
        for index in 0..MAX_NESTED_ROUNDED_CLIPS {
            parent
                .children
                .push(UiNode::new(format!("clip-{index}"), UiNodeKind::Panel));
            parent = parent.children.last_mut().unwrap();
            parent.visual.clip_children = true;
            parent.visual.corners = Some([4.0; 4]);
        }
        document.root.visual.content.gap = f32::NAN;

        let errors = document.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.contains("rounded clip depth")));
        assert!(errors.iter().any(|error| error.contains("visual metrics")));
    }
}
