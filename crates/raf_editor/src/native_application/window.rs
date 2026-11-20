//! Native window creation, resize/DPI plumbing, cursor mapping and menu commands.

use std::sync::Arc;
use std::time::Instant;

use raf_core::config::RenderExecutionPolicy;
use raf_core::project::ProjectType;
use raf_render::api_graphic_basic::ui_surface::NativeUiWindowConfig;
use raf_render::api_graphic_basic::{GraphicsAdapterPreference, NativeEditorCompositor};
use raf_ui::{UiResizeEdge, UiWindowCommand};
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_editor_runtime::NativeEditorRuntime;
use crate::native_electronics::NativeElectronicsCanvas;
use crate::native_project_controller::{initial_node_graph, initial_project, project_capabilities};
use crate::native_studio::NativeStudioSurface;
use crate::native_workbench::NativeGameWorkbench;
use crate::panels::exit_confirmation_surface_host::ExitConfirmationSurfaceHost;
use crate::panels::loading_surface::LoadingSurfaceHost;
use crate::panels::settings_surface_host::SettingsSurfaceHost;

use super::layout::project_layout_request;
use super::{
    editor_window_icon, native_environment, native_palette, NativeEditorApplication,
    NATIVE_EDITOR_MIN_SIZE, NATIVE_EDITOR_SIZE, NATIVE_SPLASH_SIZE,
};

const NATIVE_RESIZE_BORDER: f32 = 8.0;

pub(super) fn native_cursor_icon(cursor: raf_ui::UiCursorIcon) -> winit::window::CursorIcon {
    use winit::window::CursorIcon;

    match cursor {
        raf_ui::UiCursorIcon::Default => CursorIcon::Default,
        raf_ui::UiCursorIcon::PointingHand => CursorIcon::Pointer,
        raf_ui::UiCursorIcon::Text => CursorIcon::Text,
        raf_ui::UiCursorIcon::ResizeHorizontal => CursorIcon::EwResize,
        raf_ui::UiCursorIcon::ResizeVertical => CursorIcon::NsResize,
        raf_ui::UiCursorIcon::ResizeNorthEastSouthWest => CursorIcon::NeswResize,
        raf_ui::UiCursorIcon::ResizeNorthWestSouthEast => CursorIcon::NwseResize,
    }
}

pub(super) fn native_resize_edge(position: [f32; 2], size: [f32; 2]) -> Option<UiResizeEdge> {
    let [x, y] = position;
    let [width, height] = size;
    if width <= 0.0 || height <= 0.0 || x < 0.0 || y < 0.0 || x > width || y > height {
        return None;
    }
    let near_left = x <= NATIVE_RESIZE_BORDER;
    let near_right = x >= width - NATIVE_RESIZE_BORDER;
    let near_top = y <= NATIVE_RESIZE_BORDER;
    let near_bottom = y >= height - NATIVE_RESIZE_BORDER;
    match (near_left, near_right, near_top, near_bottom) {
        (true, _, true, _) => Some(UiResizeEdge::NorthWest),
        (_, true, true, _) => Some(UiResizeEdge::NorthEast),
        (true, _, _, true) => Some(UiResizeEdge::SouthWest),
        (_, true, _, true) => Some(UiResizeEdge::SouthEast),
        (true, _, _, _) => Some(UiResizeEdge::West),
        (_, true, _, _) => Some(UiResizeEdge::East),
        (_, _, true, _) => Some(UiResizeEdge::North),
        (_, _, _, true) => Some(UiResizeEdge::South),
        _ => None,
    }
}

fn center_native_window(window: &Window, size: PhysicalSize<u32>) {
    let Some(monitor) = window.current_monitor() else {
        return;
    };
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let x =
        i64::from(monitor_position.x) + (i64::from(monitor_size.width) - i64::from(size.width)) / 2;
    let y = i64::from(monitor_position.y)
        + (i64::from(monitor_size.height) - i64::from(size.height)) / 2;
    window.set_outer_position(PhysicalPosition::new(
        x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    ));
}

impl NativeEditorApplication {
    pub(super) fn sync_accessibility_enabled(&mut self) {
        #[cfg(windows)]
        if let Some(accessibility) = &mut self.accessibility {
            accessibility.set_enabled(self.settings_state.native_accessibility_enabled && self.game_runtime.in_place.is_none());
        }
    }

