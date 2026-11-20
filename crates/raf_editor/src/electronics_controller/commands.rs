//! Command dispatch, surface switching, analysis lifecycle and view commands.

use super::*;

use super::inspector::report_lines;

impl NativeElectronicsEditor {
    /// Preserve window coordinates through panel changes, with a minimal pan
    /// only when the new panel would otherwise cover the selected component.
    pub fn preserve_camera_for_canvas_change(&mut self, old: EditorRect, new: EditorRect) {
        if !self.camera_initialized || old == new {
            return;
        }
        let delta = Vec2::new(
            new.x + new.width * 0.5 - old.x - old.width * 0.5,
            new.y + new.height * 0.5 - old.y - old.height * 0.5,
        );
        self.camera.center += delta / self.camera.zoom.max(0.001);
        if let Some(center) = self.selection.and_then(|selection| {
            self.scene
                .objects
                .iter()
                .find(|object| {
                    object.source_id == Some(selection.source_id)
                        && object.kind == CadObjectKind::Component
                })
                .and_then(|object| object.rect)
                .map(|rect| rect.center)
        }) {
            let size = Vec2::new(new.width.max(1.0), new.height.max(1.0));
            let screen = self.camera.screen_from_world(center, size);
            let margin = Vec2::splat(40.0).min(size * 0.25);
            let visible = screen.clamp(margin, size - margin);
            self.camera.center += (screen - visible) / self.camera.zoom.max(0.001);
        }
        self.touch();
    }

    pub fn apply_engine_settings(&mut self, settings: &EngineSettings) {
        let grid_step = settings.electronics_grid_step_mm.clamp(5.0, 100.0);
        let grid_opacity = settings.electronics_grid_opacity.clamp(0.2, 1.0);
        let changed = self.grid_visible != settings.grid_visible
            || self.snap_enabled != settings.snap_to_grid
            || (self.grid_step - grid_step).abs() > f32::EPSILON
            || (self.grid_opacity - grid_opacity).abs() > f32::EPSILON;
        let labels_changed = self.labels_visible != settings.show_viewport_labels;
        let language_changed = self.language != settings.language;
        self.language = settings.language;
        self.grid_visible = settings.grid_visible;
        self.snap_enabled = settings.snap_to_grid;
        self.grid_step = grid_step;
        self.schematic_grid_step = grid_step;
        self.pcb_grid_step = grid_step;
        self.grid_opacity = grid_opacity;
        self.labels_visible = settings.show_viewport_labels;
        if changed || labels_changed || language_changed {
            self.touch_ui();
            self.touch();
        }
    }

    pub fn apply_project_settings(&mut self, settings: &ProjectSettings) {
        let schematic_step = settings
            .electronics_schematic_grid_step_mm
            .clamp(1.0, 100.0);
        let pcb_step = settings.electronics_pcb_grid_step_mm.clamp(0.05, 100.0);
        let active_step = match self.active_surface {
            CadSurfaceKind::Schematic => schematic_step,
            CadSurfaceKind::Pcb => pcb_step,
        };
        let changed = (self.schematic_grid_step - schematic_step).abs() > f32::EPSILON
            || (self.pcb_grid_step - pcb_step).abs() > f32::EPSILON
            || self.snap_enabled != settings.electronics_snap_to_grid
            || (self.grid_step - active_step).abs() > f32::EPSILON;
        self.schematic_grid_step = schematic_step;
        self.pcb_grid_step = pcb_step;
        self.grid_step = active_step;
        self.snap_enabled = settings.electronics_snap_to_grid;
        if changed {
            self.touch_ui();
            self.touch();
        }
    }

    pub(crate) fn start_drc_analysis(&mut self) {
        self.start_analysis(AnalysisKind::Drc);
    }

    pub(crate) fn start_simulation_analysis(&mut self) {
        self.start_analysis(AnalysisKind::Simulation);
    }

    pub(crate) fn lab_fingerprint(&self) -> u64 {
        self.built_schematic_fingerprint
    }

    pub(crate) fn prepare_lab(&mut self, root: &std::path::Path, schematic: &std::path::Path) {
        self.lab.load_for(root, schematic);
        self.lab
            .sync_graph(&self.schematic, self.built_schematic_fingerprint);
    }

