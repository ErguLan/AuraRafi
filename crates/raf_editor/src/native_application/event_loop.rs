//! Winit application loop: startup entry, per-turn scheduling, and trait
//! dispatch into the window/input/presentation handlers.

use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

use crate::native_attached_executor::poll_attached_commands;
use crate::native_workbench::NativeGameWorkbench;

use super::NativeEditorApplication;

pub fn run_native() -> Result<(), String> {
    // A user-event proxy lets attached CLI/MCP clients wake an idle editor
    // without any polling: the loop sleeps in the OS pump until wake_up().
    let event_loop = EventLoop::<()>::with_user_event()
        .build()
        .map_err(|error| format!("native event loop: {error}"))?;
    let mut application = NativeEditorApplication::default();
    if let Some(path) = startup_project_argument() {
        tracing::info!(path = %path.display(), "opening project from the command line");
        application.request_editor_project(path);
    }
    let wakeup = event_loop.create_proxy();
    application.event_loop_proxy = Some(wakeup.clone());
    #[cfg(windows)]
    {
        application.accessibility = Some(crate::native_accessibility::NativeAccessibility::new(
            wakeup.clone(),
        ));
    }
    application.attached_host.set_wakeup(wakeup);
    event_loop
        .run_app(&mut application)
        .map_err(|error| format!("native editor event loop: {error}"))
}

fn startup_project_argument() -> Option<std::path::PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--project" || argument == "--open" {
            return args.next().map(std::path::PathBuf::from);
        }
        if let Some(path) = argument
            .strip_prefix("--project=")
            .or_else(|| argument.strip_prefix("--open="))
        {
            return Some(std::path::PathBuf::from(path));
        }
    }
    None
}

