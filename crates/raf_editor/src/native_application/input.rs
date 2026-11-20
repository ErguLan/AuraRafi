//! Native window/device input routing into the shared input bridge.

use raf_core::project::ProjectType;
use raf_ui::UiWindowCommand;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::native_studio::NativeStudioSurface;
use crate::native_workbench::NativeGameWorkbench;
use crate::panels::settings_surface_host::SettingsSurfaceHost;

use super::native_environment;
use super::window::native_resize_edge;
use super::NativeEditorApplication;

impl NativeEditorApplication {
    pub(super) fn window_event_native(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                self.close_requested = true;
                self.game_runtime.stop_all();
            }
            WindowEvent::Resized(size) => {
                if let Some(host) = self.window_host.as_mut() {
                    host.resize(size.width, size.height);
                }
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.resize([size.width, size.height], window.scale_factor() as f32);
                }
                let present_refresh_hz = self
                    .window_host
                    .as_ref()
                    .and_then(|host| host.effective_present_refresh_hz());
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.set_present_refresh_hz(present_refresh_hz);
                }
                let scale = self.input.scale_factor() as f32;
                let logical_size = [
                    size.width as f32 / scale.max(0.25),
                    size.height as f32 / scale.max(0.25),
                ];
                if let Some(workbench) = self.workbench.as_mut() {
                    workbench.set_viewport_size(logical_size);
                }
                let environment = native_environment(&self.settings_state, logical_size);
                if let Some(studio) = self.studio.as_mut() {
                    studio.set_environment(environment);
                }
                if let Some(settings_surface) = self.settings_surface.as_mut() {
                    settings_surface.set_environment(environment);
                }
                if let Some(exit_confirmation_surface) = self.exit_confirmation_surface.as_mut() {
                    exit_confirmation_surface.set_environment(environment);
                }
                if let Some(loading) = self.loading.as_mut() {
                    loading.set_environment(environment);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            event @ WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if !window.is_maximized() => {
                let logical_size = [
                    window.inner_size().width as f32 / self.input.scale_factor() as f32,
                    window.inner_size().height as f32 / self.input.scale_factor() as f32,
                ];
                let edge = self
                    .input
                    .snapshot()
                    .pointer_position
                    .and_then(|position| native_resize_edge(position, logical_size));
                if let Some(edge) = edge {
                    // An interactive retained control (window buttons, drag
                    // region, menus) owns the edge press; the 8px resize band
                    // must never swallow it.
                    if !self.retained_hover_blocks_window_resize() {
                        if let Some(host) = self.window_host.as_ref() {
                            if host
                                .execute_window_command(UiWindowCommand::BeginResize(edge))
                                .is_ok()
                            {
                                return;
                            }
                        }
                    }
                }
                if self.input.ingest(&event) {
                    if let Some(runtime) = self.runtime.as_mut() {
                        runtime.request_ui_frame();
                    }
                    // Preserve each button edge before Winit can coalesce the
                    // release and subsequent motion into one CAD snapshot.
                    window.request_redraw();
                    if self.project.as_ref().is_some_and(|project| project.project_type == ProjectType::Electronics) {
                        self.redraw();
                    }
                }
            }
            other => {
                let flush_button_edge = matches!(&other, WindowEvent::MouseInput { .. });
                let flush_command_key = matches!(&other, WindowEvent::KeyboardInput { event, .. }
                    if event.state == ElementState::Pressed && self.input.snapshot().modifiers.command_modifier());
                let focused = match &other {
                    WindowEvent::Focused(focused) => Some(*focused),
                    _ => None,
                };
                if self.input.ingest(&other) {
                    if let Some(runtime) = self.runtime.as_mut() {
                        if let Some(focused) = focused {
                            if let Some(player) = &mut self.game_runtime.in_place {
                                player.set_focused(focused);
                            }
                            runtime.set_window_focused(focused);
                            if !focused {
                                runtime.input_router_mut().cancel_all();
                            }
                        } else {
                            runtime.request_ui_frame();
                        }
                    }
                    window.request_redraw();
                    if (flush_button_edge || flush_command_key) && self.project.as_ref().is_some_and(|project| project.project_type == ProjectType::Electronics) { self.redraw(); }
                }
            }
        }
    }

    pub(super) fn device_event_native(&mut self, event: winit::event::DeviceEvent) {
        if self.game_runtime.in_place.is_some()
            && self.window.as_ref().is_some_and(|w| w.has_focus())
        {
            if let winit::event::DeviceEvent::MouseMotion { delta } = event {
                self.input.ingest_relative_motion(delta);
                if let Some(runtime) = &mut self.runtime {
                    runtime.request_ui_frame();
                }
            }
        }
    }

    /// Which retained surface currently owns repeated key input. One shared
    /// answer keeps text-repeat scheduling consistent across surfaces.
    pub(super) fn has_active_text_repeat(
        &self,
        loading_active: bool,
        exit_confirmation_was_open: bool,
    ) -> bool {
        if loading_active || self.exit_confirmation_open || exit_confirmation_was_open {
            false
        } else if self.settings_open {
            self.settings_surface
                .as_ref()
                .is_some_and(SettingsSurfaceHost::has_active_text_repeat)
        } else if self.agents_open {
            self.agent_surface
                .as_ref()
                .is_some_and(|surface| surface.has_active_text_repeat())
        } else if self.project.is_some() {
            self.workbench
                .as_ref()
                .is_some_and(NativeGameWorkbench::has_active_text_repeat)
        } else {
            self.studio
                .as_ref()
                .is_some_and(NativeStudioSurface::has_active_text_repeat)
        }
    }
}

