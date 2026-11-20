//! Project setting mutations applied from the Settings surface and commands.

use raf_core::config::{RenderPreset, ScriptExecutionMode, ScriptLanguage};
use raf_core::project::{
    BuildingStyle, GeometryDetailMode, ProjectSettings, TextureQualityMode,
    ViewportResolutionMode,
};

use super::layout::project_layout_request;
use super::NativeEditorApplication;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ProjectSettingsMutation {
    pub(super) changed: bool,
    pub(super) layout_changed: bool,
}

impl ProjectSettingsMutation {
    const fn changed(layout_changed: bool) -> Self {
        Self {
            changed: true,
            layout_changed,
        }
    }

    const fn without_layout(changed: bool) -> Self {
        Self {
            changed,
            layout_changed: false,
        }
    }
}

pub(super) fn apply_project_setting_toggle(
    settings: &mut ProjectSettings,
    key: &str,
    value: bool,
) -> ProjectSettingsMutation {
    let mut layout_changed = false;
    let changed = match key {
        "project-settings.advanced-physics" => {
            let changed = settings.runtime.advanced_physics != value;
            settings.runtime.advanced_physics = value;
            changed
        }
        "project-settings.physics-debug-draw" if settings.enable_physics => {
            let changed = settings.runtime.physics_debug_draw != value;
            settings.runtime.physics_debug_draw = value;
            changed
        }
        "project-settings.animation-debug-draw" => {
            let changed = settings.runtime.animation_debug_draw != value;
            settings.runtime.animation_debug_draw = value;
            changed
        }
        "project-settings.progress-enabled" => {
            let changed = settings.runtime.progress.enabled != value;
            settings.runtime.progress.enabled = value;
            changed
        }
        "project-settings.progress-autosave" => {
            let changed = settings.runtime.progress.autosave != value;
            settings.runtime.progress.autosave = value;
            changed
        }
        "project-settings.show-hierarchy" => {
            layout_changed = settings.show_hierarchy_panel != value;
            settings.show_hierarchy_panel = value;
            layout_changed
        }
        "project-settings.show-properties" => {
            layout_changed = settings.show_properties_panel != value;
            settings.show_properties_panel = value;
            layout_changed
        }
        "project-settings.enable-console" => {
            let changed = settings.enable_console_commands != value;
            settings.enable_console_commands = value;
            changed
        }
        "project-settings.enable-audio" => {
            let changed = settings.enable_audio != value;
            settings.enable_audio = value;
            changed
        }
        "project-settings.enable-physics" => {
            let changed = settings.enable_physics != value;
            settings.enable_physics = value;
            changed
        }
        "project-settings.pause-unfocused" => {
            let changed = settings.pause_when_unfocused != value;
            settings.pause_when_unfocused = value;
            changed
        }
        "project-settings.enable-scripting" => {
            let changed = settings.enable_scripting != value;
            settings.enable_scripting = value;
            changed
        }
        "project-settings.auto-attach-scripts" => {
            let changed = settings.auto_attach_scripts != value;
            settings.auto_attach_scripts = value;
            changed
        }
        "project-settings.allow-gpu-features" => {
            let changed = settings.allow_gpu_features != value;
            settings.allow_gpu_features = value;
            changed
        }
        "project-settings.depth-accurate" => {
            let changed = settings.depth_accurate != value;
            settings.depth_accurate = value;
            changed
        }
        "project-settings.world-streaming" => {
            let changed = settings.world_streaming_enabled != value;
            settings.world_streaming_enabled = value;
            changed
        }
        "project-settings.electronics-snap-to-grid" => {
            let changed = settings.electronics_snap_to_grid != value;
            settings.electronics_snap_to_grid = value;
            changed
        }
        "project-settings.language.rhai" => {
            let before = settings.allowed_script_languages.has(ScriptLanguage::Rhai);
            settings
                .allowed_script_languages
                .set(ScriptLanguage::Rhai, value);
            before != value
        }
        "project-settings.language.cpp" => {
            let before = settings.allowed_script_languages.has(ScriptLanguage::Cpp);
            settings
                .allowed_script_languages
                .set(ScriptLanguage::Cpp, value);
            before != value
        }
        // Visual Nodes intentionally remains outside this pass. Its settings
        // contract is being replaced independently.
        "project-settings.language.nodes" => false,
        "project-settings.building-style.free" if value => {
            let changed = settings.building_style != BuildingStyle::Free;
            settings.building_style = BuildingStyle::Free;
            changed
        }
        "project-settings.building-style.professional" if value => {
            let changed = settings.building_style != BuildingStyle::Organized;
            settings.building_style = BuildingStyle::Organized;
            changed
        }
        _ => false,
    };
    ProjectSettingsMutation {
        changed,
        layout_changed,
    }
}

