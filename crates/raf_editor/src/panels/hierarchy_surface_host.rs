//! Application-side state and intent translation for the Hierarchy surface.
//!
//! The retained document stays presentation-only. This host owns filtering,
//! expansion, selection modifiers, rename state, drag state and the semantic
//! commands returned to the editor application.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiControlState, UiDispatchedAction, UiSurface,
};
use raf_ui::{UiModifiers, UiMotionSpec, UiPointerButton, UiTween};

use super::hierarchy_model::{HierarchyModel, HierarchyView};
use super::hierarchy_surface::{build_hierarchy_surface, context_menu_rect};
use super::raf_ui_surface_bridge::RafUiSurfaceBridge;

const TREE_ID: &str = "hierarchy.tree";
const SEARCH_ID: &str = "hierarchy.search";
const FULL_TABS_MIN_WIDTH: f32 = 444.0;

// Eframe/egui is used here only as the current window-shell adapter. It does
// not create, lay out, paint, or handle Hierarchy controls; RafUI owns those
// responsibilities and ApiGraphicBasic presents the WGPU/CPU surface.

#[derive(Debug, Clone, PartialEq)]
pub enum HierarchyIntent {
    Select(Vec<SceneNodeId>),
    ToggleVisibility(SceneNodeId),
    ToggleLocked(SceneNodeId),
    Rename {
        id: SceneNodeId,
        name: String,
    },
    CreateFolder {
        parent: Option<SceneNodeId>,
    },
    CreateEntity {
        parent: Option<SceneNodeId>,
    },
    Duplicate(SceneNodeId),
    Delete(SceneNodeId),
    Ungroup(SceneNodeId),
    Paste {
        sources: Vec<SceneNodeId>,
        parent: Option<SceneNodeId>,
    },
    Reparent {
        sources: Vec<SceneNodeId>,
        target: Option<SceneNodeId>,
        before: Option<SceneNodeId>,
    },
    Focus(SceneNodeId),
    SaveBookmark(usize),
    RestoreBookmark(usize),
    OpenBottomTab(String),
    OpenSearch(String),
    TogglePanel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DropTarget {
    parent: Option<SceneNodeId>,
    before: Option<SceneNodeId>,
}

#[derive(Debug, Clone, PartialEq)]
struct SurfaceKey {
    palette_dark: bool,
    query: String,
    view_hash: u64,
    selected: Vec<SceneNodeId>,
    renaming: Option<SceneNodeId>,
    menu_target: Option<(SceneNodeId, bool)>,
    menu_label: Option<String>,
    menu_position_bits: Option<(u32, u32)>,
    surface_size_bits: (u32, u32),
    row_height_bits: u32,
    indent_width_bits: u32,
    show_icons: bool,
    show_visibility: bool,
    show_locked: bool,
    compact_tabs: bool,
    can_paste: bool,
    transition_bits: u32,
    drag_label: Option<String>,
    drag_pointer_bits: Option<(u32, u32)>,
    box_selection_bits: Option<(u32, u32, u32, u32)>,
    active_tab: String,
    bookmark_filled: [bool; 3],
}

pub struct HierarchySurfaceHost {
    bridge: RafUiSurfaceBridge,
    model: HierarchyModel,
    search: String,
    show_hidden: bool,
    show_hidden_setting: Option<bool>,
    menu_target: Option<(SceneNodeId, bool)>,
    menu_position: Option<[f32; 2]>,
    renaming: Option<(SceneNodeId, String)>,
    selection_anchor: Option<SceneNodeId>,
    drag_source: Option<SceneNodeId>,
    drag_target: Option<DropTarget>,
    copy_buffer: Vec<SceneNodeId>,
    cached_key: Option<SurfaceKey>,
    cached_surface: Option<UiSurface>,
    focus_rename_next_frame: Option<String>,
    box_select_start: Option<[f32; 2]>,
    box_select_current: Option<[f32; 2]>,
    box_select_additive: bool,
    active_tab: String,
    bookmark_filled: [bool; 3],
    panel_motion: UiTween,
    last_motion_time: f64,
    closing: bool,
}

impl Default for HierarchySurfaceHost {
    fn default() -> Self {
        Self {
            bridge: RafUiSurfaceBridge::new("raf_ui_hierarchy"),
            model: HierarchyModel::default(),
            search: String::new(),
            show_hidden: false,
            show_hidden_setting: None,
            menu_target: None,
            menu_position: None,
            renaming: None,
            selection_anchor: None,
            drag_source: None,
            drag_target: None,
            copy_buffer: Vec::new(),
            cached_key: None,
            cached_surface: None,
            focus_rename_next_frame: None,
            box_select_start: None,
            box_select_current: None,
            box_select_additive: false,
            active_tab: "hierarchy".to_string(),
            bookmark_filled: [false; 3],
            panel_motion: UiTween::new(0.0, UiMotionSpec::layout()),
            last_motion_time: 0.0,
            closing: false,
        }
    }
}

impl HierarchySurfaceHost {
    pub fn sync_show_hidden_setting(&mut self, show_hidden: bool) {
        if self.show_hidden_setting != Some(show_hidden) {
            self.show_hidden_setting = Some(show_hidden);
            self.show_hidden = show_hidden;
            self.invalidate_surface();
        }
    }

