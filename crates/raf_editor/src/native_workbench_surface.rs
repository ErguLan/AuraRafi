//! Native workbench compositor.
//!
//! The shell state and input lifecycle stay in `native_workbench.rs`; this
//! module owns only retained surface composition so the compositor does not
//! become a second state/controller implementation.

use super::*;
use crate::panels::agent_surface::build_agent_surface;
use crate::panels::console_surface::build_console_surface;
use crate::panels::electronics_context_menu_surface::build_electronics_context_menu_surface;
use crate::panels::electronics_surface::build_electronics_analysis_surface;
use crate::panels::project_surface::build_project_surface;

impl NativeGameWorkbench {
    pub(super) fn build_surface(
        &mut self,
        layout: EditorFrameLayout,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        can_undo: bool,
        can_redo: bool,
        can_paste: bool,
        node_graph: &NodeGraph,
        selected_graph_node: Option<NodeId>,
        project: Option<&Project>,
        electronics: Option<&NativeElectronicsEditor>,
    ) -> UiSurface {
        let mut roots = Vec::new();
        let mut rules = Vec::new();
        let search_results = self.project_catalog.search_results(&self.hierarchy_query);
        let hierarchy_menu_target = self.hierarchy_menu_target;
        let hierarchy_menu_label = hierarchy_menu_target
            .and_then(|(id, _)| scene.get(id))
            .map(|node| node.name.as_str());

        let mut append = |surface: UiSurface, rect: EditorRect| {
            let width = rect.width.max(0.0);
            let height = rect.height.max(0.0);
            let root_id = format!("{}.host", surface.id);
            let wrapper = UiNode::new(root_id, UiNodeKind::Root)
                .with_layout(raf_ui::UiLayout {
                    overflow: raf_ui::UiOverflow::Clip,
                    ..UiLayout::absolute(UiRect::new(rect.x, rect.y, width, height))
                })
                .with_style(UiStyle::transparent())
                .with_child(surface.root);
            roots.push(wrapper);
            rules.extend(surface.style_sheet.rules);
        };

        append(
            build_application_bar_surface(
                self.palette,
                &self.project_name,
                self.project_type,
                self.open_menu.as_deref(),
            ),
            layout.application_bar,
        );

        let menu = build_application_menu(ApplicationMenuState {
            project_type: self.project_type,
            active_view: ApplicationView::Scene,
            grid_visible: self.toolbar_state.grid_visible,
            hierarchy_visible: layout.left_panel.is_some(),
            inspector_visible: layout.right_panel.is_some(),
            undo_available: can_undo,
            redo_available: can_redo,
            selection_available: !selected.is_empty(),
            select_all_available: !scene.roots().is_empty(),
            copy_available: !selected.is_empty(),
            paste_available: can_paste,
        });
        if let Some(open_menu) = self.open_menu.as_deref() {
            if let Some(menu) = menu.menus.iter().find(|menu| menu.id == open_menu) {
                let menu_x = match open_menu {
                    "file" => 238.0,
                    "edit" => 294.0,
                    "view" => 350.0,
                    "project" => 410.0,
                    "help" => 486.0,
                    _ => 238.0,
                };
                append(
                    build_application_menu_popup_surface_with_submenu(
                        self.palette,
                        menu,
                        self.open_submenu.as_deref(),
                    ),
                    EditorRect::new(
                        menu_x,
                        layout.application_bar.height - (1.0 - self.menu_motion.value()) * 8.0,
                        APPLICATION_MENU_POPUP_WIDTH,
                        application_menu_popup_height(menu),
                    ),
                );
                if open_menu == "help" {
                    if let Some(submenu) = menu.items.iter().find_map(|item| match item {
                        raf_ui::UiMenuItem::Submenu(submenu)
                            if self.open_submenu.as_deref() == Some(submenu.id.as_str()) =>
                        {
                            Some(submenu)
                        }
                        _ => None,
                    }) {
                        append(
                            build_application_menu_popup_surface(self.palette, submenu),
                            EditorRect::new(
                                menu_x + APPLICATION_MENU_POPUP_WIDTH - 1.0,
                                layout.application_bar.height
                                    - (1.0 - self.menu_motion.value()) * 8.0,
                                APPLICATION_MENU_POPUP_WIDTH,
                                application_menu_popup_height(submenu),
                            ),
                        );
                    }
                }
            }
        }

        if let Some(left) = layout.left_panel {
            if self.project_type == ProjectType::Electronics {
                let data = electronics_model::ElectronicsWorkbenchData::from_editor(electronics);
                append(
                    build_electronics_navigator_surface(
                        self.palette,
                        &self.electronics_navigator_tab,
                        &self.electronics_library_query,
                        &data.schematic_name,
                        data.counts,
                        &data.components,
                        &data.wires,
                        &data.library,
                    ),
                    left,
                );
            } else {
                let hierarchy_row_height = self.agent_settings.hierarchy_row_height.max(1.0);
                let hierarchy_tree_height = self.hierarchy_tree_viewport_height(left);
                let view = self.hierarchy_model.refresh(
                    scene,
                    &self.hierarchy_query,
                    self.agent_settings.hierarchy_show_hidden,
                    self.hierarchy_scroll_offset,
                    hierarchy_tree_height,
                    hierarchy_row_height,
                );
                append(
                    build_hierarchy_surface(
                        self.palette,
                        &view,
                        selected,
                        self.hierarchy_renaming
                            .as_ref()
                            .map(|(id, value)| (*id, value.as_str())),
                        hierarchy_menu_target,
                        self.hierarchy_empty_menu_open,
                        self.hierarchy_primitive_menu_open,
                        hierarchy_menu_label,
                        self.hierarchy_menu_position,
                        [left.width, left.height],
                        hierarchy_row_height,
                        self.agent_settings.hierarchy_indent_width,
                        self.agent_settings.hierarchy_show_icons,
                        self.agent_settings.hierarchy_show_visibility,
                        self.agent_settings.hierarchy_show_locked,
                        1.0,
                        self.hierarchy_drag
                            .as_ref()
                            .map(|drag| (drag.label.as_str(), drag.pointer)),
                        self.hierarchy_drag.as_ref().and_then(|drag| drag.target),
                        None,
                        can_paste,
                        left.width < 444.0,
                        &self.hierarchy_active_tab,
                        self.hierarchy_bookmarks,
                        Some(&search_results),
                    ),
                    left,
                );
                append(
                    build_hierarchy_context_overlay_surface(
                        self.palette,
                        hierarchy_menu_target,
                        self.hierarchy_empty_menu_open,
                        self.hierarchy_primitive_menu_open,
                        hierarchy_menu_label,
                        self.hierarchy_menu_position,
                        [layout.window.width, layout.window.height],
                        can_paste,
                    ),
                    layout.window,
                );
            }
        }

        append(
            if self.project_type == ProjectType::Electronics {
                build_electronics_toolbar_surface(
                    self.palette,
                    electronics.map_or(
                        raf_electronics::CadSurfaceKind::Schematic,
                        NativeElectronicsEditor::active_surface,
                    ),
                    electronics.map_or(ElectronicsTool::Select, NativeElectronicsEditor::tool),
                    electronics.is_none_or(NativeElectronicsEditor::grid_visible),
                    electronics.is_none_or(NativeElectronicsEditor::labels_visible),
                    electronics.is_some_and(NativeElectronicsEditor::can_undo),
                    electronics.is_some_and(NativeElectronicsEditor::can_redo),
                    electronics.is_some_and(|editor| editor.selection().is_some()),
                    electronics.is_some_and(NativeElectronicsEditor::can_rotate_selection),
                )
            } else {
                build_viewport_toolbar_surface(self.palette, self.toolbar_state)
            },
            layout.canvas,
        );
        if self.project_type == ProjectType::Electronics {
            if let Some(position) =
                electronics.and_then(NativeElectronicsEditor::context_menu_position)
            {
                let has_selection = electronics.is_some_and(|editor| editor.selection().is_some());
                let can_duplicate = electronics.is_some_and(|editor| {
                    editor.selection().is_some_and(|selection| {
                        matches!(
                            selection.kind,
                            crate::electronics_controller::ElectronicsSelectionKind::Component
                                | crate::electronics_controller::ElectronicsSelectionKind::Pin
                        )
                    })
                });
                let can_route = electronics.is_some_and(|editor| {
                    editor.active_surface() == raf_electronics::CadSurfaceKind::Pcb
                        && editor
                            .interaction()
                            .selected
                            .as_ref()
                            .is_some_and(|selected| selected.object_id.starts_with("airwire:"))
                });
                append(
                    build_electronics_context_menu_surface(
                        self.palette,
                        has_selection,
                        can_duplicate,
                        can_route,
                    ),
                    EditorRect::new(position[0], position[1], 212.0, 224.0),
                );
            }
        }

        if let Some(right) = layout.right_panel {
            if self.project_type == ProjectType::Electronics {
                let data = electronics
                    .and_then(electronics_model::inspector_data)
                    .unwrap_or(electronics_model::ElectronicsInspectorData {
                        title: raf_core::i18n::t(
                            "app.electronics_no_selection",
                            self.agent_panel.language,
                        ),
                        fields: Vec::new(),
                        pins: Vec::new(),
                    });
                append(
                    build_electronics_inspector_surface(
                        self.palette,
                        &data.title,
                        &data.fields,
                        &data.pins,
                        &self.inspector_sessions,
                        self.inspector_view.tab,
                    ),
                    right,
                );
            } else {
                append(
                    build_inspector_surface_with_unit(
                        self.palette,
                        scene,
                        selected.first().copied(),
                        &self.inspector_sessions,
                        1.0,
                        self.inspector_view,
                        self.agent_settings.display_unit,
                    ),
                    right,
                );
            }
        }

        if layout.left_panel.is_some() {
            append(
                build_editor_splitter_surface(self.palette, EditorSplitterKind::LeftPanel),
                EditorRect::new(
                    layout.canvas.x - 4.0,
                    layout.workspace.y,
                    8.0,
                    layout.workspace.height,
                ),
            );
        }
        if layout.right_panel.is_some() {
            append(
                build_editor_splitter_surface(self.palette, EditorSplitterKind::RightPanel),
                EditorRect::new(
                    layout
                        .right_panel
                        .map_or(layout.window.width, |rect| rect.x)
                        - 4.0,
                    layout.workspace.y,
                    8.0,
                    layout.workspace.height,
                ),
            );
        }
        append(
            build_editor_splitter_surface(self.palette, EditorSplitterKind::BottomDock),
            EditorRect::new(
                layout.window.x,
                layout.bottom_dock.y - 4.0,
                layout.window.width,
                8.0,
            ),
        );

        let drag_preview = self.bottom_dock.drag().and_then(|drag| {
            self.bottom_dock
                .tab(&drag.source_group_id, &drag.source_tab_id)
                .map(|moving_tab| BottomTabDragPreview {
                    source_group_id: drag.source_group_id.clone(),
                    source_tab_id: drag.source_tab_id.clone(),
                    moving_tab: moving_tab.clone(),
                    target_group_id: drag.target_group_id.clone(),
                    insertion_index: drag.insertion_index,
                    split_before: drag.split_before,
                    pulse: ((self.last_sync_time_seconds * 2.0).sin() * 0.5 + 0.5) as f32,
                    transition: self.drag_motion.value(),
                    width: 96.0,
                })
        });
        let dock_groups = self.bottom_dock.groups().to_vec();
        let dock_group_rects = self.dock_group_rects(layout);
        let agent_scroll_offset = self.agent_scroll_projection_offset;
        for (group_id, group_rect) in &dock_group_rects {
            let Some(group) = dock_groups.iter().find(|group| group.id == *group_id) else {
                continue;
            };
            let tab_height = group_rect.height.min(34.0).max(1.0);
            append(
                build_tab_strip_surface(
                    self.palette,
                    group,
                    group_rect.height <= 34.0,
                    drag_preview.as_ref(),
                ),
                EditorRect::new(group_rect.x, group_rect.y, group_rect.width, tab_height),
            );
            if group_rect.height <= tab_height + 1.0 {
                continue;
            }
            let content_rect = EditorRect::new(
                group_rect.x,
                group_rect.y + tab_height,
                group_rect.width,
                (group_rect.height - tab_height).max(1.0),
            );
            match group.active_tab.as_str() {
                "console" => append(
                    build_console_surface(
                        self.palette,
                        &self.console,
                        project.map_or(self.agent_settings.command_console_enabled, |project| {
                            self.agent_settings.command_console_enabled
                                && project.settings.enable_console_commands
                        }),
                        &[],
                        None,
                    ),
                    content_rect,
                ),
                "project" => append(
                    build_project_surface(
                        self.palette,
                        &self.project_name,
                        "Main",
                        self.project_catalog.project_entries(),
                    ),
                    content_rect,
                ),
                "nodes" => append(
                    build_nodes_surface_with_zoom(
                        self.palette,
                        node_graph,
                        selected_graph_node,
                        self.nodes_zoom,
                    ),
                    content_rect,
                ),
                "agent" => append(
                    build_agent_surface(
                        self.palette,
                        &self.agent_panel,
                        &self.agent_settings,
                        self.agent_readiness,
                        self.project_type,
                        [content_rect.width, content_rect.height],
                        agent_scroll_offset,
                        self.agent_motion.value(),
                    ),
                    content_rect,
                ),
                "drc" => append(
                    build_electronics_analysis_surface(
                        self.palette,
                        "DESIGN RULE CHECK",
                        &electronics
                            .map(|editor| {
                                electronics_model::analysis_lines(
                                    editor,
                                    "drc",
                                    self.agent_panel.language,
                                )
                            })
                            .unwrap_or_else(|| {
                                vec![crate::panels::electronics_surface::ElectronicsAnalysisLine::normal(
                                    raf_core::i18n::t(
                                        "electronics.analysis.unavailable",
                                        self.agent_panel.language,
                                    ),
                                )]
                            }),
                    ),
                    content_rect,
                ),
                "simulation" => append(
                    build_electronics_analysis_surface(
                        self.palette,
                        "SIMULATION",
                        &electronics
                            .map(|editor| {
                                electronics_model::analysis_lines(
                                    editor,
                                    "simulation",
                                    self.agent_panel.language,
                                )
                            })
                            .unwrap_or_else(|| {
                                vec![crate::panels::electronics_surface::ElectronicsAnalysisLine::normal(
                                    raf_core::i18n::t(
                                        "electronics.analysis.unavailable",
                                        self.agent_panel.language,
                                    ),
                                )]
                            }),
                    ),
                    content_rect,
                ),
                "assets" => {
                    let assets = asset_rows_with_builtins(self.project_catalog.assets());
                    append(
                        build_assets_surface(
                            self.palette,
                            &assets,
                            &self.assets_surface.query,
                            self.assets_surface.filter,
                            None,
                            self.assets_surface.script_menu_open,
                            &self.assets_surface.script_name,
                            self.assets_surface.primitive_menu_open,
                            None,
                            self.assets_surface.file_menu_open,
                            &self.assets_surface.file_name,
                        ),
                        content_rect,
                    );
                }
                "project-settings" => {
                    if let Some(project) = project {
                        append(
                            crate::project_settings_surface::build_project_settings_surface(
                                self.palette,
                                project,
                                self.agent_settings.command_console_enabled,
                            ),
                            content_rect,
                        );
                    }
                }
                _ => append(
                    UiSurface::new(
                        "editor.bottom.empty",
                        self.palette,
                        UiNode::new("editor.bottom.empty.root", UiNodeKind::Root)
                            .with_layout(UiLayout::fill(UiFlow::None))
                            .with_style(UiStyle::transparent()),
                    ),
                    content_rect,
                ),
            }
        }

        for pair in dock_group_rects.windows(2) {
            let (left_id, left_rect) = &pair[0];
            let (right_id, _) = &pair[1];
            let splitter_x = left_rect.x + left_rect.width - 4.0;
            append(
                build_dock_splitter_surface(self.palette, left_id, right_id),
                EditorRect::new(
                    splitter_x,
                    layout.bottom_dock.y,
                    8.0,
                    layout.bottom_dock.height,
                ),
            );
        }

        if let Some(drag) = self.bottom_dock.drag() {
            if let Some(group_rect) = dock_group_rects
                .iter()
                .find(|(id, _)| id == &drag.target_group_id)
                .map(|(_, rect)| *rect)
            {
                if let Some(split_before) = drag.split_before {
                    append(
                        build_drop_preview_surface(
                            self.palette,
                            ((self.last_sync_time_seconds * 2.0).sin() * 0.5 + 0.5) as f32,
                            split_before,
                            self.drag_motion.value(),
                        ),
                        EditorRect::new(
                            if split_before {
                                group_rect.x
                            } else {
                                group_rect.x + group_rect.width * 0.5
                            },
                            group_rect.y,
                            group_rect.width * 0.5,
                            group_rect.height,
                        ),
                    );
                }
            }
        }

        if self.project_type != ProjectType::Electronics
            || self.agent_settings.electronics_show_status
        {
            append(
                build_status_surface(
                    self.palette,
                    &status_items(self, selected, scene, electronics),
                ),
                layout.status_bar,
            );
        }

        if let Some(context) = self.bottom_dock.context_menu().cloned() {
            let menu_width = 244.0;
            let menu_height = 126.0;
            append(
                build_tab_context_menu_surface(
                    self.palette,
                    &context.group_id,
                    &context.tab_id,
                    self.bottom_dock.can_split(),
                ),
                EditorRect::new(
                    context.position[0].clamp(0.0, (layout.window.width - menu_width).max(0.0)),
                    context.position[1].clamp(0.0, (layout.window.height - menu_height).max(0.0)),
                    menu_width,
                    menu_height,
                ),
            );
        }

        let root = UiNode::new("editor.native.workbench.root", UiNodeKind::Root)
            .with_layout(UiLayout::fill(UiFlow::None))
            .with_style(UiStyle::transparent());
        let root = roots
            .into_iter()
            .fold(root, |root, child| root.with_child(child));
        UiSurface {
            id: "editor.native.workbench".to_string(),
            palette: self.palette,
            theme: raf_ui::UiTheme::raf_ui(),
            root,
            style_sheet: UiStyleSheet { rules },
            retained_tooltips: self.project_type == ProjectType::Electronics,
        }
    }
}