    pub(crate) fn start_lab_simulation(
        &mut self,
        tool: Option<String>,
        persist: bool,
    ) -> Result<(), String> {
        if self.analysis_running() {
            return Err("An Electronics analysis is already running.".into());
        }
        self.lab.request_dc(tool, persist);
        self.start_simulation_analysis();
        self.lab.touch();
        Ok(())
    }

    pub(crate) fn poll_lab(&mut self, visible: bool) -> bool {
        let mut changed = false;
        if self.lab.dc_requested {
            changed |= self.poll_analysis();
        }
        if visible {
            self.lab
                .sync_graph(&self.schematic, self.built_schematic_fingerprint);
        }
        if self.lab.pending() {
            changed |= self.lab.poll();
        }
        changed
    }

    /// Cancels a running analysis without destroying the previous result.
    ///
    /// A cancelled run produced no report, so dropping the stored one would hide
    /// a real previous finding and replacing it with "no issues" would be a lie.
    /// The dock keeps the lines and receives an explicit cancelled status.
    pub(crate) fn cancel_analysis(&mut self) {
        let Some(task) = self.analysis_task.take() else {
            return;
        };
        let kind = task.kind();
        task.cancel();
        self.lab.fail_dc("Simulation cancelled.");
        self.push_report_line(
            kind,
            ElectronicsReportLineKind::Status,
            "electronics.analysis.cancelled",
        );
        self.touch_ui();
    }

    pub(crate) fn analysis_running(&self) -> bool {
        self.analysis_task.is_some()
    }

    pub(crate) fn poll_analysis(&mut self) -> bool {
        let Some(task) = self.analysis_task.take() else {
            return false;
        };
        match task.try_result() {
            Ok(Some(AnalysisResult::Drc(report))) => {
                let lines = report_lines(&report, self.language);
                self.drc_report = Some(report);
                self.drc_report_surface = Some(task.surface);
                self.drc_lines = lines;
                self.drc_stale = false;
                self.simulation_stale = true;
                self.clear_surface_errors_for_key("electronics.error.drc_failed");
                self.rebuild_scene_with_drc();
                self.touch_ui();
                true
            }
            Ok(Some(AnalysisResult::Simulation(result, netlist))) => {
                self.accept_simulation_result(result, &netlist);
                true
            }
            Ok(None) => {
                self.analysis_task = Some(task);
                false
            }
            Err(()) => {
                let kind = task.kind();
                self.lab
                    .fail_dc("Simulation worker did not return a result.");
                self.drc_stale |= kind == AnalysisKind::Drc;
                self.simulation_stale |= kind == AnalysisKind::Simulation;
                let key = match kind {
                    AnalysisKind::Drc => "electronics.error.drc_failed",
                    AnalysisKind::Simulation => "electronics.error.simulation_failed",
                };
                self.push_report_line(kind, ElectronicsReportLineKind::Failed, key);
                self.push_surface_error_key(key);
                self.touch_ui();
                true
            }
        }
    }

    pub fn zoom_in(&mut self) {
        self.camera.zoom = (self.camera.zoom * 1.15).clamp(MIN_ZOOM, MAX_ZOOM);
        self.touch();
    }

    pub fn zoom_out(&mut self) {
        self.camera.zoom = (self.camera.zoom / 1.15).clamp(MIN_ZOOM, MAX_ZOOM);
        self.touch();
    }