    pub fn sync_bookmark_state(&mut self, filled: [bool; 3]) {
        if self.bookmark_filled != filled {
            self.bookmark_filled = filled;
            self.invalidate_surface();
        }
    }

    pub fn reveal_node(&mut self, scene: &SceneGraph, id: SceneNodeId) {
        self.reveal_node_with_options(scene, id, true);
    }

    pub fn reveal_node_with_options(
        &mut self,
        scene: &SceneGraph,
        id: SceneNodeId,
        expand_parent: bool,
    ) {
        if expand_parent {
            self.model.expand_parent_chain(scene, id);
        }
        self.invalidate_surface();
    }

    pub fn reset_for_scene(&mut self) {
        self.model = HierarchyModel::default();
        self.search.clear();
        self.show_hidden_setting = None;
        self.menu_target = None;
        self.menu_position = None;
        self.renaming = None;
        self.selection_anchor = None;
        self.drag_source = None;
        self.drag_target = None;
        self.copy_buffer.clear();
        self.cached_key = None;
        self.cached_surface = None;
        self.focus_rename_next_frame = None;
        self.box_select_start = None;
        self.box_select_current = None;
        self.box_select_additive = false;
        self.active_tab = "hierarchy".to_string();
        self.bookmark_filled = [false; 3];
        self.panel_motion.set_immediate(0.0);
        self.last_motion_time = 0.0;
        self.closing = false;
        self.bridge.reset_surface_interaction(Some(TREE_ID));
    }

