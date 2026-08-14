//! Retained hierarchy projection for the editor scene tree.
//!
//! The scene graph remains the source of truth. This module only caches the
//! ordered, filtered presentation rows needed by the Hierarchy surface.

use raf_core::scene::{Primitive, SceneGraph, SceneNodeId};
use raf_ui::UiVirtualRange;

pub const DEFAULT_ROW_HEIGHT: f32 = 26.0;
pub const DEFAULT_OVERSCAN_ROWS: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyRow {
    pub id: SceneNodeId,
    pub depth: usize,
    pub name: String,
    pub primitive: Primitive,
    pub is_folder: bool,
    pub visible: bool,
    pub locked: bool,
    pub has_children: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyView {
    pub total_rows: usize,
    pub visible_start: usize,
    pub top_spacer: f32,
    pub bottom_spacer: f32,
    pub rows: Vec<HierarchyRow>,
}

#[derive(Debug)]
pub struct HierarchyModel {
    collapsed: std::collections::HashSet<SceneNodeId>,
    rows: Vec<HierarchyRow>,
    query: String,
    show_hidden: bool,
    needs_rebuild: bool,
}

impl Default for HierarchyModel {
    fn default() -> Self {
        Self {
            collapsed: std::collections::HashSet::new(),
            rows: Vec::new(),
            query: String::new(),
            show_hidden: false,
            needs_rebuild: true,
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
            visible_start: range.start,
            top_spacer: range.start as f32 * row_height,
            bottom_spacer: self.rows.len().saturating_sub(range.end) as f32 * row_height,
            rows,
        }
    }

    pub fn row_ids(&self) -> impl Iterator<Item = SceneNodeId> + '_ {
        self.rows.iter().map(|row| row.id)
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
        for &root in scene.roots() {
            self.append_node(scene, root, 0, false);
        }
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

        let name_matches =
            self.query.is_empty() || node.name.to_ascii_lowercase().contains(self.query.as_str());
        let descendant_matches = !self.query.is_empty()
            && node
                .children
                .iter()
                .any(|child| self.matches_descendant(scene, *child));
        let visible_by_filter =
            self.query.is_empty() || name_matches || descendant_matches || ancestor_match;
        let visible_by_hidden =
            self.show_hidden || node.visible || self.has_visible_descendant(scene, id);
        if !visible_by_filter || !visible_by_hidden {
            return false;
        }

        let expanded = self.query.is_empty() && !self.collapsed.contains(&id);
        self.rows.push(HierarchyRow {
            id,
            depth,
            name: node.name.clone(),
            primitive: node.primitive,
            is_folder: node.is_folder,
            visible: node.visible,
            locked: node.locked,
            has_children: !node.children.is_empty(),
            expanded,
        });

        if expanded || !self.query.is_empty() {
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
        if self.show_hidden || node.visible {
            if node.name.to_ascii_lowercase().contains(self.query.as_str()) {
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
}
