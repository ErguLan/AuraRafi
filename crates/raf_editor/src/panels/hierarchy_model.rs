//! Retained hierarchy projection for the editor scene tree.
//!
//! The scene graph remains the source of truth. This module only caches the
//! ordered, filtered presentation rows needed by the Hierarchy surface.

use crate::script_relationships::{ScriptLink, ScriptLinks};
use raf_core::scene::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use raf_ui::{UiRect, UiVirtualRange};
#[path = "hierarchy_details.rs"]
pub mod details;
#[path = "hierarchy_query.rs"]
mod query;
use details::{HierarchyBadge, HierarchyViewState};

pub const DEFAULT_ROW_HEIGHT: f32 = 26.0;
pub const DEFAULT_OVERSCAN_ROWS: usize = 4;

/// Single classification for hierarchy presentation and queries.
///
/// `Primitive` rows keep their geometric icon, while the remaining kinds map
/// to one stable icon/badge through `hierarchy_details`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HierarchyEntityKind {
    Folder,
    Camera,
    Light,
    MeshAsset,
    Primitive,
    Empty,
    Audio,
    Prefab,
    Script,
    /// Linked assembly instance (reusable source + per-node overrides).
    Assembly,
    /// Spawn marker component.
    Spawn,
    /// Trigger-volume rigid body.
    Trigger,
}

/// Classification used by rows, icons, badges and `type:` queries.
pub fn entity_kind(node: &SceneNode) -> HierarchyEntityKind {
    if node.game_camera.is_some() {
        HierarchyEntityKind::Camera
    } else if node.light.is_some() {
        HierarchyEntityKind::Light
    } else if node.linked_assembly.is_some() {
        HierarchyEntityKind::Assembly
    } else if node.spawn_marker.is_some() {
        HierarchyEntityKind::Spawn
    } else if node.rigid_body.enabled && node.rigid_body.is_trigger {
        HierarchyEntityKind::Trigger
    } else if node.mesh_asset.is_some() {
        HierarchyEntityKind::MeshAsset
    } else if node.is_folder {
        HierarchyEntityKind::Folder
    } else if node
        .source_asset
        .as_deref()
        .is_some_and(|path| path.ends_with(".prefab.ron"))
    {
        HierarchyEntityKind::Prefab
    } else if node.primitive != Primitive::Empty {
        HierarchyEntityKind::Primitive
    } else if node.audio_source.enabled {
        HierarchyEntityKind::Audio
    } else {
        HierarchyEntityKind::Empty
    }
}

/// Keyboard row movement driven by the host. Left/right expansion is resolved
/// by the host because it needs the live scene parent/child relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyNavigation {
    Up,
    Down,
    Home,
    End,
}

/// Transient rectangular selection gesture.
///
/// This is view state, never scene state: ending the gesture only emits normal
/// selection commands for the rows it intersected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HierarchyBoxSelection {
    pub origin: [f32; 2],
    pub current: [f32; 2],
    pub additive: bool,
    /// Set once the retained host has crossed its drag threshold.
    pub active: bool,
}

