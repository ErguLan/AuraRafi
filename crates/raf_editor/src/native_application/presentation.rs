//! Frame composition and presentation policy for the native editor.

mod composition;

use std::time::Instant;

use raf_core::project::ProjectType;
use raf_core::session::ProjectSessionRegistry;
use winit::dpi::{PhysicalPosition, PhysicalSize};

use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_editor_commands::apply_workbench_intents;
use crate::native_project_controller::save_project_document;
use crate::native_studio::NativeStudioSurface;
use crate::native_workbench::NativeGameWorkbench;
use crate::panels::settings_surface_host::SettingsSurfaceHost;
use crate::panels::viewport_compass::ViewportCompassState;
use crate::panels::viewport_controller::NativeViewportMode;

use self::composition::{observe_editor_presentation, sync_electronics_canvas};
use super::agents;
use super::document::{open_project_folder, switch_session_documents};
use super::layout::{should_queue_eager_hierarchy_refresh, synchronize_bottom_dock_layout};
use super::shortcuts::electronics_shortcut_command;
use super::window::native_cursor_icon;
use super::{native_palette, NativeEditorApplication};

impl NativeEditorApplication {
    pub(super) fn redraw(&mut self) {
        self.sync_accessibility_enabled();
        if self.game_runtime.in_place.is_some() {
            self.present_in_place_runtime();
            return;
        }
        let Some(now) = self.begin_frame_gate() else {
            return;
        };
        let redraw_started_at = Instant::now();
        self.input.set_time_seconds(now);
        if let Some(runtime) = self.runtime.as_mut() {
            runtime
                .input_router_mut()
                .reconcile_input(self.input.snapshot());
        }
        let loading_active = self.show_loading;
        let exit_confirmation_was_open = self.exit_confirmation_open;
        let mut persist_agent_settings = false;
        let mut linear_save_requested = false;
        // Switching the Electronics surface materializes the PCB projection, which
        // is a risk moment: a crash right after the tab switch would otherwise lose
        // the derived board.
        let mut electronics_surface_switched = false;
        let mut queue_hierarchy_refresh = false;
        let mut project_save_requested = false;
        let mut refresh_workbench_after_input = false;

        if self.exit_confirmation_open {
            self.process_exit_confirmation_frame();
        }

        if loading_active {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.set_continuous_ui_motion(false);
            }
        } else if self.agents_open {
            if self.settings_open && !self.exit_confirmation_open && !exit_confirmation_was_open {
                let settings_actions = self.process_settings_input();
                self.apply_settings_actions(settings_actions);
            }
            self.ensure_agent_surface();
            let projects = self.agent_projects();
            let mut intents = Vec::new();
            let mounted = self.project.as_ref().zip(self.workbench.as_ref())
                .zip(self.electronics_editor.as_ref())
                .map(|((project, workbench), editor)| (project.path.as_path(), workbench.active_session_id(), editor));
            let laboratory = self.agents.selected_lab(mounted);
            if let (Some(runtime), Some(surface)) =
                (self.runtime.as_mut(), self.agent_surface.as_mut())
            {
                surface.sync(
                    runtime.layout().window,
                    native_palette(self.settings_state.theme),
                    &self.agents,
                    &projects,
                    &self.settings_state,
                    laboratory,
                );
                if !self.settings_open && !self.exit_confirmation_open {
                    intents = surface.process_input(
                        &self.input,
                        runtime.input_router_mut(),
                        &mut self.agents,
                        &mut self.settings_state,
                    );
                    let laboratory = self.agents.selected_lab(mounted);
                    surface.sync(
                        runtime.layout().window,
                        native_palette(self.settings_state.theme),
                        &self.agents,
                        &projects,
                        &self.settings_state,
                        laboratory,
                    );
                } else {
                    surface.suspend_preview_input(runtime.input_router_mut());
                }
                runtime.set_continuous_ui_motion(self.agents.loading() || surface.preview.moving);
                if !intents.is_empty() || surface.take_frame_request() {
                    runtime.request_ui_frame();
                }
            }
            self.apply_agent_workspace_intents(intents);
            if let Some(workspace) = self.agents.selected_workspace() {
                let settings_changed = workspace.panel.borrow_mut().take_settings_changed();
                if settings_changed {
                    self.persist_settings();
                }
            }
            if !self.settings_open {
                if let (Some(window), Some(surface)) =
                    (self.window.as_ref(), self.agent_surface.as_ref())
                {
                    window.set_cursor(native_cursor_icon(surface.cursor_hint()));
                }
            }
        } else if self.project.is_none() {
            if self.settings_open && !self.exit_confirmation_open && !exit_confirmation_was_open {
                let settings_actions = self.process_settings_input();
                self.apply_settings_actions(settings_actions);
            }
            if self.settings_open {
                if let (Some(window), Some(settings_surface)) =
                    (self.window.as_ref(), self.settings_surface.as_ref())
                {
                    window.set_cursor(native_cursor_icon(settings_surface.cursor_hint()));
                }
            }
            let intents = if self.settings_open
                || self.exit_confirmation_open
                || exit_confirmation_was_open
            {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.set_continuous_ui_motion(false);
                }
                Vec::new()
            } else {
                let Some(runtime) = self.runtime.as_mut() else {
                    return;
                };
                let Some(studio) = self.studio.as_mut() else {
                    return;
                };
                studio.sync(runtime.layout(), native_palette(self.settings_state.theme));
                let intents = studio.process_input(&self.input, runtime.input_router_mut());
                let needs_ui_frame = !intents.is_empty() || studio.needs_surface_sync();
                // Semantic input can arrive as one isolated event. Present the
                // resulting document now instead of waiting for another OS input.
                if studio.needs_surface_sync() {
                    studio.sync(runtime.layout(), native_palette(self.settings_state.theme));
                }
                let has_active_motion = studio.has_active_motion();
                runtime.set_continuous_ui_motion(has_active_motion);
                if needs_ui_frame || has_active_motion {
                    runtime.request_animation_frame();
                }
                intents
            };
            self.apply_studio_intents(intents);
            if self.project.is_some() {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                self.input.begin_frame();
                return;
            }
        } else {
            linear_save_requested = self
                .project
                .as_ref()
                .is_some_and(|project| project.settings.linear_save);
            if self.settings_open && !self.exit_confirmation_open && !exit_confirmation_was_open {
                let settings_actions = self.process_settings_input();
                self.apply_settings_actions(settings_actions);
            }
            let presentation_label = self
                .window_host
                .as_ref()
                .map(|host| host.presentation_mode().label())
                .unwrap_or("Present");
            let Some(runtime) = self.runtime.as_mut() else {
                return;
            };
            let selected_before_input = runtime.game_viewport().selected.clone();
            let mut cursor_hint = self
                .settings_open
                .then(|| {
                    self.settings_surface
                        .as_ref()
                        .map(SettingsSurfaceHost::cursor_hint)
                })
                .flatten();
            // Keyboard ownership is resolved once per frame. The CAD canvas guard
            // and the shortcut dispatcher have to agree, otherwise a focused text
            // field still loses keys to the canvas.
            let text_input_owned = if self.settings_open {
                true
            } else {
                self.workbench
                    .as_ref()
                    .is_some_and(|workbench| workbench.captures_keyboard_input())
            };
            if let Some(workbench) = self.workbench.as_mut() {
                workbench.set_inspector_transform_drag_active(
                    runtime.project_type() == ProjectType::Game
                        && runtime.game_viewport().transform_drag_active(),
                );
                let analysis_changed = self
                    .electronics_editor
                    .as_mut()
                    .is_some_and(NativeElectronicsEditor::poll_analysis);
                let analysis_running = self
                    .electronics_editor
                    .as_ref()
                    .is_some_and(NativeElectronicsEditor::analysis_running);
                if analysis_changed || analysis_running {
                    runtime.request_animation_frame();
                }
                if let Some(project) = self.project.as_ref() {
                    workbench.set_project_info(project.name.clone(), project.project_type);
                }
                let graphics_snapshot = runtime.graphics().snapshot();
                let performance = graphics_snapshot.scheduler_metrics;
                workbench.set_performance_metrics(
                    performance.presented_fps,
                    runtime.viewport_fps(now),
                    performance.last_frame_total_cpu_ms,
                    performance.last_frame_cpu_ms,
                    performance.target_fps,
                    performance.requested_target_fps,
                    performance.p95_frame_time_ms,
                    performance.hitch_count,
                    graphics_snapshot.last_frame_metrics,
                    presentation_label,
                    runtime.active_frame_activity().label(),
                    runtime.active_frame_reason_label(),
                    runtime.canvas_status_label(),
                );
                let selected = selected_before_input.clone();
                let compass_state = if runtime.project_type() == ProjectType::Game
                    && runtime.game_viewport().mode == NativeViewportMode::View3d
                {
                    ViewportCompassState::from_orbit(
                        runtime.game_viewport().bridge().orbit_yaw(),
                        runtime.game_viewport().bridge().orbit_pitch(),
                    )
                } else {
                    ViewportCompassState::hidden()
                };
                if runtime.project_type() == ProjectType::Electronics {
                    let (library, inspector) = workbench.electronics_workspace_policy(
                        self.project.as_ref(),
                        self.electronics_editor.as_ref(),
                    );
                    // A drag keeps its original canvas until release, so a
                    // contextual inspector cannot move the target mid-gesture.
                    let inspector = if self
                        .input
                        .snapshot()
                        .button_down(raf_core::PointerButton::Primary)
                    {
                        runtime.layout().right_panel.is_some()
                    } else {
                        inspector
                    };
                    let old_canvas = runtime.layout().electronics_canvas_for_pcb(self.electronics_editor.as_ref().is_some_and(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb));
                    runtime.configure_electronics_workspace(library, inspector);
                    if let Some(editor) = self.electronics_editor.as_mut() {
                        editor.preserve_camera_for_canvas_change(
                            old_canvas,
                            runtime.layout().electronics_canvas_for_pcb(editor.active_surface() == raf_electronics::CadSurfaceKind::Pcb),
                        );
                    }
                }
                workbench.sync(
                    runtime.layout(),
                    &self.scene,
                    &selected,
                    runtime.history().can_undo(),
                    runtime.history().can_redo(),
                    runtime.has_clipboard(),
                    runtime.node_graph(),
                    runtime.selected_graph_node(),
                    runtime.node_graph_revision(),
                    runtime.can_undo_node_graph(),
                    runtime.can_redo_node_graph(),
                    runtime.node_validation(),
                    self.project.as_ref(),
                    now,
                    compass_state,
                    self.electronics_editor.as_ref(),
                );
                let old_canvas = runtime.layout().electronics_canvas_for_pcb(self.electronics_editor.as_ref().is_some_and(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb));
                synchronize_bottom_dock_layout(runtime, workbench);
                if runtime.project_type() == ProjectType::Electronics {
                    if let Some(editor) = self.electronics_editor.as_mut() {
                        editor.preserve_camera_for_canvas_change(
                            old_canvas,
                            runtime.layout().electronics_canvas_for_pcb(editor.active_surface() == raf_electronics::CadSurfaceKind::Pcb),
                        );
                    }
                }
                if !self.settings_open {
                    cursor_hint = Some(workbench.cursor_hint());
                }
                if self.settings_open || self.exit_confirmation_open || exit_confirmation_was_open {
                    runtime.set_continuous_ui_motion(false);
                } else {
                    let ui_language = self.settings_state.language;
                    let previous_session_id = workbench.active_session_id();
                    let (intents, actions) = workbench.process_input(
                        &self.input,
                        runtime.input_router_mut(),
                        &self.scene,
                        &selected,
                        self.project.as_ref(),
                        |key| raf_core::i18n::t(key, ui_language),
                    );
                    let viewport_press =
                        self.input.snapshot().pointer_position.is_some_and(|point| {
                            let canvas = match runtime.project_type() {
                                ProjectType::Game => runtime.layout().canvas,
                                ProjectType::Electronics => runtime.layout().electronics_canvas_for_pcb(self.electronics_editor.as_ref().is_some_and(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb)),
                            };
                            canvas.contains(point)
                        }) && [
                            raf_core::PointerButton::Primary,
                            raf_core::PointerButton::Secondary,
                            raf_core::PointerButton::Middle,
                        ]
                        .into_iter()
                        .any(|button| self.input.snapshot().button_pressed(button));
                    if viewport_press && !workbench.has_interactive_hover() {
                        workbench.clear_input_focus(runtime.input_router_mut());
                    }
                    let viewport_navigation_key_down = [
                        raf_core::InputKey::Q,
                        raf_core::InputKey::W,
                        raf_core::InputKey::E,
                        raf_core::InputKey::A,
                        raf_core::InputKey::S,
                        raf_core::InputKey::D,
                    ]
                    .into_iter()
                    .any(|key| self.input.snapshot().key_down(key));
                    if runtime.project_type() == ProjectType::Game && viewport_navigation_key_down {
                        workbench.release_non_text_keyboard_capture(runtime.input_router_mut());
                    }
                    let had_ui_activity = !intents.is_empty() || !actions.is_empty();
                    refresh_workbench_after_input |= had_ui_activity;
                    let mut domain_intents = Vec::new();
                    for intent in intents {
                        match intent {
                            crate::native_workbench::NativeWorkbenchIntent::Window(command) => {
                                self.pending_window_commands.push(command);
                            }
                            crate::native_workbench::NativeWorkbenchIntent::OpenSettings {
                                section,
                            } => {
                                self.pending_settings_open = Some(section);
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ReturnToHub {
                                open_create,
                            } => {
                                self.pending_return_to_hub = Some(open_create);
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ProjectSettingToggle {
                                key,
                                value,
                            } => {
                                self.pending_project_setting = Some((key, value));
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ProjectSettingRange {
                                key,
                                value,
                            } => {
                                self.pending_project_range = Some((key, value));
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ProjectSettingText {
                                key,
                                value,
                            } => {
                                self.pending_project_text = Some((key, value));
                            }
                            crate::native_workbench::NativeWorkbenchIntent::ProjectSettingCommand(
                                command,
                            ) => {
                                self.pending_project_command = Some(command);
                            }
                            crate::native_workbench::NativeWorkbenchIntent::AgentSettingsChanged(
                                settings,
                            ) => {
                                self.settings_state = settings;
                                persist_agent_settings = true;
                            }
                            crate::native_workbench::NativeWorkbenchIntent::OpenProjectFolder => {
                                if let Some(project) = self.project.as_ref() {
                                    if let Err(error) = open_project_folder(&project.path) {
                                        tracing::warn!(
                                            %error,
                                            "native project folder could not be opened"
                                        );
                                    }
                                }
                            }
                            crate::native_workbench::NativeWorkbenchIntent::Command(command) => {
                                if command == "agent.workspace" {
                                    self.pending_agents_open = true;
                                } else if command == "agent.lab.open.simulation" || command == "agent.lab.open.device" {
                                    self.pending_lab_open = Some(if command.ends_with(".device") { "device" } else { "simulation" });
                                } else if let Some(raw) = command.strip_prefix("console.submit:").filter(|raw| raw.trim().starts_with("/runtime.")) {
                                    let allowed = self.settings_state.command_console_enabled && self.project.as_ref().is_some_and(|project| project.settings.enable_console_commands);
                                    if !allowed {
                                        workbench.log_console_output(crate::commands::CommandOutput::error("Runtime", "manual console commands are disabled"));
                                    } else if raw.trim().split_whitespace().count() != 1 {
                                        workbench.log_console_output(crate::commands::CommandOutput::error("Runtime", "runtime controls do not accept arguments"));
                                    } else {
                                        self.pending_runtime_command = Some(raw.trim().trim_start_matches('/').to_string());
                                    }
                                } else if let Some(raw) = command.strip_prefix("console.submit:").filter(|raw| raw.trim().starts_with("/asset.")) {
                                    let allowed = self.settings_state.command_console_enabled && self.project.as_ref().is_some_and(|project| project.settings.enable_console_commands);
                                    let output = if !allowed {
                                        crate::commands::CommandOutput::error("Assets", "Manual console commands are disabled")
                                    } else if let Ok(crate::commands::ParsedInput::Command(parsed)) = crate::commands::parse_console_input(raw) {
                                        let name = match parsed.name.as_str() {
                                            "asset.image" => "asset.generate_image",
                                            "asset.local_png" | "asset.icon" => "asset.generate_local_png",
                                            name => name,
                                        };
                                        let params: serde_json::Map<String, serde_json::Value> = parsed.args.iter().map(|(key, value)| {
                                            let value = if key == "transparent" { serde_json::json!(matches!(value.as_str(), "true" | "1" | "yes" | "on")) } else { serde_json::json!(value) };
                                            (key.clone(), value)
                                        }).collect();
                                        let mut request = raf_core::EngineCommandRequest::new(name, serde_json::Value::Object(params), raf_core::CommandSource::Console);
                                        request.confirm = true;
                                        let response = workbench.execute_asset_command(&request, self.project.as_ref(), self.attached_ledger.revision());
                                        crate::commands::CommandOutput {
                                            level: if response.ok { crate::commands::CommandLevel::Info } else { crate::commands::CommandLevel::Error },
                                            title: response.title, lines: response.lines, json: response.data, changed: false,
                                        }
                                    } else { crate::commands::CommandOutput::error("Assets", "Invalid asset command") };
                                    workbench.log_console_output(output);
                                } else if command == "assets.generate-local-icon" {
                                    if self.project.is_some() {
                                        let stamp = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|elapsed| elapsed.as_nanos())
                                            .unwrap_or_default();
                                        let mut request = raf_core::EngineCommandRequest::new(
                                            "asset.generate_local_png",
                                            serde_json::json!({
                                                "prompt": "Local generated icon",
                                                "name": format!("generated_icon_{stamp}"),
                                            }),
                                            raf_core::CommandSource::RafUi,
                                        );
                                        request.confirm = true;
                                        let response = workbench.execute_asset_command(
                                            &request,
                                            self.project.as_ref(),
                                            self.attached_ledger.revision(),
                                        );
                                        workbench.log_console_output(crate::commands::CommandOutput {
                                            level: if response.ok {
                                                crate::commands::CommandLevel::Info
                                            } else {
                                                crate::commands::CommandLevel::Error
                                            },
                                            title: response.title,
                                            lines: response.lines,
                                            json: response.data,
                                            changed: false,
                                        });
                                    }
                                } else if command.starts_with("runtime.") {
                                    self.pending_runtime_command = Some(command);
                                } else if command == "sessions.reload" {
                                    let next_session_id = workbench.active_session_id();
                                    if next_session_id != previous_session_id {
                                        if let Some(project) = self.project.as_ref() {
                                            if let Err(error) = agents::park_live_session(&mut self.agents, project, previous_session_id,
                                                &self.scene, runtime, self.electronics_editor.as_ref()) {
                                                let mut registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
                                                registry.active_session = previous_session_id;
                                                let _ = registry.save(&project.path);
                                                workbench.log_console_output(crate::commands::CommandOutput::error(
                                                    raf_core::i18n::t("agent_workspace.operation_failed", self.settings_state.language), error));
                                                continue;
                                            }
                                            match switch_session_documents(
                                                project,
                                                previous_session_id,
                                                next_session_id,
                                                &mut self.scene,
                                                runtime,
                                                &mut self.electronics_editor,
                                                &mut self.attached_ledger,
                                                &mut self.observed_scene_revision,
                                                &mut self.observed_scene_render_fingerprint,
                                            ) {
                                                Ok(()) => {
                                                    if let Err(error) = agents::mount_live_session(&mut self.agents, project, previous_session_id,
                                                        next_session_id, &mut self.scene, runtime, &mut self.electronics_editor, workbench) {
                                                        workbench.log_console_output(crate::commands::CommandOutput::error(
                                                            raf_core::i18n::t("agent_workspace.operation_failed", self.settings_state.language), error));
                                                    }
                                                    let registry =
                                                        ProjectSessionRegistry::load_or_legacy(
                                                            &project.path,
                                                            project.project_type,
                                                        );
                                                    let session_name = registry
                                                        .active()
                                                        .map(|session| session.name.clone());
                                                    self.attached_host.update_session(
                                                        Some(next_session_id.0),
                                                        session_name,
                                                    );
                                                    self.electronics_frame_key = None;
                                                    self.pending_document_saved = true;
                                                }
                                                Err(error) => tracing::warn!(
                                                    %error,
                                                    "native session switch could not reload documents"
                                                ),
                                            }
                                        }
                                    }
                                } else {
                                    if let Some(raw_value) =
                                        command.strip_prefix("layout.resize.bottom:")
                                    {
                                        if let Ok(value) = raw_value.parse::<f32>() {
                                            workbench.set_bottom_dock_height(value);
                                        }
                                        continue;
                                    }
                                    domain_intents.push(
                                        crate::native_workbench::NativeWorkbenchIntent::Command(
                                            command,
                                        ),
                                    );
                                }
                            }
                            other => domain_intents.push(other),
                        }
                    }
                    if runtime.project_type() == ProjectType::Electronics {
                        if let Some(editor) = self.electronics_editor.as_mut() {
                            let electronics_canvas = runtime.layout().electronics_canvas_for_pcb(editor.active_surface() == raf_electronics::CadSurfaceKind::Pcb);
                            for intent in &domain_intents {
                                if let crate::native_workbench::NativeWorkbenchIntent::Command(
                                    command,
                                ) = intent
                                {
                                    if let Some(raw_index) =
                                        command.strip_prefix("electronics.library.drag.start.")
                                    {
                                        if let Ok(index) = raw_index.parse::<usize>() {
                                            editor.begin_library_drag(index);
                                        }
                                        continue;
                                    }
                                    if command.starts_with("electronics.library.drag.move.") {
                                        continue;
                                    }
                                    if command.starts_with("electronics.library.drag.end.") {
                                        editor.finish_library_drag_at(
                                            self.input.snapshot().pointer_position,
                                            electronics_canvas,
                                        );
                                        continue;
                                    }
                                    if command != "sessions.reload"
                                        // `project.save` belongs to the application
                                        // boundary: the controller does not own the
                                        // project session, and the dedicated handler
                                        // below is what actually writes both documents.
                                        && command
                                            != crate::application_menu::command::PROJECT_SAVE
                                    {
                                        if matches!(
                                            command.as_str(),
                                            crate::application_menu::command::VIEW_SCHEMATIC
                                                | crate::application_menu::command::VIEW_PCB
                                        ) {
                                            electronics_surface_switched = true;
                                        }
                                        let response =
                                            crate::native_attached_executor::execute_native_ui_intent(
                                                editor,
                                                &mut self.attached_ledger,
                                                command,
                                            );
                                        if !response.ok {
                                            tracing::warn!(
                                                command,
                                                title = %response.title,
                                                "native Electronics UI command was rejected"
                                            );
                                            // A rejected command must reach the
                                            // user, not only the log.
                                            let message =
                                                response.lines.first().cloned().unwrap_or_else(
                                                    || {
                                                        raf_core::i18n::t(
                                                            "electronics.error.command_rejected",
                                                            self.settings_state.language,
                                                        )
                                                    },
                                                );
                                            editor.push_surface_error(message);
                                        }
                                    }
                                }
                            }
                            if domain_intents.iter().any(|intent| {
                                matches!(
                                    intent,
                                    crate::native_workbench::NativeWorkbenchIntent::Command(
                                        command
                                    ) if command == crate::application_menu::command::PROJECT_SAVE
                                )
                            }) {
                                let project_saved = match self.project.as_ref() {
                                    Some(project) => {
                                        match save_project_document(
                                            project,
                                            &self.scene,
                                            runtime.node_graph(),
                                        ) {
                                            Ok(()) => true,
                                            Err(error) => {
                                                tracing::warn!(
                                                    %error,
                                                    "native project save failed"
                                                );
                                                editor.push_surface_error(raf_core::i18n::t(
                                                    "electronics.error.project_save_failed",
                                                    self.settings_state.language,
                                                ));
                                                false
                                            }
                                        }
                                    }
                                    None => false,
                                };
                                // `editor.save` writes the schematic and the
                                // PCB. The dirty marker is only cleared when
                                // both landed on disk.
                                let documents_saved = match self.project.as_ref() {
                                    Some(project) => match editor.save(project) {
                                        Ok(()) => true,
                                        Err(error) => {
                                            tracing::warn!(
                                                %error,
                                                "native electronics document save failed"
                                            );
                                            editor.push_surface_error(raf_core::i18n::t(
                                                "electronics.error.save_failed",
                                                self.settings_state.language,
                                            ));
                                            false
                                        }
                                    },
                                    None => false,
                                };
                                if project_saved && documents_saved {
                                    self.pending_document_saved = true;
                                } else {
                                    self.pending_document_save_failed = true;
                                }
                            }
                        }
                    }
                    let old_canvas = runtime.layout().electronics_canvas_for_pcb(self.electronics_editor.as_ref().is_some_and(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb));
                    synchronize_bottom_dock_layout(runtime, workbench);
                    if runtime.project_type() == ProjectType::Electronics {
                        if let Some(editor) = self.electronics_editor.as_mut() {
                            editor.preserve_camera_for_canvas_change(
                                old_canvas,
                                runtime.layout().electronics_canvas_for_pcb(editor.active_surface() == raf_electronics::CadSurfaceKind::Pcb),
                            );
                        }
                    }
                    if runtime.project_type() == ProjectType::Game {
                        let (console_outputs, saved_document) = apply_workbench_intents(
                            runtime,
                            &mut self.scene,
                            self.project.as_ref(),
                            domain_intents,
                        );
                        self.pending_document_saved |= saved_document;
                        for output in console_outputs {
                            workbench.log_console_output(output);
                        }
                    }
                    if runtime.project_type() == ProjectType::Electronics {
                        let electronics_canvas_rect = runtime.layout().electronics_canvas_for_pcb(self.electronics_editor.as_ref().is_some_and(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb));
                        // Pointer hover alone is not a keyboard decision. A focused
                        // inspector or navigator text control must keep the
                        // keyboard, otherwise typing "r" rotates the component and
                        // Delete removes the selection while the user is typing.
                        let canvas_owns_pointer = runtime.input_router_mut().exclusive_pointer_owner()
                            == Some(raf_core::InputOwner::ElectronicsCanvas);
                        // A pending outline/wire deliberately holds no pointer
                        // capture between clicks. Passive panel hover must not
                        // swallow its Escape, while a text field or search that
                        // owned the keyboard at frame start keeps priority.
                        let cancel_canvas_gesture = !text_input_owned
                            && self.input.snapshot().key_pressed(raf_core::InputKey::Escape)
                            && self.electronics_editor.as_ref().is_some_and(NativeElectronicsEditor::has_pending_gesture);
                        if (cancel_canvas_gesture || ((!workbench.has_interactive_hover() || canvas_owns_pointer)
                            && (!workbench.captures_keyboard_input() || canvas_owns_pointer)))

                        {
                            if let Some(editor) = self.electronics_editor.as_mut() {
                                let result = editor.process_input(
                                    self.input.snapshot(),
                                    runtime.input_router_mut(),
                                    electronics_canvas_rect,
                                );
                                if result.request_redraw {
                                    runtime.request_animation_frame();
                                }
                            }
                        }
                    }
                    let has_active_motion = workbench.has_active_ui_motion();
                    let has_active_text_repeat = workbench.has_active_text_repeat();
                    if had_ui_activity || workbench.needs_ui_frame() {
                        runtime.request_animation_frame();
                    }
                    runtime.set_continuous_ui_motion(
                        has_active_motion
                            || has_active_text_repeat
                            || self
                                .electronics_editor
                                .as_ref()
                                .is_some_and(NativeElectronicsEditor::analysis_running),
                    );
                }
            }
            if let Some(cursor) = cursor_hint {
                if let Some(window) = self.window.as_ref() {
                    window.set_cursor(native_cursor_icon(cursor));
                }
            }
            let text_input_focused = text_input_owned;
            let focused_text_rect = if self.settings_open {
                self.settings_surface
                    .as_ref()
                    .and_then(SettingsSurfaceHost::focused_text_rect)
            } else {
                self.workbench
                    .as_ref()
                    .and_then(NativeGameWorkbench::focused_text_rect)
            };
            if let Some(window) = self.window.as_ref() {
                window.set_ime_allowed(focused_text_rect.is_some());
                if let Some(rect) = focused_text_rect {
                    let scale = self.input.scale_factor().max(0.25) as f32;
                    window.set_ime_cursor_area(
                        PhysicalPosition::new(
                            (rect.x * scale).round() as i32,
                            (rect.y * scale).round() as i32,
                        ),
                        PhysicalSize::new(
                            (rect.width * scale).round().max(1.0) as u32,
                            (rect.height * scale).round().max(1.0) as u32,
                        ),
                    );
                }
            }
            let dispatched = if !self.settings_open
                && !self.exit_confirmation_open
                && !exit_confirmation_was_open
                && runtime.project_type() == ProjectType::Game
            {
                runtime.dispatch_shortcuts(
                    self.input.snapshot(),
                    &mut self.scene,
                    text_input_focused,
                    self.project.is_some(),
                )
            } else if !self.settings_open
                && !self.exit_confirmation_open
                && !exit_confirmation_was_open
                && runtime.project_type() == ProjectType::Electronics
            {
                // Electronics offers the same accelerators in its Edit and File
                // menus, so Ctrl+S must work there too. `runtime.dispatch_shortcuts`
                // is not reused: it applies the commands to the Game scene, which
                // would make Ctrl+Z undo scene history instead of the documents.
                let has_electronics_selection = self
                    .electronics_editor
                    .as_ref()
                    .and_then(NativeElectronicsEditor::selection)
                    .is_some();
                match electronics_shortcut_command(
                    self.input.snapshot(),
                    text_input_focused
,
                    has_electronics_selection,
                ) {
                    Some(value) if value == crate::application_menu::command::PROJECT_SAVE => {
                        vec![value.to_string()]
                    }
                    Some(value) if value == crate::application_menu::command::SEARCH_OPEN => {
                        vec![value.to_string()]
                    }
                    Some(value) if value == crate::application_menu::command::EDIT_UNDO => {
                        if let Some(editor) = self.electronics_editor.as_mut() {
                            editor.undo();
                        }
                        vec![value.to_string()]
                    }
                    Some(value) if value == crate::application_menu::command::EDIT_REDO => {
                        if let Some(editor) = self.electronics_editor.as_mut() {
                            editor.redo();
                        }
                        vec![value.to_string()]
                    }
                    // Delete is destructive, so it only arms the confirmation the
                    // Electronics surface presents.
                    Some(value) if value == crate::application_menu::command::EDIT_DELETE => {
                        if let Some(editor) = self.electronics_editor.as_mut() {
                            editor.request_delete_selected();
                        }
                        vec![value.to_string()]
                    }
                    // Clipboard and duplicate have no Electronics document model yet,
                    // and Select All is single-selection here. Ignoring them keeps the
                    // accelerator honest instead of mutating the Game scene.
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            if dispatched
                .iter()
                .any(|command| command == crate::application_menu::command::SEARCH_OPEN)
            {
                if let Some(workbench) = self.workbench.as_mut() {
                    workbench.open_search();
                }
                runtime.request_ui_frame();
            }
            if dispatched
                .iter()
                .any(|command| command == crate::application_menu::command::PROJECT_SAVE)
            {
                // Deferred past the runtime borrow: persisting a document takes
                // `&mut self`, and `runtime` is a mutable borrow of this same
                // state that is still needed below.
                project_save_requested = true;
            }
            if !self.settings_open
                && !self.exit_confirmation_open
                && !exit_confirmation_was_open
                && runtime.project_type() == ProjectType::Game
            {
                if let Some(workbench) = self.workbench.as_ref() {
                    runtime
                        .game_viewport_mut()
                        .apply_engine_settings(workbench.engine_settings());
                    if let Some(project) = self.project.as_ref() {
                        runtime.apply_project_settings(&project.settings);
                    }
                }
                let viewport_update =
                    runtime.update_game_input(self.input.snapshot(), &mut self.scene);
                if let Some(key) = runtime.game_viewport_mut().placement_notice.take() {
                    if let Some(workbench) = self.workbench.as_mut() {
                        workbench.authoring_status(key);
                    }
                }
                if std::mem::take(&mut runtime.game_viewport_mut().edit_notice) {
                    if let Some(workbench) = self.workbench.as_mut() {
                        workbench.log_console_output(crate::commands::CommandOutput::info(
                            raf_core::i18n::t(
                                "authoring.edit_pending",
                                self.settings_state.language,
                            ),
                            Vec::new(),
                            serde_json::json!({"available":false,"planned_version":"0.12"}),
                        ));
                    }
                }
                let selection_changed = viewport_update.selection_changed
                    || runtime.game_viewport().selected.as_slice()
                        != selected_before_input.as_slice();
                let quality = self
                    .workbench
                    .as_ref()
                    .map(|workbench| workbench.engine_settings().render_quality);
                queue_hierarchy_refresh = quality.is_some_and(|quality| {
                    should_queue_eager_hierarchy_refresh(quality, selection_changed)
                });
            }
        }

        self.apply_deferred_persistence(
            project_save_requested,
            electronics_surface_switched,
            linear_save_requested,
            persist_agent_settings,
        );

        let has_active_text_repeat =
            self.has_active_text_repeat(loading_active, exit_confirmation_was_open);
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.set_continuous_text_input(has_active_text_repeat);
        runtime.finish_input_frame(self.input.snapshot());

        if !runtime.begin_frame(now) {
            self.input.begin_frame();
            return;
        }
        let Some(host) = self.window_host.as_mut() else {
            runtime.cancel_frame();
            self.input.begin_frame();
            return;
        };
        let Some(compositor) = self.compositor.as_mut() else {
            runtime.cancel_frame();
            self.input.begin_frame();
            return;
        };
        let target_size = [host.width(), host.height()];
        let project_open = self.project.is_some() && !self.agents_open;
        let studio_model = (!project_open)
            .then(|| {
                self.studio
                    .as_ref()
                    .map(NativeStudioSurface::model_snapshot)
            })
            .flatten();
        // Let the open settings draft preview its language in the modal. The
        // committed settings remain authoritative everywhere else until the
        // user applies the draft, but using the draft resolver here keeps the
        // preview internally consistent with the controls being edited.
        let ui_language = if self.settings_open {
            self.settings_surface
                .as_ref()
                .map(|surface| surface.draft.language)
                .unwrap_or(self.settings_state.language)
        } else {
            self.settings_state.language
        };
        let canvas_layer = if loading_active {
            None
        } else if project_open && runtime.project_type() == ProjectType::Game {
            let Some(canvas_layer) = runtime.render_game_canvas(&self.scene) else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            Some(canvas_layer)
        } else if project_open {
            let electronics_canvas_rect = runtime.layout().electronics_canvas_for_pcb(self.electronics_editor.as_ref().is_some_and(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb));
            let canvas_target =
                electronics_canvas_rect.to_physical(self.input.scale_factor() as f32, target_size);
            sync_electronics_canvas(
                self.electronics_editor.as_mut(),
                self.electronics_canvas.as_mut(),
                &mut self.electronics_frame_key,
                electronics_canvas_rect,
                canvas_target,
                native_palette(self.settings_state.theme),
                self.input.scale_factor() as f32,
            );
            self.electronics_canvas
                .as_mut()
                .and_then(|canvas| canvas.render_layer(runtime.graphics_mut(), canvas_target))
        } else {
            None
        };
        if loading_active {
            let Some(loading) = self.loading.as_mut() else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            loading.sync(
                runtime.layout().window,
                native_palette(self.settings_state.theme),
                if self.startup_loading_presented {
                    0.72
                } else {
                    0.28
                },
                self.settings_state.language,
            );
            let mut layer = loading.compositor_layer(self.input.scale_factor() as f32, target_size);
            let rendered = host.render_editor_layers(
                compositor,
                None,
                std::slice::from_mut(&mut layer),
                |key| raf_core::i18n::t(key, self.settings_state.language),
            );
            observe_editor_presentation(host, &rendered);
            #[cfg(windows)]
            if let (Some(accessibility), Some(window)) = (&mut self.accessibility, &self.window) {
                accessibility.publish(window, std::slice::from_mut(&mut layer), false, ui_language,
                    |key| raf_core::i18n::t(key, ui_language));
            }
        } else if self.agents_open {
            let Some(surface) = self.agent_surface.as_mut() else {
                runtime.cancel_frame();
                return;
            };
            let attached = self
                .project
                .as_ref()
                .zip(self.workbench.as_ref())
                .map(|(p, w)| (p.path.as_path(), w.active_session_id(), &self.scene));
            let preview_layer = agents::render_preview_layer(
                surface,
                &self.agents,
                attached,
                &self.settings_state,
                host,
                self.input.scale_factor() as f32,
                target_size,
            );
            let mut layers =
                vec![surface.compositor_layer(self.input.scale_factor() as f32, target_size)];
            if self.settings_open {
                if let Some(settings) = self.settings_surface.as_mut() {
                    layers.push(
                        settings.compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            if self.exit_confirmation_open {
                if let Some(confirmation) = self.exit_confirmation_surface.as_mut() {
                    layers.push(
                        confirmation
                            .compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            let rendered =
                host.render_editor_layers(compositor, preview_layer, &mut layers, |key| {
                    raf_core::i18n::t(key, ui_language)
                });
            observe_editor_presentation(host, &rendered);
            #[cfg(windows)]
            if let (Some(accessibility), Some(window)) = (&mut self.accessibility, &self.window) {
                accessibility.publish(window, &mut layers, self.settings_open || self.exit_confirmation_open, ui_language,
                    |key| raf_core::i18n::t(key, ui_language));
            }
        } else if project_open {
            let Some(workbench) = self.workbench.as_mut() else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            if runtime.project_type() == ProjectType::Electronics {
                workbench.sync_electronics_overlay(
                    self.electronics_editor.as_ref(),
                    runtime.layout().electronics_canvas_for_pcb(self.electronics_editor.as_ref().is_some_and(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb)),
                );
            }
            // Input commits can change the document after the pre-input sync.
            // Present and publish the resulting surface in this same frame.
            if refresh_workbench_after_input {
                let selected = runtime.game_viewport().selected.clone();
                let compass = if runtime.project_type() == ProjectType::Game && runtime.game_viewport().mode == NativeViewportMode::View3d {
                    ViewportCompassState::from_orbit(runtime.game_viewport().bridge().orbit_yaw(), runtime.game_viewport().bridge().orbit_pitch())
                } else { ViewportCompassState::hidden() };
                workbench.sync(runtime.layout(), &self.scene, &selected,
                    runtime.history().can_undo(), runtime.history().can_redo(), runtime.has_clipboard(),
                    runtime.node_graph(), runtime.selected_graph_node(), runtime.node_graph_revision(),
                    runtime.can_undo_node_graph(), runtime.can_redo_node_graph(), runtime.node_validation(),
                    self.project.as_ref(), now, compass, self.electronics_editor.as_ref());
            }
            let mut layers =
                workbench.compositor_layers(self.input.scale_factor() as f32, target_size);
            if self.settings_open {
                if let Some(settings_surface) = self.settings_surface.as_mut() {
                    layers.push(
                        settings_surface
                            .compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            if self.exit_confirmation_open {
                if let Some(surface) = self.exit_confirmation_surface.as_mut() {
                    layers.push(
                        surface.compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            let rendered =
                host.render_editor_layers(compositor, canvas_layer, &mut layers, |key| {
                    studio_model
                        .as_ref()
                        .map(|model| crate::native_studio::resolve_hub_text(key, model, ui_language))
                        .unwrap_or_else(|| raf_core::i18n::t(key, ui_language))
                });
            observe_editor_presentation(host, &rendered);
            #[cfg(windows)]
            if let (Some(accessibility), Some(window)) = (&mut self.accessibility, &self.window) {
                accessibility.publish(window, &mut layers, self.settings_open || self.exit_confirmation_open, ui_language,
                    |key| raf_core::i18n::t(key, ui_language));
            }
        } else {
            let Some(studio) = self.studio.as_mut() else {
                runtime.cancel_frame();
                self.input.begin_frame();
                return;
            };
            let mut layers =
                vec![studio.compositor_layer(self.input.scale_factor() as f32, target_size)];
            if self.settings_open {
                if let Some(settings_surface) = self.settings_surface.as_mut() {
                    layers.push(
                        settings_surface
                            .compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            if self.exit_confirmation_open {
                if let Some(surface) = self.exit_confirmation_surface.as_mut() {
                    layers.push(
                        surface.compositor_layer(self.input.scale_factor() as f32, target_size),
                    );
                }
            }
            let rendered =
                host.render_editor_layers(compositor, canvas_layer, &mut layers, |key| {
                    studio_model
                        .as_ref()
                        .map(|model| crate::native_studio::resolve_hub_text(key, model, ui_language))
                        .unwrap_or_else(|| raf_core::i18n::t(key, ui_language))
                });
            observe_editor_presentation(host, &rendered);
            #[cfg(windows)]
            if let (Some(accessibility), Some(window)) = (&mut self.accessibility, &self.window) {
                accessibility.publish(window, &mut layers, self.settings_open || self.exit_confirmation_open, ui_language, |key| {
                    studio_model.as_ref().map(|model| crate::native_studio::resolve_hub_text(key, model, ui_language))
                        .unwrap_or_else(|| raf_core::i18n::t(key, ui_language))
                });
            }
        }
        let scene_rendered_this_frame = runtime.scene_rendered_this_frame();
        let scene_metrics = runtime.graphics().snapshot().last_frame_metrics;
        let frame_cpu_ms = scene_metrics.frame_cpu_ms;
        let frame_gpu_ms = if scene_rendered_this_frame && scene_metrics.gpu_timing_sampled {
            scene_metrics.gpu_frame_ms
        } else {
            0.0
        };
        let presented_at = self.started_at.elapsed().as_secs_f64();
        let total_cpu_ms = redraw_started_at.elapsed().as_secs_f32() * 1000.0;
        runtime.finish_frame(presented_at, frame_cpu_ms, frame_gpu_ms, total_cpu_ms);
        if queue_hierarchy_refresh {
            // The workbench was synchronized before viewport input so
            // retained UI keeps ownership of the pointer first. Low, Medium
            // and High queue one follow-up UI frame after this presentation so
            // a selection cannot leave Hierarchy stale until another pointer
            // event. Potato keeps its intentional lazy path.
            runtime.request_ui_frame();
        }
        let initialize_workspace_after_present = loading_active && self.startup_loading_presented;
        if loading_active && !initialize_workspace_after_present {
            runtime.request_ui_frame();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        self.input.begin_frame();
        if initialize_workspace_after_present {
            self.finish_startup_loading();
        } else if loading_active {
            self.startup_loading_presented = true;
        }
    }
}