    pub(super) fn resumed_native(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // The lightweight surface is presented before the workbench, Hub,
        // Electronics editor, and their retained documents are constructed.
        // This is a real initialization boundary, not a minimum-duration wait.
        let startup_loading = true;
        let mut attributes = Window::default_attributes()
            .with_title("Proyecto Rafi")
            .with_window_icon(editor_window_icon())
            .with_decorations(false)
            .with_resizable(!startup_loading);
        attributes = if startup_loading {
            attributes.with_inner_size(LogicalSize::new(
                NATIVE_SPLASH_SIZE[0],
                NATIVE_SPLASH_SIZE[1],
            ))
        } else {
            attributes
                .with_inner_size(LogicalSize::new(
                    NATIVE_EDITOR_SIZE[0],
                    NATIVE_EDITOR_SIZE[1],
                ))
                .with_min_inner_size(LogicalSize::new(
                    NATIVE_EDITOR_MIN_SIZE[0],
                    NATIVE_EDITOR_MIN_SIZE[1],
                ))
        };
        let Ok(window) = event_loop.create_window(attributes) else {
            tracing::error!("native editor window could not be created");
            event_loop.exit();
            return;
        };
        let window = Arc::new(window);
        #[cfg(windows)]
        if let Some(accessibility) = &mut self.accessibility {
            accessibility.attach(&window);
            accessibility.set_enabled(self.settings_state.native_accessibility_enabled);
        }
        let size = window.inner_size();
        if startup_loading {
            center_native_window(&window, size);
        }
        let scale_factor = window.scale_factor() as f32;
        let native_graphics_config = NativeUiWindowConfig {
            adapter_preference: if self.settings_state.render_execution_policy
                == RenderExecutionPolicy::GpuPreferred
            {
                GraphicsAdapterPreference::HighPerformance
            } else {
                GraphicsAdapterPreference::LowPower
            },
            ..NativeUiWindowConfig::default()
        };
        let mut host = match pollster::block_on(
            raf_render::api_graphic_basic::ui_surface::NativeUiWindowHost::create_with_config(
                window.clone(),
                native_graphics_config,
            ),
        ) {
            Ok(host) => host,
            Err(error) => {
                tracing::error!(%error, "native ApiGraphicBasic host could not be created");
                event_loop.exit();
                return;
            }
        };
        host.set_vsync(self.settings_state.vsync);
        let graphics_ready_ms = self.boot_started_at.elapsed().as_secs_f32() * 1000.0;

        let project_type = self
            .project
            .as_ref()
            .map(|project| project.project_type)
            .unwrap_or(ProjectType::Game);
        let logical_size = [
            size.width as f32 / scale_factor.max(0.25),
            size.height as f32 / scale_factor.max(0.25),
        ];
        let layout_request =
            project_layout_request(project_type, logical_size, self.project.as_ref());
        let mut runtime = NativeEditorRuntime::new(layout_request);
        runtime.set_project_type(project_type);
        let allow_gpu_features = self
            .project
            .as_ref()
            .is_some_and(|project| project.settings.allow_gpu_features);
        runtime.apply_engine_settings(&self.settings_state, allow_gpu_features);
        runtime.resize([size.width, size.height], scale_factor);
        runtime
            .graphics_mut()
            .set_shared_graphics_context(Some(host.shared_graphics_context()));
        runtime.set_present_refresh_hz(host.effective_present_refresh_hz());
        runtime.set_node_graph(initial_node_graph(self.project.as_ref()));

        let graphics = host.graphics_context();
        let compositor = NativeEditorCompositor::new(&graphics, [8, 11, 15, 255]);
        let palette = native_palette(self.settings_state.theme);
        let mut loading = LoadingSurfaceHost::new(&graphics, runtime.layout().window, palette);
        loading.set_environment(native_environment(
            &self.settings_state,
            [logical_size[0].max(1.0), logical_size[1].max(1.0)],
        ));

        self.input.set_scale_factor(window.scale_factor());
        self.window = Some(window.clone());
        self.window_host = Some(host);
        self.compositor = Some(compositor);
        self.runtime = Some(runtime);
        self.workbench = None;
        self.studio = None;
        self.agent_surface = None;
        self.settings_surface = None;
        self.exit_confirmation_surface = None;
        self.loading = Some(loading);
        self.show_loading = true;
        self.startup_loading_presented = false;
        self.startup_graphics_ready_ms = graphics_ready_ms;
        self.electronics_canvas = None;
        self.electronics_editor = None;
        self.electronics_frame_key = None;
        tracing::info!(
            graphics_ready_ms,
            project_open = self.project.is_some(),
            "native ApiGraphicBasic loading surface ready"
        );
        self.started_at = Instant::now();
        window.request_redraw();
    }