impl HierarchyBoxSelection {
    pub fn rect(self) -> UiRect {
        let x = self.origin[0].min(self.current[0]);
        let y = self.origin[1].min(self.current[1]);
        UiRect::new(
            x,
            y,
            (self.origin[0] - self.current[0]).abs(),
            (self.origin[1] - self.current[1]).abs(),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyRow {
    pub id: SceneNodeId,
    pub depth: usize,
    pub name: String,
    pub primitive: Primitive,
    /// Central row type consumed by icons, badges and queries.
    pub kind: HierarchyEntityKind,
    pub visible: bool,
    pub locked: bool,
    pub has_children: bool,
    pub expanded: bool,
    /// Presentation-only child; id is its owning entity, not a selectable scene id.
    pub script_link: Option<(usize, ScriptLink)>,
    /// Cached attachment flag for the optional script indicator.
    pub has_scripts: bool,
    pub badges: Vec<HierarchyBadge>,
    pub insertion_parent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyView {
    pub total_rows: usize,
    pub entity_rows: usize,
    pub visible_start: usize,
    pub top_spacer: f32,
    pub bottom_spacer: f32,
    pub rows: Vec<HierarchyRow>,
    /// Local box-selection rectangle. It is part of the projection so the
    /// retained document cache invalidates while the gesture moves.
    pub box_selection: Option<UiRect>,
}

#[derive(Debug)]
pub struct HierarchyModel {
    collapsed: std::collections::HashSet<SceneNodeId>,
    rows: Vec<HierarchyRow>,
    query: String,
    show_hidden: bool,
    needs_rebuild: bool,
    script_links: ScriptLinks,
    entity_rows: usize,
    assets: Option<std::collections::HashSet<String>>,
    badge_index: std::collections::HashMap<SceneNodeId, Vec<HierarchyBadge>>,
    box_selection: Option<HierarchyBoxSelection>,
    pub insertion_parent: Option<uuid::Uuid>,
}

impl Default for HierarchyModel {
    fn default() -> Self {
        Self {
            collapsed: std::collections::HashSet::new(),
            rows: Vec::new(),
            query: String::new(),
            show_hidden: false,
            needs_rebuild: true,
            script_links: ScriptLinks::new(),
            entity_rows: 0,
            assets: None,
            badge_index: Default::default(),
            box_selection: None,
            insertion_parent: None,
        }
    }
}

impl HierarchyModel {
    pub fn refresh(
        &mut self,
        scene: &SceneGraph,
        query: &str,
        show_hidden: bool,
        scroll_offset: f32,
        viewport_height: f32,
        row_height: f32,
    ) -> HierarchyView {
        let query = query.trim().to_ascii_lowercase();
        if self.needs_rebuild || self.query != query || self.show_hidden != show_hidden {
            self.query = query.clone();
            self.show_hidden = show_hidden;
            self.rebuild(scene);
        }

        // A retained surface can deliver one input event before the next
        // layout pass has measured its viewport. Keep the virtual projection
        // valid in that transition instead of allowing an old scroll offset
        // to point past the cached rows and paint an empty tree.
        let row_height = if row_height.is_finite() {
            row_height.max(1.0)
        } else {
            DEFAULT_ROW_HEIGHT
        };
        let viewport_height = if viewport_height.is_finite() {
            viewport_height.max(row_height)
        } else {
            row_height
        };
        let max_scroll_offset = (self.rows.len() as f32 * row_height - viewport_height).max(0.0);
        let scroll_offset = if scroll_offset.is_finite() {
            scroll_offset.clamp(0.0, max_scroll_offset)
        } else {
            0.0
        };
        let range = UiVirtualRange::for_vertical_list(
            self.rows.len(),
            scroll_offset,
            viewport_height,
            row_height,
            DEFAULT_OVERSCAN_ROWS,
        );
        let rows = self.rows[range.start..range.end].to_vec();
        HierarchyView {
            total_rows: self.rows.len(),
            entity_rows: self.entity_rows,
            visible_start: range.start,
            top_spacer: range.start as f32 * row_height,
            bottom_spacer: self.rows.len().saturating_sub(range.end) as f32 * row_height,
            rows,
            box_selection: self.box_selection_rect(),
        }
    }

    pub fn set_assets(&mut self, assets: &[String]) {
        let paths = assets
            .iter()
            .map(|p| {
                p.replace('\\', "/")
                    .trim_start_matches("assets/")
                    .to_string()
            })
            .collect();
        if self.assets.as_ref() != Some(&paths) {
            self.assets = Some(paths);
            self.invalidate();
        }
    }
    pub fn view_state(
        &self,
        scene: &SceneGraph,
        query: &str,
        scroll: f32,
        show_hidden: bool,
    ) -> HierarchyViewState {
        HierarchyViewState {
            version: 1,
            collapsed: self
                .collapsed
                .iter()
                .filter_map(|id| scene.get(*id).map(|node| node.uuid))
                .collect(),
            query: query.to_string(),
            scroll: scroll.max(0.0),
            show_hidden,
            insertion_parent: self.insertion_parent,
        }
    }
    pub fn restore_view(&mut self, scene: &SceneGraph, state: &HierarchyViewState) {
        self.collapsed = scene
            .iter()
            .filter_map(|(id, node)| state.collapsed.contains(&node.uuid).then_some(id))
            .collect();
        self.insertion_parent = state.insertion_parent.filter(|uuid| {
            scene
                .iter()
                .any(|(_, n)| n.uuid == *uuid && !n.name.is_empty())
        });
        // A restored session never inherits a half-finished pointer gesture.
        self.cancel_box_selection();
        self.invalidate();
    }

    pub fn row_ids(&self) -> impl Iterator<Item = SceneNodeId> + '_ {
        self.rows
            .iter()
            .filter(|row| row.script_link.is_none())
            .map(|row| row.id)
    }

    pub fn row_position(&self, id: SceneNodeId) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.script_link.is_none() && row.id == id)
    }

    /// Ordered selectable rows. Script-link presentation rows are inert and
    /// must never be reached by keyboard navigation or range selection.
    pub fn entity_row_ids(&self) -> Vec<SceneNodeId> {
        self.row_ids().collect()
    }

    /// Resolves up/down/home/end against the retained projection. The result
    /// is clamped at both ends so navigation is stable at the list boundaries.
    pub fn navigate(
        &self,
        current: Option<SceneNodeId>,
        navigation: HierarchyNavigation,
    ) -> Option<SceneNodeId> {
        let rows = self.entity_row_ids();
        if rows.is_empty() {
            return None;
        }
        let current = current
            .filter(|id| rows.contains(id))
            .or_else(|| rows.first().copied())?;
        let index = rows.iter().position(|id| *id == current).unwrap_or(0);
        let target = match navigation {
            HierarchyNavigation::Up => index.saturating_sub(1),
            HierarchyNavigation::Down => (index + 1).min(rows.len() - 1),
            HierarchyNavigation::Home => 0,
            HierarchyNavigation::End => rows.len() - 1,
        };
        rows.get(target).copied()
    }

    pub fn is_row_expanded(&self, id: SceneNodeId) -> bool {
        self.query.is_empty() && !self.collapsed.contains(&id)
    }

    pub fn row_has_children(&self, id: SceneNodeId) -> bool {
        self.rows
            .iter()
            .find(|row| row.script_link.is_none() && row.id == id)
            .is_some_and(|row| row.has_children)
    }

    /// Arms a box selection on pointer press. [`Self::start_box_selection`]
    /// activates painting once the retained host crosses its drag threshold.
    pub fn press_box_selection(&mut self, origin: [f32; 2], additive: bool) {
        self.box_selection = Some(HierarchyBoxSelection {
            origin,
            current: origin,
            additive,
            active: false,
        });
    }

    pub fn start_box_selection(&mut self, current: [f32; 2]) -> Option<UiRect> {
        let selection = self.box_selection.as_mut()?;
        selection.current = current;
        selection.active = true;
        self.box_selection_rect()
    }

    pub fn update_box_selection(&mut self, current: [f32; 2]) -> Option<UiRect> {
        let selection = self.box_selection.as_mut()?;
        selection.current = current;
        self.box_selection_rect()
    }

    pub fn box_selection_rect(&self) -> Option<UiRect> {
        let selection = self.box_selection.filter(|selection| selection.active)?;
        let rect = selection.rect();
        (!rect.is_empty()).then_some(rect)
    }

    /// Ends the gesture and hands the final geometry to the host, which maps
    /// it to row hit rectangles before emitting selection commands.
    pub fn take_box_selection(&mut self) -> Option<HierarchyBoxSelection> {
        self.box_selection.take()
    }

    pub fn cancel_box_selection(&mut self) {
        self.box_selection = None;
    }

    pub fn set_script_links(&mut self, links: ScriptLinks) -> bool {
        if self.script_links == links {
            return false;
        }
        self.script_links = links;
        self.invalidate();
        true
    }

    pub fn script_link(&self, owner: SceneNodeId, index: usize) -> Option<&ScriptLink> {
        self.script_links.get(&owner)?.get(index)
    }

    fn script_matches(&self, id: SceneNodeId) -> bool {
        self.script_links.get(&id).is_some_and(|links| {
            links
                .iter()
                .any(|link| link.path.to_ascii_lowercase().contains(&self.query))
        })
    }

    pub fn toggle_expanded(&mut self, id: SceneNodeId) {
        if !self.collapsed.insert(id) {
            self.collapsed.remove(&id);
        }
        self.needs_rebuild = true;
    }

    pub fn expand_parent_chain(&mut self, scene: &SceneGraph, id: SceneNodeId) {
        let mut current = scene.get(id).and_then(|node| node.parent);
        while let Some(parent) = current {
            self.collapsed.remove(&parent);
            current = scene.get(parent).and_then(|node| node.parent);
        }
        self.needs_rebuild = true;
    }

    pub fn clear_removed_state(&mut self, scene: &SceneGraph) {
        self.collapsed
            .retain(|id| scene.get(*id).is_some_and(|node| !node.name.is_empty()));
    }

    pub fn invalidate(&mut self) {
        self.needs_rebuild = true;
    }

    pub fn expand_recursive(&mut self, scene: &SceneGraph, id: SceneNodeId, expanded: bool) {
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if expanded {
                self.collapsed.remove(&current);
            } else {
                self.collapsed.insert(current);
            }
            if let Some(node) = scene.get(current) {
                stack.extend(node.children.iter().copied());
            }
        }
        self.needs_rebuild = true;
    }

