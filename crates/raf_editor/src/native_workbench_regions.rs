//! Authored region reuse, separate from renderer layout/presentation caches.
use super::*;
use crate::panels::hierarchy_model::HierarchyView;

#[derive(PartialEq)]
pub(super) struct HierarchyDocumentKey {
    view: HierarchyView,
    selected: Vec<SceneNodeId>,
    size: [f32; 2],
    metrics: [f32; 2],
    flags: [bool; 4],
    palette: StudioUiPalette,
    bookmarks: [bool; 3],
    scene_revision: u64,
    insertion_parent: Option<uuid::Uuid>,
}

impl NativeGameWorkbench {
    pub(super) fn build_hierarchy_document(
        &mut self,
        view: &HierarchyView,
        selected: &[SceneNodeId],
        scene: &SceneGraph,
        left: EditorRect,
        can_paste: bool,
        search: &crate::project_catalog::ProjectCatalogResults,
    ) -> UiSurface {
        let started = std::time::Instant::now();
        let key = HierarchyDocumentKey {
            view: view.clone(),
            selected: selected.to_vec(),
            size: [left.width, left.height],
            metrics: [
                self.agent_settings.hierarchy_row_height.max(1.0),
                self.agent_settings.hierarchy_indent_width,
            ],
            flags: [
                self.agent_settings.hierarchy_show_icons,
                self.agent_settings.hierarchy_show_visibility,
                self.agent_settings.hierarchy_show_locked,
                can_paste,
            ],
            palette: self.palette,
            bookmarks: self.hierarchy_bookmarks,
            scene_revision: scene.document_revision(),
            insertion_parent: self.hierarchy_model.insertion_parent,
        };
        let reusable = self.hierarchy_active_tab == "hierarchy"
            && self.hierarchy_drag.is_none()
            && self.hierarchy_renaming.is_none()
            && self.hierarchy_menu_target.is_none()
            && !self.hierarchy_empty_menu_open
            && !self.hierarchy_primitive_menu_open;
        if reusable {
            if let Some((_, surface)) = self
                .hierarchy_document_cache
                .as_ref()
                .filter(|(previous, _)| *previous == key)
            {
                let surface = surface.clone();
                self.host.record_document_region(
                    "editor.hierarchy",
                    started.elapsed(),
                    view.rows.len(),
                    true,
                );
                return surface;
            }
        }
        let menu_label = self
            .hierarchy_menu_target
            .and_then(|(id, _)| scene.get(id))
            .map(|node| node.name.as_str());
        let surface = self.decorate_hierarchy_surface(
            build_hierarchy_surface(
                self.palette,
                view,
                selected,
                self.hierarchy_renaming
                    .as_ref()
                    .map(|(id, value)| (*id, value.as_str())),
                self.hierarchy_menu_target,
                self.hierarchy_empty_menu_open,
                self.hierarchy_primitive_menu_open,
                menu_label,
                self.hierarchy_menu_position,
                [left.width, left.height],
                key.metrics[0],
                key.metrics[1],
                key.flags[0],
                key.flags[1],
                key.flags[2],
                1.0,
                self.hierarchy_drag
                    .as_ref()
                    .map(|drag| (drag.label.as_str(), drag.pointer)),
                self.hierarchy_drag.as_ref().and_then(|drag| drag.target),
                None,
                can_paste,
                left.width < 540.0,
                &self.hierarchy_active_tab,
                self.hierarchy_bookmarks,
                Some(search),
            ),
            scene,
        );
        if reusable {
            self.hierarchy_document_cache = Some((key, surface.clone()));
        }
        self.host.record_document_region(
            "editor.hierarchy",
            started.elapsed(),
            view.rows.len(),
            false,
        );
        surface
    }
}
