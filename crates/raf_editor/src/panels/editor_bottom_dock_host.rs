//! Native state host for the retained bottom dock.
//!
//! The dock builders remain in `editor_bottom_dock_surface.rs`; this module
//! owns only tab state and project-local layout persistence so the surface
//! stays declarative.

use std::collections::HashSet;
use std::path::PathBuf;

use raf_core::project::{Project, ProjectType};
use raf_ui::{
    BottomDockLayout, DockTab, DockTabGroup, UiIconId, BOTTOM_DOCK_LAYOUT_VERSION,
    MAX_BOTTOM_DOCK_GROUPS,
};

use crate::editor_layout::{
    EDITOR_DOCK_COLLAPSED_HEIGHT, EDITOR_DOCK_MAX_HEIGHT, EDITOR_DOCK_MIN_HEIGHT,
};

pub struct EditorBottomDockHost {
    pub layout: BottomDockLayout,
    project_layout_path: Option<PathBuf>,
    layout_dirty: bool,
    context_menu: Option<BottomDockContextMenu>,
    drag: Option<BottomDockDragState>,
    resize: Option<BottomDockResizeState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BottomDockContextMenu {
    pub group_id: String,
    pub tab_id: String,
    pub position: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct BottomDockDragState {
    pub source_group_id: String,
    pub source_tab_id: String,
    pub pointer: [f32; 2],
    pub target_group_id: String,
    pub insertion_index: usize,
    pub split_before: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BottomDockResizeState {
    pub left_group_id: String,
    pub right_group_id: String,
    pub origin_x: f32,
    pub pointer_x: f32,
}

impl Default for EditorBottomDockHost {
    fn default() -> Self {
        Self {
            layout: BottomDockLayout::new(default_groups_for_type(ProjectType::Game)),
            project_layout_path: None,
            layout_dirty: false,
            context_menu: None,
            drag: None,
            resize: None,
        }
    }
}

impl EditorBottomDockHost {
    const LAYOUT_DIRECTORY: &'static str = ".aura_rafi";
    const LAYOUT_FILE: &'static str = "editor_downbar.ron";

    pub fn select(&mut self, tab: &str) -> bool {
        self.context_menu = None;
        self.resize = None;
        let changed = self
            .layout
            .groups
            .iter_mut()
            .find(|group| group.tabs.iter().any(|candidate| candidate.id == tab))
            .is_some_and(|group| group.select_tab(tab));
        self.layout_dirty |= changed;
        changed
    }

    pub fn select_in_group(&mut self, group_id: &str, tab: &str) -> bool {
        self.context_menu = None;
        let changed = self.layout.select_tab(group_id, tab);
        self.layout_dirty |= changed;
        changed
    }

    pub fn active_tab(&self) -> &str {
        self.layout
            .groups
            .first()
            .map(|group| group.active_tab.as_str())
            .unwrap_or("")
    }

    pub fn active_tab_for(&self, group_id: &str) -> Option<&str> {
        self.layout
            .groups
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.active_tab.as_str())
    }

    pub fn groups(&self) -> &[DockTabGroup] {
        &self.layout.groups
    }

    pub fn layout(&self) -> &BottomDockLayout {
        &self.layout
    }

    pub fn set_height(&mut self, height: f32) -> bool {
        let changed =
            self.layout
                .set_height(height, EDITOR_DOCK_MIN_HEIGHT, EDITOR_DOCK_MAX_HEIGHT);
        self.layout_dirty |= changed;
        changed
    }

    pub fn is_collapsed(&self) -> bool {
        self.layout.collapsed
    }

    pub fn effective_height(&self) -> f32 {
        self.layout.effective_height(EDITOR_DOCK_COLLAPSED_HEIGHT)
    }

    pub fn has_active_tab(&self, tab: &str) -> bool {
        self.layout
            .groups
            .iter()
            .any(|group| group.active_tab == tab)
    }

    pub fn group_containing_active_tab(&self, tab: &str) -> Option<&DockTabGroup> {
        self.layout
            .groups
            .iter()
            .find(|group| group.active_tab == tab)
    }

    pub fn tab(&self, group_id: &str, tab_id: &str) -> Option<&DockTab> {
        self.layout
            .groups
            .iter()
            .find(|group| group.id == group_id)
            .and_then(|group| group.tabs.iter().find(|tab| tab.id == tab_id))
    }

    pub fn can_split(&self) -> bool {
        self.layout.groups.len() < MAX_BOTTOM_DOCK_GROUPS
    }

    pub fn toggle_collapsed(&mut self) {
        self.layout
            .toggle_collapsed(EDITOR_DOCK_MIN_HEIGHT, EDITOR_DOCK_MAX_HEIGHT);
        self.layout_dirty = true;
        self.context_menu = None;
    }

    pub fn drag(&self) -> Option<&BottomDockDragState> {
        self.drag.as_ref()
    }

    pub fn resize(&self) -> Option<&BottomDockResizeState> {
        self.resize.as_ref()
    }

    pub fn begin_resize(
        &mut self,
        left_group_id: &str,
        right_group_id: &str,
        pointer_x: f32,
    ) -> bool {
        let adjacent = self
            .layout
            .resolve_columns(10_000.0, 4.0)
            .windows(2)
            .any(|window| window[0].0 == left_group_id && window[1].0 == right_group_id);
        if !adjacent {
            return false;
        }
        self.context_menu = None;
        self.drag = None;
        self.resize = Some(BottomDockResizeState {
            left_group_id: left_group_id.to_string(),
            right_group_id: right_group_id.to_string(),
            origin_x: pointer_x,
            pointer_x,
        });
        true
    }

    pub fn update_resize(&mut self, pointer_x: f32, dock_width: f32, gap: f32) -> bool {
        let Some(resize) = self.resize.as_ref() else {
            return false;
        };
        let delta = pointer_x - resize.origin_x;
        let changed = self.layout.resize_boundary(
            &resize.left_group_id,
            &resize.right_group_id,
            dock_width,
            gap,
            delta,
        );
        if changed {
            if let Some(resize) = self.resize.as_mut() {
                resize.pointer_x = pointer_x;
                resize.origin_x = pointer_x;
            }
            self.layout_dirty = true;
        } else if let Some(resize) = self.resize.as_mut() {
            resize.pointer_x = pointer_x;
        }
        changed
    }

    pub fn end_resize(&mut self) -> bool {
        let changed = self.resize.take().is_some();
        if changed {
            self.layout_dirty = true;
        }
        changed
    }

    pub fn begin_drag(&mut self, group_id: &str, tab_id: &str, pointer: [f32; 2]) -> bool {
        let Some(group) = self.layout.groups.iter().find(|group| group.id == group_id) else {
            return false;
        };
        if !group.tabs.iter().any(|tab| tab.id == tab_id) {
            return false;
        }
        self.context_menu = None;
        self.drag = Some(BottomDockDragState {
            source_group_id: group_id.to_string(),
            source_tab_id: tab_id.to_string(),
            pointer,
            target_group_id: group_id.to_string(),
            insertion_index: group
                .tabs
                .iter()
                .position(|tab| tab.id == tab_id)
                .unwrap_or_default(),
            split_before: None,
        });
        true
    }

    pub fn update_drag(
        &mut self,
        target_group_id: &str,
        insertion_index: usize,
        split_before: Option<bool>,
        pointer: [f32; 2],
    ) -> bool {
        let Some(drag) = self.drag.as_mut() else {
            return false;
        };
        if !self
            .layout
            .groups
            .iter()
            .any(|group| group.id == target_group_id)
        {
            return false;
        }
        let changed = drag.target_group_id != target_group_id
            || drag.insertion_index != insertion_index
            || drag.split_before != split_before
            || drag.pointer != pointer;
        drag.target_group_id = target_group_id.to_string();
        drag.insertion_index = insertion_index;
        drag.split_before = split_before;
        drag.pointer = pointer;
        changed
    }

    /// Resolves a drag pointer against the visible bottom-dock columns. The
    /// host owns this geometry decision so the retained surface can be
    /// rebuilt while the pointer remains captured.
    pub fn update_drag_at(&mut self, pointer: [f32; 2], dock_rect: [f32; 4], gap: f32) -> bool {
        let columns = self.layout.resolve_columns(dock_rect[2], gap);
        let Some((target_group_id, column)) = columns.iter().find(|(_, rect)| {
            pointer[0] >= dock_rect[0] + rect.x && pointer[0] < dock_rect[0] + rect.x + rect.width
        }) else {
            return false;
        };
        let Some(group) = self
            .layout
            .groups
            .iter()
            .find(|group| group.id == target_group_id.as_str())
        else {
            return false;
        };
        let local_x = pointer[0] - dock_rect[0] - column.x;
        let split_before = if self.can_split() && local_x < 16.0 {
            Some(true)
        } else if self.can_split() && local_x > column.width - 16.0 {
            Some(false)
        } else {
            None
        };
        let insertion_index = if split_before.is_some() {
            0
        } else {
            let mut cursor = 5.0;
            let mut index = group.tabs.len();
            for (candidate_index, tab) in group.tabs.iter().enumerate() {
                let width = match tab.id.as_str() {
                    "project-settings" => 104.0,
                    "nodes" => 92.0,
                    "console" => 96.0,
                    "assets" => 86.0,
                    "drc" => 86.0,
                    "simulation" => 112.0,
                    _ => 96.0,
                };
                if local_x < cursor + width * 0.5 {
                    index = candidate_index;
                    break;
                }
                cursor += width + 2.0;
            }
            index
        };
        self.update_drag(target_group_id, insertion_index, split_before, pointer)
    }

    pub fn end_drag(&mut self) -> bool {
        let Some(drag) = self.drag.take() else {
            return false;
        };
        let changed = if let Some(split_before) = drag.split_before {
            if self.layout.groups.len() >= MAX_BOTTOM_DOCK_GROUPS {
                false
            } else {
                let new_id = self.next_group_id();
                self.layout.split_tab_next_to(
                    &drag.source_group_id,
                    &drag.source_tab_id,
                    new_id,
                    &drag.target_group_id,
                    split_before,
                )
            }
        } else if drag.source_group_id == drag.target_group_id {
            self.layout.move_tab_within_group(
                &drag.source_group_id,
                &drag.source_tab_id,
                drag.insertion_index,
            )
        } else {
            self.layout.move_tab_to_group(
                &drag.source_group_id,
                &drag.target_group_id,
                &drag.source_tab_id,
            )
        };
        self.layout_dirty |= changed;
        changed
    }

    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }

    pub fn split_context(&mut self, before: bool) -> bool {
        let Some(context) = self.context_menu.take() else {
            return false;
        };
        if self.layout.groups.len() >= MAX_BOTTOM_DOCK_GROUPS {
            return false;
        }
        let new_id = self.next_group_id();
        let changed = self.layout.split_tab_next_to(
            &context.group_id,
            &context.tab_id,
            new_id,
            &context.group_id,
            before,
        );
        self.layout_dirty |= changed;
        changed
    }

    pub fn open_context_menu(
        &mut self,
        group_id: impl Into<String>,
        tab_id: impl Into<String>,
        position: [f32; 2],
    ) {
        self.context_menu = Some(BottomDockContextMenu {
            group_id: group_id.into(),
            tab_id: tab_id.into(),
            position,
        });
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    pub fn context_menu(&self) -> Option<&BottomDockContextMenu> {
        self.context_menu.as_ref()
    }

    /// Loads and normalizes the project-local layout. The serialized document
    /// owns tab order, active tabs, group weights, and split topology; the
    /// native workbench only resolves those descriptors into current window
    /// rectangles.
    pub fn sync_project_layout(&mut self, project: Option<&Project>) {
        let next_path = project.map(|project| {
            project
                .path
                .join(Self::LAYOUT_DIRECTORY)
                .join(Self::LAYOUT_FILE)
        });
        if self.project_layout_path == next_path {
            return;
        }

        self.project_layout_path = next_path;
        self.layout_dirty = false;
        self.context_menu = None;
        self.drag = None;
        self.resize = None;
        let project_type = project.map_or(ProjectType::Game, |project| project.project_type);
        let Some(path) = self.project_layout_path.as_deref() else {
            self.reset_group(project_type);
            return;
        };

        let loaded = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| ron::from_str::<BottomDockLayout>(&raw).ok());
        if let Some(layout) = loaded.filter(|layout| layout.version >= BOTTOM_DOCK_LAYOUT_VERSION) {
            let defaults = default_tabs_for_type(project_type);
            let supported = defaults
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<HashSet<_>>();
            let mut used = HashSet::new();
            let mut groups = Vec::new();
            for (index, persisted_group) in layout.groups.iter().enumerate() {
                let tabs = persisted_group
                    .tabs
                    .iter()
                    .filter_map(|tab| {
                        if !supported.contains(tab.id.as_str()) || !used.insert(tab.id.clone()) {
                            return None;
                        }
                        defaults.iter().find(|item| item.id == tab.id).cloned()
                    })
                    .collect::<Vec<_>>();
                if tabs.is_empty() {
                    continue;
                }
                let mut group = DockTabGroup::new(
                    if persisted_group.id.trim().is_empty() {
                        format!("native.group.{index}")
                    } else {
                        persisted_group.id.clone()
                    },
                    tabs,
                )
                .with_weight(persisted_group.weight)
                .with_min_width(persisted_group.min_width);
                if group
                    .tabs
                    .iter()
                    .any(|tab| tab.id == persisted_group.active_tab)
                {
                    group.active_tab = persisted_group.active_tab.clone();
                }
                groups.push(group);
            }
            if groups.is_empty() {
                self.reset_group(project_type);
                return;
            }
            let missing_tabs = defaults
                .iter()
                .filter(|tab| !used.contains(&tab.id))
                .cloned()
                .collect::<Vec<_>>();
            groups[0].tabs.extend(missing_tabs);
            let migrated_single_group = groups.len() == 1;
            if migrated_single_group {
                groups = split_legacy_single_group(groups.remove(0), project_type);
            }
            for group in &mut groups {
                if !group.tabs.iter().any(|tab| tab.id == group.active_tab) {
                    group.active_tab = group
                        .tabs
                        .first()
                        .map(|tab| tab.id.clone())
                        .unwrap_or_default();
                }
            }
            self.layout = BottomDockLayout::new(groups);
            self.layout.height = layout.height;
            self.layout.expanded_height = layout.expanded_height;
            self.layout.collapsed = layout.collapsed;
            self.layout.normalize();
            self.layout_dirty = migrated_single_group;
        } else {
            self.reset_group(project_type);
        }
    }

    pub fn persist_project_layout(&mut self) {
        if !self.layout_dirty {
            return;
        }
        let Some(path) = self.project_layout_path.as_deref() else {
            self.layout_dirty = false;
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let mut layout = self.layout.clone();
        layout.version = BOTTOM_DOCK_LAYOUT_VERSION;
        layout.normalize();
        if let Ok(raw) = ron::ser::to_string_pretty(&layout, ron::ser::PrettyConfig::default()) {
            if std::fs::write(path, raw).is_ok() {
                self.layout_dirty = false;
            }
        }
    }

    pub fn reset_group(&mut self, project_type: ProjectType) {
        self.layout = BottomDockLayout::new(default_groups_for_type(project_type));
        self.layout_dirty = true;
        self.context_menu = None;
        self.drag = None;
        self.resize = None;
    }

    fn next_group_id(&self) -> String {
        let mut index = self.layout.groups.len() + 1;
        loop {
            let candidate = format!("native.group.{index}");
            if !self.layout.groups.iter().any(|group| group.id == candidate) {
                return candidate;
            }
            index += 1;
        }
    }
}

/// Returns the stable authoring arrangement used by a new project. The left
/// track is for editor utilities and the right track is reserved for Assets,
/// matching the original workbench hierarchy without flattening the dock into
/// one crowded tab strip.
fn default_groups_for_type(project_type: ProjectType) -> Vec<DockTabGroup> {
    let tabs = default_tabs_for_type(project_type);
    let left_ids = if project_type == ProjectType::Game {
        ["console", "agent", "nodes", "project-settings"].as_slice()
    } else {
        ["console", "agent", "drc", "simulation"].as_slice()
    };
    let left = left_ids
        .iter()
        .filter_map(|id| tabs.iter().find(|tab| tab.id == *id).cloned())
        .collect::<Vec<_>>();
    let right = tabs
        .iter()
        .filter(|tab| !left_ids.contains(&tab.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    vec![
        DockTabGroup::new("native.console", left),
        DockTabGroup::new("native.assets", right),
    ]
}

/// Older project-local layouts serialized every tab into `native.primary`.
/// Preserve each tab's order while restoring the two-track workbench shape.
fn split_legacy_single_group(group: DockTabGroup, project_type: ProjectType) -> Vec<DockTabGroup> {
    let defaults = default_groups_for_type(project_type);
    let right_ids = defaults
        .get(1)
        .map(|group| {
            group
                .tabs
                .iter()
                .map(|tab| tab.id.as_str())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let mut left_tabs = Vec::new();
    let mut right_tabs = Vec::new();
    for tab in group.tabs {
        if right_ids.contains(tab.id.as_str()) {
            right_tabs.push(tab);
        } else {
            left_tabs.push(tab);
        }
    }
    let mut left = DockTabGroup::new("native.console", left_tabs);
    let mut right = DockTabGroup::new("native.assets", right_tabs);
    if left.tabs.iter().any(|tab| tab.id == group.active_tab) {
        left.active_tab = group.active_tab.clone();
    } else if right.tabs.iter().any(|tab| tab.id == group.active_tab) {
        right.active_tab = group.active_tab;
    }
    vec![left, right]
}

fn default_tabs_for_type(project_type: ProjectType) -> Vec<DockTab> {
    let mut tabs = vec![
        DockTab::new("console", "app.studio_console", UiIconId::Console),
        DockTab::new("assets", "app.studio_assets", UiIconId::Assets),
    ];
    if project_type == ProjectType::Game {
        tabs.push(DockTab::new("nodes", "app.nodes", UiIconId::Node));
    } else {
        tabs.push(DockTab::new("drc", "DRC", UiIconId::Warning));
        tabs.push(DockTab::new("simulation", "Simulation", UiIconId::Play));
    }
    tabs.push(DockTab::new("agent", "app.agent_tab", UiIconId::Agent));
    tabs.push(DockTab::new(
        "project-settings",
        "app.project_settings_tab",
        UiIconId::Settings,
    ));
    tabs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_dock_drag_reorders_tabs_without_flattening_the_layout() {
        let mut host = EditorBottomDockHost::default();
        assert!(host.begin_drag("native.console", "console", [20.0, 20.0]));
        assert!(host.update_drag(
            "native.console",
            host.groups()[0].tabs.len(),
            None,
            [260.0, 20.0],
        ));
        assert!(host.end_drag());
        assert_eq!(host.groups().len(), 2);
        assert_eq!(
            host.groups()[0].tabs.last().map(|tab| tab.id.as_str()),
            Some("console")
        );
    }

    #[test]
    fn native_dock_context_split_creates_a_second_real_group() {
        let mut host = EditorBottomDockHost::default();
        host.open_context_menu("native.console", "console", [120.0, 120.0]);
        assert!(host.split_context(false));
        assert_eq!(host.groups().len(), 3);
        assert!(host
            .groups()
            .iter()
            .any(|group| { group.tabs.iter().any(|tab| tab.id == "console") }));
    }

    #[test]
    fn settings_is_not_registered_as_a_bottom_dock_tab() {
        let tabs = default_tabs_for_type(ProjectType::Game);
        assert!(!tabs.iter().any(|tab| tab.id == "settings"));
        assert!(!tabs.iter().any(|tab| tab.id == "project"));
        assert!(!default_groups_for_type(ProjectType::Game)
            .iter()
            .flat_map(|group| group.tabs.iter())
            .any(|tab| tab.id == "settings"));
        assert!(!default_groups_for_type(ProjectType::Game)
            .iter()
            .flat_map(|group| group.tabs.iter())
            .any(|tab| tab.id == "project"));
    }
}