    pub(super) fn current_window_rect(&self) -> crate::editor_layout::EditorRect {
        self.runtime
            .as_ref()
            .map(|runtime| runtime.layout().window)
            .unwrap_or_default()
    }

    pub(super) fn ensure_workbench(&mut self) {
        if self.workbench.is_some() {
            return;
        }
        let rect = self.current_window_rect();
        let palette = native_palette(self.settings_state.theme);
        let Some(host) = self.window_host.as_ref() else {
            return;
        };
        let mut workbench = NativeGameWorkbench::new(&host.graphics_context(), palette, rect);
        workbench.set_script_relationship_wakeup(self.event_loop_proxy.clone());
        workbench.set_engine_settings(self.settings_state.clone());
        if let Some(project) = self.project.as_ref() {
            workbench.set_project_info(project.name.clone(), project.project_type);
        }
        self.workbench = Some(workbench);
    }

    pub(super) fn ensure_studio(&mut self) {
        if self.studio.is_some() {
            return;
        }
        let rect = self.current_window_rect();
        let palette = native_palette(self.settings_state.theme);
        let Some(host) = self.window_host.as_ref() else {
            return;
        };
        let mut studio = NativeStudioSurface::new(
            &host.graphics_context(),
            rect,
            palette,
            self.event_loop_proxy.clone(),
        );
        studio.set_language(self.settings_state.language);
        studio.set_environment(native_environment(
            &self.settings_state,
            [rect.width.max(1.0), rect.height.max(1.0)],
        ));
        self.studio = Some(studio);
    }

    pub(super) fn ensure_settings_surface(&mut self) {
        if self.settings_surface.is_some() {
            return;
        }
        let rect = self.current_window_rect();
        let palette = native_palette(self.settings_state.theme);
        let Some(host) = self.window_host.as_ref() else {
            return;
        };
        let mut surface = SettingsSurfaceHost::new(&host.graphics_context(), rect, palette);
        surface.set_environment(native_environment(
            &self.settings_state,
            [rect.width.max(1.0), rect.height.max(1.0)],
        ));
        self.settings_surface = Some(surface);
    }

    pub(super) fn ensure_exit_confirmation_surface(&mut self) {
        if self.exit_confirmation_surface.is_some() {
            return;
        }
        let rect = self.current_window_rect();
        let palette = native_palette(self.settings_state.theme);
        let Some(host) = self.window_host.as_ref() else {
            return;
        };
        let mut surface = ExitConfirmationSurfaceHost::new(&host.graphics_context(), rect, palette);
        surface.set_environment(native_environment(
            &self.settings_state,
            [rect.width.max(1.0), rect.height.max(1.0)],
        ));
        self.exit_confirmation_surface = Some(surface);
    }

    pub(super) fn ensure_electronics_canvas(&mut self) {
        if self.electronics_canvas.is_some() {
            return;
        }
        let Some(host) = self.window_host.as_ref() else {
            return;
        };
        self.electronics_canvas = Some(NativeElectronicsCanvas::new(
            &host.graphics_context(),
            [10, 10, 11, 255],
            raf_render::bridge::GraphicsSurfaceKind::SchematicCanvas,
        ));
    }

    fn initialize_startup_project(&mut self) {
        if self.project.is_some() {
            return;
        }
        if let Some(project) = initial_project() {
            self.request_editor_project(project.path);
        }
    }

    pub(super) fn finish_startup_loading(&mut self) {
        if !self.show_loading {
            return;
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        self.initialize_startup_project();
        let project_type = self
            .project
            .as_ref()
            .map(|project| project.project_type)
            .unwrap_or(ProjectType::Game);
        let logical_size = self
            .runtime
            .as_ref()
            .map(|runtime| {
                let [width, height] = runtime.layout().window.logical_size();
                [width as f32, height as f32]
            })
            .unwrap_or([NATIVE_SPLASH_SIZE[0] as f32, NATIVE_SPLASH_SIZE[1] as f32]);
        let startup_layout = project_layout_request(
            project_type,
            [logical_size[0], logical_size[1]],
            self.project.as_ref(),
        );
        let allow_gpu_features = self
            .project
            .as_ref()
            .is_some_and(|project| project.settings.allow_gpu_features);
        let startup_node_graph = initial_node_graph(self.project.as_ref());
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.set_project_type(project_type);
            runtime.set_layout_request(startup_layout);
            runtime.apply_engine_settings(&self.settings_state, allow_gpu_features);
            runtime.set_project_asset_root(
                self.project.as_ref().map(|project| project.path.as_path()),
            );
            if let Some(project) = self.project.as_ref() {
                runtime.apply_project_settings(&project.settings);
            }
            runtime.set_node_graph(startup_node_graph);
        }

        if self.project.is_some() {
            self.ensure_workbench();
        } else {
            self.ensure_studio();
        }
        if self.project.is_some() && project_type == ProjectType::Electronics {
            self.ensure_electronics_canvas();
        }
        self.electronics_editor = self
            .project
            .as_ref()
            .map(NativeElectronicsEditor::from_project);
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_engine_settings(&self.settings_state);
            if let Some(project) = self.project.as_ref() {
                editor.apply_project_settings(&project.settings);
            }
        }
        self.electronics_frame_key = None;
        self.attached_host.update_project(
            self.project.as_ref(),
            self.attached_ledger.revision(),
            None,
            None,
            project_capabilities(project_type),
        );
        self.mark_document_saved();