    fn rebuild(&mut self, scene: &SceneGraph) {
        self.rows.clear();
        self.clear_removed_state(scene);
        self.badge_index = details::badge_index(scene, self.assets.as_ref());
        for &root in scene.roots() {
            self.append_node(scene, root, 0, false);
        }
        self.entity_rows = self
            .rows
            .iter()
            .filter(|row| row.script_link.is_none())
            .count();
        self.needs_rebuild = false;
    }

    fn append_node(
        &mut self,
        scene: &SceneGraph,
        id: SceneNodeId,
        depth: usize,
        ancestor_match: bool,
    ) -> bool {
        let Some(node) = scene.get(id) else {
            return false;
        };
        if node.name.is_empty() {
            return false;
        }

        let name_matches = self.query.is_empty()
            || query::matches_with_assets(scene, id, &self.query, self.assets.as_ref());
        let descendant_matches = !self.query.is_empty()
            && (self.script_matches(id)
                || node
                    .children
                    .iter()
                    .any(|child| self.matches_descendant(scene, *child)));
        let visible_by_filter = self.query.is_empty()
            || name_matches
            || descendant_matches
            || (ancestor_match && !self.query.contains(':'));
        let visible_by_hidden = self.show_hidden
            || node.visible
            || self.has_visible_descendant(scene, id)
            || !self.query.is_empty();
        if !visible_by_filter || !visible_by_hidden {
            return false;
        }

        let expanded = self.query.is_empty() && !self.collapsed.contains(&id);
        self.rows.push(HierarchyRow {
            id,
            depth,
            name: node.name.clone(),
            primitive: node.primitive,
            kind: entity_kind(node),
            visible: node.visible,
            locked: node.locked,
            has_children: !node.children.is_empty()
                || self
                    .script_links
                    .get(&id)
                    .is_some_and(|links| !links.is_empty()),
            expanded,
            script_link: None,
            has_scripts: !node.scripts.is_empty(),
            badges: self.badge_index.get(&id).cloned().unwrap_or_default(),
            insertion_parent: self.insertion_parent == Some(node.uuid),
        });

        if expanded || !self.query.is_empty() {
            if let Some(links) = self.script_links.get(&id) {
                for (index, link) in links.iter().enumerate() {
                    if self.query.is_empty()
                        || name_matches
                        || ancestor_match
                        || link.path.to_ascii_lowercase().contains(&self.query)
                    {
                        self.rows.push(HierarchyRow {
                            id,
                            depth: depth + 1,
                            name: crate::script_support::script_file_name(&link.path),
                            primitive: Primitive::Empty,
                            kind: HierarchyEntityKind::Script,
                            visible: node.visible,
                            locked: false,
                            has_children: false,
                            expanded: false,
                            script_link: Some((index, link.clone())),
                            has_scripts: false,
                            badges: Vec::new(),
                            insertion_parent: false,
                        });
                    }
                }
            }
            for &child in &node.children {
                self.append_node(scene, child, depth + 1, name_matches || ancestor_match);
            }
        }
        true
    }

