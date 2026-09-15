//! Direct retained UI host.
//!
//! The host owns retained UI state and the WGPU compositor but not a window.
//! A platform shell supplies the target texture view, allowing the editor to
//! keep UI documents and event logic independent from the native window host.

use std::sync::Arc;

use raf_core::{CaptureMode, InputOwner, InputRouter, PointerButton};

use super::compilation::{UiSurfaceCompilationCache, UiSurfaceCompileMetrics};
use super::{
    NativeUiInputBridge, UiDispatchedAction, UiInputState, UiSurface, UiSurfaceDiagnostics,
    UiSurfaceDrawList, UiSurfaceFrame, UiSurfaceGpuMetrics, UiSurfaceGpuRenderer,
    UiSurfaceGpuSharedResources, UiSurfaceImageStore, UiSurfaceSession,
};
use raf_ui::UiEnvironment;

use crate::api_graphic_basic::canvas_presenter::CanvasTargetRect;
use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;

pub struct DirectUiSurfaceHost {
    surface: UiSurface,
    surface_revision: u64,
    paint_revision: u64,
    session: UiSurfaceSession,
    compositor: UiSurfaceGpuRenderer,
    images: UiSurfaceImageStore,
    clear_color: [u8; 4],
    compilation: UiSurfaceCompilationCache,
    last_gpu_metrics: UiSurfaceGpuMetrics,
}

impl DirectUiSurfaceHost {
    pub fn new(
        surface: UiSurface,
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        clear_color: [u8; 4],
    ) -> Self {
        Self::with_shared(
            surface,
            Arc::new(UiSurfaceGpuSharedResources::new(device, color_format)),
            clear_color,
        )
    }

    pub(crate) fn with_shared(
        surface: UiSurface,
        shared: Arc<UiSurfaceGpuSharedResources>,
        clear_color: [u8; 4],
    ) -> Self {
        Self::with_shared_and_budget(
            surface,
            shared,
            clear_color,
            GraphicsMemoryBudget::default(),
        )
    }

    pub(crate) fn with_shared_and_budget(
        surface: UiSurface,
        shared: Arc<UiSurfaceGpuSharedResources>,
        clear_color: [u8; 4],
        memory_budget: GraphicsMemoryBudget,
    ) -> Self {
        Self {
            surface,
            surface_revision: 0,
            paint_revision: 0,
            session: UiSurfaceSession::default(),
            compositor: UiSurfaceGpuRenderer::with_shared_and_budget(shared, memory_budget),
            images: UiSurfaceImageStore::default(),
            clear_color,
            compilation: UiSurfaceCompilationCache::default(),
            last_gpu_metrics: UiSurfaceGpuMetrics::default(),
        }
    }

    pub fn surface(&self) -> &UiSurface {
        &self.surface
    }

    pub fn surface_mut(&mut self) -> &mut UiSurface {
        self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
        self.paint_revision = self.paint_revision.wrapping_add(1).max(1);
        &mut self.surface
    }

    pub fn set_surface(&mut self, surface: UiSurface) {
        if self.surface != surface {
            self.surface = surface;
            self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
            self.paint_revision = self.paint_revision.wrapping_add(1).max(1);
        }
    }

    /// Updates a fixed-size text node without invalidating the retained
    /// geometry. Hosts use this for diagnostics and other dynamic labels whose
    /// content cannot change the layout contract.
    pub fn patch_text_value(&mut self, id: &str, value: impl Into<String>) -> bool {
        self.patch_text_value_with_layout(id, value, false)
    }

    pub fn patch_text_value_with_layout(
        &mut self,
        id: &str,
        value: impl Into<String>,
        layout_affects: bool,
    ) -> bool {
        let value = value.into();
        let changed = {
            let Some(node) = self.surface.root.find_mut(id) else {
                return false;
            };
            if node.text_value.as_deref() == Some(value.as_str()) {
                false
            } else {
                node.text_value = Some(value.clone());
                true
            }
        };
        if !changed {
            return false;
        }
        if layout_affects {
            self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
        } else {
            self.compilation.patch_text_value(id, &value);
        }
        self.paint_revision = self.paint_revision.wrapping_add(1).max(1);
        true
    }

    pub fn session(&self) -> &UiSurfaceSession {
        &self.session
    }