    pub fn dispatch_action(
        &mut self,
        action: crate::electronics_action::ElectronicsAction,
    ) -> bool {
        use crate::electronics_action::ElectronicsAction;
        match action {
            ElectronicsAction::SelectTool(tool) => {
                self.set_tool(tool);
                true
            }
            ElectronicsAction::FitView => {
                self.fit_view();
                true
            }
            ElectronicsAction::ToggleGrid => {
                self.toggle_grid();
                true
            }
            ElectronicsAction::ToggleLabels => {
                self.toggle_labels();
                true
            }
            ElectronicsAction::ToggleSnap => {
                self.toggle_snap();
                true
            }
            ElectronicsAction::ZoomIn => {
                self.zoom_in();
                true
            }
            ElectronicsAction::ZoomOut => {
                self.zoom_out();
                true
            }
            ElectronicsAction::Rotate => {
                self.rotate_selected();
                true
            }
            ElectronicsAction::Delete => {
                self.request_delete_selected();
                true
            }
            ElectronicsAction::Undo => {
                self.undo();
                true
            }
            ElectronicsAction::Redo => {
                self.redo();
                true
            }
            // The controller owns the documents but not the project session, so
            // it cannot save without creating a second persistence path. The
            // application boundary owns `project.save`; an Electron-only caller
            // is told so instead of receiving a silent success.
            ElectronicsAction::SaveProject => {
                self.push_surface_error_key("electronics.error.save_outside_application");
                false
            }
            ElectronicsAction::SetSurface(surface) => {
                self.set_surface(surface);
                true
            }
            ElectronicsAction::SyncPcb => {
                self.sync_pcb_from_schematic();
                true
            }
            // Both analyses run on the background task handle so the UI thread
            // keeps painting and the dock can show running, cancel and failed.
            ElectronicsAction::RunDrc => {
                self.start_drc_analysis();
                true
            }
            ElectronicsAction::RunSimulation => {
                self.start_simulation_analysis();
                true
            }
            ElectronicsAction::CancelAnalysis => {
                self.cancel_analysis();
                true
            }
            // Blocking confirmation: the surface armed a destructive action and is
            // waiting for an explicit accept or dismiss before anything mutates.
            ElectronicsAction::ConfirmDelete => {
                self.confirm_delete();
                self.context_menu_position = None;
                true
            }
            ElectronicsAction::CancelPending => {
                self.cancel_pending_action();
                true
            }
            // Moves the document selection to whatever the clicked finding points
            // at, so a report row is a navigation target and not just text.
            ElectronicsAction::FocusAnalysisIssue { panel, index } => {
                let focused = self.focus_analysis_issue(&panel, index);
                if !focused {
                    self.push_surface_error_key("electronics.analysis.focus_line_hint");
                }
                focused
            }
            ElectronicsAction::FocusAnalysisComponent { source_id } => {
                if self.select_component_by_id(source_id) {
                    self.recenter_on_selection();
                    true
                } else {
                    self.push_surface_error_key("electronics.analysis.focus_issue_hint");
                    false
                }
            }
            ElectronicsAction::ContextDelete => {
                self.request_delete_selected();
                self.context_menu_position = None;
                true
            }
            ElectronicsAction::ContextRoute => {
                self.route_selected_airwire();
                self.context_menu_position = None;
                true
            }
            ElectronicsAction::ContextSelect => {
                self.context_menu_position = None;
                self.touch_ui();
                true
            }
            ElectronicsAction::ContextDuplicate => {
                if let Some(selection) = self.selection {
                    if let Some(index) = self
                        .schematic
                        .components
                        .iter()
                        .position(|component| component.id == selection.source_id)
                    {
                        let before = self.snapshot();
                        if let Some(id) = self.schematic.duplicate_component(index) {
                            if self.active_surface == CadSurfaceKind::Pcb {
                                self.pcb.sync_from_schematic(&self.schematic);
                            }
                            self.selection = Some(ElectronicsSelection {
                                source_id: id,
                                kind: ElectronicsSelectionKind::Component,
                            });
                            self.history.record(before, &self.snapshot());
                            self.dirty = true;
                            self.rebuild_scene();
                        }
                    }
                }
                self.context_menu_position = None;
                self.touch_ui();
                true
            }
            ElectronicsAction::ContextCancel => {
                self.wire_start = None;
                self.pcb_route_draft = None;
                self.context_menu_position = None;
                self.touch_ui();
                true
            }
            ElectronicsAction::CommitInspectorValue(val) => self.set_selected_value(&val),
            ElectronicsAction::CommitInspectorValueFor { source_id, value } => {
                self.set_component_value(source_id, &value)
            }
            ElectronicsAction::SelectNavigator(idx) => self.select_component(idx),
            ElectronicsAction::InspectNavigator(idx) => {
                let selected = self.select_component(idx);
                if selected {
                    self.touch_ui();
                }
                selected
            }
            ElectronicsAction::SelectTab(_) => {
                self.touch_ui();
                true
            }
        }
    }

