//! Native workbench compositor.
//!
//! The shell state and input lifecycle stay in `native_workbench.rs`; this
//! module owns only retained surface composition so the compositor does not
//! become a second state/controller implementation.

use super::*;
use crate::panels::agent_surface::build_dock_agent_surface;
use crate::panels::electronics_context_menu_surface::{
    build_electronics_context_menu_surface, ELECTRONICS_CONTEXT_MENU_HEIGHT,
    ELECTRONICS_CONTEXT_MENU_WIDTH,
};
use crate::panels::electronics_footprint_surface::build_electronics_footprint_surface;
use crate::panels::electronics_surface::{
    build_electronics_analysis_surface, build_electronics_delete_backdrop_surface,
    build_electronics_delete_modal_surface, ElectronicsAnalysisMotion,
    ELECTRONICS_DELETE_MODAL_HEIGHT, ELECTRONICS_DELETE_MODAL_WIDTH,
};
use crate::panels::nodes_surface::{
    build_nodes_context_menu_surface, build_nodes_palette_popup_surface,
    build_nodes_surface_with_host, NODES_CONTEXT_MENU_HEIGHT, NODES_CONTEXT_MENU_WIDTH,
    NODES_PALETTE_POPUP_HEIGHT, NODES_PALETTE_POPUP_WIDTH,
};
use crate::panels::project_surface::build_project_surface;
use crate::panels::viewport_toolbar_surface::build_viewport_toolbar_surface_measured;
use raf_render::api_graphic_basic::ui_surface::UiTextAtlasRequest;
use raf_ui::{UiTextOverflow, UiTextStyle};

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
        node_can_undo: bool,
        node_can_redo: bool,
        node_validation: Option<&crate::native_editor_runtime::NodeGraphValidation>,
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

        let mut application_bar = build_application_bar_surface(
            self.palette,
            &self.project_name,
            self.project_type,
            self.open_menu.as_deref(),
        );
        if let Some(index) = application_bar
            .root
            .children
            .iter()
            .position(|node| node.id == "application-bar.drag-region")
        {
            application_bar.root.children.insert(
                index,
                crate::agent_workspace_surface::button(
                    "application-bar.agents",
                    "agent_workspace.title",
                    "agent.workspace",
                    &self.palette.tokens(),
                ),
            );
        }
        if self.project_type == ProjectType::Game {
            let children = &mut application_bar.root.children;
            let index = children
                .iter()
                .position(|node| node.id == "application-bar.drag-region")
                .unwrap_or(children.len());
            children.insert(
                index,
                raf_player::surface::runtime_controls(self.palette, &self.runtime_toolbar),
            );
            if layout.window.width < 1600.0 {
                children.retain(|node| {
                    node.id != "application-bar.context"
                        && !node.id.starts_with("application-bar.saved")
                });
            }
            if layout.window.width < 1300.0 {
                children.retain(|node| !node.id.starts_with("application-bar.command-search"));
            }
            if layout.window.width < 1100.0 {
                if let Some(project) = children
                    .iter_mut()
                    .find(|node| node.id == "application-bar.project")
                {
                    project.layout.basis[0] = 108.0;
                }
            }
        }
        if self.project_type == ProjectType::Electronics {
            application_bar = electronics_layout::compact_application_bar(application_bar, layout.window.width);
        }
        let electronics_menu_anchor = self.open_menu.as_deref().and_then(|menu| electronics_layout::menu_anchor(&application_bar, menu));
        append(application_bar, layout.application_bar);

        let menu = build_application_menu(ApplicationMenuState {
            project_type: self.project_type,
            active_view: if self.project_type == ProjectType::Electronics {
                if electronics.is_some_and(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb) { ApplicationView::Pcb } else { ApplicationView::Schematic }
            } else { ApplicationView::Scene },
            grid_visible: electronics.map_or(self.toolbar_state.grid_visible, NativeElectronicsEditor::grid_visible),
            hierarchy_visible: layout.left_panel.is_some_and(|left| self.project_type != ProjectType::Electronics || left.width > crate::editor_layout::ELECTRONICS_RAIL_WIDTH + 0.5),
            inspector_visible: layout.right_panel.is_some(),
            undo_available: electronics.map_or(can_undo, NativeElectronicsEditor::can_undo),
            redo_available: electronics.map_or(can_redo, NativeElectronicsEditor::can_redo),
            selection_available: electronics.map_or(!selected.is_empty(), |e| e.selection().is_some()),
            select_all_available: electronics.map_or(!scene.roots().is_empty(), |e| !e.scene().objects.is_empty()),
            copy_available: electronics.map_or(!selected.is_empty(), |e| e.selection().is_some()),
            paste_available: can_paste,
        });
        if let Some(open_menu) = self.open_menu.as_deref() {
            if let Some(menu) = menu.menus.iter().find(|menu| menu.id == open_menu) {
                let menu_x = if self.project_type == ProjectType::Electronics { electronics_menu_anchor.unwrap_or(8.0) } else {
                    (match open_menu {
                        "file" => 238.0,
                        "edit" => 294.0,
                        "view" => 350.0,
                        "project" => 410.0,
                        "help" => 486.0,
                        _ => 238.0,
                    }) - if self.project_type == ProjectType::Game && layout.window.width < 1100.0 {
                        40.0
                    } else {
                        0.0
                    } };
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
                    if left.width <= crate::editor_layout::ELECTRONICS_RAIL_WIDTH + 0.5 { self.build_electronics_rail() } else { crate::panels::electronics_navigator_surface::build_electronics_navigator_surface_with_state(
                        self.palette,
                        &self.electronics_navigator_tab,
                        &self.electronics_library_query,
                        &data.schematic_name,
                        data.counts,
                        &data.components,
                        &data.wires,
                        &data.library,
                        electronics.map_or(
                            raf_electronics::CadSurfaceKind::Schematic,
                            NativeElectronicsEditor::active_surface,
                        ),
                        data.layer_visibility,
                        crate::panels::electronics_navigator_surface::NavigatorDesktopState {
                            collapsed: Some(&self.electronics_workspace.collapsed_categories),
                            footprints_open: self.footprint_chooser.is_open(),
                            footprints: self
                                .footprint_chooser
                                .is_open()
                                .then_some(&self.footprint_chooser.model),
                        },
                    ) },
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
                let max_scroll = (view.total_rows as f32 * hierarchy_row_height - hierarchy_tree_height).max(0.0);
                let requested_scroll = self.hierarchy_scroll_offset;
                sync_hierarchy_scroll_state(&mut self.host.session_mut().interaction.controls, &mut self.hierarchy_scroll_offset, requested_scroll, max_scroll);
                append(
                    if self.interface.workspace.active() {
                        crate::panels::interface_surface::build_interface_hierarchy(self.palette,&self.interface,[left.width,left.height])
                    } else if self.hierarchy_active_tab == "lighting" {
                        crate::panels::lighting_surface::navigation(
                            self.palette,
                            &self.lighting_section,
                        )
                    } else {
                        self.build_hierarchy_document(&view, selected, scene, left, can_paste, &search_results)
                    },
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

        if self.project_type==ProjectType::Game && !self.interface.workspace.active() && self.interface_overlay_visible() {
            append(crate::panels::interface_surface::build_interface_canvas(self.palette,&self.interface,[layout.canvas.width,layout.canvas.height]),layout.canvas);
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
                    electronics.is_none_or(NativeElectronicsEditor::snap_enabled),
                    electronics.is_some_and(NativeElectronicsEditor::can_undo),
                    electronics.is_some_and(NativeElectronicsEditor::can_redo),
                    electronics.is_some_and(|editor| editor.selection().is_some()),
                    electronics.is_some_and(NativeElectronicsEditor::can_rotate_selection),
                    layout.window.width,
                )
            } else if self.interface.workspace.active() {
                crate::panels::interface_surface::build_interface_canvas(self.palette,&self.interface,[layout.canvas.width,layout.canvas.height])
            } else {
                let compact = self.toolbar_state.compact || layout.canvas.width < 396.0;
                let toolbar_state = crate::panels::viewport_toolbar_surface::ViewportToolbarState {
                    compact,
                    ..self.toolbar_state
                };
                let toolbar_width = (if compact { 230.0_f32 } else { 396.0_f32 })
                    .min(layout.canvas.width.max(0.0));
                let theme = self.host.surface().theme.metrics;
                let text_scale = self.host.session().text_scale();
                let text_style = UiTextStyle::button(self.palette.tokens().text_muted)
                    .scaled_for_ui(text_scale);
                let atlas = &self.host.session().text_atlas;
                let measure_text = |text: &str| {
                    let request = UiTextAtlasRequest::new(
                        "viewport.toolbar.measure",
                        text,
                        text_style,
                        1024.0,
                    )
                    .with_overflow(UiTextOverflow::Clip)
                    .with_single_line(true);
                    atlas.measure_prefix_width(&request, text, text.chars().count())
                };
                build_viewport_toolbar_surface_measured(
                    self.palette,
                    toolbar_state,
                    toolbar_width,
                    self.agent_settings.language,
                    theme,
                    measure_text,
                    toolbar_state.actions_menu_open,
                )
                .0
            },
            if self.project_type == ProjectType::Electronics {layout.electronics_toolbar_rect()} else {layout.canvas},
        );
        if self.project_type==ProjectType::Game && !self.interface.workspace.active() {
            append(crate::panels::interface_surface::build_interface_entry(self.palette,layout.canvas.width),layout.canvas);
        }
        if self.project_type == ProjectType::Electronics {
            {
                if let Some(editor) = electronics {
                    append(crate::panels::electronics_desktop_surface::build_electronics_document_chrome(self.palette, editor, layout.canvas), layout.canvas);
                }
                if let Some(editor)=electronics.filter(|e|e.active_surface()==raf_electronics::CadSurfaceKind::Pcb) {
                    let width=(layout.canvas.width-crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH).max(0.0);
                    let height=crate::panels::pcb_tool_options_surface::pcb_tool_options_height(width);
                    append(crate::panels::pcb_tool_options_surface::build_pcb_tool_options_surface_with_zoom(self.palette,editor.pcb_route_layer(),editor.pcb_route_width(),editor.grid_step(),editor.camera().zoom*25.4/96.0*100.0,width,self.agent_settings.language,self.electronics_workspace.pcb_menu.as_deref()),
                        crate::editor_layout::EditorRect{x:layout.canvas.x+crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH,y:layout.canvas.y+crate::panels::electronics_desktop_surface::DOCUMENT_HEIGHT,width,height});
                    if editor.removed_trace_count()>0 {
                        append(crate::panels::pcb_tool_options_surface::build_pcb_parked_traces_surface(self.palette,editor.removed_trace_count(),width,self.agent_settings.language),
                            crate::editor_layout::EditorRect{x:layout.electronics_canvas_for_pcb(true).x+10.0,y:layout.electronics_canvas_for_pcb(true).y+24.0,width:width.min(if width<340.0 {184.0}else{340.0}),height:if width<340.0 {64.0}else{36.0}});
                    }
                }
            }
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
                let entrance = self.electronics_menu_entrance();
                append(
                    build_electronics_context_menu_surface(
                        self.palette,
                        has_selection,
                        can_duplicate,
                        can_route,
                    ),
                    clamp_rect_to_window(
                        EditorRect::new(
                            position[0],
                            position[1] - (1.0 - entrance) * 6.0,
                            ELECTRONICS_CONTEXT_MENU_WIDTH,
                            ELECTRONICS_CONTEXT_MENU_HEIGHT,
                        ),
                        layout.window,
                    ),
                );
            }
        }

        // Destructive confirmation is a blocking, window-space overlay: the scrim
        // covers the whole window so it cannot be mistaken for a panel, and the
        // modal itself is centred and clamped so it stays fully readable.
        if let Some(editor) = electronics.filter(|editor| editor.delete_confirm_pending()) {
            let item_count = editor.selection().map_or(0, |selection| {
                usize::from(
                    selection.kind
                        != crate::electronics_controller::ElectronicsSelectionKind::Other,
                )
            });
            append(
                build_electronics_delete_backdrop_surface(self.palette),
                layout.window,
            );
            let modal = EditorRect::new(
                layout.window.x + (layout.window.width - ELECTRONICS_DELETE_MODAL_WIDTH) * 0.5,
                layout.window.y + (layout.window.height - ELECTRONICS_DELETE_MODAL_HEIGHT) * 0.5,
                ELECTRONICS_DELETE_MODAL_WIDTH,
                ELECTRONICS_DELETE_MODAL_HEIGHT,
            );
            append(
                build_electronics_delete_modal_surface(
                    self.palette,
                    UiRect::new(modal.x, modal.y, modal.width, modal.height),
                    item_count,
                    self.electronics_menu_entrance(),
                ),
                modal,
            );
        }

        if let Some(right) = layout.right_panel {
            if self.project_type == ProjectType::Electronics {
                if self.electronics_workspace.pcb_menu.as_deref() == Some("rules") {
                    if let Some(editor) = electronics {
                        append(crate::panels::electronics_rules_surface::build_rules_surface(
                            self.palette, editor.pcb(), editor.pcb_rule_net().as_deref()), right);
                    }
                } else if self.footprint_chooser.is_open() {
                    append(
                        crate::panels::electronics_footprint_surface::build_footprint_inspector_surface(
                            self.palette,
                            &self.footprint_chooser.model,
                            right.width,
                        ),
                        right,
                    );
                } else {
                let data = electronics
                    .and_then(electronics_model::inspector_data)
                    .unwrap_or(electronics_model::ElectronicsInspectorData {
                        title: raf_core::i18n::t(
                            "app.electronics_no_selection",
                            self.agent_panel.borrow_mut().language,
                        ),
                        fields: Vec::new(),
                        pins: Vec::new(),
                    });
                append(
                    crate::panels::electronics_inspector_surface::build_electronics_inspector_surface_with_page(
                        self.palette,
                        &data.title,
                        &data.fields,
                        &data.pins,
                        &self.inspector_sessions,
                        &self.electronics_workspace.inspector_page,
                        electronics.and_then(|editor| editor.fabrication_status()),
                        self.inspector_footprint_presentation.as_ref(),
                    ),
                    right,
                );
                }
            } else {
                append(
                    if self.hierarchy_active_tab == "lighting" {
                        crate::panels::lighting_surface::inspector(
                            self.palette,
                            scene.environment(),
                            &self.lighting_section,
                            self.lighting_preview_neutral,
                            self.lighting_error,
                        )
                    } else if self.interface.workspace.active() && self.inspector_view.tab==crate::panels::inspector_surface::InspectorTab::Properties {
                        let properties=crate::panels::interface_surface::build_interface_inspector(self.palette,&self.interface,[(right.width-12.0).max(1.0),right.height]);
                        let mut inspector=crate::panels::inspector_surface::build_inspector_with_content(self.palette,1.0,self.inspector_view,properties.root);
                        inspector.style_sheet.rules.extend(properties.style_sheet.rules);
                        inspector
                    } else {
                        build_inspector_surface_with_assets(
                            self.palette,
                            scene,
                            selected.first().copied(),
                            &self.inspector_sessions,
                            1.0,
                            self.inspector_view,
                            self.agent_settings.display_unit,
                            self.project_catalog.assets(),
                        )
                    },
                    right,
                );
            }
        }

        if layout.left_panel.is_some_and(|left| self.project_type != ProjectType::Electronics || left.width > crate::editor_layout::ELECTRONICS_RAIL_WIDTH + 0.5) {
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
        let mut assets_panel_rect: Option<EditorRect> = None;
        for (group_id, group_rect) in &dock_group_rects {
            let Some(group) = dock_groups.iter().find(|group| group.id == *group_id) else {
                continue;
            };
            let tab_height = group_rect.height.min(34.0).max(1.0);
            append(
                crate::panels::editor_bottom_dock_surface::build_tab_strip_surface_sized(
                    self.palette,
                    group,
                    group_rect.height <= 34.0,
                    drag_preview.as_ref(),
                    group_rect.width,
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
            let analysis_panel =
                electronics_model::ElectronicsAnalysisPanel::from_tab(&group.active_tab);
            if let Some(panel) = analysis_panel {
                self.note_electronics_tab(panel.tab_id());
            }
            // The panel swap slides a few pixels and settles through the shared
            // crossfade curve instead of hard cutting between tabs.
            let crossfade = self.electronics_tab_crossfade();
            let analysis_rect = EditorRect::new(
                content_rect.x,
                content_rect.y + (1.0 - crossfade) * 8.0,
                content_rect.width,
                content_rect.height,
            );
            match group.active_tab.as_str() {
                "interfaces" => append(
                    crate::panels::interface_surface::build_interface_surface(self.palette,&self.interface,[content_rect.width,content_rect.height]),
                    content_rect,
                ),
                "console" => append(
                    crate::panels::console_surface::build_console_table_surface(
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
                    build_nodes_surface_with_host(
                        self.palette,
                        node_graph,
                        selected_graph_node,
                        self.nodes_zoom,
                        self.nodes_pan,
                        [content_rect.width, content_rect.height],
                        &self.nodes_host,
                        node_can_undo,
                        node_can_redo,
                        node_validation,
                        self.agent_panel.borrow_mut().language,
                    ),
                    content_rect,
                ),
                "agent" => append(
                    build_dock_agent_surface(
                        self.palette,
                        &self.agent_panel.borrow_mut(),
                        &self.agent_settings,
                        self.agent_readiness,
                        self.project_type,
                        [content_rect.width, content_rect.height],
                        agent_scroll_offset,
                        self.agent_motion.value(),
                    ),
                    content_rect,
                ),
                electronics_model::ELECTRONICS_ANALYSIS_DRC_TAB => append(
                    build_electronics_analysis_surface(
                        self.palette,
                        electronics_model::ELECTRONICS_ANALYSIS_DRC_TAB,
                        &electronics_analysis_lines(
                            electronics,
                            electronics_model::ElectronicsAnalysisPanel::Drc,
                            self.agent_panel.borrow_mut().language,
                        ),
                        ElectronicsAnalysisMotion::new(
                            crossfade,
                            self.electronics_analysis_phase(),
                        ),
                    ),
                    analysis_rect,
                ),
                electronics_model::ELECTRONICS_ANALYSIS_SIMULATION_TAB => append(
                    build_electronics_analysis_surface(
                        self.palette,
                        electronics_model::ELECTRONICS_ANALYSIS_SIMULATION_TAB,
                        &electronics_analysis_lines(
                            electronics,
                            electronics_model::ElectronicsAnalysisPanel::Simulation,
                            self.agent_panel.borrow_mut().language,
                        ),
                        ElectronicsAnalysisMotion::new(
                            crossfade,
                            self.electronics_analysis_phase(),
                        ),
                    ),
                    analysis_rect,
                ),
                "assets" => {
                    let assets = asset_rows_with_builtins(self.project_catalog.assets());
                    let grid_viewport =
                        (content_rect.height - ASSET_TOOLBAR_HEIGHT - ASSET_SELECTION_BAR_HEIGHT)
                            .max(ASSET_CARD_MIN_VIEWPORT);
                    let visible_range = asset_visible_range(
                        &assets,
                        &self.assets_surface.query,
                        self.assets_surface.filter,
                        &self.assets_surface.script_extension,
                        self.assets_surface.sort,
                        self.assets_scroll_offset,
                        grid_viewport,
                        self.assets_surface.view,
                    );
                    let entries = raf_assets::authoring_catalog::project_assets(&assets, scene);
                    let visible = crate::panels::assets_surface::visible_asset_rows(
                        &assets,
                        &self.assets_surface.query,
                        self.assets_surface.filter,
                        &self.assets_surface.script_extension,
                        self.assets_surface.sort,
                    );
                    let visuals = visible
                        .iter()
                        .copied()
                        .skip(visible_range.0)
                        .take(visible_range.1.saturating_sub(visible_range.0))
                        .chain(self.assets_surface.selected_asset.as_ref().into_iter())
                        .filter_map(|row| {
                            entries.get(row.as_str()).map(|entry| (row.as_str(), entry))
                        })
                        .map(|(row, entry)| {
                            let preview =
                                self.asset_previews.request(entry, self.host.images_mut());
                            (
                                row.to_string(),
                                crate::panels::assets_surface::AssetVisual {
                                    entry: entry.clone(),
                                    preview,
                                    can_apply_texture: selected.iter().filter_map(|id|scene.get(*id)).any(|node|!node.locked && !node.is_folder && node.primitive!=raf_core::scene::Primitive::Empty),
                                },
                            )
                        })
                        .collect::<std::collections::HashMap<_, _>>();
                    append(
                        build_assets_surface(AssetsSurfaceParams {
                            palette: self.palette,
                            rows: &assets,
                            language: self.agent_panel.borrow_mut().language,
                            query: &self.assets_surface.query,
                            filter: self.assets_surface.filter,
                            script_extension: &self.assets_surface.script_extension,
                            sort: self.assets_surface.sort,
                            view: self.assets_surface.view,
                            selected: self.assets_surface.selected_asset.as_deref(),
                            highlight: self.assets_surface.highlight_asset.as_deref(),
                            status: self.assets_surface.status.as_ref(),
                            pending: self.project_catalog.is_pending(),
                            catalog_error: self.project_catalog.error(),
                            visible_range: Some(visible_range),
                            create_open: self.assets_surface.create_menu_open,
                            motion: self.assets_motion(),
                            visuals: Some(&visuals),
                        }),
                        content_rect,
                    );
                    assets_panel_rect = Some(content_rect);
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

        if let Some(menu) = self.nodes_host.context_menu() {
            if let Some(surface) = build_nodes_context_menu_surface(
                self.palette,
                node_graph,
                menu.node_id,
                self.agent_panel.borrow_mut().language,
            ) {
                // Window-level overlay: clamped against the window and nudged
                // by the entrance tween so the menu settles into place.
                let entrance = (1.0 - self.nodes_host.menu_motion()) * 6.0;
                append(
                    surface,
                    EditorRect::new(
                        menu.position[0].clamp(
                            0.0,
                            (layout.window.width - NODES_CONTEXT_MENU_WIDTH).max(0.0),
                        ),
                        menu.position[1].clamp(
                            0.0,
                            (layout.window.height - NODES_CONTEXT_MENU_HEIGHT).max(0.0),
                        ) - entrance,
                        NODES_CONTEXT_MENU_WIDTH,
                        NODES_CONTEXT_MENU_HEIGHT,
                    ),
                );
            }
        }

        // Assets menus, popovers and modals are window-level so the dock panel
        // never clips them. Anchors are clamped to the window and nudged by the
        // shared entrance tween.
        if let Some(panel) = assets_panel_rect {
            let window = layout.window;
            let slide = (1.0 - self.assets_menu_motion.value()) * 6.0;
            let toolbar_anchor = [panel.x + 10.0, panel.y + ASSET_TOOLBAR_HEIGHT];
            let project_root = self.assets_project_root.as_deref();
            let modal = self
                .assets_surface
                .open_asset
                .as_ref()
                .map(|row| {
                    (
                        build_assets_open_modal_surface(
                            self.palette,
                            row,
                            &asset_absolute_path(project_root, row),
                        ),
                        ASSETS_OPEN_MODAL_HEIGHT,
                    )
                })
                .or_else(|| {
                    self.assets_surface.rename_asset.as_ref().map(|row| {
                        (
                            build_assets_rename_modal_surface(self.palette, row),
                            ASSETS_RENAME_MODAL_HEIGHT,
                        )
                    })
                })
                .or_else(|| {
                    self.assets_surface.delete_asset.as_ref().map(|row| {
                        (
                            build_assets_delete_modal_surface(self.palette, row),
                            ASSETS_DELETE_MODAL_HEIGHT,
                        )
                    })
                });
            if let Some((surface, height)) = modal {
                append(
                    build_assets_backdrop_surface(self.palette),
                    EditorRect::new(window.x, window.y, window.width, window.height),
                );
                append(
                    surface,
                    EditorRect::new(
                        window.x + (window.width - ASSETS_MODAL_WIDTH) * 0.5,
                        window.y + (window.height - height) * 0.5,
                        ASSETS_MODAL_WIDTH,
                        height,
                    ),
                );
            } else if let Some(context) = self.assets_surface.context_menu.as_ref() {
                let height = if context.builtin {
                    ASSETS_CONTEXT_MENU_BUILTIN_HEIGHT
                } else {
                    ASSETS_CONTEXT_MENU_HEIGHT
                };
                append(
                    build_assets_context_menu_surface(
                        self.palette,
                        &context.row,
                        context.builtin,
                        &asset_absolute_path(project_root, &context.row),
                    ),
                    assets_overlay_rect(
                        window,
                        context.position,
                        [ASSETS_MENU_WIDTH, height],
                        slide,
                    ),
                );
            } else if self.assets_surface.create_menu_open {
                append(
                    build_assets_create_menu_surface(self.palette),
                    assets_overlay_rect(
                        window,
                        toolbar_anchor,
                        [ASSETS_MENU_WIDTH, ASSETS_CREATE_MENU_HEIGHT],
                        slide,
                    ),
                );
            } else if self.assets_surface.script_menu_open {
                append(
                    build_assets_script_popover_surface(
                        self.palette,
                        &self.assets_surface.script_name,
                    ),
                    assets_overlay_rect(
                        window,
                        toolbar_anchor,
                        [ASSETS_POPOVER_WIDTH, ASSETS_SCRIPT_POPOVER_HEIGHT],
                        slide,
                    ),
                );
            } else if self.assets_surface.file_menu_open {
                append(
                    build_assets_file_popover_surface(self.palette, &self.assets_surface.file_name),
                    assets_overlay_rect(
                        window,
                        toolbar_anchor,
                        [ASSETS_POPOVER_WIDTH, ASSETS_FILE_POPOVER_HEIGHT],
                        slide,
                    ),
                );
            } else if self.assets_surface.primitive_menu_open {
                append(
                    build_assets_primitive_popover_surface(self.palette),
                    assets_overlay_rect(
                        window,
                        toolbar_anchor,
                        [ASSETS_POPOVER_WIDTH, ASSETS_PRIMITIVE_POPOVER_HEIGHT],
                        slide,
                    ),
                );
            }
        }

        if let Some(popup) = self.nodes_host.palette_popup() {
            if let Some(surface) = build_nodes_palette_popup_surface(
                self.palette,
                &self.nodes_host,
                self.agent_panel.borrow_mut().language,
            ) {
                let entrance = (1.0 - self.nodes_host.menu_motion()) * 6.0;
                append(
                    surface,
                    EditorRect::new(
                        popup.position[0].clamp(
                            0.0,
                            (layout.window.width - NODES_PALETTE_POPUP_WIDTH).max(0.0),
                        ),
                        popup.position[1].clamp(
                            0.0,
                            (layout.window.height - NODES_PALETTE_POPUP_HEIGHT).max(0.0),
                        ) - entrance,
                        NODES_PALETTE_POPUP_WIDTH,
                        NODES_PALETTE_POPUP_HEIGHT,
                    ),
                );
            }
        }

        if let Some(notice)=&self.authoring_notice {
            let notice=UiNode::new("authoring.notice",UiNodeKind::Button)
                .with_text_value(notice.clone()).with_icon(raf_ui::UiIcon::new(UiIconId::Close))
                .with_style(self.palette.panel_style()).focusable()
                .with_layout(UiLayout::fill(UiFlow::Row))
                .with_event(raf_ui::UiEventBinding::command(raf_ui::UiEventKind::Click,"authoring.notice.dismiss"));
            append(UiSurface::new("authoring.notice",self.palette,notice),EditorRect::new(
                layout.canvas.x+12.0,layout.canvas.y+48.0,(layout.canvas.width-24.0).clamp(120.0,520.0),52.0));
        }
        if self.project_type==ProjectType::Electronics {
            if let (Some(menu),Some(editor))=(self.electronics_workspace.pcb_menu.as_deref().filter(|menu| *menu != "rules"),electronics.filter(|e|e.active_surface()==raf_electronics::CadSurfaceKind::Pcb)) {
                let mut popup=crate::panels::pcb_tool_options_surface::build_pcb_options_menu(self.palette,menu,self.agent_settings.language,editor.pcb_route_layer(),editor.pcb_route_width(),editor.grid_step(),editor.camera().zoom*25.4/96.0*100.0);
                let mut rect=crate::panels::pcb_tool_options_surface::pcb_menu_anchor(layout.canvas,menu);
                rect.height=popup.root.children.len()as f32*30.0+8.0;
                if let Some(anchor)=self.host.layout_rect(&format!("electronics.pcb.options.{menu}")) {
                    rect.x=anchor.x;rect.y=anchor.y+anchor.height+3.0;rect.width=anchor.width.max(140.0);
                }
                let backdrop=UiNode::new("electronics.pcb.menu.backdrop",UiNodeKind::Panel)
                    .with_layout(UiLayout::fill(UiFlow::None)).with_style(UiStyle::transparent())
                    .with_event(raf_ui::UiEventBinding::command(raf_ui::UiEventKind::Click,"electronics.pcb.menu.close"));
                append(UiSurface::new("electronics.pcb.menu.backdrop",self.palette,backdrop),layout.window);
                popup.root.layout=UiLayout{flow:UiFlow::Column,padding:raf_ui::UiSpacing::same(4.0),..UiLayout::fill(UiFlow::Column)};
                append(popup,rect);
            }
        }
        if self.console.filters_open {
            if let Some((_,rect))=dock_group_rects.iter().find(|(id,_)|dock_groups.iter().any(|g|&g.id==id && g.active_tab=="console")) {
                let backdrop=UiNode::new("console.filters.backdrop",UiNodeKind::Panel).with_layout(UiLayout::fill(UiFlow::None))
                    .with_style(UiStyle::transparent()).with_event(raf_ui::UiEventBinding::command(raf_ui::UiEventKind::Click,"console.filters.close"));
                append(UiSurface::new("console.filters.backdrop",self.palette,backdrop),layout.window);
                append(crate::panels::console_surface::build_console_filters_surface(self.palette,&self.console),
                    EditorRect::new((rect.x+rect.width-246.0).max(rect.x), (rect.y-254.0).max(layout.workspace.y),238.0,254.0));
            }
        }
        if self.project_type==ProjectType::Game && self.interface.workspace.active() && self.interface.workspace.insert_open {
            let trigger=self.host.layout_rect("ui.designer.insert").unwrap_or(UiRect::new(layout.canvas.x+8.0,layout.canvas.y+8.0,30.0,30.0));
            let placement=raf_ui::overlays::place_overlay(trigger,crate::panels::interface_surface::INSERT_MENU_SIZE,
                UiRect::new(layout.window.x,layout.window.y,layout.window.width,layout.window.height),raf_ui::overlays::UiPlacement::BottomStart,4.0).rect;
            append(crate::panels::interface_surface::build_interface_insert_menu(self.palette),EditorRect::new(placement.x,placement.y,placement.width,placement.height));
        }
        if self.ui_performance.open {
            let selected_rect = self.ui_performance.selected
                .and_then(|index| self.ui_performance.snapshot.regions.get(index))
                .and_then(|region| self.host.layout_rect(&region.id));
            append(crate::panels::ui_performance_surface::build_ui_performance_surface(
                self.palette, &self.ui_performance.snapshot, self.ui_performance.selected,
                [layout.window.width, layout.window.height], selected_rect,
            ), layout.window);
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

/// Builds the retained analysis lines for one dock panel, including the
/// workbench-level notices (stale synchronization and surface errors) so a
/// failure is never presented as if it were a design result.
fn electronics_analysis_lines(
    editor: Option<&NativeElectronicsEditor>,
    panel: electronics_model::ElectronicsAnalysisPanel,
    language: raf_core::config::Language,
) -> Vec<crate::panels::electronics_surface::ElectronicsAnalysisLine> {
    let Some(editor) = editor else {
        return vec![
            crate::panels::electronics_surface::ElectronicsAnalysisLine::normal(raf_core::i18n::t(
                "electronics.analysis.unavailable",
                language,
            )),
        ];
    };
    let mut lines = electronics_model::analysis_lines(editor, panel, language);
    let mut notices = electronics_model::notice_lines(editor, language);
    if panel == electronics_model::ElectronicsAnalysisPanel::Drc && editor.fabrication_status().is_some() {
        notices.extend(lines);
        notices
    } else {
        lines.extend(notices);
        lines
    }
}

/// Clamps an Assets overlay to the window so a menu, popover or modal is
/// always fully reachable even when the panel sits against a window edge.
fn assets_overlay_rect(
    window: EditorRect,
    anchor: [f32; 2],
    size: [f32; 2],
    slide: f32,
) -> EditorRect {
    EditorRect::new(
        anchor[0].clamp(window.x, (window.x + window.width - size[0]).max(window.x)),
        (anchor[1] + slide).clamp(window.y, (window.y + window.height - size[1]).max(window.y)),
        size[0],
        size[1],
    )
}

/// Keeps a floating menu fully inside the window.
/// A context menu is opened at the raw pointer position, so a right-click near
/// the right or bottom edge would otherwise place most of the menu offscreen
/// with no way to reach the last items. The menu flips to the other side of the
/// pointer when it does not fit, matching the contract in
/// `docs/EDITOR_RAFUI.md`.
fn clamp_rect_to_window(rect: EditorRect, window: EditorRect) -> EditorRect {
    let max_x = (window.x + window.width - rect.width).max(window.x);
    let max_y = (window.y + window.height - rect.height).max(window.y);
    EditorRect::new(
        rect.x.clamp(window.x, max_x),
        rect.y.clamp(window.y, max_y),
        rect.width,
        rect.height,
    )
}

fn status_items(
    workbench: &NativeGameWorkbench,
    selected: &[SceneNodeId],
    scene: &SceneGraph,
    electronics: Option<&NativeElectronicsEditor>,
) -> Vec<String> {
    if workbench.project_type == ProjectType::Electronics {
        let language = workbench.agent_panel.borrow_mut().language;
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
        // Divergence and unresolved failures stay visible in the status bar so
        // they are not only discoverable by opening the analysis dock.
        let notices = electronics
            .map(|editor| {
                let first = editor.surface_errors().last().cloned().or_else(|| {
                    editor
                        .sync_is_stale()
                        .then(|| translate("electronics.sync.pcb_stale"))
                }).or_else(||editor.fabrication_status().map(str::to_string));
                first.unwrap_or_default()
            })
            .unwrap_or_default();
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
                "{}: {:.2} mm",
                translate("app.electronics_grid_status"),
                grid_step
            ),
            if electronics.is_some_and(NativeElectronicsEditor::is_dirty) {
                translate("app.electronics_modified")
            } else {
                translate("app.electronics_saved")
            },
            notices.chars().take(160).collect(),
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