fn status_items(
    workbench: &NativeGameWorkbench,
    selected: &[SceneNodeId],
    scene: &SceneGraph,
    electronics: Option<&NativeElectronicsEditor>,
) -> Vec<String> {
    if workbench.project_type == ProjectType::Electronics {
        let language = workbench.agent_panel.language;
        let translate = |key: &str| raf_core::i18n::t(key, language);
        let (components, connections, nets) = electronics
            .map(|editor| {
                electronics_model::ElectronicsWorkbenchData::from_editor(Some(editor)).counts
            })
            .unwrap_or_default();
        let surface = electronics
            .map(NativeElectronicsEditor::active_surface)
            .map(|surface| match surface {
                raf_electronics::CadSurfaceKind::Schematic => {
                    translate("app.electronics_schematic_tab")
                }
                raf_electronics::CadSurfaceKind::Pcb => translate("app.electronics_pcb_tab"),
            })
            .unwrap_or_else(|| translate("app.electronics_schematic_tab"));
        let selected_kind = electronics
            .and_then(NativeElectronicsEditor::selection)
            .map(|selection| {
                format!(
                    "{}: {}",
                    translate("app.electronics_selection"),
                    translate(electronics_selection_key(selection.kind))
                )
            })
            .unwrap_or_else(|| translate("app.electronics_no_selection"));
        let grid_step = electronics
            .map(NativeElectronicsEditor::grid_step)
            .unwrap_or(workbench.agent_settings.electronics_grid_step_mm);
        vec![
            workbench.project_name.clone(),
            surface,
            selected_kind,
            format!("{}: {components}", translate("app.schematic_components")),
            format!(
                "{}: {connections}",
                translate("app.electronics_connections")
            ),
            format!("{}: {nets}", translate("app.schematic_nets")),
            format!(
                "{}: {:.0} mm",
                translate("app.electronics_grid_status"),
                grid_step
            ),
            if electronics.is_some_and(NativeElectronicsEditor::is_dirty) {
                translate("app.electronics_modified")
            } else {
                translate("app.electronics_saved")
            },
        ]
    } else {
        vec![
            workbench.project_name.clone(),
            format!("Selected: {}", selected.len()),
            format!("Actors: {}", scene.iter().count()),
            "Snap: On".to_string(),
            if workbench.agent_settings.show_fps_counter {
                workbench.performance_status_text()
            } else {
                "FPS: hidden".to_string()
            },
        ]
    }
}

fn electronics_selection_key(
    kind: crate::electronics_controller::ElectronicsSelectionKind,
) -> &'static str {
    match kind {
        crate::electronics_controller::ElectronicsSelectionKind::Component => {
            "app.electronics_component"
        }
        crate::electronics_controller::ElectronicsSelectionKind::Pin => "app.electronics_pin",
        crate::electronics_controller::ElectronicsSelectionKind::Wire => "app.electronics_wire",
        crate::electronics_controller::ElectronicsSelectionKind::Trace => "app.electronics_trace",
        crate::electronics_controller::ElectronicsSelectionKind::Other => "app.electronics_other",
    }
}
