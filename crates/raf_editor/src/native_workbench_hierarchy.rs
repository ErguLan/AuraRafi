//! Session-local tree state and contextual navigation.
use super::*;
use crate::panels::hierarchy_model::details::HierarchyViewState;

impl NativeGameWorkbench {
    pub(crate) fn decorate_hierarchy_surface(
        &self,
        mut surface: UiSurface,
        scene: &SceneGraph,
    ) -> UiSurface {
        if let Some(id) = self.insertion_parent(scene) {
            let path = scene.node_path(id).unwrap_or_default();
            let label =
                raf_core::i18n::t("hierarchy.creating_inside", self.agent_settings.language);
            let strip = UiNode::new("hierarchy.insertion-context", UiNodeKind::Button)
                .with_text_value(format!("{label} {path}"))
                .with_tooltip_key("hierarchy.clear_insertion_parent")
                .with_layout(UiLayout::fixed(0.0, 26.0).with_width_mode(raf_ui::UiSizeMode::Fill))
                .focusable()
                .with_event(raf_ui::UiEventBinding::command(
                    raf_ui::UiEventKind::Click,
                    "hierarchy.insertion-parent:clear",
                ));
            surface.root.children.insert(2, strip);
        }
        if let Some(drag) = &self.hierarchy_drag {
            fn find<'a>(node: &'a mut UiNode, id: &str) -> Option<&'a mut UiNode> {
                if node.id == id {
                    return Some(node);
                }
                node.children.iter_mut().find_map(|n| find(n, id))
            }
            if let Some(id) = drag.hovered {
                if let Some(row) = find(&mut surface.root, &format!("hierarchy.row.{}", id.0)) {
                    let color = if drag.invalid {
                        [226, 87, 87, 255]
                    } else {
                        self.palette.tokens().accent
                    };
                    row.tooltip_key = Some(
                        if drag.invalid {
                            "hierarchy.drop_invalid"
                        } else if drag.edge == 0 {
                            "hierarchy.drop_inside"
                        } else if drag.edge < 0 {
                            "hierarchy.drop_before"
                        } else {
                            "hierarchy.drop_after"
                        }
                        .into(),
                    );
                    if drag.edge == 0 {
                        row.style.border = color;
                        row.style.border_width = 1.0;
                    } else {
                        let width = self
                            .host
                            .layout_rect(&format!("hierarchy.row.{}", id.0))
                            .map_or(160.0, |r| r.width);
                        let y = if drag.edge < 0 {
                            0.0
                        } else {
                            self.agent_settings.hierarchy_row_height - 2.0
                        };
                        row.children.push(
                            UiNode::new("hierarchy.drop-line", UiNodeKind::Panel)
                                .with_layout(UiLayout {
                                    position_mode: raf_ui::UiPositionMode::Absolute,
                                    rect: Some(raf_ui::UiRect::new(0.0, y, width, 2.0)),
                                    z_index: 10,
                                    ..Default::default()
                                })
                                .with_style(UiStyle {
                                    fill: color,
                                    ..UiStyle::transparent()
                                }),
                        );
                    }
                }
            }
        }
        surface
    }

    pub(crate) fn insertion_parent(&self, scene: &SceneGraph) -> Option<SceneNodeId> {
        let uuid = self.hierarchy_model.insertion_parent?;
        scene.iter().find_map(|(id, node)| {
            (node.uuid == uuid && !node.name.is_empty() && !node.locked).then_some(id)
        })
    }

    pub(crate) fn sync_hierarchy_state(&mut self, scene: &SceneGraph, project: Option<&Project>) {
        let path = project.and_then(|p| {
            self.inspector_sessions
                .active()
                .map(|session| p.path.join(&session.directory).join("editor_hierarchy.ron"))
        });
        if self.hierarchy_state_path != path {
            self.flush_hierarchy_state();
            let state = path
                .as_ref()
                .filter(|p| {
                    std::fs::metadata(p).is_ok_and(|metadata| metadata.len() <= 2 * 1024 * 1024)
                })
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| ron::from_str::<HierarchyViewState>(&s).ok())
                .filter(|s| s.version <= 1)
                .unwrap_or_default();
            self.hierarchy_model.restore_view(scene, &state);
            self.hierarchy_query = state.query.chars().take(256).collect();
            self.hierarchy_scroll_offset = if state.scroll.is_finite() {
                state.scroll.max(0.0)
            } else {
                0.0
            };
            self.agent_settings.hierarchy_show_hidden = state.show_hidden;
            self.hierarchy_state_path = path;
            self.hierarchy_pending_state = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if !self.project_catalog.is_pending() && self.project_catalog.error().is_none() {
            self.hierarchy_model
                .set_assets(self.project_catalog.assets());
        }
        let parent = self.insertion_parent(scene);
        self.agent_panel.borrow_mut().authoring_parent = parent.and_then(|id| scene.node_path(id));
        let mut state = self.hierarchy_model.view_state(
            scene,
            &self.hierarchy_query,
            self.hierarchy_scroll_offset,
            self.agent_settings.hierarchy_show_hidden,
        );
        state.collapsed.sort();
        if self.hierarchy_pending_state.as_ref() != Some(&state) {
            self.hierarchy_pending_state = Some(state);
            self.hierarchy_state_dirty = true;
        }
        // Commit only after a completed interaction; idle frames perform no writes.
        if self.hierarchy_drag.is_none()
            && !self
                .host
                .session()
                .interaction
                .focus
                .focused
                .as_deref()
                .is_some_and(|id| id.contains("search") || id.contains("rename"))
        {
            self.flush_hierarchy_state();
        }
    }

    pub(crate) fn flush_hierarchy_state(&mut self) {
        if !self.hierarchy_state_dirty {
            return;
        }
        let Some((path, state)) = self
            .hierarchy_state_path
            .as_ref()
            .zip(self.hierarchy_pending_state.as_ref())
        else {
            return;
        };
        let result = ron::ser::to_string(state)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                raf_core::project_store::write_atomic(path, text.as_bytes())
            });
        if let Err(error) = result {
            tracing::warn!(%error, "Hierarchy view state save failed");
        }
        self.hierarchy_state_dirty = false;
    }
}
