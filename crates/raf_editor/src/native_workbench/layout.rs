//! Workbench construction, frame geometry, dock layout and compositor layers.

use super::*;

impl NativeGameWorkbench {
    pub(super) fn new_workbench(
        graphics: &NativeGraphicsContext<'_>,
        palette: StudioUiPalette,
        rect: EditorRect,
    ) -> Self {
        let empty_surface = UiSurface::new(
            "editor.native.workbench.empty",
            palette,
            UiNode::new("editor.native.workbench.empty.root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::None))
                .with_style(UiStyle::transparent()),
        );
        let mut host = graphics.create_ui_host(empty_surface, WORKBENCH_CLEAR);
        register_bar_images(host.images_mut());
        let overlay_surface = UiSurface::new(
            "editor.native.electronics.overlay.empty",
            palette,
            UiNode::new(
                "editor.native.electronics.overlay.empty.root",
                UiNodeKind::Root,
            )
            .with_layout(UiLayout::fill(UiFlow::None))
            .with_style(UiStyle::transparent()),
        );
        let electronics_overlay_host = graphics.create_ui_host(overlay_surface, WORKBENCH_CLEAR);
        Self {
            region: InputRegionId::from_static("native.editor.workbench"),
            ui_performance: Default::default(),
            hierarchy_document_cache: None,
            rect,
            host,
            electronics_overlay_host,
            electronics_images_registered: false,
            electronics_overlay_images_registered: false,
            electronics_overlay_key: None,
            electronics_minimap_cache: Default::default(),
            electronics_overlay_canvas: EditorRect::default(),
            electronics_overlay_active: false,
            search_surface: SearchSurfaceHost::new(graphics, rect, palette),
            viewport_compass: ViewportCompassHost::new(graphics, palette),
            agent_panel: std::rc::Rc::new(std::cell::RefCell::new(AgentPanel::default())),
            agent_settings: EngineSettings::default(),
            agent_catalog: CommandCatalog::builtin(),
            agent_asset_tools: std::rc::Rc::new(std::cell::RefCell::new(
                crate::agent_asset_tools::AgentAssetTools::default(),
            )),
            agent_save: Default::default(),
            agent_readiness: AgentReadiness::ProviderDisabled,
            agent_canvas_changed: false,
            settings_section: SettingsSection::Appearance,
            project_numeric_edits: Default::default(),
            project_numeric_scope: None,
            project_catalog: ProjectCatalog::default(),
            last_catalog_revision: 0,
            assets_surface: AssetsSurfaceHost::default(),
            animations: Default::default(),
            assets_menu_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            assets_view_motion: UiTween::new(1.0, UiMotionSpec::dock()),
            assets_highlight_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            assets_filter_key: None,
            assets_project_root: None,
            assets_scroll_offset: 0.0,
            asset_previews: Default::default(),
            authoring_notice: None,
            console: ConsolePanel::default(),
            palette,
            project_name: DEFAULT_PROJECT_NAME.to_string(),
            project_type: ProjectType::Game,
            interface: crate::panels::interface_surface_host::InterfaceSurfaceHost::default(),
            hierarchy_model: HierarchyModel::default(),
            script_relationships: crate::script_relationships::ScriptRelationshipIndex::default(),
            hierarchy_folder_ids: HashSet::new(),
            hierarchy_query: String::new(),
            hierarchy_scroll_offset: 0.0,
            hierarchy_active_tab: "hierarchy".to_string(),
            hierarchy_state_path: None,
            hierarchy_pending_state: None,
            hierarchy_state_dirty: false,
            hierarchy_state_writer: hierarchy_state::HierarchyStateWriter::default(),
            lighting_section: "overview".into(),
            lighting_preview_neutral: false,
            lighting_error: false,
            electronics_navigator_tab: "library".to_string(),
            electronics_library_query: String::new(),
            electronics_workspace: Default::default(),
            electronics_menu_motion: UiTween::new(1.0, UiMotionSpec::dock()),
            electronics_tab_motion: UiTween::new(1.0, UiMotionSpec::dock()),
            electronics_menu_open: false,
            electronics_active_tab: None,
            electronics_analysis_running: false,
            electronics_analysis_phase: 0.0,
            electronics_hint_motion: UiTween::new(1.0, UiMotionSpec::dock()),
            electronics_value_draft: None,
            footprint_chooser: footprints::FootprintChooserState::default(),
            inspector_footprint_signature: String::new(),
            inspector_footprint_presentation: None,
            inspector_session_name: String::new(),
            hierarchy_bookmarks: [false; 3],
            hierarchy_renaming: None,
            hierarchy_drag: None,
            inspector_name_editing: None,
            inspector_view: InspectorViewState::default(),
            inspector_transform_drag_active: false,
            toolbar_state: default_toolbar_state(),
            popup_press_consumed: false,
            runtime_toolbar: raf_player::surface::RuntimeToolbarState::default(),
            bottom_dock: EditorBottomDockHost::default(),
            inspector_sessions: ProjectSessionRegistry::new(ProjectType::Game),
            inspector_sessions_key: None,
            surface_revision: 0,
            toolbar_revision: 0,
            last_toolbar_revision: 0,
            last_console_revision: 0,
            last_electronics_revision: 0,
            last_agent_revision: 0,
            agent_scroll_projection_offset: 0.0,
            open_menu: None,
            open_submenu: None,
            search_restore_focus: None,
            menu_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            drag_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            agent_motion: UiTween::new(1.0, UiMotionSpec::dock()),
            last_sync_time_seconds: 0.0,
            presented_fps: 0.0,
            estimated_capacity_fps: 0,
            viewport_fps: 0.0,
            frame_cpu_ms: 0.0,
            frame_gpu_ms: 0.0,
            frame_gpu_timing_sampled: false,
            scene_draw_calls: 0,
            scene_upload_bytes: 0,
            scene_upload_budget_exceeded: false,
            shadow_metrics: SceneFrameMetrics::default(),
            performance_readout: PerformanceReadout::default(),
            last_performance_segment: String::new(),
            frame_activity_label: "Idle",
            redraw_reason_label: "Idle",
            canvas_status_label: "Idle",
            target_fps: 0,
            requested_target_fps: 0,
            p95_frame_time_ms: 0.0,
            hitch_count: 0,
            presentation_label: "Present",
            last_status_text: String::new(),
            last_status_refresh_seconds: 0.0,
            last_scene_fingerprint: 0,
            last_hierarchy_fingerprint: u64::MAX,
            last_selection: Vec::new(),
            last_layout: None,
            last_history_state: (false, false, false),
            hierarchy_empty_menu_open: false,
            hierarchy_primitive_menu_open: false,
            hierarchy_menu_target: None,
            hierarchy_menu_position: None,
            nodes_zoom: 1.0,
            nodes_pan: [0.0, 0.0],
            nodes_graph_bounds: [0.0, 0.0, 0.0, 0.0],
            nodes_host: NodesSurfaceHost::default(),
            last_node_fingerprint: 0,
            panel_resize: None,
        }
    }

    pub fn persist_project_layout(&mut self) {
        self.flush_hierarchy_state();
        self.bottom_dock.persist_project_layout();
    }

    pub fn bottom_dock_height(&self) -> f32 {
        self.bottom_dock.layout.height
    }

    pub fn bottom_dock_collapsed(&self) -> bool {
        self.bottom_dock.is_collapsed()
    }

    pub fn bottom_dock_effective_height(&self) -> f32 {
        self.bottom_dock.effective_height()
    }

    pub fn set_bottom_dock_height(&mut self, height: f32) -> bool {
        let changed = self.bottom_dock.set_height(height);
        if changed {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        changed
    }

    pub fn set_engine_settings(&mut self, mut settings: EngineSettings) {
        settings.viewport_render_mode = settings.viewport_render_mode.normalized();
        let next_palette = match settings.theme {
            Theme::Light => StudioUiPalette::PaperLight,
            Theme::Dark | Theme::System => StudioUiPalette::IndustrialDark,
        };
        if self.agent_settings == settings && self.palette == next_palette {
            return;
        }
        self.agent_settings = settings;
        let palette_changed = self.palette != next_palette;
        self.palette = next_palette;
        if palette_changed {
            self.electronics_overlay_key = None;
        }
        self.toolbar_state.grid_visible = self.agent_settings.grid_visible;
        self.toolbar_state.labels_visible = self.agent_settings.show_viewport_labels;
        self.toolbar_state.render_style = ViewportRenderStyle::Solid;
        let mut environment = UiEnvironment::new(
            self.rect.width.max(1.0).round() as u32,
            self.rect.height.max(1.0).round() as u32,
        );
        environment.color_mode = match self.agent_settings.theme {
            Theme::Light => UiColorMode::Light,
            Theme::Dark => UiColorMode::Dark,
            Theme::System => UiColorMode::System,
        };
        environment.prefers_reduced_motion = self.agent_settings.prefers_reduced_motion;
        environment.high_contrast = self.agent_settings.high_contrast;
        environment.reduce_transparency = self.agent_settings.reduce_transparency
            || self.agent_settings.render_quality == RenderQuality::Potato;
        environment.font_size = self.agent_settings.font_size.clamp(
            if self.project_type == ProjectType::Electronics {
                14.0
            } else {
                10.0
            },
            24.0,
        );
        environment.theme_experimental = self.agent_settings.theme_experimental.clamp(0.0, 100.0);
        environment.ui_scale = if self.agent_settings.auto_ui_scale {
            1.0
        } else {
            self.agent_settings.ui_scale.clamp(0.5, 3.0)
        };
        self.host.set_environment(environment);
        self.electronics_overlay_host.set_environment(environment);
        self.search_surface.set_environment(environment);
        self.viewport_compass.set_environment(environment);
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub fn set_viewport_size(&mut self, logical_size: [f32; 2]) {
        let mut environment = UiEnvironment::new(
            logical_size[0].max(1.0).round() as u32,
            logical_size[1].max(1.0).round() as u32,
        );
        environment.color_mode = match self.agent_settings.theme {
            Theme::Light => UiColorMode::Light,
            Theme::Dark => UiColorMode::Dark,
            Theme::System => UiColorMode::System,
        };
        environment.prefers_reduced_motion = self.agent_settings.prefers_reduced_motion;
        environment.high_contrast = self.agent_settings.high_contrast;
        environment.reduce_transparency = self.agent_settings.reduce_transparency
            || self.agent_settings.render_quality == RenderQuality::Potato;
        environment.font_size = self.agent_settings.font_size.clamp(
            if self.project_type == ProjectType::Electronics {
                14.0
            } else {
                10.0
            },
            24.0,
        );
        environment.theme_experimental = self.agent_settings.theme_experimental.clamp(0.0, 100.0);
        environment.ui_scale = if self.agent_settings.auto_ui_scale {
            1.0
        } else {
            self.agent_settings.ui_scale.clamp(0.5, 3.0)
        };
        self.host.set_environment(environment);
        self.electronics_overlay_host.set_environment(environment);
        self.search_surface.set_environment(environment);
        self.viewport_compass.set_environment(environment);
    }

    pub fn set_project_info(&mut self, name: impl Into<String>, project_type: ProjectType) {
        let name = name.into();
        if project_type == ProjectType::Electronics && !self.electronics_images_registered {
            register_electronics_images(self.host.images_mut());
            self.electronics_images_registered = true;
        }
        if self.project_name == name && self.project_type == project_type {
            return;
        }
        self.project_name = name;
        self.project_type = project_type;
        self.set_viewport_size([self.rect.width, self.rect.height]);
        self.electronics_overlay_key = None;
        self.electronics_overlay_canvas = EditorRect::default();
        self.electronics_overlay_active = false;
        self.hierarchy_query.clear();
        self.hierarchy_scroll_offset = 0.0;
        self.hierarchy_active_tab = "hierarchy".to_string();
        self.lighting_section = "overview".into();
        self.lighting_error = false;
        self.hierarchy_bookmarks = [false; 3];
        self.hierarchy_renaming = None;
        self.hierarchy_drag = None;
        self.inspector_name_editing = None;
        self.hierarchy_model.invalidate();
        self.last_hierarchy_fingerprint = 0;
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }

    pub(super) fn dock_group_rects(&self, layout: EditorFrameLayout) -> Vec<(String, EditorRect)> {
        self.bottom_dock
            .layout()
            .resolve_columns(layout.bottom_dock.width, 4.0)
            .into_iter()
            .map(|(group_id, column)| {
                (
                    group_id,
                    EditorRect::new(
                        layout.bottom_dock.x + column.x,
                        layout.bottom_dock.y,
                        column.width,
                        layout.bottom_dock.height,
                    ),
                )
            })
            .collect()
    }

    pub fn compositor_layers(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> Vec<EditorUiLayer<'_>> {
        let rect = self.rect.to_physical(scale_factor, target_size);
        let logical_size = self.rect.logical_size();
        let raster_scale = scale_factor.max(1.0);
        let main = EditorUiLayer {
            host: &mut self.host,
            target_rect: rect,
            logical_size,
            raster_scale,
        };
        let mut layers = vec![main];
        if self.viewport_compass.active() && !self.interface.workspace.active() {
            layers.push(
                self.viewport_compass
                    .compositor_layer(scale_factor, target_size),
            );
        }
        if self.electronics_overlay_active {
            layers.push(EditorUiLayer {
                host: &mut self.electronics_overlay_host,
                target_rect: self
                    .electronics_overlay_canvas
                    .to_physical(scale_factor, target_size),
                logical_size: self.electronics_overlay_canvas.logical_size(),
                raster_scale,
            });
        }
        if self.electronics_overlay_active
            && (self
                .electronics_workspace
                .pcb_menu
                .as_deref()
                .is_some_and(|menu| menu != "rules")
                || self.console.filters_open)
        {
            let main = layers.remove(0);
            layers.push(main);
        }
        if self.search_surface.is_open() {
            layers.push(
                self.search_surface
                    .compositor_layer(scale_factor, target_size),
            );
        }
        layers
    }

    pub(super) fn finish_bottom_drag(&mut self) -> bool {
        if self.bottom_dock.drag().is_none() {
            return false;
        }
        let changed = self.bottom_dock.end_drag();
        self.drag_motion.set_immediate(0.0);
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        if changed {
            self.bottom_dock.persist_project_layout();
        }
        true
    }

    pub(super) fn seed_project_settings_values(&mut self, project: &Project) {
        let progress_folder = self
            .agent_settings
            .runtime_save_roots
            .get(&project.id)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                raf_core::i18n::t("runtime.folder_not_granted", self.agent_settings.language)
            });
        let focused = self.host.session().interaction.focus.focused.clone();
        let controls = &mut self.host.session_mut().interaction.controls;
        let values = [
            (
                "project-settings.character-jump-height.text",
                "project-settings.character-jump-height.value",
                project.settings.runtime.character_jump_height.to_string(),
            ),
            (
                "project-settings.character-speed.text",
                "project-settings.character-speed.value",
                project.settings.runtime.character_speed.to_string(),
            ),
            (
                "project-settings.character-max-slope.text",
                "project-settings.character-max-slope.value",
                project
                    .settings
                    .runtime
                    .character_max_slope_degrees
                    .to_string(),
            ),
            (
                "project-settings.runtime.gravity",
                "project-settings.runtime-gravity.control",
                serde_json::to_string(&project.settings.runtime.gravity.to_array())
                    .unwrap_or_else(|_| "[0,-9.81,0]".into()),
            ),
            (
                "project-settings.runtime.input_axes",
                "project-settings.runtime-input_axes.control",
                serde_json::to_string(&project.settings.runtime.input_axes)
                    .unwrap_or_else(|_| "[]".into()),
            ),
            (
                "project-settings.runtime.progress_folder",
                "project-settings.runtime-progress_folder.control",
                progress_folder,
            ),
            (
                "project-settings.runtime.autosave_slot",
                "project-settings.runtime-autosave_slot.control",
                project.settings.runtime.progress.autosave_slot.clone(),
            ),
            (
                "project-settings.runtime.progress_schema",
                "project-settings.runtime-progress_schema.control",
                project.settings.runtime.progress.schema_version.to_string(),
            ),
            (
                "project-settings.runtime.progress_slot_kib",
                "project-settings.runtime-progress_slot_kib.control",
                (project.settings.runtime.progress.max_slot_bytes / 1024).to_string(),
            ),
            (
                "project-settings.runtime.progress_slot_count",
                "project-settings.runtime-progress_slot_count.control",
                project.settings.runtime.progress.max_slots.to_string(),
            ),
            (
                "project-settings.progress-interval.text",
                "project-settings.progress-interval.value",
                project
                    .settings
                    .runtime
                    .progress
                    .autosave_seconds
                    .to_string(),
            ),
            (
                "project-settings.runtime.active_camera",
                "project-settings.runtime-active_camera.control",
                project
                    .settings
                    .runtime
                    .active_camera
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            ),
            (
                "project-settings.runtime.startup_session",
                "project-settings.runtime-startup_session.control",
                project
                    .settings
                    .runtime
                    .startup_session
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            ),
            (
                "project-settings.runtime.input_actions",
                "project-settings.runtime-input_actions.control",
                serde_json::to_string(&project.settings.runtime.input_actions)
                    .unwrap_or_else(|_| "[]".into()),
            ),
            (
                "project-settings.runtime-fixed-hz.text",
                "project-settings.runtime-fixed-hz.value",
                project.settings.runtime.fixed_hz.to_string(),
            ),
            (
                "project-settings.runtime-max-entities.text",
                "project-settings.runtime-max-entities.value",
                project.settings.runtime.max_entities.to_string(),
            ),
            (
                "project-settings.building-snap-step.text",
                "project-settings.building-snap-step.value",
                format!("{:.2}", project.settings.building_snap_step),
            ),
            (
                "project-settings.depth-resolution-scale.text",
                "project-settings.depth-resolution-scale.value",
                format!("{:.2}", project.settings.depth_resolution_scale),
            ),
            (
                "project-settings.stream-region-size.text",
                "project-settings.stream-region-size.value",
                format!("{:.0}", project.settings.world_stream_region_size),
            ),
            (
                "project-settings.stream-radius.text",
                "project-settings.stream-radius.value",
                project.settings.world_stream_load_radius.to_string(),
            ),
            (
                "project-settings.stream-lod-bias.text",
                "project-settings.stream-lod-bias.value",
                project.settings.world_stream_lod_bias.to_string(),
            ),
            (
                "project-settings.default_scene_name",
                "project-settings.default-scene.control",
                project.settings.default_scene_name.clone(),
            ),
        ];
        for (key, node_id, value) in values {
            // Numeric drafts are seeded by the shared edit bank. A rebuild or
            // focus change must not replace an unfinished number with model text.
            if self
                .project_numeric_edits
                .binding_for_text_key(key)
                .is_some()
            {
                continue;
            }
            if !controls.has_text(key) || focused.as_deref() != Some(node_id) {
                controls.set_text(
                    key,
                    value,
                    if matches!(
                        key,
                        "project-settings.runtime.input_actions"
                            | "project-settings.runtime.input_axes"
                    ) {
                        8192
                    } else if key == "project-settings.runtime.progress_folder" {
                        1024
                    } else {
                        256
                    },
                );
            }
        }
    }
}