    fn matches_descendant(&self, scene: &SceneGraph, id: SceneNodeId) -> bool {
        let Some(node) = scene.get(id) else {
            return false;
        };
        if node.name.is_empty() {
            return false;
        }
        if self.show_hidden || node.visible || !self.query.is_empty() {
            if query::matches_with_assets(scene, id, &self.query, self.assets.as_ref())
                || self.script_matches(id)
            {
                return true;
            }
        }
        node.children
            .iter()
            .any(|child| self.matches_descendant(scene, *child))
    }

    fn has_visible_descendant(&self, scene: &SceneGraph, id: SceneNodeId) -> bool {
        let Some(node) = scene.get(id) else {
            return false;
        };
        node.children.iter().any(|child| {
            scene.get(*child).is_some_and(|child_node| {
                child_node.visible || self.has_visible_descendant(scene, *child)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authoring_hierarchy_restores_uuid_state_and_hidden_field_queries() {
        let mut scene = SceneGraph::new();
        let parent = scene.add_root("Group");
        let child = scene.add_child_with_primitive(parent, "Hidden", Primitive::Capsule);
        scene.get_mut(child).unwrap().visible = false;
        let mut model = HierarchyModel::default();
        model.insertion_parent = Some(scene.get(parent).unwrap().uuid);
        model.toggle_expanded(parent);
        let state = model.view_state(&scene, "type:capsule hidden:true", 24.0, false);
        let encoded = ron::to_string(&state).unwrap();
        let mut restored = HierarchyModel::default();
        restored.restore_view(&scene, &ron::from_str(&encoded).unwrap());
        assert_eq!(restored.insertion_parent, model.insertion_parent);
        let view = restored.refresh(&scene, &state.query, false, 0.0, 200.0, 26.0);
        assert_eq!(view.entity_rows, 2, "hidden result retains its ancestor");
        let id = scene.add_root("Another");
        scene.get_mut(id).unwrap().locked = true;
        let view = restored.refresh(&scene, "type:capsule locked:true", false, 0.0, 200.0, 26.0);
        assert_eq!(view.entity_rows, 0, "structured filters combine using AND");
    }

    #[test]
    fn script_children_are_virtual_searchable_collapsible_and_excluded_from_selection() {
        let mut scene = SceneGraph::new();
        let first = scene.add_root("First");
        let second = scene.add_root("Second");
        let original = scene.document_revision();
        let mut model = HierarchyModel::default();
        let link = ScriptLink {
            path: "scripts/power.rhai".into(),
            kind: crate::script_relationships::ScriptLinkKind::Reference,
        };
        model.set_script_links([(first, vec![link.clone()])].into_iter().collect());
        let view = model.refresh(&scene, "", false, 0.0, 200.0, 26.0);
        assert_eq!(view.total_rows, 3);
        assert_eq!(view.entity_rows, 2);
        assert_eq!(model.row_ids().collect::<Vec<_>>(), vec![first, second]);
        assert_eq!(model.row_position(second), Some(2));
        assert!(scene.get(first).unwrap().children.is_empty());
        assert_eq!(scene.document_revision(), original);
        model.toggle_expanded(first);
        assert_eq!(
            model
                .refresh(&scene, "", false, 0.0, 200.0, 26.0)
                .total_rows,
            2
        );
        assert_eq!(
            model
                .refresh(&scene, "power.rhai", false, 0.0, 200.0, 26.0)
                .total_rows,
            2
        );
        model.set_script_links(ScriptLinks::new());
        assert_eq!(
            model
                .refresh(&scene, "", false, 0.0, 200.0, 26.0)
                .total_rows,
            2
        );
    }

    #[test]
    fn projection_keeps_hierarchy_order_and_virtualizes_rows() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("World");
        for index in 0..20 {
            scene.add_child(root, &format!("Entity {index}"));
        }
        let mut model = HierarchyModel::default();
        let view = model.refresh(&scene, "", false, 130.0, 52.0, 26.0);

        assert_eq!(view.total_rows, 21);
        assert!(view.rows.len() < view.total_rows);
        assert!(view.top_spacer > 0.0);
        assert_eq!(
            view.rows.first().map(|row| row.name.as_str()),
            Some("Entity 0")
        );
    }

    #[test]
    fn projection_clamps_stale_scroll_before_slicing_rows() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("World");
        for index in 0..20 {
            scene.add_child(root, &format!("Entity {index}"));
        }

        let mut model = HierarchyModel::default();
        let view = model.refresh(&scene, "", false, f32::MAX, 0.0, 26.0);

        assert_eq!(view.total_rows, 21);
        assert!(!view.rows.is_empty());
        assert_eq!(
            view.rows.last().map(|row| row.name.as_str()),
            Some("Entity 19")
        );
    }

    #[test]
    fn filtering_auto_expands_matching_descendants() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("World");
        scene.add_child(root, "Camera");
        scene.add_child(root, "Player");
        let mut model = HierarchyModel::default();
        let view = model.refresh(&scene, "player", false, 0.0, 160.0, 26.0);

        assert_eq!(view.total_rows, 2);
        assert_eq!(view.rows[0].name, "World");
        assert_eq!(view.rows[1].name, "Player");
    }

    #[test]
    fn hidden_parent_stays_visible_when_it_contains_visible_content() {
        let mut scene = SceneGraph::new();
        let parent = scene.add_root("Hidden group");
        scene.get_mut(parent).unwrap().visible = false;
        scene.add_child(parent, "Visible child");

        let mut model = HierarchyModel::default();
        let view = model.refresh(&scene, "", false, 0.0, 160.0, 26.0);

        assert_eq!(view.total_rows, 2);
        assert_eq!(view.rows[0].name, "Hidden group");
        assert_eq!(view.rows[1].name, "Visible child");
    }

    #[test]
    fn recursive_expand_and_collapse_are_retained() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("Root");
        let child = scene.add_child(root, "Child");
        scene.add_child(child, "Leaf");

        let mut model = HierarchyModel::default();
        model.expand_recursive(&scene, root, false);
        let collapsed = model.refresh(&scene, "", false, 0.0, 160.0, 26.0);
        assert_eq!(collapsed.total_rows, 1);

        model.expand_recursive(&scene, root, true);
        let expanded = model.refresh(&scene, "", false, 0.0, 160.0, 26.0);
        assert_eq!(expanded.total_rows, 3);
    }

    #[test]
    fn rows_share_one_kind_registry_for_lights_meshes_and_cameras() {
        use raf_core::scene::{MeshAssetRef, SceneLight};

        let mut scene = SceneGraph::new();
        let light = scene.add_root("Key light");
        scene.get_mut(light).unwrap().light = Some(SceneLight::default());
        let mesh = scene.add_root("Chair");
        scene.get_mut(mesh).unwrap().mesh_asset = Some(MeshAssetRef::new("models/chair.glb"));
        let camera = scene.add_root("Camera");
        scene.get_mut(camera).unwrap().game_camera = Some(Default::default());
        let folder = scene.add_root("Group");
        scene.get_mut(folder).unwrap().is_folder = true;

        let mut model = HierarchyModel::default();
        let view = model.refresh(&scene, "", false, 0.0, 400.0, 26.0);
        let kind = |id: SceneNodeId| {
            view.rows
                .iter()
                .find(|row| row.id == id)
                .map(|row| row.kind)
                .unwrap()
        };
        assert_eq!(kind(light), HierarchyEntityKind::Light);
        assert_eq!(kind(mesh), HierarchyEntityKind::MeshAsset);
        assert_eq!(kind(camera), HierarchyEntityKind::Camera);
        assert_eq!(kind(folder), HierarchyEntityKind::Folder);
    }

    #[test]
    fn keyboard_navigation_moves_over_selectable_rows_and_clamps_edges() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("Root");
        let child = scene.add_child(root, "Child");
        let leaf = scene.add_child(child, "Leaf");
        let mut model = HierarchyModel::default();
        model.refresh(&scene, "", false, 0.0, 400.0, 26.0);

        assert_eq!(
            model.navigate(Some(root), HierarchyNavigation::Down),
            Some(child)
        );
        assert_eq!(
            model.navigate(Some(child), HierarchyNavigation::Up),
            Some(root)
        );
        assert_eq!(
            model.navigate(Some(root), HierarchyNavigation::Up),
            Some(root),
            "up clamps at the first row"
        );
        assert_eq!(
            model.navigate(Some(leaf), HierarchyNavigation::Down),
            Some(leaf),
            "down clamps at the last row"
        );
        assert_eq!(
            model.navigate(Some(child), HierarchyNavigation::Home),
            Some(root)
        );
        assert_eq!(
            model.navigate(Some(child), HierarchyNavigation::End),
            Some(leaf)
        );
        assert_eq!(
            model.navigate(None, HierarchyNavigation::Down),
            Some(child),
            "an empty selection starts at the first row before moving"
        );

        // Left/right expansion uses the same retained projection state.
        assert!(model.row_has_children(root));
        assert!(model.is_row_expanded(root));
        model.toggle_expanded(root);
        assert!(!model.is_row_expanded(root));
        model.refresh(&scene, "", false, 0.0, 400.0, 26.0);
        assert_eq!(
            model.row_position(child),
            None,
            "collapsed children are not rows"
        );
        assert_eq!(
            model.navigate(Some(root), HierarchyNavigation::Down),
            Some(root),
            "down stays on the last visible row after collapsing"
        );
    }

    #[test]
    fn box_selection_rect_is_transient_projection_state() {
        let mut scene = SceneGraph::new();
        scene.add_root("Root");
        let mut model = HierarchyModel::default();
        model.refresh(&scene, "", false, 0.0, 400.0, 26.0);

        model.press_box_selection([40.0, 80.0], false);
        assert_eq!(
            model.box_selection_rect(),
            None,
            "press alone paints nothing"
        );
        assert!(model.start_box_selection([20.0, 40.0]).is_some());
        let view = model.refresh(&scene, "", false, 0.0, 400.0, 26.0);
        assert_eq!(
            view.box_selection,
            Some(raf_ui::UiRect::new(20.0, 40.0, 20.0, 40.0))
        );
        let selection = model.take_box_selection().unwrap();
        assert!(!selection.additive);
        assert_eq!(model.box_selection_rect(), None);
        assert_eq!(
            model
                .refresh(&scene, "", false, 0.0, 400.0, 26.0)
                .box_selection,
            None
        );
    }
}