    /// Mutable access to transient focus, text, and scroll state. The retained
    /// document stays declarative while platform hosts can restore short-lived
    /// values after a renderer or surface transition.
    pub fn session_mut(&mut self) -> &mut UiSurfaceSession {
        &mut self.session
    }

    pub fn layout_rect(&self, id: &str) -> Option<raf_ui::UiRect> {
        self.compilation.layout_rect(id)
    }

    /// Returns the latest logical rect for the focused text input. The
    /// platform shell uses it to place native IME candidate windows next to
    /// the caret without making RafUI depend on Winit.
    pub fn focused_text_rect(&self) -> Option<raf_ui::UiRect> {
        let id = self.session.interaction.focus.focused.as_deref()?;
        self.surface.root.find(id)?.control.text_input()?;
        self.layout_rect(id)
    }

    pub fn has_pointer_capture(&self) -> bool {
        self.session.interaction.has_pointer_capture()
    }

    /// True when the latest retained hit-test found an interactive control.
    /// Native hosts use this to shield secondary/middle camera gestures over
    /// toolbars without treating passive hover over transparent chrome as a
    /// capture.
    pub fn has_interactive_hover(&self) -> bool {
        self.session.interaction.focus.hovered.is_some()
    }

    pub fn has_active_motion(&self) -> bool {
        self.surface.retained_tooltips && self.session.has_active_motion()
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.session.has_active_text_repeat()
    }

    pub fn captures_keyboard_input(&self) -> bool {
        self.session.captures_keyboard_input(&self.surface)
    }

    /// Returns the semantic cursor requested by the currently hovered
    /// retained node. Native shells translate this into their platform cursor
    /// without making RafUI depend on Winit.
    pub fn cursor_hint(&self) -> raf_ui::UiCursorIcon {
        self.session.cursor_hint(&self.surface)
    }

    pub fn set_environment(&mut self, environment: UiEnvironment) {
        self.session.set_environment(environment);
    }

    pub fn images(&self) -> &UiSurfaceImageStore {
        &self.images
    }

    pub fn images_mut(&mut self) -> &mut UiSurfaceImageStore {
        &mut self.images
    }

    pub fn compilation_metrics(&self) -> UiSurfaceCompileMetrics {
        self.compilation.metrics()
    }

    pub fn last_gpu_metrics(&self) -> UiSurfaceGpuMetrics {
        self.last_gpu_metrics
    }

