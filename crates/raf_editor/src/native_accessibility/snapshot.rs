use std::collections::HashMap;
use raf_core::{Language, i18n::t};
use raf_render::api_graphic_basic::{EditorUiLayer, ui_surface::UiLayoutBox};
use raf_ui::{UiAccessibilityRole, UiEventKind, UiNode, UiRect};
use raf_ui::automation::UiAutomationInbox;

const MAX_ELEMENTS: usize = 4096;
const MAX_TEXT: usize = 8192;

#[derive(Clone)]
pub(super) struct Element {
    pub key: String, pub parent: usize, pub children: Vec<usize>,
    pub name: String, pub role: UiAccessibilityRole, pub bounds: [f64; 4],
    pub enabled: bool, pub focused: bool, pub focusable: bool, pub password: bool,
    pub value: Option<String>, pub range: Option<[f64; 4]>,
    pub checked: Option<bool>, pub expanded: Option<bool>, pub selected: Option<bool>,
    pub invokable: bool, pub target: String, pub inbox: Option<UiAutomationInbox>,
}
impl Element {
    pub fn root(name: String, bounds: [f64; 4]) -> Self {
        Self { key: "rafui.window".into(), parent: 0, children: Vec::new(), name,
            role: UiAccessibilityRole::Generic, bounds, enabled: true, focused: false, focusable: false,
            password: false, value: None, range: None, checked: None, expanded: None, selected: None,
            invokable: false, target: String::new(), inbox: None }
    }
}
pub(super) struct Snapshot { pub elements: Vec<Element>, pub indexes: HashMap<String, usize> }
impl Snapshot {
    pub fn new(root: Element) -> Self {
        Self { indexes: HashMap::from([(root.key.clone(), 0)]), elements: vec![root] }
    }
    fn push(&mut self, element: Element) -> usize {
        let index = self.elements.len();
        self.indexes.insert(element.key.clone(), index);
        self.elements[element.parent].children.push(index);
        self.elements.push(element);
        index
    }
    pub fn add_layer(&mut self, layer: &mut EditorUiLayer<'_>, origin: [f64; 2], lang: Language,
        resolve: &mut impl FnMut(&str) -> String) {
        let Some(frame) = layer.host.accessibility_frame() else { return; };
        let inbox = layer.host.automation_inbox();
        let layouts: HashMap<_, _> = frame.layout_boxes.iter().map(|item| (item.id.as_str(), item)).collect();
        let sx = layer.target_rect.width as f64 / layer.logical_size[0].max(1) as f64;
        let sy = layer.target_rect.height as f64 / layer.logical_size[1].max(1) as f64;
        let offset = [origin[0] + layer.target_rect.x as f64, origin[1] + layer.target_rect.y as f64];
        let surface = layer.host.surface();
        let scope = format!("{}:{}", surface.id, surface.root.id);
        let modal = frame.layout_boxes.iter().filter(|layout| layout.accessibility_role == UiAccessibilityRole::Dialog
            && layout.rect.intersection(layout.clip_rect).width > 0.0)
            .max_by_key(|layout| layout.z_index).and_then(|layout| surface.root.find(&layout.id));
        self.add_node(modal.unwrap_or(&surface.root), &scope, 0, &layouts, &inbox, layer.host.session(), offset, [sx, sy], lang, resolve, 0);
    }
    fn add_node(&mut self, node: &UiNode, scope: &str, parent: usize,
        layouts: &HashMap<&str, &UiLayoutBox>, inbox: &UiAutomationInbox,
        session: &raf_render::api_graphic_basic::ui_surface::UiSurfaceSession,
        offset: [f64; 2], scale: [f64; 2], lang: Language,
        resolve: &mut impl FnMut(&str) -> String, depth: usize) {
        if node.hidden || depth > 96 || self.elements.len() >= MAX_ELEMENTS { return; }
        let mut next_parent = parent;
        if let Some(layout) = layouts.get(node.id.as_str()) {
            let visible = layout.rect.intersection(layout.clip_rect);
            if visible.width > 0.0 && visible.height > 0.0 {
                let input = node.control.text_input();
                let password = input.is_some_and(|input| input.password || input.value_key.contains("api_key"));
                let has_click = node.event_handlers.iter().any(|binding| binding.event == UiEventKind::Click);
                let control_key = input.map(|input| input.value_key.as_str())
                    .or_else(|| node.control.toggle().map(|control| control.value_key.as_str()))
                    .or_else(|| node.control.range().map(|control| control.value_key.as_str()))
                    .or_else(|| node.control.select().map(|control| control.value_key.as_str()));
                let name = node.accessibility_label_key.as_deref().map(|key| t(key, lang))
                    .or_else(|| (!password).then(|| node.text_value.clone()).flatten())
                    .or_else(|| node.text_key.as_deref().map(&mut *resolve))
                    .or_else(|| control_key.and_then(|key| { let label = t(key, lang); (label != key).then_some(label) }))
                    .or_else(|| input.and_then(|input| input.placeholder_key.as_deref()).map(|key| t(key, lang)))
                    .or_else(|| node.tooltip_key.as_deref().map(|key| t(key, lang)))
                    .unwrap_or_default().chars().take(MAX_TEXT).collect::<String>();
                let range = node.control.range().map(|r| [r.value as f64, r.min as f64, r.max as f64, r.step as f64]);
                let value = input.filter(|_| !password).map(|input| session.interaction.controls.text(&input.value_key).chars().take(MAX_TEXT).collect());
                // Decorative, unnamed leaves are omitted; container hierarchy is retained.
                if !name.is_empty() || !node.children.is_empty() || input.is_some() || has_click || range.is_some() || node.control.toggle().is_some() {
                    let bounds = screen_bounds(visible, offset, scale);
                    let element = Element { key: format!("{scope}/{}", node.id), parent, children: Vec::new(), name,
                        role: layout.accessibility_role, bounds, enabled: self.elements[parent].enabled && !layout.disabled, focused: layout.focused,
                        focusable: layout.focusable, password, value, range,
                        checked: node.control.toggle().map(|toggle| toggle.value).or(layout.accessibility_checked),
                        expanded: node.control.select().map(|select| select.open).or(layout.accessibility_expanded),
                        selected: layout.accessibility_selected,
                        invokable: has_click || node.control.toggle().is_some() || node.control.select().is_some(),
                        target: node.id.clone(), inbox: Some(inbox.clone()) };
                    next_parent = self.push(element);
                }
            }
        }
        for child in &node.children { self.add_node(child, scope, next_parent, layouts, inbox, session, offset, scale, lang, resolve, depth + 1); }
        if next_parent != parent && self.elements[next_parent].invokable && self.elements[next_parent].name.is_empty() {
            // Reuse visible semantic leaves, including nested labels, without another document walk.
            let mut labels = Vec::new();
            for child in &self.elements[next_parent + 1..] {
                if !child.password && child.children.is_empty() && !child.name.is_empty() && !labels.contains(&child.name.as_str()) {
                    labels.push(child.name.as_str());
                    if labels.len() == 8 { break; }
                }
            }
            self.elements[next_parent].name = labels.join(", ").chars().take(MAX_TEXT).collect();
        }
    }
}
pub(super) fn screen_bounds(rect: UiRect, offset: [f64; 2], scale: [f64; 2]) -> [f64; 4] {
    [offset[0] + rect.x as f64 * scale[0], offset[1] + rect.y as f64 * scale[1], rect.width as f64 * scale[0], rect.height as f64 * scale[1]]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_accessibility_bounds_preserve_fractional_dpi_and_window_origin() {
        assert_eq!(screen_bounds(UiRect::new(8.0, 16.0, 40.0, 24.0), [-1200.0, 60.0], [1.25, 1.25]),
            [-1190.0, 80.0, 50.0, 30.0]);
    }
}