pub(super) fn apply_project_setting_range(
    settings: &mut ProjectSettings,
    key: &str,
    value: f32,
) -> ProjectSettingsMutation {
    if !value.is_finite() {
        return ProjectSettingsMutation::default();
    }
    let snap = |value: f32, min: f32, max: f32, step: f32| {
        (min + ((value.clamp(min, max) - min) / step).round() * step).clamp(min, max)
    };
    let changed = match key {
        "project-settings.character-jump-height" => {
            let next = snap(value, 0.0, 20.0, 0.05);
            let changed = settings.runtime.character_jump_height != next;
            settings.runtime.character_jump_height = next;
            changed
        }
        "project-settings.character-speed" => {
            let next = snap(value, 0.0, 100.0, 0.1);
            let changed = settings.runtime.character_speed != next;
            settings.runtime.character_speed = next;
            changed
        }
        "project-settings.character-max-slope" => {
            let next = snap(value, 0.0, 89.0, 1.0);
            let changed = settings.runtime.character_max_slope_degrees != next;
            settings.runtime.character_max_slope_degrees = next;
            changed
        }
        "project-settings.progress-interval" => {
            let next = value.round().clamp(10.0, 3600.0) as u32;
            let changed = settings.runtime.progress.autosave_seconds != next;
            settings.runtime.progress.autosave_seconds = next;
            changed
        }
        "project-settings.runtime-fixed-hz" => {
            let next = value.round().clamp(15.0, 120.0) as u32;
            let changed = next != settings.runtime.fixed_hz;
            settings.runtime.fixed_hz = next;
            changed
        }
        "project-settings.runtime-max-entities" => {
            let next = value.round().clamp(100.0, 100_000.0) as usize;
            let changed = next != settings.runtime.max_entities;
            settings.runtime.max_entities = next;
            changed
        }
        "project-settings.building-snap-step" => {
            let next = snap(value, 0.5, 2.0, 0.05);
            if (settings.building_snap_step - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.building_snap_step = next;
                true
            }
        }
        "project-settings.electronics-schematic-grid-step" => {
            let next = snap(value, 1.0, 100.0, 1.0);
            if (settings.electronics_schematic_grid_step_mm - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.electronics_schematic_grid_step_mm = next;
                true
            }
        }
        "project-settings.electronics-pcb-grid-step" => {
            let next = snap(value, 0.05, 100.0, 0.05);
            if (settings.electronics_pcb_grid_step_mm - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.electronics_pcb_grid_step_mm = next;
                true
            }
        }
        "project-settings.depth-resolution-scale" => {
            let next = snap(value, 0.35, 1.0, 0.05);
            if (settings.depth_resolution_scale - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.depth_resolution_scale = next;
                true
            }
        }
        "project-settings.stream-region-size" => {
            let next = snap(value, 32.0, 512.0, 16.0);
            if (settings.world_stream_region_size - next).abs() <= f32::EPSILON {
                false
            } else {
                settings.world_stream_region_size = next;
                true
            }
        }
        "project-settings.stream-radius" => {
            let next = snap(value, 1.0, 8.0, 1.0) as u32;
            if settings.world_stream_load_radius == next {
                false
            } else {
                settings.world_stream_load_radius = next;
                true
            }
        }
        "project-settings.stream-lod-bias" => {
            let next = snap(value, 0.0, 4.0, 1.0) as i8;
            if settings.world_stream_lod_bias == next {
                false
            } else {
                settings.world_stream_lod_bias = next;
                true
            }
        }
        _ => false,
    };
    ProjectSettingsMutation {
        changed,
        layout_changed: false,
    }
}