    pub fn build_frame<F>(&mut self, size: [u32; 2], resolve: F) -> UiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        self.compilation.invalidate();
        self.session.build_frame_with_resolved_text(
            &self.surface,
            size[0].max(1),
            size[1].max(1),
            self.clear_color,
            resolve,
        )
    }

    pub fn process_input<F>(
        &mut self,
        size: [u32; 2],
        resolve: F,
        input: &UiInputState,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        self.process_input_at_scale(size, 1.0, resolve, input)
    }

    /// Processes pointer input in logical surface points while keeping the
    /// text atlas at the physical density of the current presentation target.
    pub fn process_input_at_scale<F>(
        &mut self,
        size: [u32; 2],
        raster_scale: f32,
        _resolve: F,
        input: &UiInputState,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        let frame = self.compilation.layout(
            &self.surface,
            &mut self.session,
            self.surface_revision,
            size,
            raster_scale,
            self.clear_color,
        );
        self.session.process_input(&self.surface, &frame, input)
    }

    /// Routes one native snapshot through the shared ownership arbiter before
    /// RafUI sees it. A passive hover never mutates `InputRouter`; primary
    /// capture begins only after the retained surface accepted a real press.
    pub fn process_routed_input<F>(
        &mut self,
        size: [u32; 2],
        raster_scale: f32,
        resolve: F,
        native_input: &NativeUiInputBridge,
        router: &mut InputRouter,
        owner: InputOwner,
        window_rect: raf_ui::UiRect,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        let input = native_input.ui_state_for_owner(window_rect, owner, router);
        let actions = self.process_input_at_scale(size, raster_scale, resolve, &input);
        for action in &actions {
            if let raf_ui::UiAction::SetClipboard { text } = &action.action {
                native_input.write_clipboard(text);
            }
        }
        let snapshot = native_input.snapshot();

        if self.has_pointer_capture() && snapshot.button_down(PointerButton::Primary) {
            let origin = snapshot.pointer_position.unwrap_or([
                window_rect.x + window_rect.width * 0.5,
                window_rect.y + window_rect.height * 0.5,
            ]);
            let mode = if matches!(owner, InputOwner::Modal(_) | InputOwner::DragDrop(_)) {
                CaptureMode::Exclusive
            } else {
                CaptureMode::PerButton
            };
            let _ = router.try_capture_pointer(
                PointerButton::Primary,
                owner,
                mode,
                origin,
                snapshot.time_seconds,
            );
        }

        if self.has_interactive_hover() {
            for button in [PointerButton::Secondary, PointerButton::Middle] {
                if snapshot.button_pressed(button) {
                    let _ = router.try_capture_pointer(
                        button,
                        owner,
                        CaptureMode::PerButton,
                        snapshot
                            .pointer_position
                            .unwrap_or([window_rect.x, window_rect.y]),
                        snapshot.time_seconds,
                    );
                }
            }
        }

        if self.captures_keyboard_input() {
            let _ = router.try_capture_keyboard(owner);
        } else {
            let _ = router.release_keyboard(owner);
        }
        actions
    }

    pub fn render<F>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        resolve: F,
    ) -> DirectUiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        self.render_at_scale(device, queue, target, size, size, 1.0, resolve)
    }

    /// Renders a logical retained layout into a potentially denser physical
    /// texture. This removes the post-render upscale blur on HiDPI displays
    /// without changing pointer coordinates or document layout values.
    pub fn render_at_scale<F>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        logical_size: [u32; 2],
        raster_scale: f32,
        mut resolve: F,
    ) -> DirectUiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        let compiled = self.compilation.compile_with_paint_revision(
            &self.surface,
            &mut self.session,
            self.surface_revision,
            logical_size,
            raster_scale,
            self.clear_color,
            self.paint_revision,
            |key| resolve(key),
        );
        for quad in &compiled.draw_list.images {
            self.images.ensure_builtin_key(&quad.source_key);
        }
        let metrics = self.compositor.render_at_scale_with_revision(
            device,
            queue,
            target,
            target_size,
            logical_size,
            compiled.draw_list_revision,
            &compiled.draw_list,
            &mut self.session.text_atlas,
            &self.images,
            self.clear_color,
        );
        self.last_gpu_metrics = metrics;
        DirectUiSurfaceFrame {
            frame: compiled.frame,
            draw_list: compiled.draw_list,
            metrics,
            compilation: self.compilation.metrics(),
        }
    }

    pub(crate) fn encode_in_rect<F>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        target_rect: CanvasTargetRect,
        logical_size: [u32; 2],
        raster_scale: f32,
        load: wgpu::LoadOp<wgpu::Color>,
        mut resolve: F,
    ) -> DirectUiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        let compiled = self.compilation.compile_with_paint_revision(
            &self.surface,
            &mut self.session,
            self.surface_revision,
            logical_size,
            raster_scale,
            self.clear_color,
            self.paint_revision,
            |key| resolve(key),
        );
        for quad in &compiled.draw_list.images {
            self.images.ensure_builtin_key(&quad.source_key);
        }
        let metrics = self.compositor.encode_in_rect_with_revision(
            device,
            queue,
            encoder,
            target,
            target_size,
            target_rect,
            logical_size,
            compiled.draw_list_revision,
            &compiled.draw_list,
            &mut self.session.text_atlas,
            &self.images,
            load,
        );
        self.last_gpu_metrics = metrics;
        DirectUiSurfaceFrame {
            frame: compiled.frame,
            draw_list: compiled.draw_list,
            metrics,
            compilation: self.compilation.metrics(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectUiSurfaceFrame {
    pub frame: Arc<UiSurfaceFrame>,
    pub draw_list: Arc<UiSurfaceDrawList>,
    pub metrics: UiSurfaceGpuMetrics,
    pub compilation: UiSurfaceCompileMetrics,
}

impl DirectUiSurfaceFrame {
    pub fn diagnostics(&self) -> UiSurfaceDiagnostics {
        UiSurfaceDiagnostics::from_frame(&self.frame)
    }
}
