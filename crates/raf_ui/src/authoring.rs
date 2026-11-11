//! Atomic document edits shared by visual tools and structured authoring adapters.
use crate::game_ui::{UiMount, UiValueBinding};
use crate::{UiControl, UiDocument, UiEventBinding, UiImage, UiLayout, UiNode, UiStyle};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiNodePatch {
    pub text: Option<String>,
    pub text_key: Option<String>,
    pub layout: Option<UiLayout>,
    pub style: Option<UiStyle>,
    pub image: Option<UiImage>,
    pub visible: Option<bool>,
    pub enabled: Option<bool>,
    pub events: Option<Vec<UiEventBinding>>,
    pub text_style: Option<crate::UiTextStyle>,
    pub control: Option<UiControl>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiEdit {
    Insert {
        parent: String,
        id: String,
        kind: UiElementKind,
        #[serde(default)]
        rect: Option<crate::UiRect>,
        #[serde(default)]
        text: String,
    },
    Add {
        parent: String,
        node: UiNode,
    },
    Remove {
        node: String,
    },
    Patch {
        node: String,
        patch: UiNodePatch,
    },
    Reparent {
        node: String,
        parent: String,
    },
    Rename {
        node: String,
        id: String,
    },
    Mount {
        mount: UiMount,
    },
    Bind {
        binding: UiValueBinding,
    },
    Rules {
        rules: Vec<crate::UiStyleRule>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiElementKind {
    Frame,
    Label,
    Image,
    Button,
    Input,
}
impl UiElementKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "frame" => Some(Self::Frame),
            "label" => Some(Self::Label),
            "image" => Some(Self::Image),
            "button" => Some(Self::Button),
            "input" => Some(Self::Input),
            _ => None,
        }
    }
    pub fn node(self, id: &str) -> UiNode {
        let mut node = UiNode::new(
            id,
            match self {
                Self::Frame => crate::UiNodeKind::Panel,
                Self::Label => crate::UiNodeKind::Label,
                Self::Image => crate::UiNodeKind::Image,
                Self::Button => crate::UiNodeKind::Button,
                Self::Input => crate::UiNodeKind::TextInput,
            },
        );
        match self {
            Self::Image => {
                node.control = UiControl::Image(UiImage {
                    source: crate::UiImageSource::new(""),
                    fit: Default::default(),
                    tint: None,
                    options: Default::default(),
                })
            }
            Self::Input => {
                node.control = UiControl::TextInput(crate::UiTextInput::new(id));
                node = node.focusable();
            }
            Self::Button => node = node.focusable(),
            _ => {}
        }
        node.style.text = [240, 240, 240, 255];
        node
    }
}