    pub fn sync_scene(&mut self, scene: &SceneGraph) {
        self.model.clear_removed_state(scene);
        self.model.invalidate();
        if self
            .menu_target
            .is_some_and(|(id, _)| !scene.is_valid_node(id))
        {
            self.menu_target = None;
            self.menu_position = None;
        }
        if self
            .renaming
            .as_ref()
            .is_some_and(|(id, _)| !scene.is_valid_node(*id))
        {
            self.renaming = None;
        }
        if self.drag_source.is_some_and(|id| !scene.is_valid_node(id)) {
            self.drag_source = None;
            self.drag_target = None;
        }
        if self.drag_target.is_some_and(|target| {
            target.parent.is_some_and(|id| !scene.is_valid_node(id))
                || target.before.is_some_and(|id| !scene.is_valid_node(id))
        }) {
            self.drag_target = None;
        }
        if self
            .selection_anchor
            .is_some_and(|id| !scene.is_valid_node(id))
        {
            self.selection_anchor = None;
        }
        self.copy_buffer.retain(|id| scene.is_valid_node(*id));
        if self.box_select_start.is_some() {
            self.box_select_start = None;
            self.box_select_current = None;
            self.box_select_additive = false;
        }
        self.cached_key = None;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        row_height: f32,
        indent_width: f32,
        show_icons: bool,
        show_visibility: bool,
        show_locked: bool,
        animations_enabled: bool,
    ) -> Vec<HierarchyIntent> {
        let now = ui.ctx().input(|input| input.time);
        let delta = if self.last_motion_time <= 0.0 {
            0.0
        } else {
            (now - self.last_motion_time).clamp(0.0, 0.1) as f32
        };
        self.last_motion_time = now;
        self.panel_motion
            .set_target(if self.closing { 0.0 } else { 1.0 });
        let transition = self.panel_motion.advance(delta, !animations_enabled);
        if animations_enabled && !self.panel_motion.is_settled() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        let compact_tabs = ui.available_width() < FULL_TABS_MIN_WIDTH;
        let surface_size = [
            ui.available_width().max(1.0),
            ui.available_height().max(1.0),
        ];
        let scroll_offset = self
            .bridge
            .with_control_state_read(|controls| controls.scroll_offset(TREE_ID)[1])
            .unwrap_or(0.0);
        let view = self.model.refresh(
            scene,
            &self.search,
            self.show_hidden,
            scroll_offset,
            surface_size[1],
            row_height.max(16.0),
        );
        let box_selection = self.box_selection_rect();
        let drag_label = self.drag_source.and_then(|id| {
            scene.get(id).map(|node| {
                let extra = selected
                    .iter()
                    .filter(|candidate| **candidate != id && scene.is_valid_node(**candidate))
                    .count();
                if extra == 0 {
                    node.name.clone()
                } else {
                    format!("{} (+{})", node.name, extra)
                }
            })
        });
        let menu_label = self
            .menu_target
            .and_then(|(id, _)| scene.get(id).map(|node| node.name.clone()));
        let key = surface_key(
            palette,
            &self.search,
            &view,
            selected,
            self.renaming.as_ref().map(|(id, _)| *id),
            self.menu_target,
            menu_label.clone(),
            self.menu_position
                .map(|point| (point[0].to_bits(), point[1].to_bits())),
            (surface_size[0].to_bits(), surface_size[1].to_bits()),
            row_height,
            indent_width,
            show_icons,
            show_visibility,
            show_locked,
            compact_tabs,
            !self.copy_buffer.is_empty(),
            transition.to_bits(),
            drag_label.clone(),
            self.drag_source.and_then(|_| {
                self.bridge
                    .pointer_position()
                    .map(|point| (point[0].to_bits(), point[1].to_bits()))
            }),
            box_selection.map(|rect| {
                (
                    rect.x.to_bits(),
                    rect.y.to_bits(),
                    rect.width.to_bits(),
                    rect.height.to_bits(),
                )
            }),
            self.active_tab.clone(),
            self.bookmark_filled,
        );
        if self.cached_key.as_ref() != Some(&key) {
            let rename = self
                .renaming
                .as_ref()
                .map(|(id, value)| (*id, value.as_str()));
            self.cached_surface = Some(build_hierarchy_surface(
                palette,
                &view,
                selected,
                rename,
                self.menu_target,
                menu_label.as_deref(),
                self.menu_position,
                surface_size,
                row_height,
                indent_width,
                show_icons,
                show_visibility,
                show_locked,
                transition,
                drag_label.as_deref().map(|label| {
                    (
                        label,
                        self.bridge.pointer_position().unwrap_or([12.0, 12.0]),
                    )
                }),
                box_selection,
                !self.copy_buffer.is_empty(),
                compact_tabs,
                &self.active_tab,
                self.bookmark_filled,
            ));
            self.cached_key = Some(key);
        }

        let Some(surface) = self.cached_surface.as_ref() else {
            return Vec::new();
        };
        let search = self.search.clone();
        let rename = self.renaming.clone();
        let active_tab = self.active_tab.clone();
        let actions = self.bridge.show_with_control_state_ref(
            ui,
            render_state,
            palette,
            surface,
            |controls| {
                seed_text_if_changed(controls, SEARCH_ID, &search, 256);
                if active_tab == "search" {
                    seed_text_if_changed(controls, "hierarchy.search.global", &search, 256);
                }
                if let Some((id, value)) = rename.as_ref() {
                    seed_text_if_changed(
                        controls,
                        &format!("hierarchy.rename.{}", id.0),
                        value,
                        256,
                    );
                }
            },
            |key| t(key, language),
        );
        if let Some(id) = self.focus_rename_next_frame.take() {
            self.bridge.request_focus(id);
        }
        let mut intents = self.apply_actions(actions, scene, selected, row_height);
        self.close_menu_on_pointer_press(surface_size);
        if self.closing && animations_enabled && !self.panel_motion.is_settled() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        if self.closing && self.panel_motion.is_settled() {
            self.closing = false;
            intents.push(HierarchyIntent::TogglePanel);
        }
        intents
    }