    pub fn apply_ui_command(&mut self, command: &str) -> bool {
        if self.apply_component_creator_command(command) {
            return true;
        }
        if self.apply_pcb_tool_command(command) {
            return true;
        }
        if let Some(payload) = command.strip_prefix("electronics.footprint.assign_for:") {
            return self.apply_footprint_assignment_command(payload);
        }
        if let Some(action) = crate::electronics_action::ElectronicsAction::parse(command) {
            return self.dispatch_action(action);
        }
        if let Some(key) = command.strip_prefix("electronics.layer.toggle.") {
            return self.toggle_pcb_layer(key);
        }
        if let Some(index) = command.strip_prefix("electronics.navigator.select-wire.") {
            if let Ok(index) = index.parse::<usize>() {
                return self.select_wire(index);
            }
        }
        if let Some(index) = command.strip_prefix("electronics.navigator.select-trace.") {
            if let Ok(index) = index.parse::<usize>() {
                return self.select_trace(index);
            }
        }
        if let Some(index) = command.strip_prefix("electronics.place.template.") {
            if let Ok(index) = index.parse::<usize>() {
                if index < self.library.components.len() {
                    self.placement_template = Some(index);
                    self.set_tool(ElectronicsTool::Place);
                    return true;
                }
            }
        }
        if let Some(index) = command.strip_prefix("electronics.library.drag.start.") {
            if let Ok(index) = index.parse::<usize>() {
                return self.begin_library_drag(index);
            }
        }
        if command.starts_with("electronics.library.drag.move.")
            || command.starts_with("electronics.library.drag.end.")
        {
            // Pointer coordinates are consumed by the native frame
            // so the controller can convert them against the actual
            // Electronics canvas. The command itself is retained as
            // a semantic no-op for direct/headless callers.
            return self.placement_drag_active;
        }
        false
    }

    pub fn set_surface(&mut self, surface: CadSurfaceKind) {
        if self.active_surface == surface {
            return;
        }
        // The creator is schematic authoring; switching the CAD surface must
        // not leave a stale modal owning focus over a different document.
        if self.component_creator.is_open() {
            self.component_creator = ComponentCreatorState::default();
        }
        let cross_probe_component = self
            .selection
            .filter(|selection| {
                matches!(
                    selection.kind,
                    ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
                )
            })
            .map(|selection| selection.source_id);
        let previous_camera = self.camera_initialized.then_some(self.camera);
        let next_camera = self.inactive_camera.take();
        self.inactive_camera = previous_camera;
        self.camera = next_camera.unwrap_or_default();
        self.camera_initialized = next_camera.is_some();
        self.active_surface = surface;
        // A PCB view is a derived document. Keep the established workflow in
        // which entering it materializes missing footprints/links from the
        // schematic, but through the quiet projection: pressing the PCB tab is
        // navigation, not an edit, so it must not consume the user's next undo.
        if surface == CadSurfaceKind::Pcb {
            self.sync_pcb_from_schematic_quiet();
        }
        self.grid_step = match surface {
            CadSurfaceKind::Schematic => self.schematic_grid_step,
            CadSurfaceKind::Pcb => self.pcb_grid_step,
        };
        self.selection = None;
        self.hovered = None;
        self.highlight_net = None;
        self.interaction.clear_selection();
        self.wire_start = None;
        self.board_outline_start = None;
        self.board_outline_vertices.clear();
        self.pcb_route_draft = None;
        self.tool = ElectronicsTool::Select;
        self.placement_template = None;
        self.placement_drag_active = false;
        self.placement_preview = None;
        self.pointer_world = None;
        self.rebuild_scene_internal();
        if let Some(component_id) = cross_probe_component {
            let index = match surface {
                CadSurfaceKind::Schematic => self
                    .schematic
                    .components
                    .iter()
                    .position(|component| component.id == component_id),
                CadSurfaceKind::Pcb => self
                    .pcb
                    .components
                    .iter()
                    .position(|component| component.component_id == component_id),
            };
            if let Some(index) = index {
                self.select_component(index);
            }
        }
        self.touch_ui();
    }

