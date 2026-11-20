//! Workbench synchronization: retained surface refresh, overlays and seeded controls.

use super::*;

impl NativeGameWorkbench {
    pub(crate) fn flush_interface_document(&mut self) -> Result<(), String> {
        self.interface.finish_gesture();
        if self.interface.dirty {
            self.interface.save()?;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        Ok(())
    }

    pub(crate) fn execute_asset_command(
        &mut self,
        request: &raf_core::EngineCommandRequest,
        project: Option<&Project>,
        revision: u64,
    ) -> raf_core::EngineCommandResponse {
        let selected = self
            .agent_panel
            .borrow_mut()
            .image_provider(&self.agent_settings);
        let (provider, model) = selected
            .map(|(provider, model)| (Some(provider), model))
            .unwrap_or_default();
        self.agent_asset_tools.borrow_mut().command(
            request,
            project.map(|project| project.path.as_path()),
            &self.agent_settings.external_asset_studio,
            provider.as_ref(),
            &model,
            revision,
            self.agent_panel.borrow_mut().language,
        )
    }

    /// Refreshes the canvas-local RafUI artwork without invalidating the full
    /// workbench chrome. The overlay is presented into the effective
    /// Electronics viewport, not the full window, so its z-order cannot cover
    /// the toolbar or side panels. The UI revision is included because
    /// selection, tools and analysis markers can change without changing CAD
    /// geometry.
    pub fn sync_electronics_overlay(
        &mut self,
        editor: Option<&NativeElectronicsEditor>,
        canvas: EditorRect,
    ) {
        let Some(editor) = editor.filter(|_| self.project_type == ProjectType::Electronics) else {
            self.electronics_overlay_active = false;
            self.electronics_overlay_key = None;
            self.electronics_overlay_canvas = EditorRect::default();
            return;
        };
        if !self.electronics_overlay_images_registered {
            register_electronics_images(self.electronics_overlay_host.images_mut());
            self.electronics_overlay_images_registered = true;
        }
        self.electronics_overlay_canvas =
            crate::panels::electronics_canvas_overlay_surface::electronics_overlay_rect(
                editor.active_surface() == raf_electronics::CadSurfaceKind::Pcb,
                canvas,
            );
        // The overlay document is retained RafUI, so its cache key must cover
        // every input that can change the resolved document: scene revision,
        // retained UI revision, canvas size, label visibility, and the two
        // presentation inputs the document reads (palette and language) plus
        // the host-owned hint opacity sample. Omitting the last two left the
        // overlay stale after a theme or language change.
        let key = (
            editor.revision(),
            editor.ui_revision(),
            canvas.logical_size(),
            editor.labels_visible(),
            self.palette,
            self.agent_panel.borrow_mut().language,
            self.electronics_hint_fade(),
        );
        if self.electronics_overlay_key == Some(key) {
            self.electronics_overlay_active = true;
            return;
        }
        if electronics_minimap::has_document_content(editor.scene()) {
            if self
                .electronics_overlay_host
                .images()
                .get(electronics_minimap::IMAGE_KEY)
                .is_none()
            {
                self.electronics_minimap_cache.invalidate();
            }
            if let Some((size, pixels)) = self.electronics_minimap_cache.update(
                editor.overview_revision(),
                self.palette,
                editor.scene(),
                editor.selection(),
            ) {
                if self
                    .electronics_overlay_host
                    .images_mut()
                    .insert_rgba(electronics_minimap::IMAGE_KEY, size, pixels)
                    .is_err()
                {
                    self.electronics_minimap_cache.invalidate();
                }
            }
        }
        self.electronics_overlay_host
            .set_surface(crate::panels::electronics_canvas_overlay_surface::build_electronics_framed_overlay_surface(
                self.palette,
                editor,
                canvas,
                ElectronicsCanvasOverlayParams::new(
                    self.electronics_hint_fade(),
                    self.agent_panel.borrow_mut().language,
                ),
            ));
        self.electronics_overlay_key = Some(key);
        self.electronics_overlay_active = true;
    }

    pub(crate) fn sync(
        &mut self,
        layout: EditorFrameLayout,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        can_undo: bool,
        can_redo: bool,
        can_paste: bool,
        node_graph: &NodeGraph,
        selected_graph_node: Option<NodeId>,
        node_graph_revision: u64,
        node_can_undo: bool,
        node_can_redo: bool,
        node_validation: Option<&crate::native_editor_runtime::NodeGraphValidation>,
        project: Option<&Project>,
        now_seconds: f64,
        compass: ViewportCompassState,
        electronics: Option<&NativeElectronicsEditor>,
    ) {
        self.rect = layout.window;
        if self.footprint_chooser.sync(project, electronics) {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        self.sync_footprint_preview(electronics);
        self.sync_inspector_footprint_preview(electronics);
        if self.asset_previews.sync(
            project.map(|p| p.path.as_path()),
            self.project_catalog.revision(),
            self.host.images_mut(),
        ) {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if self.agent_panel.borrow().opening && !self.agent_settings.prefers_reduced_motion {
            self.agent_panel.borrow_mut().spinner_step = ((now_seconds / 0.08) as u64 % 12) as u8;
        }
        self.viewport_compass.sync(
            self.palette,
            layout.canvas,
            compass,
            self.project_type == ProjectType::Game && !self.interface.workspace.active(),
        );
        self.agent_readiness = self.agent_panel.borrow_mut().prepare(
            &self.agent_settings,
            project,
            &self.agent_catalog,
        );
        let asset_results = self.agent_asset_tools.borrow_mut().poll_commands(
            project.map(|project| project.path.as_path()),
            self.agent_panel.borrow_mut().language,
        );
        for result in asset_results {
            let output = match result {
                Ok(result) => {
                    crate::commands::CommandOutput::info(result.summary, Vec::new(), result.data)
                }
                Err(error) => crate::commands::CommandOutput::error(
                    raf_core::i18n::t(
                        "app.agent_asset_failed",
                        self.agent_panel.borrow_mut().language,
                    ),
                    error,
                ),
            };
            self.log_console_output(output);
        }
        if self.agent_asset_tools.borrow_mut().take_catalog_dirty() {
            self.project_catalog.refresh();
        }
        if self
            .inspector_name_editing
            .as_ref()
            .is_some_and(|(id, _)| selected.first().copied() != Some(*id))
        {
            self.inspector_name_editing = None;
        }
        self.menu_motion
            .set_target(self.open_menu.as_ref().map_or(0.0, |_| 1.0));
        let delta = (now_seconds - self.last_sync_time_seconds).clamp(0.0, 0.25) as f32;
        self.last_sync_time_seconds = now_seconds;
        let hierarchy_motion_disabled =
            !self.agent_settings.hierarchy_animations || self.agent_settings.prefers_reduced_motion;
        self.menu_motion.advance(delta, hierarchy_motion_disabled);
        self.drag_motion.advance(delta, hierarchy_motion_disabled);
        self.agent_motion.set_target(
            self.agent_panel
                .borrow_mut()
                .sidebar_open
                .then_some(1.0)
                .unwrap_or(0.0),
        );
        self.agent_motion
            .advance(delta as f32, self.agent_settings.prefers_reduced_motion);
        self.sync_assets_motion(delta, hierarchy_motion_disabled);
        // Electronics motion shares the same clock and reduced-motion policy as
        // the rest of the shell.
        self.electronics_menu_open = self.project_type == ProjectType::Electronics
            && (electronics
                .and_then(NativeElectronicsEditor::context_menu_position)
                .is_some()
                || electronics.is_some_and(|editor| editor.delete_confirm_pending()));
        self.electronics_analysis_running =
            electronics.is_some_and(NativeElectronicsEditor::analysis_running);
        self.tick_electronics_motion(delta, hierarchy_motion_disabled);
        self.nodes_host.note_selection(selected_graph_node);
        self.nodes_host
            .tick_motion(delta, self.agent_settings.prefers_reduced_motion);
        let hierarchy_fingerprint = scene.document_revision();
        if self.last_hierarchy_fingerprint != hierarchy_fingerprint {
            self.hierarchy_folder_ids = scene
                .iter()
                .filter_map(|(id, node)| node.is_folder.then_some(id))
                .collect();
            self.hierarchy_model.invalidate();
        }
        self.project_catalog
            .sync_project(project.map(|project| project.path.as_path()));
        self.assets_project_root = project.map(|project| project.path.clone());
        if let Some(links) = self.script_relationships.sync(
            self.agent_settings.hierarchy_show_script_links
                && self.project_type == ProjectType::Game,
            project.map(|p| p.path.as_path()),
            scene,
        ) {
            if self.hierarchy_model.set_script_links(links) {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            for diagnostic in std::mem::take(&mut self.script_relationships.diagnostics) {
                self.console
                    .log(crate::console::LogLevel::Warning, diagnostic);
            }
        }
        let numeric_scope = project.map(|project| project.id);
        if self.project_numeric_scope != numeric_scope {
            self.project_numeric_edits = Default::default();
            self.project_numeric_scope = numeric_scope;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        let session_key = project.map(|project| (project.path.clone(), project.project_type));
        if self.inspector_sessions_key != session_key {
            self.inspector_sessions = project
                .map(|project| {
                    ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type)
                })
                .unwrap_or_else(|| ProjectSessionRegistry::new(self.project_type));
            self.inspector_sessions_key = session_key;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        self.sync_hierarchy_state(scene, project);
        self.host.images_mut().pump_project_images(4);
        self.interface
            .set_viewport([layout.canvas.width, layout.canvas.height]);
        if let Some((project, session)) = project.zip(self.inspector_sessions.active()) {
            if self
                .interface
                .sync(&project.path, &session.ui_document_file)
            {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
        }
        if let Some(style) = project.map(|project| project.settings.building_style) {
            if self.toolbar_state.building_style != style {
                self.toolbar_state.building_style = style;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
        }
        if self.assets_surface.refresh_requested {
            self.project_catalog.refresh();
            self.assets_surface.refresh_requested = false;
        }
        let catalog_changed = self.project_catalog.poll();
        self.verify_assets_operation(now_seconds);
        self.bottom_dock.sync_project_layout(project);
        self.sync_search_surface(layout, scene, project);
        let fingerprint = scene.render_fingerprint();
        let node_fingerprint = node_graph_revision;
        if self.last_node_fingerprint != node_fingerprint {
            self.nodes_host.clear_pending_pin();
            self.nodes_graph_bounds = nodes_graph_bounds(node_graph);
        }
        let selection_changed = self.last_selection != selected;
        if self.project_type == ProjectType::Game && selection_changed {
            if self.agent_settings.hierarchy_expand_on_select {
                if let Some(id) = selected.first().copied() {
                    self.hierarchy_model.expand_parent_chain(scene, id);
                }
            }
            if self.agent_settings.hierarchy_auto_reveal_selection {
                self.reveal_hierarchy_selection(layout.left_panel, scene, selected);
            }
        }
        let layout_changed = self.last_layout != Some(layout);
        let status_refresh_due = self.last_status_text.is_empty()
            || now_seconds - self.last_status_refresh_seconds >= 0.25;
        let status_text = status_refresh_due.then(|| self.performance_status_text());
        if status_refresh_due {
            // Sampling has its own cadence even when the displayed value is
            // unchanged; do not keep formatting telemetry on every frame.
            self.last_status_refresh_seconds = now_seconds;
        }
        let status_fps_changed = status_text
            .as_ref()
            .is_some_and(|status_text| status_text != &self.last_status_text);
        let electronics_revision = electronics.map_or(0, NativeElectronicsEditor::ui_revision);
        let agent_revision = self.agent_panel.borrow_mut().visual_revision();
        let surface_changed = selection_changed
            || layout_changed
            || self.toolbar_revision != self.last_toolbar_revision
            || self.last_scene_fingerprint != fingerprint
            || self.last_hierarchy_fingerprint != hierarchy_fingerprint
            || self.last_node_fingerprint != node_fingerprint
            || self.last_history_state != (can_undo, can_redo, can_paste)
            || self.last_console_revision != self.console.revision()
            || self.last_electronics_revision != electronics_revision
            || self.last_agent_revision != agent_revision
            || catalog_changed
            || self.last_catalog_revision != self.project_catalog.revision()
            || !self.menu_motion.is_settled()
            || !self.drag_motion.is_settled()
            || !self.agent_motion.is_settled()
            || !self.assets_menu_motion.is_settled()
            || !self.assets_view_motion.is_settled()
            || !self.assets_highlight_motion.is_settled()
            || !self.electronics_menu_motion.is_settled()
            || !self.electronics_tab_motion.is_settled()
            || self.nodes_host.is_animating();
        if self.project_type == ProjectType::Electronics {
            if let Some(editor) = electronics {
                let percent = editor.camera().zoom * 25.4 / 96.0 * 100.0;
                let caption =
                    raf_core::i18n::t("electronics.desktop.zoom", self.agent_settings.language);
                self.host.patch_text_value(
                    "electronics.pcb.options.zoom.value",
                    if self.last_layout.is_some_and(|l| {
                        l.canvas.width - crate::panels::electronics_desktop_surface::TOOL_RAIL_WIDTH
                            >= 760.0
                    }) {
                        format!("{percent:.0}%")
                    } else {
                        format!("{caption}: {percent:.0}%")
                    },
                );
            }
        }
        if !surface_changed {
            if status_fps_changed && self.project_type == ProjectType::Game {
                self.host
                    .patch_text_value("editor.status.item.4", status_text.clone().unwrap());
                self.last_status_text = status_text.unwrap();
                self.last_status_refresh_seconds = now_seconds;
            }
            return;
        }
        let document_started = std::time::Instant::now();
        let document_cause = self.workbench_change_cause(layout, fingerprint, selected);
        let mut surface = self.build_surface(
            layout,
            scene,
            selected,
            can_undo,
            can_redo,
            can_paste,
            node_graph,
            selected_graph_node,
            node_can_undo,
            node_can_redo,
            node_validation,
            project,
            electronics,
        );
        self.sync_project_numeric_fields(&mut surface);
        if self.project_type == ProjectType::Electronics {
            fn validate_fields(
                node: &mut UiNode,
                focus: &raf_ui::UiFocusState,
                controls: &raf_ui::UiControlState,
            ) {
                if node.id.starts_with("electronics.pcb.rule.")
                    && node.kind == UiNodeKind::TextInput
                    && focus.has_focus(&node.id)
                {
                    let raw = controls.text(&node.id).trim().replace(',', ".");
                    node.invalid = raw
                        .parse::<f32>()
                        .map_or(true, |v| !v.is_finite() || v <= 0.0 || v > 10.0);
                }
                if node.id.starts_with("electronics.inspector.transform.")
                    && node.kind == UiNodeKind::TextInput
                    && focus.has_focus(&node.id)
                {
                    let raw = controls.text(&node.id).trim().replace(',', ".");
                    node.invalid = raw
                        .parse::<f32>()
                        .map_or(true, |v| !v.is_finite() || v.abs() > 1_000_000.0);
                }
                for child in &mut node.children {
                    validate_fields(child, focus, controls);
                }
            }
            let state = &self.host.session().interaction;
            validate_fields(&mut surface.root, &state.focus, &state.controls);
        }
        self.host.set_surface(surface);
        if self.project_type == ProjectType::Electronics {
            fn rule_fields(node: &UiNode, fields: &mut Vec<(String, String)>) {
                if node.kind == UiNodeKind::TextInput
                    && node.id.starts_with("electronics.pcb.rule.")
                {
                    fields.push((node.id.clone(), node.text_value.clone().unwrap_or_default()));
                }
                for child in &node.children {
                    rule_fields(child, fields);
                }
            }
            let mut fields = Vec::new();
            rule_fields(&self.host.surface().root, &mut fields);
            for (id, value) in fields {
                if !self.host.session().interaction.focus.has_focus(&id) {
                    self.host
                        .session_mut()
                        .interaction
                        .controls
                        .set_text(&id, value, 16);
                }
            }
        }
        self.interface
            .seed(&mut self.host.session_mut().interaction.controls);
        if self.interface.take_image_changes() {
            if let Some(root) = &self.interface.project_root {
                self.host.images_mut().register_project_images_with_wakeup(
                    root,
                    &self.interface.document.root,
                    self.asset_previews.wakeup.clone(),
                );
            }
        }
        self.host.images_mut().pump_project_images(4);
        self.host
            .record_document_work(document_started.elapsed(), document_cause);
        if let Some(focus_id) = self.footprint_chooser.take_focus_after_sync() {
            if self.host.surface().root.find(&focus_id).is_some() {
                self.host
                    .session_mut()
                    .interaction
                    .focus
                    .request_focus(focus_id);
            }
        }
        self.host.session_mut().interaction.controls.set_text(
            "nodes.search",
            self.nodes_host.query(),
            128,
        );
        // Keep the quick-add filter in sync with the host, and restore it after
        // the popup is rebuilt so typing is never lost to a surface change.
        if self.nodes_host.palette_popup().is_some() {
            self.host.session_mut().interaction.controls.set_text(
                "nodes.palette-popup.search",
                self.nodes_host.palette_query(),
                64,
            );
        }
        if let Some(node) = selected_graph_node.and_then(|id| node_graph.node(id)) {
            for property in &node.properties {
                let key = format!("nodes.property.{}.{}", node.id.0, property.key);
                let value = self
                    .nodes_host
                    .property_draft(&key)
                    .unwrap_or(property.value.as_str());
                self.host
                    .session_mut()
                    .interaction
                    .controls
                    .set_text(&key, value, 512);
            }
        }
        self.host.session_mut().interaction.controls.set_text(
            "assets.search",
            &self.assets_surface.query,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "application-bar.command-search.value",
            self.search_surface.query(),
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "hierarchy.search",
            &self.hierarchy_query,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "hierarchy.search.global",
            &self.hierarchy_query,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "electronics.library.search",
            &self.electronics_library_query,
            256,
        );
        // The component creator is a retained form. Seed every field from the
        // live draft so a rebuild, validation error or preset change never
        // loses the author's text.
        if let Some(editor) = electronics {
            for (key, value) in editor.component_creator().control_values() {
                self.host
                    .session_mut()
                    .interaction
                    .controls
                    .set_text(key, &value, 512);
            }
        }
        self.host.session_mut().interaction.controls.set_text(
            "electronics.footprint.query",
            &self.footprint_chooser.model.query,
            192,
        );
        self.host.session_mut().interaction.controls.set_text(
            "electronics.footprint.import_path",
            &self.footprint_chooser.model.import_path,
            2048,
        );
        if let Some((target, position, rotation, _)) =
            electronics.and_then(NativeElectronicsEditor::inspector_transform)
        {
            for (axis, value) in [("x", position.x), ("y", position.y), ("rotation", rotation)] {
                let key = format!("electronics.inspector.transform.{target}:{axis}");
                if !self.host.session().interaction.focus.has_focus(&key) {
                    self.host.session_mut().interaction.controls.set_text(
                        &key,
                        format!("{value:.3}"),
                        24,
                    );
                }
            }
        }
        let electronics_component = electronics.and_then(|editor| {
            let selection = editor.selection()?;
            editor
                .schematic()
                .components
                .iter()
                .find(|component| component.id == selection.source_id)
        });
        let electronics_value_focused = self
            .host
            .session()
            .interaction
            .focus
            .has_focus("electronics.inspector.value");
        if let Some(component) = electronics_component {
            if self
                .electronics_value_draft
                .as_ref()
                .is_none_or(|draft| draft.source_id != component.id)
                || !electronics_value_focused
            {
                self.electronics_value_draft = Some(electronics_model::InspectorValueDraft::new(
                    component.id,
                    component.value.clone(),
                ));
            }
        } else {
            self.electronics_value_draft = None;
        }
        let electronics_value = self
            .electronics_value_draft
            .as_ref()
            .map(|draft| draft.value.as_str());
        self.host.session_mut().interaction.controls.set_text(
            "electronics.inspector.value",
            electronics_value.unwrap_or_default(),
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "inspector.session.new_name",
            &self.inspector_session_name,
            128,
        );
        self.host.session_mut().interaction.controls.set_text(
            "assets.script-name",
            &self.assets_surface.script_name,
            128,
        );
        self.host.session_mut().interaction.controls.set_text(
            "assets.file-name",
            &self.assets_surface.file_name,
            256,
        );
        self.host.session_mut().interaction.controls.set_text(
            "assets.rename-value",
            &self.assets_surface.rename_value,
            128,
        );
        self.host.session_mut().interaction.controls.set_text(
            "console.input",
            self.console.input(),
            4096,
        );
        self.host.session_mut().interaction.controls.set_text(
            "agent.input",
            &self.agent_panel.borrow_mut().input_text,
            16_384,
        );
        self.host.session_mut().interaction.controls.set_text(
            "agent.new-model.id",
            &self.agent_panel.borrow_mut().new_model_id,
            256,
        );
        if let Some(project) = project {
            self.seed_project_settings_values(project);
        }
        if self.agent_settings.inspector_live_transform_updates
            || !self.inspector_transform_drag_active
        {
            self.seed_inspector_values(scene, selected);
        }
        if let Some((id, value)) = self.hierarchy_renaming.as_ref() {
            self.host.session_mut().interaction.controls.set_text(
                &format!("hierarchy.rename.{id}", id = id.0),
                value,
                256,
            );
        }
        self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
        self.last_toolbar_revision = self.toolbar_revision;
        self.last_scene_fingerprint = fingerprint;
        self.last_hierarchy_fingerprint = hierarchy_fingerprint;
        self.last_node_fingerprint = node_fingerprint;
        self.last_selection = selected.to_vec();
        self.last_layout = Some(layout);
        self.last_history_state = (can_undo, can_redo, can_paste);
        self.last_console_revision = self.console.revision();
        self.last_electronics_revision = electronics_revision;
        self.last_agent_revision = agent_revision;
        self.last_catalog_revision = self.project_catalog.revision();
        if let Some(status_text) = status_text {
            self.last_status_text = status_text;
            self.last_status_refresh_seconds = now_seconds;
        }
    }

    pub(super) fn hierarchy_tree_viewport_height(&self, left: EditorRect) -> f32 {
        self.host
            .layout_rect("hierarchy.tree")
            .map(|rect| (rect.height - 8.0).max(1.0))
            .unwrap_or_else(|| (left.height - 130.0).max(1.0))
            .max(1.0)
    }

    fn reveal_hierarchy_selection(
        &mut self,
        left_panel: Option<EditorRect>,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
    ) {
        let Some(left) = left_panel else {
            return;
        };
        let Some(selected_id) = selected.first().copied() else {
            return;
        };
        let row_height = self.agent_settings.hierarchy_row_height.max(1.0);
        let viewport_height = self.hierarchy_tree_viewport_height(left);
        let view = self.hierarchy_model.refresh(
            scene,
            &self.hierarchy_query,
            self.agent_settings.hierarchy_show_hidden,
            self.hierarchy_scroll_offset,
            viewport_height,
            row_height,
        );
        let Some(index) = self.hierarchy_model.row_position(selected_id) else {
            return;
        };
        let selected_top = index as f32 * row_height;
        let selected_bottom = selected_top + row_height;
        let max_offset = (view.total_rows as f32 * row_height - viewport_height).max(0.0);
        let current = self.hierarchy_scroll_offset.clamp(0.0, max_offset);
        let next = if selected_top < current {
            selected_top
        } else if selected_bottom > current + viewport_height {
            selected_bottom - viewport_height
        } else {
            current
        }
        .clamp(0.0, max_offset);
        let scroll_changed = {
            let controls = &mut self.host.session_mut().interaction.controls;
            sync_hierarchy_scroll_state(
                controls,
                &mut self.hierarchy_scroll_offset,
                next,
                max_offset,
            )
        };
        if scroll_changed {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }

    fn sync_search_surface(
        &mut self,
        layout: EditorFrameLayout,
        scene: &SceneGraph,
        project: Option<&Project>,
    ) {
        if !self.search_surface.is_open() {
            return;
        }
        let query = self.search_surface.query().trim().to_ascii_lowercase();
        let matches = |label: &str, detail: &str| {
            query.is_empty()
                || label.to_ascii_lowercase().contains(&query)
                || detail.to_ascii_lowercase().contains(&query)
        };
        let mut results = Vec::new();
        for command in &self.agent_catalog.commands {
            let label = format!("/{}", command.name);
            let detail = command.category.clone();
            if matches(&label, &detail) {
                results.push(SearchResult {
                    label,
                    detail,
                    kind: SearchResultKind::Command(format!("console.submit:/{}", command.name)),
                    icon: UiIconId::Menu,
                });
            }
        }
        if let Some(project) = project {
            let detail = project.path.display().to_string();
            if matches(&project.name, &detail) {
                results.push(SearchResult {
                    label: project.name.clone(),
                    detail,
                    kind: SearchResultKind::Project(project.name.clone()),
                    icon: UiIconId::Project,
                });
            }
        }
        for (id, node) in scene.iter() {
            if matches(&node.name, "Hierarchy") {
                results.push(SearchResult {
                    label: node.name.clone(),
                    detail: format!("Hierarchy / {}", id.0),
                    kind: SearchResultKind::Hierarchy(id),
                    icon: UiIconId::Cube,
                });
            }
        }
        let width = self.rect.width.min(620.0).max(320.0);
        let height = ((self.rect.height - 120.0).max(1.0)).min(480.0).max(180.0);
        self.search_surface
            .set_results(results, SearchSurfaceState::Ready);
        self.search_surface.sync(
            self.palette,
            EditorRect::new(
                (self.rect.width - width) * 0.5,
                layout.application_bar.height + 8.0,
                width,
                height,
            ),
        );
    }

    /// Advances the Electronics entrance and panel crossfade tweens.
    ///
    /// Called from the same place the other shell motions are advanced, so both
    /// follow the workbench clock and the global reduced-motion preference.
    pub(crate) fn tick_electronics_motion(&mut self, delta_seconds: f32, reduced_motion: bool) {
        self.electronics_menu_motion
            .set_target(if self.electronics_menu_open { 0.0 } else { 1.0 });
        self.electronics_menu_motion
            .advance(delta_seconds, reduced_motion);
        self.electronics_tab_motion.set_target(1.0);
        self.electronics_tab_motion
            .advance(delta_seconds, reduced_motion);
        self.electronics_hint_motion
            .set_target(if self.electronics_menu_open {
                0.25
            } else {
                1.0
            });
        self.electronics_hint_motion
            .advance(delta_seconds, reduced_motion);
        // Indeterminate progress sweep for the DRC/Simulation dock. It only
        // advances while an analysis actually runs, so an idle editor never
        // pays for it, and a reduced-motion host parks it at a fixed point
        // instead of animating.
        if self.electronics_analysis_running && !reduced_motion {
            self.electronics_analysis_phase =
                (self.electronics_analysis_phase + delta_seconds * 0.55).fract();
        } else if !self.electronics_analysis_running {
            self.electronics_analysis_phase = 0.0;
        }
    }

    /// 0..1 sweep of the indeterminate analysis progress track.
    ///
    /// The host owns the clock so the surface stays declarative. Returns
    /// `0.0` when no analysis is running, which the surface renders as a
    /// static track rather than an empty one.
    pub(crate) fn electronics_analysis_phase(&self) -> f32 {
        if self.electronics_analysis_running {
            self.electronics_analysis_phase
        } else {
            0.0
        }
    }

    /// Opacity sample for the canvas tool hint.
    ///
    /// The hint explains what the active tool expects, which is only useful
    /// while the pointer is working on the canvas. It recedes behind a
    /// floating menu so the two never overlap.
    pub(crate) fn electronics_hint_fade(&self) -> f32 {
        self.electronics_hint_motion.value()
    }

    /// Electronics menu entrance sample: 0.0 fully open, 1.0 fully closed.
    pub(crate) fn electronics_menu_entrance(&self) -> f32 {
        self.electronics_menu_motion.value()
    }

    /// Electronics dock panel crossfade sample: 1.0 means settled on the new
    /// panel, lower values mean the swap is still in progress.
    pub(crate) fn electronics_tab_crossfade(&self) -> f32 {
        self.electronics_tab_motion.value()
    }

    /// Restarts the panel crossfade when the Electronics dock changes its active
    /// analysis tab, so the swap reads as a transition instead of a hard cut.
    pub(crate) fn note_electronics_tab(&mut self, tab: &str) {
        if self.electronics_active_tab.as_deref() == Some(tab) {
            return;
        }
        self.electronics_active_tab = Some(tab.to_string());
        self.electronics_tab_motion.set_immediate(0.0);
        self.electronics_tab_motion.set_target(1.0);
    }

    /// Normalized 0..1 samples shared by the Assets panel and its overlays.
    pub(super) fn assets_motion(&self) -> AssetsMotion {
        AssetsMotion {
            overlay: self.assets_menu_motion.value(),
            view: self.assets_view_motion.value(),
            highlight: self.assets_highlight_motion.value(),
        }
    }

    /// Advances every Assets tween from the workbench clock so the panel, its
    /// overlays and the row highlight share one easing policy.
    fn sync_assets_motion(&mut self, delta_seconds: f32, reduced_motion: bool) {
        let overlay_open = self.assets_surface.has_open_overlay();
        self.assets_menu_motion
            .set_target(if overlay_open { 1.0 } else { 0.0 });
        if self.assets_filter_key != Some(self.assets_surface.filter) {
            self.assets_filter_key = Some(self.assets_surface.filter);
            self.assets_view_motion.set_immediate(0.0);
        }
        self.assets_view_motion.set_target(1.0);
        if self.assets_surface.highlight_asset.is_some() {
            let pulse_done = self.assets_highlight_motion.is_settled()
                && self.assets_highlight_motion.value() >= 1.0;
            if pulse_done {
                self.assets_surface.highlight_asset = None;
                self.assets_highlight_motion.set_target(0.0);
            } else {
                self.assets_highlight_motion.set_target(1.0);
            }
        } else {
            self.assets_highlight_motion.set_target(0.0);
        }
        self.assets_menu_motion
            .advance(delta_seconds, reduced_motion);
        self.assets_view_motion
            .advance(delta_seconds, reduced_motion);
        self.assets_highlight_motion
            .advance(delta_seconds, reduced_motion);
    }

    fn seed_inspector_values(&mut self, scene: &SceneGraph, selected: &[SceneNodeId]) {
        if self.hierarchy_active_tab == "lighting" {
            self.seed_lighting_values(scene);
            return;
        }
        let Some(id) = selected.first().copied() else {
            return;
        };
        let Some(node) = scene.get(id) else {
            return;
        };
        let focused = self.host.session().interaction.focus.focused.clone();
        let controls = &mut self.host.session_mut().interaction.controls;
        for (field, value) in [
            ("mass", node.rigid_body.mass.to_string()),
            ("friction", node.rigid_body.friction.to_string()),
            ("restitution", node.rigid_body.restitution.to_string()),
            (
                "angular_damping",
                node.rigid_body.angular_damping.to_string(),
            ),
            ("gravity_scale", node.rigid_body.gravity_scale.to_string()),
            ("collision_layer", node.collider.collision_layer.to_string()),
            ("collision_mask", node.collider.collision_mask.to_string()),
        ] {
            let key = format!("inspector.physics.{field}.text");
            if focused.as_deref() != Some(key.as_str()) || !controls.has_text(&key) {
                controls.set_text(key, value, 24);
            }
        }
        let joint_key = "inspector.physics.joints.text";
        if focused.as_deref() != Some(joint_key) || !controls.has_text(joint_key) {
            if let Ok(value) = serde_json::to_string(&node.physics_joints) {
                controls.set_text(joint_key, value, 32768);
            }
        }
        crate::native_behavior_authoring::seed_parameters(
            node,
            controls,
            focused.as_deref(),
            self.inspector_view.variables && self.inspector_view.components,
            self.inspector_view.behavior_parameters,
        );
        if let Some(lens) = &node.game_camera {
            for (field, value) in [
                ("fov_degrees", lens.fov_degrees),
                ("near", lens.near),
                ("far", lens.far),
                ("ortho_scale", lens.ortho_scale),
            ] {
                let key = format!("inspector.camera.{field}.text");
                if focused.as_deref() != Some(key.as_str()) || !controls.has_text(&key) {
                    controls.set_text(key, value.to_string(), 24);
                }
            }
        }
        let name = self
            .inspector_name_editing
            .as_ref()
            .filter(|(editing_id, _)| *editing_id == id)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| node.name.clone());
        controls.set_text("inspector.name", name, 256);
        for (label, values) in [
            ("position", node.position.to_array()),
            ("rotation", node.rotation.to_array()),
            ("scale", node.scale.to_array()),
            ("size", node.primitive.dimensions(node.scale).to_array()),
            ("velocity", node.rigid_body.velocity.to_array()),
            (
                "angular_velocity",
                node.rigid_body.angular_velocity.to_array(),
            ),
        ] {
            for (axis, value) in [("x", values[0]), ("y", values[1]), ("z", values[2])] {
                let display_value = if label == "position" {
                    self.agent_settings.display_unit.from_meters(value)
                } else {
                    value
                };
                let key = format!("inspector.{label}.{axis}.text");
                let input = format!("inspector.{label}.{axis}.input");
                if focused.as_deref() != Some(input.as_str()) || !controls.has_text(&key) {
                    controls.set_text(key, format!("{display_value:.4}"), 24);
                }
            }
        }
        controls.set_text(
            "inspector.color.hex",
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                node.color.r, node.color.g, node.color.b, node.color.a
            ),
            9,
        );
    }
}

/// `[min_x, min_y, width, height]` of every node in world units, including the
/// implicit fallback positions used by nodes saved without one. Cached by
/// `sync` so the Fit command never walks the graph from input.
pub(crate) fn nodes_graph_bounds(graph: &NodeGraph) -> [f32; 4] {
    let mut min = [f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN];
    for (index, node) in graph.nodes.iter().enumerate() {
        let geometry = crate::panels::nodes_canvas::node_geometry(node, index, 1.0);
        min[0] = min[0].min(geometry.position[0]);
        min[1] = min[1].min(geometry.position[1]);
        max[0] = max[0].max(geometry.position[0] + geometry.size[0]);
        max[1] = max[1].max(geometry.position[1] + geometry.size[1]);
    }
    if min[0] > max[0] {
        return [0.0, 0.0, 0.0, 0.0];
    }
    [min[0], min[1], max[0] - min[0], max[1] - min[1]]
}

pub(super) fn sync_hierarchy_scroll_state(
    controls: &mut raf_ui::UiControlState,
    projection_offset: &mut f32,
    requested_offset: f32,
    max_offset: f32,
) -> bool {
    const SCROLL_ID: &str = "hierarchy.tree";

    let previous_control_offset = controls.scroll_offset(SCROLL_ID);
    let previous_projection_offset = *projection_offset;
    let max_offset = if max_offset.is_finite() {
        max_offset.max(0.0)
    } else {
        0.0
    };
    let requested_offset = if requested_offset.is_finite() {
        requested_offset.clamp(0.0, max_offset)
    } else {
        0.0
    };

    controls.set_scroll_metrics(SCROLL_ID, [0.0, max_offset]);
    let applied_offset = controls.set_scroll_offset(SCROLL_ID, [0.0, requested_offset]);
    *projection_offset = applied_offset[1];

    previous_control_offset != applied_offset
        || (previous_projection_offset - applied_offset[1]).abs() > f32::EPSILON
}