        let scale_factor = window.scale_factor().max(0.25);
        let target_size = PhysicalSize::new(
            (NATIVE_EDITOR_SIZE[0] * scale_factor).round().max(1.0) as u32,
            (NATIVE_EDITOR_SIZE[1] * scale_factor).round().max(1.0) as u32,
        );
        window.set_resizable(true);
        window.set_min_inner_size(Some(LogicalSize::new(
            NATIVE_EDITOR_MIN_SIZE[0],
            NATIVE_EDITOR_MIN_SIZE[1],
        )));
        let applied_size = window
            .request_inner_size(LogicalSize::new(
                NATIVE_EDITOR_SIZE[0],
                NATIVE_EDITOR_SIZE[1],
            ))
            .unwrap_or(target_size);
        center_native_window(&window, applied_size);
        self.show_loading = false;
        self.startup_loading_presented = false;
        self.loading = None;
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.request_ui_frame();
        }
        tracing::info!(
            graphics_ready_ms = self.startup_graphics_ready_ms,
            startup_total_ms = self.boot_started_at.elapsed().as_secs_f32() * 1000.0,
            project_open = self.project.is_some(),
            "native ApiGraphicBasic startup ready"
        );
        self.started_at = Instant::now();
        window.request_redraw();
    }

    pub(super) fn execute_window_command(&mut self, command: UiWindowCommand) {
        let Some(host) = self.window_host.as_ref() else {
            return;
        };
        match host.execute_window_command(command) {
            Ok(
                raf_render::api_graphic_basic::ui_surface::NativeWindowCommandResult::RequestClose,
            ) => {
                self.close_requested = true;
                self.game_runtime.stop_all();
            }
            Ok(raf_render::api_graphic_basic::ui_surface::NativeWindowCommandResult::ShowSystemMenu) => {
                if let Some(window) = self.window.as_ref() {
                    // winit 0.30 has no cursor getter; the native input bridge
                    // already tracks the logical pointer position for this
                    // window, so reuse it and fall back to the window center.
                    let scale = (window.scale_factor().max(0.25)) as f32;
                    let position = self
                        .input
                        .snapshot()
                        .pointer_position
                        .map(|logical| {
                            winit::dpi::Position::Physical(winit::dpi::PhysicalPosition::new(
                                (logical[0] * scale) as i32,
                                (logical[1] * scale) as i32,
                            ))
                        })
                        .unwrap_or_else(|| {
                            let size = window.inner_size();
                            winit::dpi::Position::Physical(winit::dpi::PhysicalPosition::new(
                                (size.width / 2) as i32,
                                (size.height / 2) as i32,
                            ))
                        });
                    window.show_window_menu(position);
                }
            }
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => tracing::warn!(%error, ?command, "native window command failed"),
        }
    }

    /// A press that lands on an interactive retained control must reach the
    /// surface instead of being treated as a window resize: the 8px native
    /// edge band covers the upper half of the title-bar controls, the drag
    /// regions, and the menu triggers.
    pub(super) fn retained_hover_blocks_window_resize(&self) -> bool {
        self.workbench
            .as_ref()
            .is_some_and(|workbench| workbench.has_interactive_hover())
            || self
                .studio
                .as_ref()
                .is_some_and(|studio| studio.has_interactive_hover())
            || self
                .loading
                .as_ref()
                .is_some_and(|loading| loading.has_interactive_hover())
            || self
                .exit_confirmation_surface
                .as_ref()
                .is_some_and(|host| host.has_interactive_hover())
            || self
                .settings_surface
                .as_ref()
                .is_some_and(|host| host.has_interactive_hover())
    }
}