pub fn apply_ui_edits(document: &mut UiDocument, edits: &[UiEdit]) -> Result<bool, String> {
    if edits.len() > 128 {
        return Err("ui.edit_budget".into());
    }
    let mut candidate = document.clone();
    for edit in edits {
        apply_one(&mut candidate, edit)?;
    }
    candidate.validate().map_err(|e| e.join("; "))?;
    if candidate == *document {
        return Ok(false);
    }
    *document = candidate;
    Ok(true)
}
fn apply_one(document: &mut UiDocument, edit: &UiEdit) -> Result<(), String> {
    match edit {
        UiEdit::Insert {
            parent,
            id,
            kind,
            rect,
            text,
        } => {
            let mut node = kind.node(id);
            node.text_value = Some(text.clone());
            node.layout = rect
                .map(crate::UiLayout::absolute)
                .unwrap_or_else(|| crate::UiLayout::fixed(180.0, 44.0));
            document.add_node(parent, node)?;
        }
        UiEdit::Add { parent, node } => document.add_node(parent, node.clone())?,
        UiEdit::Remove { node } => {
            if document.remove_node(node).is_none() {
                return Err("ui.element_missing".into());
            }
            let mut ids = std::collections::BTreeSet::new();
            crate::game_ui::visit_ui_nodes(&document.root, &mut |n| {
                ids.insert(n.id.clone());
            });
            document.bindings.retain(|b| ids.contains(&b.node));
        }
        UiEdit::Patch { node, patch } => {
            let node = document.find_node_mut(node).ok_or("ui.element_missing")?;
            if let Some(text) = &patch.text {
                node.text_value = Some(text.clone());
                node.text_key = None;
            }
            if let Some(key) = &patch.text_key {
                node.text_key = Some(key.clone());
                node.text_value = None;
            }
            if let Some(layout) = &patch.layout {
                node.layout = layout.clone();
            }
            if let Some(style) = &patch.style {
                node.style = style.clone();
            }
            if let Some(image) = &patch.image {
                node.control = UiControl::Image(image.clone());
                node.kind = crate::UiNodeKind::Image;
            }
            if let Some(visible) = patch.visible {
                node.hidden = !visible;
            }
            if let Some(enabled) = patch.enabled {
                node.disabled = !enabled;
            }
            if let Some(events) = &patch.events {
                node.event_handlers = events.clone();
            }
            if let Some(style) = patch.text_style {
                node.text_style = Some(style);
            }
            if let Some(control) = &patch.control {
                node.control = control.clone();
            }
        }
        UiEdit::Reparent { node, parent } => {
            if node == parent
                || document
                    .find_node(node)
                    .ok_or("ui.element_missing")?
                    .find(parent)
                    .is_some()
            {
                return Err("ui.hierarchy_cycle".into());
            }
            let item = document.remove_node(node).ok_or("ui.element_missing")?;
            document.add_node(parent, item)?;
        }
        UiEdit::Rename { node, id } => {
            if id.is_empty() || id.len() > 128 || document.find_node(id).is_some() {
                return Err("ui.duplicate_element".into());
            }
            document.find_node_mut(node).ok_or("ui.element_missing")?.id = id.clone();
            for binding in &mut document.bindings {
                if &binding.node == node {
                    binding.node = id.clone();
                }
            }
            for rule in &mut document.style_sheet.rules {
                if let crate::UiStyleSelector::Id(old) = &mut rule.selector {
                    if old == node {
                        *old = id.clone();
                    }
                }
            }
        }
        UiEdit::Mount { mount } => {
            mount.validate()?;
            if let Some(old) = document.mounts.iter_mut().find(|old| old.id == mount.id) {
                *old = mount.clone();
            } else {
                document.mounts.push(mount.clone());
            }
        }
        UiEdit::Bind { binding } => {
            document
                .bindings
                .retain(|old| old.node != binding.node || old.property != binding.property);
            document.bindings.push(binding.clone());
        }
        UiEdit::Rules { rules } => document.style_sheet.rules.extend(rules.iter().cloned()),
    }
    Ok(())
}

/// Copy a component with local IDs and stateful styles scoped to the copied nodes.
pub fn namespace_subtree(
    node: &mut UiNode,
    prefix: &str,
    styles: &crate::UiStyleSheet,
) -> Vec<crate::UiStyleRule> {
    let mut rules = Vec::new();
    fn visit(
        node: &mut UiNode,
        prefix: &str,
        styles: &crate::UiStyleSheet,
        rules: &mut Vec<crate::UiStyleRule>,
    ) {
        let id = format!("{prefix}{}", node.id);
        for rule in &styles.rules {
            if rule.selector.matches(node) {
                let mut rule = rule.clone();
                rule.selector = crate::UiStyleSelector::Id(id.clone());
                rules.push(rule);
            }
        }
        node.id = id;
        node.classes.clear();
        match &mut node.control {
            UiControl::TextInput(input) => input.value_key = format!("{prefix}{}", input.value_key),
            UiControl::Select(select) => {
                select.value_key = format!("{prefix}{}", select.value_key);
                select.popup_id = select.popup_id.as_ref().map(|id| format!("{prefix}{id}"));
            }
            UiControl::Toggle(toggle) => toggle.value_key = format!("{prefix}{}", toggle.value_key),
            UiControl::Range(range) => range.value_key = format!("{prefix}{}", range.value_key),
            _ => {}
        }
        for child in &mut node.children {
            visit(child, prefix, styles, rules);
        }
    }
    visit(node, prefix, styles, &mut rules);
    rules
}