    /// Resolves the canvas presentation options for one frame.
    ///
    /// `palette` decides the CAD backdrop and grid so the canvas follows the
    /// active theme instead of staying a hardcoded dark rectangle, and
    /// `scale_factor` is target-pixels-per-logical-point so line work keeps its
    /// visual weight on high-DPI targets.
    pub fn render_options(
        &mut self,
        canvas: EditorRect,
        palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette,
        scale_factor: f32,
    ) -> CadSurfaceOptions {
        self.ensure_camera(canvas);
        let size = Vec2::new(canvas.width.max(1.0), canvas.height.max(1.0));
        let tokens = crate::panels::electronics_surface::electronics_tokens(palette);
        let mut options = CadSurfaceOptions::default();
        options.clear_color = tokens.canvas;
        options.world_bounds = Some(self.camera.world_bounds(size));
        options.selected_source_ids = self
            .selection
            .map(|selection| vec![*selection.source_id.as_bytes()])
            .unwrap_or_default();
        // The hover target is drawn differently from the selection, so the two
        // never read as the same outline.
        options.hovered_source_ids = self.hovered_source_ids();
        options.highlight_net_id = self.highlight_net;
        options.raster_scale = if scale_factor.is_finite() {
            scale_factor.clamp(0.25, 4.0)
        } else {
            1.0
        };
        // Picking goes through `cad_interaction::pick_editable`, which also
        // filters the non-interactive overlays. Building the presentation hit
        // regions as well only cloned four strings per object per frame for a
        // list nobody reads.
        options.collect_hit_regions = false;
        options.grid_step = self.grid_step;
        options.dotted_grid = self.active_surface == CadSurfaceKind::Pcb;
        // Grid alpha is applied on top of a token-derived base, because a fixed
        // dark-theme blue is invisible over a light canvas and a fixed dark
        // canvas is a black hole in a light editor.
        let grid_opacity = self.grid_opacity.clamp(0.0, 1.0);
        let base = |color: [u8; 4], alpha: f32| {
            let alpha = (f32::from(color[3]) * alpha * grid_opacity).round();
            [color[0], color[1], color[2], alpha.clamp(0.0, 255.0) as u8]
        };
        options.grid_color = base(tokens.border, 0.30);
        options.major_grid_color = base(tokens.border, 0.55);
        if options.dotted_grid {
            options.grid_color = base(tokens.text_muted, 0.32);
            options.major_grid_color = base(tokens.text_muted, 0.55);
        }
        options.axis_color = base(tokens.text_muted, 0.75);
        options.show_grid = self.grid_visible;
        options.show_labels = self.labels_visible;
        options
    }

    pub fn fit_view(&mut self) {
        self.camera_initialized = false;
        self.touch();
    }

    pub fn toggle_grid(&mut self) {
        self.grid_visible = !self.grid_visible;
        self.touch_ui();
    }

    pub fn toggle_labels(&mut self) {
        self.labels_visible = !self.labels_visible;
        self.touch_ui();
    }

    pub fn run_drc(&mut self) {
        self.cancel_analysis();
        let report = match self.active_surface {
            CadSurfaceKind::Schematic => self.schematic.run_drc(),
            CadSurfaceKind::Pcb => self.pcb.run_drc(),
        };
        self.drc_lines = report_lines(&report, self.language);
        self.drc_report = Some(report);
        self.drc_report_surface = Some(self.active_surface);
        self.drc_stale = false;
        self.simulation_stale = true;
        self.clear_surface_errors_for_key("electronics.error.drc_failed");
        self.rebuild_scene_with_drc();
        self.touch_ui();
    }

    pub fn run_simulation(&mut self) {
        self.cancel_analysis();
        if let Some((result, netlist)) = raf_electronics::simulation::analyze_dc_cancellable(
            &self.schematic,
            &std::sync::atomic::AtomicBool::new(false),
        ) {
            self.accept_simulation_result(result, &netlist);
        }
    }

    fn start_analysis(&mut self, kind: AnalysisKind) {
        if let Some(task) = self.analysis_task.take() {
            task.cancel();
            self.lab
                .fail_dc("Simulation was replaced by another analysis.");
            self.push_report_line(
                kind,
                ElectronicsReportLineKind::Status,
                "electronics.analysis.cancelled",
            );
        }
        // The previous result is kept while the new one runs: it stays visible
        // and `drc_stale` keeps telling the surface it is obsolete.
        self.push_report_line(
            kind,
            ElectronicsReportLineKind::Running,
            "electronics.analysis.running",
        );
        let pcb = (kind == AnalysisKind::Drc && self.active_surface == CadSurfaceKind::Pcb)
            .then(|| self.pcb.clone());
        let worker = self.analysis_worker.get_or_insert_with(AnalysisWorker::new);
        self.analysis_task = Some(worker.submit(kind, self.schematic.clone(), pcb));
        self.touch_ui();
    }
}
