use super::*;

impl NativeGameWorkbench {
    pub(super) fn handle_hierarchy_structure_command(
        &mut self,
        name: &String,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(raw_id) = name.strip_prefix("hierarchy.insertion-parent:") {
            if raw_id == "clear" {
                self.hierarchy_model.insertion_parent = None;
                self.hierarchy_model.invalidate();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            if let Ok(raw) = raw_id.parse::<usize>() {
                self.hierarchy_model.insertion_parent = scene.get(SceneNodeId(raw)).map(|n| n.uuid);
                self.hierarchy_model.invalidate();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if let Some(rest) = name.strip_prefix("hierarchy.inspect:") {
            if let Some((raw, section)) = rest.split_once(':') {
                if let Ok(id) = raw.parse::<usize>() {
                    self.hierarchy_active_tab = "hierarchy".into();
                    self.inspector_view.tab = InspectorTab::Properties;
                    self.inspector_view.components = true;
                    if section == "components" {
                        self.inspector_view.appearance = true;
                        self.inspector_view.audio = true;
                        self.inspector_view.physics = true;
                        self.inspector_view.metadata = true;
                    }
                    if self
                        .last_layout
                        .is_some_and(|layout| layout.right_panel.is_none())
                    {
                        intents.push(NativeWorkbenchIntent::Command(
                            crate::application_menu::command::VIEW_INSPECTOR.to_string(),
                        ));
                    }
                    if let Some(section) = inspector_section_from_slug(section) {
                        set_inspector_section(&mut self.inspector_view, section, true);
                    }
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "hierarchy.select:{id}:replace"
                    )));
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
            }
            return true;
        }
        if let Some(raw) = name.strip_prefix("hierarchy.snap-selected:") {
            if let Some(source) = raw
                .parse::<usize>()
                .ok()
                .and_then(|raw| scene.get(SceneNodeId(raw)))
            {
                if let Some(surface) = selected
                    .iter()
                    .filter_map(|id| scene.get(*id))
                    .find(|node| node.uuid != source.uuid)
                {
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "console.submit:/game.snap target={} mode=surface snap_to={}",
                        source.uuid, surface.uuid
                    )));
                }
            }
            return true;
        }
        if let Some(raw) = name.strip_prefix("hierarchy.snap-floor:") {
            if let Ok(id) = raw.parse::<usize>() {
                if let Some(node) = scene.get(SceneNodeId(id)) {
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "console.submit:/game.snap target={} mode=floor",
                        node.uuid
                    )));
                }
            }
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_hierarchy_tab_command(
        &mut self,
        name: &String,
        router: &mut InputRouter,
    ) -> bool {
        if let Some(tab) = name.strip_prefix("hierarchy.tab:") {
            self.interface_scene_view();
            // Only the two real tree modes remain. Removed placeholder tabs
            // fall back to Scene instead of rendering an empty body.
            if matches!(tab, "hierarchy" | "lighting") {
                self.hierarchy_active_tab = tab.to_string();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            } else {
                self.hierarchy_active_tab = "hierarchy".to_string();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if name == "hierarchy.toggle-hidden" {
            self.agent_settings.hierarchy_show_hidden = !self.agent_settings.hierarchy_show_hidden;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name == "hierarchy.search.open" {
            // Search everywhere is a modal, not a hierarchy tab.
            self.hierarchy_active_tab = "hierarchy".to_string();
            self.open_search();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(tab) = name.strip_prefix("hierarchy.open-bottom:") {
            if self.bottom_dock.select(tab) {
                self.reset_input_state(router);
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if let Some(slot) = name
            .strip_prefix("hierarchy.bookmark.save:")
            .and_then(|value| value.parse::<usize>().ok())
        {
            if let Some(bookmark) = self.hierarchy_bookmarks.get_mut(slot) {
                *bookmark = true;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_hierarchy_menu_command(
        &mut self,
        name: &String,
        native_input: &NativeUiInputBridge,
    ) -> bool {
        match name.as_str() {
            "hierarchy.empty-menu" => {
                self.hierarchy_empty_menu_open = true;
                self.hierarchy_primitive_menu_open = false;
                self.hierarchy_menu_target = None;
                self.hierarchy_menu_position = native_input.snapshot().pointer_position;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "hierarchy.empty.create-primitive-menu" => {
                self.hierarchy_empty_menu_open = true;
                self.hierarchy_primitive_menu_open = true;
                self.hierarchy_menu_target = None;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "hierarchy.menu.close" => {
                self.hierarchy_empty_menu_open = false;
                self.hierarchy_primitive_menu_open = false;
                self.hierarchy_menu_target = None;
                self.hierarchy_menu_position = None;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "hierarchy.rename.cancel" => {
                self.hierarchy_renaming = None;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            _ => {}
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_hierarchy_menu_pick_command(
        &mut self,
        name: &String,
        native_input: &NativeUiInputBridge,
        selected: &[SceneNodeId],
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(raw_id) = name.strip_prefix("hierarchy.menu:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                let target = SceneNodeId(id);
                if let Some(intent) = hierarchy_context_selection_intent(target, selected) {
                    intents.push(intent);
                }
                self.hierarchy_menu_target =
                    Some((target, self.hierarchy_target_is_folder(target)));
                self.hierarchy_empty_menu_open = false;
                self.hierarchy_primitive_menu_open = false;
                self.hierarchy_menu_position = native_input.snapshot().pointer_position;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_hierarchy_script_link_command(&mut self, name: &String) -> bool {
        if let Some(link_id) = name.strip_prefix("hierarchy.script.open:") {
            if let Some((owner, index)) = link_id.split_once(':').and_then(|(owner, index)| {
                Some((owner.parse::<usize>().ok()?, index.parse::<usize>().ok()?))
            }) {
                if let Some(link) = self.hierarchy_model.script_link(SceneNodeId(owner), index) {
                    match self.script_relationships.resolve_open_path(&link.path) {
                        Ok(path) => {
                            if !crate::script_support::open_script_in_external_editor(&path) {
                                self.console.log(
                                    crate::console::LogLevel::Warning,
                                    "Unable to open linked script",
                                );
                            }
                        }
                        Err(error) => self.console.log(crate::console::LogLevel::Warning, error),
                    }
                }
            }
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_hierarchy_row_command(
        &mut self,
        name: &String,
        native_input: &NativeUiInputBridge,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(raw_id) = name.strip_prefix("hierarchy.expand:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                self.hierarchy_model.toggle_expanded(SceneNodeId(id));
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        if let Some(raw_id) = name.strip_prefix("hierarchy.expand-recursive:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                self.hierarchy_model
                    .expand_recursive(scene, SceneNodeId(id), true);
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        if let Some(raw_id) = name.strip_prefix("hierarchy.collapse-recursive:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                self.hierarchy_model
                    .expand_recursive(scene, SceneNodeId(id), false);
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        if let Some(raw_id) = name.strip_prefix("hierarchy.rename.begin:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                if let Some(node) = scene.get(SceneNodeId(id)) {
                    self.hierarchy_renaming = Some((SceneNodeId(id), node.name.clone()));
                    // The inline control is rebuilt on the next authored frame;
                    // requesting the control id (not the value key) makes the
                    // text input immediately own keyboard input.
                    self.host
                        .session_mut()
                        .interaction
                        .focus
                        .request_focus(hierarchy_rename_focus_id(SceneNodeId(id)));
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
        }
        if let Some(raw) = name.strip_prefix("hierarchy.navigate:") {
            let shift = native_input.snapshot().modifiers.shift;
            self.navigate_hierarchy(raw, scene, shift, intents);
            return true;
        }
        if name == "hierarchy.box-select.press" {
            if let Some(pointer) = native_input.snapshot().pointer_position {
                let additive = native_input.snapshot().modifiers.shift;
                self.hierarchy_model.press_box_selection(pointer, additive);
            }
            return true;
        }
        if name == "hierarchy.box-select.start" {
            if let Some(pointer) = native_input.snapshot().pointer_position {
                if self.hierarchy_model.start_box_selection(pointer).is_some() {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
            }
            return true;
        }
        if name == "hierarchy.box-select.move" {
            if let Some(pointer) = native_input.snapshot().pointer_position {
                if self.hierarchy_model.update_box_selection(pointer).is_some() {
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
            }
            return true;
        }
        if name == "hierarchy.box-select.end" {
            if let Some(pointer) = native_input.snapshot().pointer_position {
                self.hierarchy_model.update_box_selection(pointer);
            }
            let selection = self.hierarchy_model.take_box_selection();
            let Some(selection) = selection.filter(|selection| selection.active) else {
                return true;
            };
            let rect = selection.rect();
            let rows = self
                .hierarchy_model
                .row_ids()
                .filter(|id| {
                    self.host
                        .layout_rect(&format!("hierarchy.row.{}", id.0))
                        .is_some_and(|row| !row.intersection(rect).is_empty())
                })
                .collect::<Vec<_>>();
            if rows.is_empty() {
                if !selection.additive {
                    intents.push(NativeWorkbenchIntent::Command(
                        "hierarchy.clear-selection".to_string(),
                    ));
                }
            } else {
                for (index, id) in rows.iter().enumerate() {
                    // `shift` toggles an already selected row, so an additive
                    // box keeps existing members instead of deselecting them.
                    if selection.additive && selected.contains(id) {
                        continue;
                    }
                    let mode = if index == 0 && !selection.additive {
                        "replace"
                    } else {
                        "shift"
                    };
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "hierarchy.select:{}:{mode}",
                        id.0
                    )));
                }
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw_id) = name.strip_prefix("hierarchy.select:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                let additive = native_input.snapshot().modifiers.shift;
                if additive {
                    if let Some(anchor) = selected.last().copied() {
                        let ordered = self.hierarchy_model.row_ids().collect::<Vec<_>>();
                        if ordered.contains(&anchor) && ordered.contains(&SceneNodeId(id)) {
                            let ids = ordered
                                .iter()
                                .map(|id| id.0.to_string())
                                .collect::<Vec<_>>()
                                .join(",");
                            intents.push(NativeWorkbenchIntent::Command(format!(
                                "hierarchy.select.range:{}:{}:{ids}",
                                anchor.0, id
                            )));
                            return true;
                        }
                    }
                }
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "hierarchy.select:{id}:{}",
                    if additive { "shift" } else { "replace" }
                )));
                return true;
            }
        }
        if let Some(raw_id) = name.strip_prefix("hierarchy.drag.start:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                let id = SceneNodeId(id);
                let Some(node) = scene.get(id) else {
                    return true;
                };
                let pointer = native_input
                    .snapshot()
                    .pointer_position
                    .unwrap_or([0.0, 0.0]);
                let source = if selected.contains(&id) {
                    selected.to_vec()
                } else {
                    vec![id]
                };
                self.hierarchy_drag = Some(HierarchyDragState {
                    source,
                    before: None,
                    hovered: None,
                    edge: 0,
                    invalid: false,
                    target: None,
                    pointer,
                    label: node.name.clone(),
                });
                if !selected.contains(&id) {
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "hierarchy.select:{}:replace",
                        id.0
                    )));
                }
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        if let Some(raw_id) = name.strip_prefix("hierarchy.drag.move:") {
            if raw_id.parse::<usize>().is_ok() && self.hierarchy_drag.is_some() {
                let pointer = native_input
                    .snapshot()
                    .pointer_position
                    .unwrap_or([0.0, 0.0]);
                let hovered = self.host.session().interaction.focus.hovered.as_deref();
                let hovered_row = hierarchy_drop_target_from_hovered(hovered);
                let (target, before) = hovered_row
                    .map(|id| {
                        let rect = self.host.layout_rect(&format!("hierarchy.row.{}", id.0));
                        let fraction = rect
                            .map(|r| (pointer[1] - r.y) / r.height.max(1.0))
                            .unwrap_or(0.5);
                        let node = scene.get(id);
                        if fraction < 0.25 {
                            (node.and_then(|n| n.parent), Some(id))
                        } else if fraction > 0.75 {
                            let parent = node.and_then(|n| n.parent);
                            let siblings = parent
                                .and_then(|p| scene.get(p))
                                .map(|n| n.children.as_slice())
                                .unwrap_or(scene.roots());
                            let before = siblings
                                .iter()
                                .position(|s| *s == id)
                                .and_then(|i| siblings.get(i + 1))
                                .copied();
                            (parent, before)
                        } else {
                            (Some(id), None)
                        }
                    })
                    .unwrap_or((None, None));
                let invalid = self.hierarchy_drag.as_ref().is_some_and(|drag| {
                    !hierarchy_drop_target_is_valid(scene, &drag.source, target)
                        || before.is_some_and(|id| drag.source.contains(&id))
                });
                if let Some(drag) = self.hierarchy_drag.as_mut() {
                    drag.pointer = pointer;
                    drag.hovered = hovered_row;
                    drag.edge = hovered_row
                        .and_then(|id| self.host.layout_rect(&format!("hierarchy.row.{}", id.0)))
                        .map(|r| {
                            if pointer[1] < r.y + r.height * 0.25 {
                                -1
                            } else if pointer[1] > r.y + r.height * 0.75 {
                                1
                            } else {
                                0
                            }
                        })
                        .unwrap_or(0);
                    drag.target = target;
                    drag.before = before;
                    drag.invalid = invalid;
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                return true;
            }
        }
        if name == "hierarchy.drag.over:root" && self.hierarchy_drag.is_some() {
            if let Some(drag) = self.hierarchy_drag.as_mut() {
                drag.pointer = native_input
                    .snapshot()
                    .pointer_position
                    .unwrap_or(drag.pointer);
                drag.target = None;
                drag.before = None;
                drag.hovered = None;
                drag.invalid = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if name.starts_with("hierarchy.drag.end:") {
            if let Some(drag) = self.hierarchy_drag.take() {
                if !drag.invalid && hierarchy_drop_target_is_valid(scene, &drag.source, drag.target)
                {
                    let sources = drag
                        .source
                        .iter()
                        .map(|id| id.0.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    let target = drag
                        .target
                        .map(|id| id.0.to_string())
                        .unwrap_or_else(|| "root".to_string());
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "hierarchy.reorder:{sources}:{target}:{}",
                        drag.before
                            .map(|id| id.0.to_string())
                            .unwrap_or_else(|| "end".into())
                    )));
                }
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    /// Keyboard tree navigation. Up/down/home/end move the selection over
    /// selectable rows; left/right collapse/expand the current branch first and
    /// only move to the parent/first child when the branch is already resolved.
    fn navigate_hierarchy(
        &mut self,
        raw: &str,
        scene: &SceneGraph,
        shift: bool,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        use crate::panels::hierarchy_model::HierarchyNavigation;

        let Some((direction, raw_id)) = raw.split_once(':') else {
            return;
        };
        let Ok(id) = raw_id.parse::<usize>() else {
            return;
        };
        let id = SceneNodeId(id);
        let target = match direction {
            "up" => self
                .hierarchy_model
                .navigate(Some(id), HierarchyNavigation::Up),
            "down" => self
                .hierarchy_model
                .navigate(Some(id), HierarchyNavigation::Down),
            "home" => self
                .hierarchy_model
                .navigate(Some(id), HierarchyNavigation::Home),
            "end" => self
                .hierarchy_model
                .navigate(Some(id), HierarchyNavigation::End),
            "left" => {
                if self.hierarchy_model.row_has_children(id)
                    && self.hierarchy_model.is_row_expanded(id)
                {
                    self.hierarchy_model.toggle_expanded(id);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    None
                } else {
                    scene
                        .get(id)
                        .and_then(|node| node.parent)
                        .filter(|parent| self.hierarchy_model.row_position(*parent).is_some())
                }
            }
            "right" => {
                if self.hierarchy_model.row_has_children(id)
                    && !self.hierarchy_model.is_row_expanded(id)
                {
                    self.hierarchy_model.toggle_expanded(id);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    None
                } else {
                    scene.get(id).and_then(|node| {
                        node.children
                            .iter()
                            .copied()
                            .find(|child| self.hierarchy_model.row_position(*child).is_some())
                    })
                }
            }
            _ => None,
        };
        let Some(target) = target else {
            return;
        };
        intents.push(NativeWorkbenchIntent::Command(format!(
            "hierarchy.select:{}:{}",
            target.0,
            if shift { "shift" } else { "replace" }
        )));
        // Keep the keyboard origin on the newly selected row so repeated arrow
        // presses continue navigating from the visible selection.
        self.host
            .session_mut()
            .interaction
            .focus
            .request_focus(format!("hierarchy.row.{}.select", target.0));
        self.reveal_hierarchy_row(target);
    }

    /// Scrolls only when the target row leaves the measured tree viewport.
    fn reveal_hierarchy_row(&mut self, id: SceneNodeId) {
        let Some(index) = self.hierarchy_model.row_position(id) else {
            return;
        };
        let row_height = self.agent_settings.hierarchy_row_height.max(1.0);
        let Some(viewport) = self.host.layout_rect("hierarchy.tree") else {
            return;
        };
        let offset = self
            .host
            .session()
            .interaction
            .controls
            .scroll_offset("hierarchy.tree")[1];
        let top = index as f32 * row_height;
        let bottom = top + row_height;
        let visible = viewport.height.max(row_height);
        let delta = if top < offset {
            top - offset
        } else if bottom > offset + visible {
            bottom - (offset + visible)
        } else {
            0.0
        };
        if delta.abs() <= f32::EPSILON {
            return;
        }
        let next = self
            .host
            .session_mut()
            .interaction
            .controls
            .scroll_by("hierarchy.tree", [0.0, delta]);
        self.note_hierarchy_scroll(next[1]);
    }
}

/// Node id of the inline rename input. The value key is intentionally
/// different, so focusing the value key would never reach the control.
pub(super) fn hierarchy_rename_focus_id(id: SceneNodeId) -> String {
    format!("hierarchy.rename.control.{}", id.0)
}

impl NativeGameWorkbench {
    pub(super) fn handle_hierarchy_open_menu(
        &mut self,
        id: &String,
        native_input: &NativeUiInputBridge,
        selected: &[SceneNodeId],
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(raw_id) = id.strip_prefix("hierarchy.menu:") {
            if let Ok(id) = raw_id.parse::<usize>() {
                let target = SceneNodeId(id);
                self.open_menu = None;
                if let Some(intent) = hierarchy_context_selection_intent(target, selected) {
                    intents.push(intent);
                }
                self.hierarchy_menu_target =
                    Some((target, self.hierarchy_target_is_folder(target)));
                self.hierarchy_empty_menu_open = false;
                self.hierarchy_primitive_menu_open = false;
                self.hierarchy_menu_position = native_input.snapshot().pointer_position;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        false
    }
}

pub(super) fn hierarchy_context_selection_intent(
    target: SceneNodeId,
    selected: &[SceneNodeId],
) -> Option<NativeWorkbenchIntent> {
    (!selected.contains(&target))
        .then(|| NativeWorkbenchIntent::Command(format!("hierarchy.select:{}:replace", target.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_focus_targets_the_inline_control_not_the_value_key() {
        assert_eq!(
            hierarchy_rename_focus_id(SceneNodeId(7)),
            "hierarchy.rename.control.7"
        );
        assert_ne!(
            hierarchy_rename_focus_id(SceneNodeId(7)),
            "hierarchy.rename.7"
        );
    }
}