impl NativeEditorApplication {
    fn about_to_wait_native(&mut self, event_loop: &ActiveEventLoop) {
        // Winit can miss a Focused event after a restore/alt-tab. Reconcile
        // against the window's real focus before pacing the next frame so the
        // editor does not stay stranded on the 5 FPS background ceiling.
        if let (Some(window), Some(runtime)) = (self.window.as_ref(), self.runtime.as_mut()) {
            let focused = window.has_focus();
            if runtime.window_focused() != focused {
                runtime.set_window_focused(focused);
            }
        }
        if let Some(view) = self.pending_lab_open.take() {
            self.open_agents(None);
            self.focus_mounted_laboratory();
            self.apply_laboratory_command(view);
        }
        if std::mem::take(&mut self.pending_agents_open) {
            self.open_agents(None);
        }
        self.poll_agent_service();
        self.poll_electronics_laboratory();
        let preview_deadline = self.poll_agent_preview();
        if let Some(command) = self.pending_runtime_command.take() {
            self.apply_runtime_command(&command);
        }
        let report = self.game_runtime.poll();
        let notice = report.notice;
        let clear_notice = report.clear_notice && notice.is_none();
        if let Some(notice) = notice {
            if let Some(workbench) = &mut self.workbench {
                workbench.set_runtime_notice(Some(notice.clone()));
            }
            self.runtime_notice = Some(notice);
            if let Some(runtime) = &mut self.runtime {
                runtime.request_ui_frame();
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        if clear_notice {
            let mut changed = self.runtime_notice.take().is_some();
            if let Some(workbench) = &mut self.workbench {
                changed |= workbench.set_runtime_notice(None);
            }
            if changed {
                if let Some(runtime) = &mut self.runtime {
                    runtime.request_ui_frame();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
        if let Some(workbench) = &mut self.workbench {
            let had_entries = !report.entries.is_empty();
            for entry in report.entries {
                workbench.log_runtime_entry(entry);
            }
            if had_entries {
                if let Some(runtime) = &mut self.runtime {
                    runtime.request_ui_frame();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            if workbench.set_runtime_toolbar(self.game_runtime.toolbar_state(&self.settings_state))
            {
                if let Some(runtime) = &mut self.runtime {
                    runtime.request_ui_frame();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
        // Attached CLI/MCP clients wake this loop through the user-event
        // proxy; draining here answers them without waiting for a rendered
        // frame. Document serialization is revision-gated and never belongs
        // to the redraw hot path.
        self.sync_attached_document_revision();
        let pending_attached = self.attached_host.drain();
        let attached_result = if pending_attached.is_empty() {
            Default::default()
        } else {
            let project_assets = self
                .workbench
                .as_ref()
                .map(|workbench| workbench.agent_assets().to_vec())
                .unwrap_or_default();
            let catalog_pending = self
                .workbench
                .as_ref()
                .is_some_and(|workbench| workbench.agent_catalog_pending());
            let catalog_error = self
                .workbench
                .as_ref()
                .and_then(|workbench| workbench.agent_catalog_error().map(str::to_string));
            poll_attached_commands(
                &mut self.attached_host,
                pending_attached,
                &mut self.attached_ledger,
                self.runtime.as_mut(),
                self.workbench.as_mut(),
                &mut self.scene,
                self.electronics_editor.as_mut(),
                self.project.as_ref(),
                &project_assets,
                catalog_pending,
                catalog_error.as_deref(),
                self.settings_state.language,
                &self.settings_state,
            )
        };
        if let Some(path) = attached_result.open_project.clone() {
            self.request_editor_project(path);
        }
        if attached_result.changed {
            self.attached_ledger
                .mark_document(crate::scene_history::scene_fingerprint(&self.scene));
            self.persist_attached_ledger();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        if attached_result.document_saved {
            self.pending_document_saved = true;
        }
        if self.game_runtime.in_place.is_some() {
            if Instant::now() >= self.next_runtime_frame {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                self.next_runtime_frame
                    .max(Instant::now() + Duration::from_millis(1)),
            ));
            return;
        }
        if self.pending_document_saved {
            self.pending_document_saved = false;
            // A partial save must not clear the dirty marker: the editor would
            // then claim everything is on disk while a document is missing.
            if !self.pending_document_save_failed {
                self.mark_document_saved();
                self.refresh_agent_save_receipt();
            }
            self.pending_document_save_failed = false;
        }
        if let Some((key, value)) = self.pending_project_range.take() {
            self.apply_project_setting_range_intent(key, value);
        }
        if let Some((key, value)) = self.pending_project_text.take() {
            self.apply_project_setting_text_intent(key, value);
        }
        if let Some((key, value)) = self.pending_project_setting.take() {
            self.apply_project_setting_toggle_intent(key, value);
        }
        if let Some(command) = self.pending_project_command.take() {
            self.apply_project_setting_command_intent(command);
        }
        self.maybe_auto_save();
        if let Some(section) = self.pending_settings_open.take() {
            self.open_settings(section);
        }
        if !self.exit_confirmation_open {
            if let Some(open_create) = self.pending_return_to_hub {
                if self.has_unsaved_changes() {
                    self.ensure_exit_confirmation_surface();
                    self.exit_confirmation_open = true;
                    if let Some(runtime) = self.runtime.as_mut() {
                        runtime.request_animation_frame();
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                } else {
                    self.pending_return_to_hub = None;
                    self.return_to_hub(open_create);
                }
            }
        }
        for command in std::mem::take(&mut self.pending_window_commands) {
            self.execute_window_command(command);
        }
        if self.close_requested {
            let save = if self.project.is_some() {
                self.save_current_documents()
            } else {
                Ok(())
            };
            if let Err(error) = save.and_then(|_| self.agents.shutdown()) {
                self.close_requested = false;
                self.open_agents(None);
                if let Some(workspace) = self.agents.selected_workspace() {
                    workspace.panel.borrow_mut().save_error = Some(error);
                }
                return;
            }
            self.game_runtime.stop_all();
            self.persist_settings();
            event_loop.exit();
            return;
        }
        if let Some(runtime) = self.runtime.as_ref() {
            let auto_save_deadline = self.auto_save_deadline();
            match runtime.seconds_until_next_frame(self.elapsed_seconds()) {
                Some(delay) if delay <= f64::EPSILON => {
                    event_loop.set_control_flow(ControlFlow::Wait);
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                Some(delay) if delay.is_finite() => {
                    let frame_deadline =
                        Instant::now() + Duration::from_secs_f64(delay.max(0.0).min(60.0));
                    let deadline = auto_save_deadline
                        .map_or(frame_deadline, |auto_save| frame_deadline.min(auto_save));
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                }
                _ => {
                    if let Some(deadline) = auto_save_deadline {
                        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                    } else {
                        event_loop.set_control_flow(ControlFlow::Wait);
                    }
                }
            }
        } else {
            if let Some(deadline) = self.auto_save_deadline() {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
        if self.game_runtime.has_instances() {
            let runtime_deadline = Instant::now() + Duration::from_millis(100);
            let deadline = match event_loop.control_flow() {
                ControlFlow::WaitUntil(existing) => existing.min(runtime_deadline),
                _ => runtime_deadline,
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
        if self.agents.has_work()
            || (self.project.is_some()
                && self
                    .workbench
                    .as_ref()
                    .is_some_and(NativeGameWorkbench::has_agent_work))
        {
            let agent_deadline = Instant::now() + Duration::from_millis(40);
            let deadline = match event_loop.control_flow() {
                ControlFlow::WaitUntil(existing) => existing.min(agent_deadline),
                _ => agent_deadline,
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
        if self.agents.has_lab_work()
            || self
                .electronics_editor
                .as_ref()
                .is_some_and(|editor| editor.lab.pending() || editor.lab.dc_requested)
        {
            let wake = Instant::now() + Duration::from_millis(50);
            let deadline = match event_loop.control_flow() {
                ControlFlow::WaitUntil(existing) => existing.min(wake),
                _ => wake,
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
        if self
            .workbench
            .as_ref()
            .is_some_and(NativeGameWorkbench::has_pending_footprint_import)
        {
            let download_deadline = Instant::now() + Duration::from_millis(80);
            let deadline = match event_loop.control_flow() {
                ControlFlow::WaitUntil(existing) => existing.min(download_deadline),
                _ => download_deadline,
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        if let Some(preview_deadline) = preview_deadline {
            let deadline = match event_loop.control_flow() {
                ControlFlow::WaitUntil(existing) => existing.min(preview_deadline),
                _ => preview_deadline,
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }
}

impl ApplicationHandler for NativeEditorApplication {
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.device_event_native(event);
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.resumed_native(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.window_event_native(event_loop, window_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.about_to_wait_native(event_loop);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.request_ui_frame();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