pub(super) fn apply_project_setting_command(
    settings: &mut ProjectSettings,
    command: &str,
) -> ProjectSettingsMutation {
    match command {
        command if command.starts_with("project-settings.gravity-preset.") => {
            if !settings.enable_physics || !settings.runtime.advanced_physics {
                return ProjectSettingsMutation::default();
            }
            let (gravity, jump_height, speed, slope) = match command.rsplit('.').next() {
                Some("earth") => (glam::Vec3::new(0.0, -9.81, 0.0), 1.5, 4.0, 45.0),
                Some("moon") => (glam::Vec3::new(0.0, -1.62, 0.0), 1.5, 4.0, 45.0),
                Some("arcade") => (glam::Vec3::new(0.0, -20.0, 0.0), 2.5, 6.0, 50.0),
                _ => return ProjectSettingsMutation::default(),
            };
            let changed = settings.runtime.gravity != gravity
                || settings.runtime.character_jump_height != jump_height
                || settings.runtime.character_speed != speed
                || settings.runtime.character_max_slope_degrees != slope;
            settings.runtime.gravity = gravity;
            settings.runtime.character_jump_height = jump_height;
            settings.runtime.character_speed = speed;
            settings.runtime.character_max_slope_degrees = slope;
            ProjectSettingsMutation::without_layout(changed)
        }
        command if command.starts_with("project-settings.progress-location.") => {
            use raf_core::runtime_config::ProgressLocation;
            let location = match command.rsplit('.').next() {
                Some("appdata") => ProgressLocation::AppData,
                Some("portable") => ProgressLocation::Portable,
                Some("custom") => ProgressLocation::Custom,
                _ => return ProjectSettingsMutation::default(),
            };
            let changed = settings.runtime.progress.location != location;
            settings.runtime.progress.location = location;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.reset-panels" => {
            let changed = !settings.show_hierarchy_panel || !settings.show_properties_panel;
            settings.show_hierarchy_panel = true;
            settings.show_properties_panel = true;
            if changed {
                ProjectSettingsMutation::changed(true)
            } else {
                ProjectSettingsMutation::default()
            }
        }
        "project-settings.save.standard" => {
            let changed = settings.linear_save;
            settings.linear_save = false;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.save.linear" => {
            let changed = !settings.linear_save;
            settings.linear_save = true;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.script-mode.disabled" if settings.enable_scripting => {
            let changed = settings.script_execution_mode != ScriptExecutionMode::Disabled;
            settings.script_execution_mode = ScriptExecutionMode::Disabled;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.script-mode.editor" if settings.enable_scripting => {
            let changed = settings.script_execution_mode != ScriptExecutionMode::EditorOnly;
            settings.script_execution_mode = ScriptExecutionMode::EditorOnly;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.script-mode.runtime" if settings.enable_scripting => {
            let changed = settings.script_execution_mode != ScriptExecutionMode::Runtime;
            settings.script_execution_mode = ScriptExecutionMode::Runtime;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.preset.potato" => {
            let changed = settings.runtime_render_preset != RenderPreset::Potato;
            settings.runtime_render_preset = RenderPreset::Potato;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.preset.low" => {
            let changed = settings.runtime_render_preset != RenderPreset::Low;
            settings.runtime_render_preset = RenderPreset::Low;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.preset.medium" if settings.allow_gpu_features => {
            let changed = settings.runtime_render_preset != RenderPreset::Medium;
            settings.runtime_render_preset = RenderPreset::Medium;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.preset.high" if settings.allow_gpu_features => {
            let changed = settings.runtime_render_preset != RenderPreset::High;
            settings.runtime_render_preset = RenderPreset::High;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.viewport-resolution.efficient" => {
            let changed = settings.viewport_resolution != ViewportResolutionMode::Efficient;
            settings.viewport_resolution = ViewportResolutionMode::Efficient;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.viewport-resolution.adaptive" => {
            let changed = settings.viewport_resolution != ViewportResolutionMode::Adaptive;
            settings.viewport_resolution = ViewportResolutionMode::Adaptive;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.viewport-resolution.full" => {
            let changed = settings.viewport_resolution != ViewportResolutionMode::FullResolution;
            settings.viewport_resolution = ViewportResolutionMode::FullResolution;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.geometry-detail.efficient" => {
            let changed = settings.geometry_detail != GeometryDetailMode::Efficient;
            settings.geometry_detail = GeometryDetailMode::Efficient;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.geometry-detail.adaptive" => {
            let changed = settings.geometry_detail != GeometryDetailMode::Adaptive;
            settings.geometry_detail = GeometryDetailMode::Adaptive;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.geometry-detail.detailed" => {
            let changed = settings.geometry_detail != GeometryDetailMode::Detailed;
            settings.geometry_detail = GeometryDetailMode::Detailed;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.texture-quality.efficient" => {
            let changed = settings.texture_quality != TextureQualityMode::Efficient;
            settings.texture_quality = TextureQualityMode::Efficient;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.texture-quality.adaptive" => {
            let changed = settings.texture_quality != TextureQualityMode::Adaptive;
            settings.texture_quality = TextureQualityMode::Adaptive;
            ProjectSettingsMutation::without_layout(changed)
        }
        "project-settings.texture-quality.detailed" => {
            let changed = settings.texture_quality != TextureQualityMode::Detailed;
            settings.texture_quality = TextureQualityMode::Detailed;
            ProjectSettingsMutation::without_layout(changed)
        }
        _ => ProjectSettingsMutation::default(),
    }
}