    fn apply_actions(
        &mut self,
        actions: Vec<UiDispatchedAction>,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        row_height: f32,
    ) -> Vec<HierarchyIntent> {
        let modifiers = self.bridge.current_modifiers();
        let mut intents = Vec::new();
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetText { key, value } if key == SEARCH_ID => {
                    self.search = value;
                    self.invalidate_surface();
                }
                UiAction::SetText { key, value } if key == "hierarchy.search.global" => {
                    self.search = value;
                    self.invalidate_surface();
                }
                UiAction::SetText { key, value } if key.starts_with("hierarchy.rename.") => {
                    if let Some(id) = parse_id(key.trim_start_matches("hierarchy.rename.")) {
                        if let Some((current_id, current_value)) = self.renaming.as_mut() {
                            if *current_id == id {
                                *current_value = value;
                            }
                        }
                    }
                }
                UiAction::OpenMenu { id } => {
                    self.open_menu_from_id(&id, scene, self.bridge.pointer_position());
                }
                UiAction::Command { name } => {
                    self.apply_command(&name, scene, selected, row_height, modifiers, &mut intents);
                }
                _ => {}
            }
        }
        intents
    }

    fn apply_command(
        &mut self,
        command: &str,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        row_height: f32,
        modifiers: UiModifiers,
        intents: &mut Vec<HierarchyIntent>,
    ) {
        if let Some(tab) = command.strip_prefix("hierarchy.tab:") {
            if matches!(
                tab,
                "hierarchy" | "assets" | "world" | "bookmarks" | "search"
            ) {
                if self.active_tab != tab {
                    self.active_tab = tab.to_string();
                }
                self.close_menu(false);
            }
            return;
        }
        if let Some(slot) = command
            .strip_prefix("hierarchy.bookmark.save:")
            .and_then(|value| value.parse::<usize>().ok())
        {
            intents.push(HierarchyIntent::SaveBookmark(slot));
            return;
        }
        if let Some(slot) = command
            .strip_prefix("hierarchy.bookmark.restore:")
            .and_then(|value| value.parse::<usize>().ok())
        {
            intents.push(HierarchyIntent::RestoreBookmark(slot));
            return;
        }
        if let Some(tab) = command.strip_prefix("hierarchy.open-bottom:") {
            intents.push(HierarchyIntent::OpenBottomTab(tab.to_string()));
            return;
        }
        if command == "hierarchy.search.open" {
            intents.push(HierarchyIntent::OpenSearch(self.search.clone()));
            return;
        }
        if command == "hierarchy.box-select.start" {
            if let Some(point) = self.bridge.pointer_position() {
                self.box_select_start = Some(point);
                self.box_select_current = Some(point);
                self.box_select_additive = modifiers.control || modifiers.command;
                if !self.box_select_additive {
                    self.selection_anchor = None;
                    intents.push(HierarchyIntent::Select(Vec::new()));
                }
                self.invalidate_surface();
            }
            return;
        }
        if command == "hierarchy.box-select.move" {
            if self.box_select_start.is_some() {
                self.box_select_current = self.bridge.pointer_position();
                self.invalidate_surface();
            }
            return;
        }
        if command == "hierarchy.box-select.end" {
            if self.box_select_start.is_some() {
                let selected_ids = self
                    .box_selection_rect()
                    .map(|selection| self.box_selected_ids(selection, row_height))
                    .unwrap_or_default();
                let mut result = if self.box_select_additive {
                    selected.to_vec()
                } else {
                    Vec::new()
                };
                for id in selected_ids {
                    if !result.contains(&id) {
                        result.push(id);
                    }
                }
                self.selection_anchor = result.first().copied();
                self.box_select_start = None;
                self.box_select_current = None;
                self.box_select_additive = false;
                intents.push(HierarchyIntent::Select(result));
                self.invalidate_surface();
            }
            return;
        }
        if let Some(id) = command.strip_prefix("hierarchy.select:").and_then(parse_id) {
            let ids = self.selection_for_click(id, selected, modifiers);
            self.selection_anchor = Some(id);
            intents.push(HierarchyIntent::Select(ids));
            return;
        }
        if command == "hierarchy.clear-selection" {
            if self.menu_target.is_some() {
                self.close_menu(true);
                return;
            }
            self.selection_anchor = None;
            intents.push(HierarchyIntent::Select(Vec::new()));
            return;
        }
        if let Some(id) = command.strip_prefix("hierarchy.expand:").and_then(parse_id) {
            self.model.toggle_expanded(id);
            self.invalidate_surface();
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.expand-recursive:")
            .and_then(parse_id)
        {
            self.model.expand_recursive(scene, id, true);
            self.close_menu(true);
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.collapse-recursive:")
            .and_then(parse_id)
        {
            self.model.expand_recursive(scene, id, false);
            self.close_menu(true);
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.select-children:")
            .and_then(parse_id)
        {
            let mut ids = Vec::new();
            collect_descendants(scene, id, &mut ids);
            self.selection_anchor = ids.first().copied();
            self.close_menu(true);
            intents.push(HierarchyIntent::Select(ids));
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.rename.begin:")
            .and_then(parse_id)
        {
            if let Some(node) = scene.get(id) {
                self.renaming = Some((id, node.name.clone()));
                self.close_menu(false);
                self.focus_rename_next_frame = Some(format!("hierarchy.rename.control.{}", id.0));
                self.invalidate_surface();
            }
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.rename.commit:")
            .and_then(parse_id)
        {
            if let Some((current_id, value)) = self.renaming.take() {
                if current_id == id && !value.trim().is_empty() {
                    intents.push(HierarchyIntent::Rename {
                        id,
                        name: value.trim().to_string(),
                    });
                }
            }
            self.invalidate_surface();
            return;
        }
        if command == "hierarchy.rename.cancel" {
            self.renaming = None;
            self.invalidate_surface();
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.visibility:")
            .and_then(parse_id)
        {
            intents.push(HierarchyIntent::ToggleVisibility(id));
            return;
        }
        if let Some(id) = command.strip_prefix("hierarchy.lock:").and_then(parse_id) {
            intents.push(HierarchyIntent::ToggleLocked(id));
            return;
        }
        if let Some(value) = command.strip_prefix("hierarchy.navigate:") {
            let mut parts = value.split(':');
            let direction = parts.next().unwrap_or_default();
            if let Some(id) = parts.next().and_then(parse_id) {
                let ids = self.model.row_ids().collect::<Vec<_>>();
                if let Some(index) = ids.iter().position(|candidate| *candidate == id) {
                    let target_index = match direction {
                        "up" => index.saturating_sub(1),
                        "down" => (index + 1).min(ids.len().saturating_sub(1)),
                        "home" => 0,
                        "end" => ids.len().saturating_sub(1),
                        _ => index,
                    };
                    if let Some(target) = ids.get(target_index).copied() {
                        self.ensure_row_visible(target, row_height);
                        self.bridge
                            .request_focus(format!("hierarchy.row.{}.select", target.0));
                        let next = if modifiers.shift {
                            if self.selection_anchor.is_none() {
                                self.selection_anchor = Some(id);
                            }
                            self.selection_for_click(target, selected, modifiers)
                        } else {
                            self.selection_anchor = Some(target);
                            vec![target]
                        };
                        intents.push(HierarchyIntent::Select(next));
                    }
                }
            }
            return;
        }
        if command == "hierarchy.toggle-hidden" {
            self.show_hidden = !self.show_hidden;
            self.invalidate_surface();
            return;
        }
        if let Some(id) = command.strip_prefix("hierarchy.copy:").and_then(parse_id) {
            let candidates = if selected.contains(&id) {
                selected.to_vec()
            } else {
                vec![id]
            };
            self.copy_buffer = top_level_nodes(scene, candidates);
            self.close_menu(true);
            return;
        }
        if let Some(id) = command.strip_prefix("hierarchy.paste:").and_then(parse_id) {
            if !self.copy_buffer.is_empty() && scene.is_valid_node(id) {
                self.close_menu(true);
                intents.push(HierarchyIntent::Paste {
                    sources: self.copy_buffer.clone(),
                    parent: Some(id),
                });
            }
            return;
        }
        if command == "hierarchy.create-folder:root" {
            self.close_menu(false);
            intents.push(HierarchyIntent::CreateFolder { parent: None });
            return;
        }
        if command == "hierarchy.create-entity:root" {
            self.close_menu(false);
            intents.push(HierarchyIntent::CreateEntity { parent: None });
            return;
        }
        if let Some(parent) = command
            .strip_prefix("hierarchy.create-entity:")
            .and_then(parse_id)
        {
            self.close_menu(true);
            intents.push(HierarchyIntent::CreateEntity {
                parent: Some(parent),
            });
            return;
        }
        if let Some(parent) = command
            .strip_prefix("hierarchy.create-folder:")
            .and_then(parse_id)
        {
            self.close_menu(true);
            intents.push(HierarchyIntent::CreateFolder {
                parent: Some(parent),
            });
            return;
        }
        for (prefix, map) in [
            ("hierarchy.duplicate:", 0),
            ("hierarchy.delete:", 1),
            ("hierarchy.ungroup:", 2),
            ("hierarchy.focus:", 3),
            ("hierarchy.reparent-root:", 4),
        ] {
            if let Some(id) = command.strip_prefix(prefix).and_then(parse_id) {
                self.close_menu(true);
                match map {
                    0 => intents.push(HierarchyIntent::Duplicate(id)),
                    1 => intents.push(HierarchyIntent::Delete(id)),
                    2 => intents.push(HierarchyIntent::Ungroup(id)),
                    3 => intents.push(HierarchyIntent::Focus(id)),
                    _ => intents.push(HierarchyIntent::Reparent {
                        sources: vec![id],
                        target: None,
                        before: None,
                    }),
                }
                return;
            }
        }
        if let Some(id) = command.strip_prefix("hierarchy.menu:").and_then(parse_id) {
            self.open_menu(id, scene, self.bridge.pointer_position());
            return;
        }
        if command == "hierarchy.menu.close" {
            self.close_menu(true);
            return;
        }
        if command == "hierarchy.panel.toggle" {
            self.close_menu(false);
            self.closing = true;
            self.invalidate_surface();
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.drag.start:")
            .and_then(parse_id)
        {
            self.drag_source = Some(id);
            self.drag_target = None;
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.drag.over:")
            .and_then(parse_id)
        {
            self.drag_target = self.drop_target_for_pointer(scene, id);
            self.auto_scroll_drag_target();
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.drag.over-before:")
            .and_then(parse_id)
        {
            self.drag_target = scene.get(id).map(|node| DropTarget {
                parent: node.parent,
                before: Some(id),
            });
            self.auto_scroll_drag_target();
            return;
        }
        if let Some(id) = command
            .strip_prefix("hierarchy.drag.over-after:")
            .and_then(parse_id)
        {
            self.drag_target = scene.get(id).map(|node| DropTarget {
                parent: node.parent,
                before: next_sibling(scene, id),
            });
            self.auto_scroll_drag_target();
            return;
        }
        if command == "hierarchy.drag.over:root" {
            self.drag_target = Some(DropTarget {
                parent: None,
                before: None,
            });
            self.auto_scroll_drag_target();
            return;
        }
        if command.starts_with("hierarchy.drag.end:") {
            if let Some(source) = self.drag_source.take() {
                let target = self.drag_target.take().unwrap_or(DropTarget {
                    parent: None,
                    before: None,
                });
                if target.parent != Some(source) && target.before != Some(source) {
                    let mut sources = if selected.contains(&source) {
                        selected.to_vec()
                    } else {
                        vec![source]
                    };
                    sources.retain(|id| scene.is_valid_node(*id));
                    sources.dedup();
                    intents.push(HierarchyIntent::Reparent {
                        sources,
                        target: target.parent,
                        before: target.before,
                    });
                }
            }
        }
    }

    fn drop_target_for_pointer(&self, scene: &SceneGraph, id: SceneNodeId) -> Option<DropTarget> {
        let node = scene.get(id)?;
        let rect = self.bridge.layout_rect(&format!("hierarchy.row.{}", id.0));
        let pointer_y = self.bridge.pointer_position().map(|point| point[1]);
        let Some((rect, pointer_y)) = rect.zip(pointer_y) else {
            return Some(DropTarget {
                parent: Some(id),
                before: None,
            });
        };
        let local_y = pointer_y - rect.y;
        let before_band = rect.height * 0.28;
        let after_band = rect.height * 0.72;
        if local_y <= before_band {
            Some(DropTarget {
                parent: node.parent,
                before: Some(id),
            })
        } else if local_y >= after_band {
            Some(DropTarget {
                parent: node.parent,
                before: next_sibling(scene, id),
            })
        } else {
            Some(DropTarget {
                parent: Some(id),
                before: None,
            })
        }
    }

    fn selection_for_click(
        &self,
        id: SceneNodeId,
        selected: &[SceneNodeId],
        modifiers: UiModifiers,
    ) -> Vec<SceneNodeId> {
        if modifiers.shift {
            if let Some(anchor) = self.selection_anchor {
                let ids = self.model.row_ids().collect::<Vec<_>>();
                let Some(anchor_index) = ids.iter().position(|candidate| *candidate == anchor)
                else {
                    return vec![id];
                };
                let Some(target_index) = ids.iter().position(|candidate| *candidate == id) else {
                    return vec![id];
                };
                let (start, end) = if anchor_index <= target_index {
                    (anchor_index, target_index)
                } else {
                    (target_index, anchor_index)
                };
                return ids[start..=end].to_vec();
            }
        }
        if modifiers.control || modifiers.command {
            let mut result = selected.to_vec();
            if let Some(index) = result.iter().position(|candidate| *candidate == id) {
                result.remove(index);
            } else {
                result.push(id);
            }
            return result;
        }
        vec![id]
    }

    fn open_menu_from_id(
        &mut self,
        command_id: &str,
        scene: &SceneGraph,
        position: Option<[f32; 2]>,
    ) {
        if let Some(id) = command_id
            .strip_prefix("hierarchy.menu:")
            .and_then(parse_id)
        {
            self.open_menu(id, scene, position);
        }
    }

    fn open_menu(&mut self, id: SceneNodeId, scene: &SceneGraph, position: Option<[f32; 2]>) {
        if let Some(node) = scene.get(id) {
            self.menu_target = Some((id, node.is_folder));
            self.menu_position = position;
            self.bridge.request_focus("hierarchy.context-menu");
            self.invalidate_surface();
        }
    }

    fn close_menu(&mut self, restore_focus: bool) {
        let target = self.menu_target.take();
        self.menu_position = None;
        if restore_focus {
            if let Some((id, _)) = target {
                self.bridge
                    .request_focus(format!("hierarchy.row.{}.select", id.0));
            }
        }
        self.invalidate_surface();
    }

    fn close_menu_on_pointer_press(&mut self, surface_size: [f32; 2]) {
        let Some((_, is_folder)) = self.menu_target else {
            return;
        };
        let input = self.bridge.input_snapshot();
        let primary_pressed = input
            .pointer_pressed_buttons
            .contains(&UiPointerButton::Primary);
        if !primary_pressed && !input.pointer_pressed_outside {
            return;
        }
        let inside_menu = input.pointer_position.is_some_and(|point| {
            context_menu_rect(self.menu_position, is_folder, surface_size).contains(point)
        });
        if input.pointer_pressed_outside || !inside_menu {
            self.close_menu(true);
        }
    }

    fn box_selection_rect(&self) -> Option<raf_ui::UiRect> {
        let (start, current) = self.box_select_start.zip(self.box_select_current)?;
        let selection = raf_ui::UiRect::new(
            start[0].min(current[0]),
            start[1].min(current[1]),
            (start[0] - current[0]).abs(),
            (start[1] - current[1]).abs(),
        );
        self.bridge
            .layout_rect(TREE_ID)
            .map(|tree_rect| selection.intersection(tree_rect))
            .or(Some(selection))
    }

    fn box_selected_ids(&self, selection: raf_ui::UiRect, row_height: f32) -> Vec<SceneNodeId> {
        let Some(tree_rect) = self.bridge.layout_rect(TREE_ID) else {
            return Vec::new();
        };
        let selection = selection.intersection(tree_rect);
        if selection.is_empty() {
            return Vec::new();
        }
        let scroll_offset = self
            .bridge
            .with_control_state_read(|controls| controls.scroll_offset(TREE_ID)[1])
            .unwrap_or(0.0);
        let row_height = row_height.max(16.0);
        let row_x = tree_rect.x + 6.0;
        let row_width = (tree_rect.width - 12.0).max(0.0);
        let first_row_y = tree_rect.y + 4.0 - scroll_offset;
        self.model
            .row_ids()
            .enumerate()
            .filter_map(|(index, id)| {
                let row_rect = raf_ui::UiRect::new(
                    row_x,
                    first_row_y + index as f32 * (row_height + 2.0),
                    row_width,
                    row_height,
                );
                (!row_rect.intersection(selection).is_empty()).then_some(id)
            })
            .collect()
    }

    fn auto_scroll_drag_target(&mut self) {
        if self.drag_source.is_none() {
            return;
        }
        let (Some(pointer), Some(tree_rect)) = (
            self.bridge.pointer_position(),
            self.bridge.layout_rect(TREE_ID),
        ) else {
            return;
        };
        let edge = 28.0;
        let delta = if pointer[1] <= tree_rect.y + edge {
            -24.0
        } else if pointer[1] >= tree_rect.bottom() - edge {
            24.0
        } else {
            0.0
        };
        if delta != 0.0 {
            self.bridge.scroll_by(TREE_ID, [0.0, delta]);
            self.invalidate_surface();
        }
    }

    fn ensure_row_visible(&mut self, id: SceneNodeId, row_height: f32) {
        let Some(index) = self.model.row_ids().position(|candidate| candidate == id) else {
            return;
        };
        let Some(tree_rect) = self.bridge.layout_rect(TREE_ID) else {
            return;
        };
        let row_height = row_height.max(16.0);
        let stride = row_height + 2.0;
        let row_top = index as f32 * stride;
        let row_bottom = row_top + row_height;
        let scroll = self
            .bridge
            .with_control_state_read(|controls| controls.scroll_offset(TREE_ID)[1])
            .unwrap_or(0.0);
        let delta = if row_top < scroll {
            row_top - scroll
        } else if row_bottom > scroll + tree_rect.height {
            row_bottom - (scroll + tree_rect.height)
        } else {
            0.0
        };
        if delta != 0.0 {
            self.bridge.scroll_by(TREE_ID, [0.0, delta]);
            self.invalidate_surface();
        }
    }

    fn invalidate_surface(&mut self) {
        self.cached_key = None;
    }
}

fn seed_text_if_changed(controls: &mut UiControlState, key: &str, value: &str, max_length: usize) {
    if !controls.has_text(key) || controls.text(key) != value {
        controls.set_text(key, value, max_length);
    }
}

fn parse_id(value: &str) -> Option<SceneNodeId> {
    value.parse::<usize>().ok().map(SceneNodeId)
}

fn collect_descendants(scene: &SceneGraph, id: SceneNodeId, output: &mut Vec<SceneNodeId>) {
    let Some(node) = scene.get(id) else {
        return;
    };
    for &child in &node.children {
        if scene.is_valid_node(child) {
            output.push(child);
            collect_descendants(scene, child, output);
        }
    }
}

fn top_level_nodes(scene: &SceneGraph, ids: Vec<SceneNodeId>) -> Vec<SceneNodeId> {
    let mut result = ids
        .into_iter()
        .filter(|id| scene.is_valid_node(*id))
        .collect::<Vec<_>>();
    result.dedup();
    let candidates = result.clone();
    result.retain(|candidate| {
        !candidates
            .iter()
            .any(|ancestor| ancestor != candidate && is_descendant(scene, *candidate, *ancestor))
    });
    result
}

fn is_descendant(scene: &SceneGraph, candidate: SceneNodeId, ancestor: SceneNodeId) -> bool {
    let mut current = scene.get(candidate).and_then(|node| node.parent);
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        current = scene.get(id).and_then(|node| node.parent);
    }
    false
}

fn next_sibling(scene: &SceneGraph, id: SceneNodeId) -> Option<SceneNodeId> {
    let node = scene.get(id)?;
    let siblings = node
        .parent
        .and_then(|parent| scene.get(parent).map(|node| node.children.as_slice()))
        .unwrap_or_else(|| scene.roots());
    siblings
        .iter()
        .position(|candidate| *candidate == id)
        .and_then(|index| siblings.get(index + 1).copied())
}

fn surface_key(
    palette: StudioUiPalette,
    query: &str,
    view: &HierarchyView,
    selected: &[SceneNodeId],
    renaming: Option<SceneNodeId>,
    menu_target: Option<(SceneNodeId, bool)>,
    menu_label: Option<String>,
    menu_position_bits: Option<(u32, u32)>,
    surface_size_bits: (u32, u32),
    row_height: f32,
    indent_width: f32,
    show_icons: bool,
    show_visibility: bool,
    show_locked: bool,
    compact_tabs: bool,
    can_paste: bool,
    transition_bits: u32,
    drag_label: Option<String>,
    drag_pointer_bits: Option<(u32, u32)>,
    box_selection_bits: Option<(u32, u32, u32, u32)>,
    active_tab: String,
    bookmark_filled: [bool; 3],
) -> SurfaceKey {
    let mut hasher = DefaultHasher::new();
    view.total_rows.hash(&mut hasher);
    view.visible_start.hash(&mut hasher);
    view.top_spacer.to_bits().hash(&mut hasher);
    view.bottom_spacer.to_bits().hash(&mut hasher);
    for row in &view.rows {
        row.id.hash(&mut hasher);
        row.depth.hash(&mut hasher);
        row.name.hash(&mut hasher);
        row.visible.hash(&mut hasher);
        row.locked.hash(&mut hasher);
        row.has_children.hash(&mut hasher);
        row.expanded.hash(&mut hasher);
    }
    SurfaceKey {
        palette_dark: matches!(palette, StudioUiPalette::IndustrialDark),
        query: query.to_string(),
        view_hash: hasher.finish(),
        selected: selected.to_vec(),
        renaming,
        menu_target,
        menu_label,
        menu_position_bits,
        surface_size_bits,
        row_height_bits: row_height.to_bits(),
        indent_width_bits: indent_width.to_bits(),
        show_icons,
        show_visibility,
        show_locked,
        compact_tabs,
        can_paste,
        transition_bits,
        drag_label,
        drag_pointer_bits,
        box_selection_bits,
        active_tab,
        bookmark_filled,
    }
}