impl NativeEditorApplication {
    fn commit_project_settings_mutation(&mut self, mutation: ProjectSettingsMutation) {
        if !mutation.changed {
            return;
        }

        let layout_request = if mutation.layout_changed {
            match (self.runtime.as_ref(), self.project.as_ref()) {
                (Some(runtime), Some(project)) => {
                    let size = runtime.layout().window.logical_size();
                    Some(project_layout_request(
                        project.project_type,
                        [size[0] as f32, size[1] as f32],
                        Some(project),
                    ))
                }
                _ => None,
            }
        } else {
            None
        };

        if let Some(project) = self.project.as_mut() {
            if let Err(error) = project.save() {
                tracing::warn!(%error, "project settings save failed");
            }
        }
        if let Some(request) = layout_request {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.set_layout_request(request);
            }
        }
        self.apply_live_project_settings();
    }

    pub(super) fn apply_project_setting_toggle_intent(&mut self, key: String, value: bool) {
        // The project flag is intentionally subordinate to the global
        // command capability. The disabled UI never emits this path, but the
        // domain boundary also enforces it for automation and stale surfaces.
        if key == "project-settings.enable-console" && !self.settings_state.command_console_enabled
        {
            return;
        }
        let mutation = self
            .project
            .as_mut()
            .map(|project| apply_project_setting_toggle(&mut project.settings, &key, value))
            .unwrap_or_default();
        self.commit_project_settings_mutation(mutation);
    }

    pub(super) fn apply_project_setting_range_intent(&mut self, key: String, value: f32) {
        let mutation = self
            .project
            .as_mut()
            .map(|project| apply_project_setting_range(&mut project.settings, &key, value))
            .unwrap_or_default();
        self.commit_project_settings_mutation(mutation);
    }

    pub(super) fn apply_project_setting_text_intent(&mut self, key: String, value: String) {
        if let Some(field) = key.strip_prefix("project-settings.runtime.") {
            let result = self.project.as_mut().map(|project| -> Result<(), String> {
                if field == "gravity"
                    && (!project.settings.enable_physics || !project.settings.runtime.advanced_physics)
                {
                    return Err("runtime.invalid_setting".into());
                }
                crate::runtime_project_settings::apply_text(
                    &mut project.settings.runtime,
                    field,
                    &value,
                )
            });
            match result {
                Some(Ok(())) => self.commit_project_settings_mutation(
                    ProjectSettingsMutation::without_layout(true),
                ),
                Some(Err(error)) => {
                    if let Some(workbench) = &mut self.workbench {
                        workbench.log_console_output(crate::commands::CommandOutput::error(
                            raf_core::i18n::t("app.project_settings", self.settings_state.language),
                            raf_core::i18n::t(
                                "runtime.invalid_setting",
                                self.settings_state.language,
                            ) + ": "
                                + error.as_str(),
                        ));
                    }
                }
                None => {}
            }
            return;
        }
        let mutation = if key == "project-settings.default_scene_name" {
            let value = value.trim();
            if value.is_empty() {
                ProjectSettingsMutation::default()
            } else if let Some(project) = self.project.as_mut() {
                if project.settings.default_scene_name == value {
                    ProjectSettingsMutation::default()
                } else {
                    project.settings.default_scene_name = value.to_string();
                    ProjectSettingsMutation::without_layout(true)
                }
            } else {
                ProjectSettingsMutation::default()
            }
        } else {
            ProjectSettingsMutation::default()
        };
        self.commit_project_settings_mutation(mutation);
    }

    pub(super) fn apply_project_setting_command_intent(&mut self, command: String) {
        if matches!(
            command.as_str(),
            "project-settings.progress-folder.choose" | "project-settings.progress-folder.revoke"
        ) {
            let Some(project) = &self.project else {
                return;
            };
            let id = project.id;
            if command.ends_with(".revoke") {
                self.settings_state.runtime_save_roots.remove(&id);
            } else {
                let current = self
                    .settings_state
                    .runtime_save_roots
                    .get(&id)
                    .unwrap_or(&project.path)
                    .display()
                    .to_string();
                let Some(path) = crate::folder_picker::pick_folder(&current) else {
                    return;
                };
                let Ok(path) = path.canonicalize() else {
                    return;
                };
                if !path.is_absolute() || !path.is_dir() {
                    return;
                }
                self.settings_state.runtime_save_roots.insert(id, path);
            }
            self.persist_settings();
            self.apply_committed_settings();
            return;
        }
        if command == "project-settings.runtime.camera.selected"
            || command == "project-settings.runtime.camera.clear"
        {
            let target = self
                .runtime
                .as_ref()
                .and_then(|runtime| runtime.game_viewport().selected.first().copied());
            let configured = if command.ends_with(".clear") {
                Some(None)
            } else if let Some(id) = target {
                let uuid = self
                    .scene
                    .get(id)
                    .filter(|_| self.scene.is_valid_node(id))
                    .map(|node| node.uuid);
                if let Some(runtime) = &mut self.runtime {
                    runtime.mutate_scene(&mut self.scene, |scene| {
                        if let Some(node) = scene.get_mut(id) {
                            node.game_camera.get_or_insert_with(Default::default);
                        }
                    });
                }
                uuid.map(Some)
            } else {
                None
            };
            if let Some(camera) = configured {
                if let Some(project) = &mut self.project {
                    project.settings.runtime.active_camera = camera;
                }
                self.commit_project_settings_mutation(ProjectSettingsMutation::without_layout(
                    true,
                ));
            } else if let Some(workbench) = &mut self.workbench {
                workbench.log_console_output(crate::commands::CommandOutput::error(
                    "Game camera",
                    "select an object before configuring a game camera",
                ));
            }
            return;
        }
        let mutation = self
            .project
            .as_mut()
            .map(|project| apply_project_setting_command(&mut project.settings, &command))
            .unwrap_or_default();
        self.commit_project_settings_mutation(mutation);
    }

    pub(super) fn apply_live_project_settings(&mut self) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let project_settings = project.settings.clone();
        let global_settings = self.settings_state.clone();
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.apply_engine_settings(&global_settings, project_settings.allow_gpu_features);
            runtime.apply_project_settings(&project_settings);
            runtime.set_project_asset_root(Some(project.path.as_path()));
            runtime.request_canvas_frame();
        }
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_project_settings(&project_settings);
        }
    }
}

