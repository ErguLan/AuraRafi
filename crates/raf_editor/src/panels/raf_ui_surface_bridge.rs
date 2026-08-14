//! Small eframe placement bridge for one retained RafUI surface.
//!
//! ApiGraphicBasic owns document composition, input, text, and GPU/CPU output.
//! Eframe only supplies the temporary window loop and paints the resulting
//! texture while the native window host is introduced incrementally.

use std::sync::Arc;

use eframe::{egui, egui_wgpu, wgpu};
use raf_render::api_graphic_basic::device::{GpuTextureView, SceneFrameOutput};
use raf_render::api_graphic_basic::ui_surface::{
    CpuUiSurfaceHost, DirectUiSurfaceHost, StudioUiPalette, UiDispatchedAction, UiInputState,
    UiPointerButton, UiSurface, UiSurfaceImageStore,
};
use raf_ui::{
    UiCursorIcon, UiEnvironment, UiEventKind, UiMotionSpec, UiOverlayLayer, UiOverlayManager,
    UiOverlayRequest, UiPlacement, UiTween, KEYBOARD_CAPTURE_TEMP_ID,
};

use super::gpu_canvas::GpuCanvas;
use super::raf_ui_tooltip;

struct GpuSurface {
    host: DirectUiSurfaceHost,
    texture: wgpu::Texture,
    view: Arc<wgpu::TextureView>,
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

const HOVER_SAMPLE_DISTANCE_PX: f32 = 4.0;
const TOOLTIP_HOVER_DELAY_SECONDS: f32 = 0.32;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TooltipRenderKey {
    backend_is_gpu: bool,
    text_key: String,
    opacity_bits: u32,
    logical_size: [u32; 2],
    target_size: [u32; 2],
    palette: StudioUiPalette,
}

impl GpuSurface {
    fn new(
        surface: UiSurface,
        render_state: &egui_wgpu::RenderState,
        size: [u32; 2],
        clear_color: [u8; 4],
    ) -> Self {
        let format = render_state.target_format;
        let (texture, view) = create_target_texture(render_state.device.as_ref(), format, size);
        Self {
            host: DirectUiSurfaceHost::new(
                surface,
                render_state.device.as_ref(),
                format,
                clear_color,
            ),
            texture,
            view,
            size,
            format,
        }
    }

    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.size == size {
            return;
        }
        let (texture, view) = create_target_texture(device, self.format, size);
        self.texture = texture;
        self.view = view;
        self.size = size;
    }
}

/// Reusable temporary presentation bridge for a retained surface.
///
/// It deliberately does not know what a surface means. Hosts own their typed
/// model and translate emitted `UiAction`s at the application boundary.
pub struct RafUiSurfaceBridge {
    canvas: GpuCanvas,
    tooltip_canvas: GpuCanvas,
    gpu: Option<GpuSurface>,
    cpu: Option<CpuUiSurfaceHost>,
    tooltip_gpu: Option<GpuSurface>,
    tooltip_cpu: Option<CpuUiSurfaceHost>,
    surface: Option<UiSurface>,
    surface_revision: Option<u64>,
    palette: Option<StudioUiPalette>,
    images: UiSurfaceImageStore,
    last_logical_size: Option<[u32; 2]>,
    last_rendered_logical_size: Option<[u32; 2]>,
    last_rendered_target_size: Option<[u32; 2]>,
    cpu_pixels: Option<(Vec<u8>, [u32; 2])>,
    last_input: UiInputState,
    active_renderer_is_gpu: Option<bool>,
    reduced_motion: bool,
    tooltip_motion: UiTween,
    last_tooltip_time_seconds: f64,
    tooltip_render_key: Option<TooltipRenderKey>,
    overlay_manager: UiOverlayManager,
    diag_frames: u64,
    diag_renders: u64,
    diag_last_log_seconds: f64,
    diag_process_ms_total: f64,
    diag_render_ms_total: f64,
    diag_present_ms_total: f64,
    diag_layout_builds: u64,
    diag_paint_builds: u64,
    diag_layout_hit: bool,
    diag_paint_hit: bool,
    diag_geom_hit: bool,
    diag_draw_calls: u32,
    diag_text_vertices: u32,
    diag_solid_vertices: u32,
    diag_atlas_uploads: u32,
}

impl RafUiSurfaceBridge {
    pub fn new(texture_name: impl Into<String>) -> Self {
        let texture_name = texture_name.into();
        Self {
            canvas: GpuCanvas::new(texture_name.clone()).with_retained_ui_sampling(),
            tooltip_canvas: GpuCanvas::new(format!("{texture_name}.tooltip"))
                .with_retained_ui_sampling(),
            gpu: None,
            cpu: None,
            tooltip_gpu: None,
            tooltip_cpu: None,
            surface: None,
            surface_revision: None,
            palette: None,
            images: UiSurfaceImageStore::default(),
            last_logical_size: None,
            last_rendered_logical_size: None,
            last_rendered_target_size: None,
            cpu_pixels: None,
            last_input: UiInputState::default(),
            active_renderer_is_gpu: None,
            reduced_motion: false,
            tooltip_motion: UiTween::new(0.0, UiMotionSpec::tooltip()),
            last_tooltip_time_seconds: 0.0,
            tooltip_render_key: None,
            overlay_manager: UiOverlayManager::default(),
            diag_frames: 0,
            diag_renders: 0,
            diag_last_log_seconds: 0.0,
            diag_process_ms_total: 0.0,
            diag_render_ms_total: 0.0,
            diag_present_ms_total: 0.0,
            diag_layout_builds: 0,
            diag_paint_builds: 0,
            diag_layout_hit: false,
            diag_paint_hit: false,
            diag_geom_hit: false,
            diag_draw_calls: 0,
            diag_text_vertices: 0,
            diag_solid_vertices: 0,
            diag_atlas_uploads: 0,
        }
    }

    /// Registers a small embedded raster asset for any retained surface that
    /// uses image nodes. The bridge keeps one store so GPU and CPU hosts see
    /// the same image set after a renderer transition.
    pub fn register_embedded_png(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        let decoded = image::load_from_memory(bytes)
            .map_err(|error| format!("Unable to decode retained UI image '{key}': {error}"))?
            .to_rgba8();
        self.images.insert_rgba(
            key.to_string(),
            [decoded.width(), decoded.height()],
            decoded.into_raw(),
        )?;
        self.sync_images();
        Ok(())
    }

    pub fn show<F>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: UiSurface,
        resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        self.show_with_control_state(ui, render_state, palette, surface, |_| {}, resolve_text)
    }

    /// Presents a retained surface into a transparent texture. This is used
    /// only by transient, borderless surfaces such as the startup splash;
    /// normal editor surfaces keep their palette-owned opaque clear color.
    pub fn show_transparent<F>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: UiSurface,
        resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        self.show_with_control_state_and_clear_color(
            ui,
            render_state,
            palette,
            surface,
            [0, 0, 0, 0],
            |_| {},
            resolve_text,
        )
    }

    /// Presents a borrowed transparent surface. Hosts use this for animated
    /// overlays so a retained document is not cloned on every motion frame.
    pub fn show_transparent_ref<F>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: &UiSurface,
        resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        self.show_with_control_state_ref_and_clear_color(
            ui,
            render_state,
            palette,
            surface,
            None,
            [0, 0, 0, 0],
            |_| {},
            resolve_text,
        )
    }

    pub fn show_transparent_ref_revision<F>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: &UiSurface,
        surface_revision: u64,
        resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        self.show_with_control_state_ref_and_clear_color(
            ui,
            render_state,
            palette,
            surface,
            Some(surface_revision),
            [0, 0, 0, 0],
            |_| {},
            resolve_text,
        )
    }

    /// Presents a retained surface after its owning panel has seeded transient
    /// control values. This avoids a first-frame empty text field without
    /// making text state part of a serializable UI document.
    pub fn show_with_control_state<F, S>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: UiSurface,
        seed_control_state: S,
        resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
        S: FnMut(&mut raf_ui::UiControlState),
    {
        self.show_with_control_state_and_clear_color(
            ui,
            render_state,
            palette,
            surface,
            clear_color(palette),
            seed_control_state,
            resolve_text,
        )
    }

    /// Presents a borrowed retained surface. Hosts that already retain an
    /// unchanged document can use this path to avoid cloning the whole node
    /// tree before every input pass.
    pub fn show_with_control_state_ref<F, S>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: &UiSurface,
        seed_control_state: S,
        resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
        S: FnMut(&mut raf_ui::UiControlState),
    {
        self.show_with_control_state_ref_and_clear_color(
            ui,
            render_state,
            palette,
            surface,
            None,
            clear_color(palette),
            seed_control_state,
            resolve_text,
        )
    }

    /// Presents a borrowed retained surface with a caller-owned revision.
    ///
    /// Large editor surfaces already retain their document and know exactly
    /// when that document changed. Supplying that revision avoids comparing
    /// the entire UiNode tree on every engine frame.
    pub fn show_with_control_state_ref_revision<F, S>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: &UiSurface,
        surface_revision: u64,
        seed_control_state: S,
        resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
        S: FnMut(&mut raf_ui::UiControlState),
    {
        self.show_with_control_state_ref_and_clear_color(
            ui,
            render_state,
            palette,
            surface,
            Some(surface_revision),
            clear_color(palette),
            seed_control_state,
            resolve_text,
        )
    }

    fn show_with_control_state_and_clear_color<F, S>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: UiSurface,
        clear_color: [u8; 4],
        seed_control_state: S,
        resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
        S: FnMut(&mut raf_ui::UiControlState),
    {
        let surface = surface.with_retained_tooltips(false);
        self.show_with_control_state_ref_and_clear_color(
            ui,
            render_state,
            palette,
            &surface,
            None,
            clear_color,
            seed_control_state,
            resolve_text,
        )
    }

    fn show_with_control_state_ref_and_clear_color<F, S>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: &UiSurface,
        surface_revision: Option<u64>,
        clear_color: [u8; 4],
        mut seed_control_state: S,
        mut resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
        S: FnMut(&mut raf_ui::UiControlState),
    {
        let rect = ui.available_rect_before_wrap();
        let _response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let logical_size = [
            rect.width().round().max(1.0) as u32,
            rect.height().round().max(1.0) as u32,
        ];
        let pixels_per_point = ui.ctx().pixels_per_point().clamp(0.5, 4.0);
        // Geometry is allocated exactly at the presentation density. Text has
        // its own modest source-density floor, so 12-13px labels resolve
        // cleanly without supersampling panel edges or icon geometry.
        let presentation = UiEnvironment {
            viewport_size: [logical_size[0] as f32, logical_size[1] as f32],
            scale_factor: pixels_per_point,
            prefers_reduced_motion: self.reduced_motion,
            ..UiEnvironment::default()
        };
        let raster_scale = presentation.text_raster_scale();
        let target_size = presentation.physical_size();
        let surface_changed = self.sync_surface(
            render_state,
            palette,
            surface,
            surface_revision,
            target_size,
            clear_color,
        );
        self.sync_motion_environment();
        if surface_changed {
            self.cpu_pixels = None;
        }
        self.reset_text_atlas_when_resized(logical_size);
        self.with_control_state(|controls| seed_control_state(controls));

        let retained_pointer_capture = if render_state.is_some() {
            self.gpu
                .as_ref()
                .is_some_and(|gpu| gpu.host.session().interaction.has_pointer_capture())
        } else {
            self.cpu
                .as_ref()
                .is_some_and(|cpu| cpu.session().interaction.has_pointer_capture())
        };
        let input = egui_input(ui.ctx(), rect, retained_pointer_capture);
        self.last_input = input.clone();
        let click_away_actions = self.click_away_actions(&input);
        let mut render_needed = surface_changed
            || self.last_rendered_logical_size != Some(logical_size)
            || self.last_rendered_target_size != Some(target_size);
        let (actions, hover_changed, cursor_hint) = if let (Some(render_state), Some(gpu)) =
            (render_state, self.gpu.as_mut())
        {
            gpu.resize(render_state.device.as_ref(), target_size);
            let pointer_capture = gpu.host.session().interaction.has_pointer_capture();
            let should_process = should_process_input(
                &input,
                surface_changed,
                pointer_capture,
                gpu.host.session().interaction.pointer_position(),
                gpu.host.session().interaction.focus.focused.is_some(),
                surface.retained_tooltips,
            );
            let hovered_before = gpu.host.session().interaction.focus.hovered.clone();
            let diag_process_start = should_process.then(std::time::Instant::now);
            let actions = if should_process {
                gpu.host.process_input_at_scale(
                    logical_size,
                    raster_scale,
                    |key| resolve_text(key),
                    &input,
                )
            } else {
                Vec::new()
            };
            let diag_process_ms = diag_process_start
                .map(|start| start.elapsed().as_secs_f64() * 1000.0)
                .unwrap_or(0.0);
            let hover_changed = hovered_before != gpu.host.session().interaction.focus.hovered;
            render_needed |= !actions.is_empty()
                || hover_changed
                || pointer_visual_state_changed(
                    &input,
                    pointer_capture,
                    gpu.host.session().interaction.focus.focused.is_some(),
                );
            let mut diag_render_ms = 0.0;
            if render_needed {
                let diag_render_start = std::time::Instant::now();
                let rendered = gpu.host.render_at_scale(
                    render_state.device.as_ref(),
                    render_state.queue.as_ref(),
                    gpu.view.as_ref(),
                    target_size,
                    logical_size,
                    raster_scale,
                    |key| resolve_text(key),
                );
                diag_render_ms = diag_render_start.elapsed().as_secs_f64() * 1000.0;
                self.diag_layout_builds = rendered.compilation.layout_builds;
                self.diag_paint_builds = rendered.compilation.paint_builds;
                self.diag_layout_hit = rendered.compilation.layout_cache_hit;
                self.diag_paint_hit = rendered.compilation.paint_cache_hit;
                self.diag_geom_hit = rendered.metrics.geometry_cache_hit;
                self.diag_draw_calls = rendered.metrics.draw_calls;
                self.diag_text_vertices = rendered.metrics.text_vertices;
                self.diag_solid_vertices = rendered.metrics.solid_vertices;
                self.diag_atlas_uploads = rendered.metrics.atlas_uploads;
                // The GPU path must acknowledge the completed presentation;
                // otherwise the size gate above schedules a full render on
                // every idle frame.
                self.last_rendered_logical_size = Some(logical_size);
                self.last_rendered_target_size = Some(target_size);
            }
            let mut diag_present_ms = 0.0;
            if render_needed || !self.canvas.is_ready() {
                let diag_present_start = std::time::Instant::now();
                self.canvas.present(
                    ui.ctx(),
                    Some(render_state),
                    SceneFrameOutput::GpuTexture {
                        view: GpuTextureView::from_wgpu(
                            gpu.view.clone(),
                            raf_render::api_graphic_basic::TextureHandle::new(0, 1),
                        ),
                        width: target_size[0],
                        height: target_size[1],
                    },
                    target_size[0],
                    target_size[1],
                );
                diag_present_ms = diag_present_start.elapsed().as_secs_f64() * 1000.0;
            }
            self.diag_process_ms_total += diag_process_ms;
            self.diag_render_ms_total += diag_render_ms;
            self.diag_present_ms_total += diag_present_ms;
            (
                actions,
                hover_changed,
                gpu.host
                    .session()
                    .cursor_hint(self.surface.as_ref().expect("retained surface synced")),
            )
        } else {
            let cpu = self
                .cpu
                .as_mut()
                .expect("CPU RafUI bridge must be prepared");
            let pointer_capture = cpu.session().interaction.has_pointer_capture();
            let should_process = should_process_input(
                &input,
                surface_changed,
                pointer_capture,
                cpu.session().interaction.pointer_position(),
                cpu.session().interaction.focus.focused.is_some(),
                surface.retained_tooltips,
            );
            let hovered_before = cpu.session().interaction.focus.hovered.clone();
            let actions = if should_process {
                cpu.process_input_at_scale(
                    logical_size,
                    raster_scale,
                    |key| resolve_text(key),
                    &input,
                )
            } else {
                Vec::new()
            };
            let hover_changed = hovered_before != cpu.session().interaction.focus.hovered;
            render_needed |= !actions.is_empty()
                || hover_changed
                || pointer_visual_state_changed(
                    &input,
                    pointer_capture,
                    cpu.session().interaction.focus.focused.is_some(),
                );
            if render_needed {
                let frame = cpu.render_at_scale(target_size, logical_size, raster_scale, |key| {
                    resolve_text(key)
                });
                self.cpu_pixels = Some((frame.pixels.to_vec(), frame.size));
                self.last_rendered_logical_size = Some(logical_size);
                self.last_rendered_target_size = Some(target_size);
            }
            if render_needed || !self.canvas.is_ready() {
                let (pixels, size) = self
                    .cpu_pixels
                    .as_ref()
                    .map(|(pixels, size)| (pixels.clone(), *size))
                    .unwrap_or_default();
                self.canvas.present(
                    ui.ctx(),
                    None,
                    SceneFrameOutput::CpuPixels(pixels),
                    size[0],
                    size[1],
                );
            }
            (
                actions,
                hover_changed,
                cpu.session()
                    .cursor_hint(self.surface.as_ref().expect("retained surface synced")),
            )
        };
        let keyboard_capture =
            self.surface
                .as_ref()
                .is_some_and(|surface| match self.active_renderer_is_gpu {
                    Some(true) => self
                        .gpu
                        .as_ref()
                        .is_some_and(|gpu| gpu.host.session().captures_keyboard_input(surface)),
                    Some(false) => self
                        .cpu
                        .as_ref()
                        .is_some_and(|cpu| cpu.session().captures_keyboard_input(surface)),
                    None => false,
                });
        let mut actions = actions;
        actions.splice(0..0, click_away_actions);
        ui.ctx().data_mut(|data| {
            let already_captured = data
                .get_temp::<bool>(egui::Id::new(KEYBOARD_CAPTURE_TEMP_ID))
                .unwrap_or(false);
            data.insert_temp(
                egui::Id::new(KEYBOARD_CAPTURE_TEMP_ID),
                already_captured || keyboard_capture,
            );
        });
        if self
            .surface
            .as_ref()
            .is_some_and(|surface| surface.id == "agent.workbench")
        {
            self.diag_frames += 1;
            if render_needed {
                self.diag_renders += 1;
            }
            let diag_now = ui.ctx().input(|input| input.time);
            if diag_now - self.diag_last_log_seconds >= 1.0 {
                tracing::info!(
                    "DIAG[bridge-agent] frames={} renders={} surf_ch={} hover_ch={} actions={} proc_ms={:.2} render_ms={:.2} present_ms={:.2} layout_hit={} paint_hit={} geom_hit={} lb={} pb={} draws={} text_v={} solid_v={} atlas_up={}",
                    self.diag_frames,
                    self.diag_renders,
                    surface_changed,
                    hover_changed,
                    actions.len(),
                    self.diag_process_ms_total / self.diag_frames.max(1) as f64,
                    self.diag_render_ms_total / self.diag_frames.max(1) as f64,
                    self.diag_present_ms_total / self.diag_frames.max(1) as f64,
                    self.diag_layout_hit,
                    self.diag_paint_hit,
                    self.diag_geom_hit,
                    self.diag_layout_builds,
                    self.diag_paint_builds,
                    self.diag_draw_calls,
                    self.diag_text_vertices,
                    self.diag_solid_vertices,
                    self.diag_atlas_uploads,
                );
                self.diag_frames = 0;
                self.diag_renders = 0;
                self.diag_last_log_seconds = diag_now;
                self.diag_process_ms_total = 0.0;
                self.diag_render_ms_total = 0.0;
                self.diag_present_ms_total = 0.0;
            }
        }
        ui.output_mut(|output| {
            output.cursor_icon = match cursor_hint {
                UiCursorIcon::Default => egui::CursorIcon::Default,
                UiCursorIcon::PointingHand => egui::CursorIcon::PointingHand,
                UiCursorIcon::Text => egui::CursorIcon::Text,
                UiCursorIcon::ResizeHorizontal => egui::CursorIcon::ResizeHorizontal,
                UiCursorIcon::ResizeVertical => egui::CursorIcon::ResizeVertical,
                UiCursorIcon::ResizeNorthEastSouthWest => egui::CursorIcon::ResizeNeSw,
                UiCursorIcon::ResizeNorthWestSouthEast => egui::CursorIcon::ResizeNwSe,
            };
        });
        self.canvas.paint(&ui.painter_at(rect), rect);
        self.render_retained_tooltip_overlay(
            ui.ctx(),
            render_state,
            palette,
            rect,
            input.pointer_position,
            &mut resolve_text,
        );

        if !actions.is_empty() || hover_changed {
            ui.ctx().request_repaint();
        }
        actions
    }

    fn current_tooltip_key(&self) -> Option<String> {
        let surface = self.surface.as_ref()?;
        match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.host.session().hovered_tooltip_key(surface)),
            Some(false) => self
                .cpu
                .as_ref()
                .and_then(|cpu| cpu.session().hovered_tooltip_key(surface)),
            None => None,
        }
    }

    fn current_tooltip_hover_progress(&self, now_seconds: f64) -> f32 {
        match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .map(|gpu| {
                    gpu.host
                        .session()
                        .interaction
                        .hover_intent_progress(now_seconds, TOOLTIP_HOVER_DELAY_SECONDS)
                })
                .unwrap_or(0.0),
            Some(false) => self
                .cpu
                .as_ref()
                .map(|cpu| {
                    cpu.session()
                        .interaction
                        .hover_intent_progress(now_seconds, TOOLTIP_HOVER_DELAY_SECONDS)
                })
                .unwrap_or(0.0),
            None => 0.0,
        }
    }

    fn hide_retained_tooltip(&mut self) {
        self.overlay_manager.remove("rafui.tooltip.overlay");
        self.tooltip_motion.set_immediate(0.0);
        self.tooltip_render_key = None;
    }

    /// Turns an outside press into the focused overlay's existing Escape
    /// action. This keeps click-away semantic and renderer-neutral: the panel
    /// still owns the command, while the bridge owns window/input boundaries.
    fn click_away_actions(&self, input: &UiInputState) -> Vec<UiDispatchedAction> {
        if !input.button_pressed(UiPointerButton::Primary) && !input.pointer_pressed_outside {
            return Vec::new();
        }
        let focused_id = match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.host.session().interaction.focus.focused.as_deref()),
            Some(false) => self
                .cpu
                .as_ref()
                .and_then(|cpu| cpu.session().interaction.focus.focused.as_deref()),
            None => None,
        };
        let Some(focused_id) = focused_id else {
            return Vec::new();
        };
        let pointer_capture = match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .is_some_and(|gpu| gpu.host.session().interaction.has_pointer_capture()),
            Some(false) => self
                .cpu
                .as_ref()
                .is_some_and(|cpu| cpu.session().interaction.has_pointer_capture()),
            None => false,
        };
        if pointer_capture {
            return Vec::new();
        }
        let inside_focused_node = input.pointer_position.is_some_and(|point| {
            self.layout_rect(focused_id)
                .is_some_and(|rect| rect.contains(point))
        });
        if inside_focused_node {
            return Vec::new();
        }
        let Some(surface) = self.surface.as_ref() else {
            return Vec::new();
        };
        let Some(node) = find_node(&surface.root, focused_id) else {
            return Vec::new();
        };
        node.event_handlers
            .iter()
            .filter_map(|binding| {
                matches!(
                    &binding.event,
                    UiEventKind::KeyPress(key) if key.eq_ignore_ascii_case("escape")
                )
                .then(|| UiDispatchedAction {
                    target_id: focused_id.to_string(),
                    event: UiEventKind::KeyPress("escape".to_string()),
                    action: binding.action.clone(),
                })
            })
            .collect()
    }

    fn render_retained_tooltip_overlay<F>(
        &mut self,
        ctx: &egui::Context,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface_rect: egui::Rect,
        pointer_position: Option<[f32; 2]>,
        resolve_text: &mut F,
    ) where
        F: FnMut(&str) -> String,
    {
        let now_seconds = ctx.input(|input| input.time);
        let delta_seconds = (now_seconds - self.last_tooltip_time_seconds)
            .max(0.0)
            .min(0.25) as f32;
        self.last_tooltip_time_seconds = now_seconds;
        let Some(text) = self.current_tooltip_text(resolve_text) else {
            self.hide_retained_tooltip();
            return;
        };
        let Some(pointer_position) = pointer_position else {
            self.hide_retained_tooltip();
            return;
        };
        let hover_progress = self.current_tooltip_hover_progress(now_seconds);
        let tooltip_ready = self.reduced_motion || hover_progress >= 1.0;
        self.tooltip_motion.set_target(f32::from(tooltip_ready));
        if !tooltip_ready {
            self.hide_retained_tooltip();
            if !self.reduced_motion {
                let remaining = (TOOLTIP_HOVER_DELAY_SECONDS * (1.0 - hover_progress)).max(0.001);
                ctx.request_repaint_after(std::time::Duration::from_secs_f32(remaining));
            }
            return;
        }
        let opacity = self
            .tooltip_motion
            .advance(delta_seconds, self.reduced_motion);
        if opacity <= 0.001 {
            self.overlay_manager.remove("rafui.tooltip.overlay");
            self.tooltip_render_key = None;
            if !self.tooltip_motion.is_settled() {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            return;
        }
        let logical_size = raf_ui_tooltip::logical_size_for_text(&text);
        let pixels_per_point = ctx.pixels_per_point().clamp(0.5, 4.0);
        let presentation = UiEnvironment {
            viewport_size: [logical_size[0] as f32, logical_size[1] as f32],
            scale_factor: pixels_per_point,
            prefers_reduced_motion: self.reduced_motion,
            ..UiEnvironment::default()
        };
        let raster_scale = presentation.text_raster_scale();
        let target_size = presentation.physical_size();
        let render_key = TooltipRenderKey {
            backend_is_gpu: render_state.is_some(),
            text_key: text.clone(),
            opacity_bits: opacity.to_bits(),
            logical_size,
            target_size,
            palette,
        };
        let render_key_changed = self.tooltip_render_key.as_ref() != Some(&render_key);

        if let Some(render_state) = render_state {
            let recreate = self
                .tooltip_gpu
                .as_ref()
                .map(|gpu| gpu.format != render_state.target_format)
                .unwrap_or(true);
            let needs_render = recreate || render_key_changed || !self.tooltip_canvas.is_ready();
            if needs_render {
                let tooltip_surface = raf_ui_tooltip::build_surface(palette, &text, opacity);
                if recreate {
                    self.tooltip_gpu = Some(GpuSurface::new(
                        tooltip_surface.clone(),
                        render_state,
                        target_size,
                        [0, 0, 0, 0],
                    ));
                } else {
                    let gpu = self
                        .tooltip_gpu
                        .as_mut()
                        .expect("retained tooltip GPU surface must exist");
                    gpu.host.set_surface(tooltip_surface);
                    gpu.resize(render_state.device.as_ref(), target_size);
                }
                let gpu = self
                    .tooltip_gpu
                    .as_mut()
                    .expect("retained tooltip GPU surface must exist");
                gpu.host.set_environment(presentation);
                gpu.host.render_at_scale(
                    render_state.device.as_ref(),
                    render_state.queue.as_ref(),
                    gpu.view.as_ref(),
                    target_size,
                    logical_size,
                    raster_scale,
                    |key| resolve_text(key),
                );
                self.tooltip_canvas.present(
                    ctx,
                    Some(render_state),
                    SceneFrameOutput::GpuTexture {
                        view: GpuTextureView::from_wgpu(
                            gpu.view.clone(),
                            raf_render::api_graphic_basic::TextureHandle::new(0, 2),
                        ),
                        width: target_size[0],
                        height: target_size[1],
                    },
                    target_size[0],
                    target_size[1],
                );
                self.tooltip_render_key = Some(render_key);
            }
        } else {
            let needs_render =
                self.tooltip_cpu.is_none() || render_key_changed || !self.tooltip_canvas.is_ready();
            if needs_render {
                let tooltip_surface = raf_ui_tooltip::build_surface(palette, &text, opacity);
                if self.tooltip_cpu.is_none() {
                    self.tooltip_cpu =
                        Some(CpuUiSurfaceHost::new(tooltip_surface.clone(), [0, 0, 0, 0]));
                }
                let cpu = self
                    .tooltip_cpu
                    .as_mut()
                    .expect("retained tooltip CPU surface must exist");
                cpu.set_surface(tooltip_surface);
                cpu.session_mut().set_reduced_motion(self.reduced_motion);
                let frame = cpu.render_at_scale(target_size, logical_size, raster_scale, |key| {
                    resolve_text(key)
                });
                self.tooltip_canvas.present(
                    ctx,
                    None,
                    SceneFrameOutput::CpuPixels(frame.pixels.to_vec()),
                    frame.size[0],
                    frame.size[1],
                );
                self.tooltip_render_key = Some(render_key);
            }
        }

        let screen = ctx.screen_rect();
        let anchor = raf_ui::UiRect::new(
            surface_rect.min.x + pointer_position[0],
            surface_rect.min.y + pointer_position[1],
            1.0,
            12.0,
        );
        let viewport = raf_ui::UiRect::new(
            screen.left() + 4.0,
            screen.top() + 4.0,
            (screen.width() - 8.0).max(1.0),
            (screen.height() - 8.0).max(1.0),
        );
        self.overlay_manager.submit(
            UiOverlayRequest::new(
                "rafui.tooltip.overlay",
                UiOverlayLayer::Tooltip,
                anchor,
                [logical_size[0] as f32, logical_size[1] as f32],
                UiPlacement::BottomStart,
            )
            .with_gap(raf_ui_tooltip::TOOLTIP_GAP),
        );
        let placement = self
            .overlay_manager
            .placements(viewport)
            .into_iter()
            .find(|(id, _, _)| id == "rafui.tooltip.overlay")
            .map(|(_, _, placement)| placement)
            .expect("tooltip overlay placement");
        let overlay_rect = egui::Rect::from_min_size(
            egui::pos2(placement.rect.x, placement.rect.y),
            egui::vec2(placement.rect.width, placement.rect.height),
        );
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("rafui.retained.tooltip"),
        ));
        self.tooltip_canvas.paint(&painter, overlay_rect);
        if !self.tooltip_motion.is_settled() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn current_tooltip_value(&self) -> Option<String> {
        let surface = self.surface.as_ref()?;
        match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.host.session().hovered_tooltip_value(surface)),
            Some(false) => self
                .cpu
                .as_ref()
                .and_then(|cpu| cpu.session().hovered_tooltip_value(surface)),
            None => None,
        }
    }

    fn current_tooltip_text<F>(&self, resolve_text: &mut F) -> Option<String>
    where
        F: FnMut(&str) -> String,
    {
        self.current_tooltip_key()
            .map(|key| resolve_text(&key))
            .or_else(|| self.current_tooltip_value())
    }

    /// A retained surface can produce a different text request width while a
    /// dock is being resized. The text atlas keys that width, so retaining
    /// every intermediate variant eventually exhausts the bounded atlas and
    /// makes labels disappear after the dock is restored. Resize is a layout
    /// boundary: clear the transient atlas once, then rebuild the current
    /// frame for both presentation paths.
    fn reset_text_atlas_when_resized(&mut self, logical_size: [u32; 2]) {
        let logical_size = [logical_size[0].max(1), logical_size[1].max(1)];
        if self.last_logical_size == Some(logical_size) {
            return;
        }
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.host.session_mut().text_atlas.clear();
        }
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.session_mut().text_atlas.clear();
        }
        self.last_logical_size = Some(logical_size);
    }

    /// Applies the host environment to every retained presentation session.
    /// The bridge has no platform dependency, so callers can feed the same
    /// environment used by a native window or accessibility preferences.
    pub fn set_environment(&mut self, environment: UiEnvironment) {
        self.set_reduced_motion(environment.prefers_reduced_motion);
    }

    pub fn set_reduced_motion(&mut self, reduced_motion: bool) {
        if self.reduced_motion == reduced_motion {
            self.sync_motion_environment();
            return;
        }
        self.reduced_motion = reduced_motion;
        self.tooltip_render_key = None;
        self.last_rendered_logical_size = None;
        self.sync_motion_environment();
    }

    fn sync_motion_environment(&mut self) {
        let environment = UiEnvironment {
            prefers_reduced_motion: self.reduced_motion,
            ..UiEnvironment::default()
        };
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.host.set_environment(environment);
        }
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.session_mut()
                .set_reduced_motion(environment.prefers_reduced_motion);
        }
        if let Some(gpu) = self.tooltip_gpu.as_mut() {
            gpu.host.set_environment(environment);
        }
        if let Some(cpu) = self.tooltip_cpu.as_mut() {
            cpu.session_mut()
                .set_reduced_motion(environment.prefers_reduced_motion);
        }
    }

    /// Seeds or reads transient control state without coupling a retained
    /// document to a specific editor panel. Text fields stay owned by their
    /// panel model while the bridge owns the active GPU/CPU surface session.
    pub fn with_control_state(&mut self, mut apply: impl FnMut(&mut raf_ui::UiControlState)) {
        if let Some(gpu) = self.gpu.as_mut() {
            apply(&mut gpu.host.session_mut().interaction.controls);
        }
        if let Some(cpu) = self.cpu.as_mut() {
            apply(&mut cpu.session_mut().interaction.controls);
        }
    }

    pub fn scroll_by(&mut self, id: &str, delta: [f32; 2]) {
        self.with_control_state(|controls| {
            controls.scroll_by(id, delta);
        });
        self.invalidate_render();
    }

    /// Forces one presentation pass after a host mutates retained control
    /// state directly, such as middle-button canvas panning.
    pub fn invalidate_render(&mut self) {
        self.last_rendered_logical_size = None;
    }

    /// Reads transient control state without exposing the presentation host.
    ///
    /// Hosts use this for lightweight overlays that depend on a retained
    /// surface's scroll position while keeping the overlay in its own bridge.
    pub fn with_control_state_read<T>(
        &self,
        read: impl Fn(&raf_ui::UiControlState) -> T,
    ) -> Option<T> {
        match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .map(|gpu| read(&gpu.host.session().interaction.controls)),
            Some(false) => self
                .cpu
                .as_ref()
                .map(|cpu| read(&cpu.session().interaction.controls)),
            None => None,
        }
    }

    pub fn current_modifiers(&self) -> raf_ui::UiModifiers {
        match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .map(|gpu| gpu.host.session().interaction.modifiers())
                .unwrap_or_default(),
            Some(false) => self
                .cpu
                .as_ref()
                .map(|cpu| cpu.session().interaction.modifiers())
                .unwrap_or_default(),
            None => raf_ui::UiModifiers::default(),
        }
    }

    /// Returns the retained control that currently owns keyboard focus.
    /// Panel hosts use this to avoid overwriting a user's in-progress text
    /// while still refreshing inactive fields from their application model.
    pub fn focused_control_id(&self) -> Option<&str> {
        match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.host.session().interaction.focus.focused.as_deref()),
            Some(false) => self
                .cpu
                .as_ref()
                .and_then(|cpu| cpu.session().interaction.focus.focused.as_deref()),
            None => None,
        }
    }

    pub fn pointer_position(&self) -> Option<[f32; 2]> {
        match self.active_renderer_is_gpu {
            Some(true) => self
                .gpu
                .as_ref()
                .and_then(|gpu| gpu.host.session().interaction.pointer_position()),
            Some(false) => self
                .cpu
                .as_ref()
                .and_then(|cpu| cpu.session().interaction.pointer_position()),
            None => None,
        }
    }

    /// Returns the latest renderer-neutral input snapshot for a retained host.
    ///
    /// This is intentionally read-only. It lets specialized surfaces such as
    /// the Nodes canvas consume middle-button panning without reaching into
    /// Egui input state or duplicating the native input adapter.
    pub fn input_snapshot(&self) -> &UiInputState {
        &self.last_input
    }

    pub fn layout_rect(&self, id: &str) -> Option<raf_ui::UiRect> {
        match self.active_renderer_is_gpu {
            Some(true) => self.gpu.as_ref().and_then(|gpu| gpu.host.layout_rect(id)),
            Some(false) => self.cpu.as_ref().and_then(|cpu| cpu.layout_rect(id)),
            None => None,
        }
    }

    pub fn request_focus(&mut self, id: impl Into<String>) {
        let id = id.into();
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.host
                .session_mut()
                .interaction
                .focus
                .request_focus(id.clone());
        }
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.session_mut()
                .interaction
                .focus
                .request_focus(id.clone());
        }
        self.invalidate_render();
    }

    /// Clears transient navigation state after a retained Agent session or
    /// page is replaced. Text field values remain owned by the host.
    pub fn reset_surface_interaction(&mut self, scroll_id: Option<&str>) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.host
                .session_mut()
                .reset_interaction_for_surface_change(scroll_id);
        }
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.session_mut()
                .reset_interaction_for_surface_change(scroll_id);
        }
        self.hide_retained_tooltip();
        self.invalidate_render();
    }

    /// Drops a captured pointer gesture when the owner resets its layout.
    /// This keeps GPU and CPU presentation sessions from carrying a stale
    /// drag into a newly rebuilt retained surface.
    pub fn cancel_pointer_gesture(&mut self) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.host.session_mut().interaction.cancel_pointer_gesture();
        }
        if let Some(cpu) = self.cpu.as_mut() {
            cpu.session_mut().interaction.cancel_pointer_gesture();
        }
        self.invalidate_render();
    }

    fn sync_surface(
        &mut self,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: &UiSurface,
        surface_revision: Option<u64>,
        size: [u32; 2],
        clear_color: [u8; 4],
    ) -> bool {
        let renderer_is_gpu = render_state.is_some();
        let renderer_changed = self.active_renderer_is_gpu != Some(renderer_is_gpu);
        let previous_session = if renderer_changed {
            if renderer_is_gpu {
                self.cpu.as_ref().map(|cpu| cpu.session().clone())
            } else {
                self.gpu.as_ref().map(|gpu| gpu.host.session().clone())
            }
        } else {
            None
        };
        self.active_renderer_is_gpu = Some(renderer_is_gpu);
        // The bridge composes tooltips in the window layer. Compare document
        // content without the presentation-only tooltip flag so borrowed
        // callers do not clone their whole node tree on every frame.
        let surface_changed = if let Some(revision) = surface_revision {
            self.surface.is_none() || self.surface_revision != Some(revision)
        } else {
            self.surface
                .as_ref()
                .is_none_or(|current| surface_content_changed(current, surface))
        };
        let palette_changed = self.palette != Some(palette);
        let mut host_recreated = false;
        if surface_changed {
            self.surface = Some(surface.clone().with_retained_tooltips(false));
        }
        self.surface_revision = surface_revision;
        self.palette = Some(palette);

        if let Some(render_state) = render_state {
            let recreate = self
                .gpu
                .as_ref()
                .map(|gpu| gpu.format != render_state.target_format || palette_changed)
                .unwrap_or(true);
            if recreate {
                host_recreated = true;
                let mut gpu = GpuSurface::new(
                    self.surface
                        .as_ref()
                        .expect("retained surface must be stored before host sync")
                        .clone(),
                    render_state,
                    size,
                    clear_color,
                );
                *gpu.host.images_mut() = self.images.clone();
                if let Some(session) = previous_session.clone().or_else(|| {
                    self.gpu
                        .as_ref()
                        .map(|current| current.host.session().clone())
                }) {
                    *gpu.host.session_mut() = session;
                }
                self.gpu = Some(gpu);
            } else {
                let gpu = self.gpu.as_mut().expect("GPU RafUI bridge must exist");
                if let Some(session) = previous_session.clone() {
                    *gpu.host.session_mut() = session;
                }
                if surface_changed {
                    gpu.host.set_surface(
                        self.surface
                            .as_ref()
                            .expect("retained surface must be stored before host sync")
                            .clone(),
                    );
                }
                // A document update does not invalidate already-rasterized
                // text. Keeping the atlas here is important for streaming
                // Agent messages and host-side motion: clearing it forces all
                // visible labels through a blank upload/rebuild window.
            }
        }

        if render_state.is_none() {
            if self.cpu.is_none() || palette_changed {
                host_recreated = true;
                let mut cpu = CpuUiSurfaceHost::new(
                    self.surface
                        .as_ref()
                        .expect("retained surface must be stored before host sync")
                        .clone(),
                    clear_color,
                );
                *cpu.images_mut() = self.images.clone();
                if let Some(session) = previous_session
                    .clone()
                    .or_else(|| self.cpu.as_ref().map(|current| current.session().clone()))
                {
                    *cpu.session_mut() = session;
                }
                self.cpu = Some(cpu);
            } else {
                let cpu = self.cpu.as_mut().expect("CPU RafUI bridge must exist");
                if let Some(session) = previous_session {
                    *cpu.session_mut() = session;
                }
                if surface_changed {
                    cpu.set_surface(
                        self.surface
                            .as_ref()
                            .expect("retained surface must be stored before host sync")
                            .clone(),
                    );
                }
                // Preserve cached text across retained-document updates. The
                // atlas is cleared only at an actual logical resize boundary.
            }
        }

        surface_changed || palette_changed || host_recreated || renderer_changed
    }

    fn sync_images(&mut self) {
        if let Some(gpu) = self.gpu.as_mut() {
            *gpu.host.images_mut() = self.images.clone();
        }
        if let Some(cpu) = self.cpu.as_mut() {
            *cpu.images_mut() = self.images.clone();
        }
    }
}

fn create_target_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> (wgpu::Texture, Arc<wgpu::TextureView>) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ApiGraphicBasic.RafUiBridgeTarget"),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));
    (texture, view)
}

fn clear_color(palette: StudioUiPalette) -> [u8; 4] {
    match palette {
        StudioUiPalette::IndustrialDark => [8, 11, 15, 255],
        StudioUiPalette::PaperLight => [250, 250, 250, 255],
    }
}

fn distance_squared(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

fn surface_content_changed(current: &UiSurface, incoming: &UiSurface) -> bool {
    current.id != incoming.id
        || current.palette != incoming.palette
        || current.theme != incoming.theme
        || current.root != incoming.root
        || current.style_sheet != incoming.style_sheet
}

fn should_process_input(
    input: &UiInputState,
    surface_changed: bool,
    pointer_capture: bool,
    previous_pointer_position: Option<[f32; 2]>,
    keyboard_focus: bool,
    _tooltip_active: bool,
) -> bool {
    let pointer_inside = input.pointer_position.is_some();
    let pointer_event = pointer_inside
        && (input.scroll_delta != [0.0, 0.0]
            || !input.pointer_buttons_down.is_empty()
            || !input.pointer_pressed_buttons.is_empty()
            || !input.pointer_released_buttons.is_empty())
        || input.pointer_pressed_outside;
    let keyboard_event = keyboard_focus
        && (!input.pressed_keys.is_empty()
            || !input.text_input.is_empty()
            || !input.ime_preedit.is_empty());
    let pointer_moved = input.pointer_position != previous_pointer_position;
    let hover_sample = pointer_moved
        && (input.pointer_position.is_none()
            || previous_pointer_position.is_none()
            || previous_pointer_position.is_some_and(|previous| {
                input.pointer_position.is_some_and(|current| {
                    distance_squared(previous, current)
                        >= HOVER_SAMPLE_DISTANCE_PX * HOVER_SAMPLE_DISTANCE_PX
                })
            }));

    // A passive mouse move must not rebuild layout, run the retained hit-test
    // tree and upload a new UI texture. That path was the main reason the
    // Game viewport lost FPS while the pointer merely crossed Agent or other
    // dense RafUI surfaces. A four-pixel hover sample keeps tooltips usable
    // without turning every passive pointer event into a layout pass. Pointer
    // presses, scroll, drags and keyboard input still enter the full path.
    surface_changed || pointer_capture || pointer_event || keyboard_event || hover_sample
}

fn pointer_visual_state_changed(
    input: &UiInputState,
    pointer_capture: bool,
    keyboard_focus: bool,
) -> bool {
    let local_pointer = input.pointer_position.is_some() || pointer_capture;
    (local_pointer
        && (!input.pointer_buttons_down.is_empty()
            || !input.pointer_pressed_buttons.is_empty()
            || !input.pointer_released_buttons.is_empty()))
        || (keyboard_focus
            && (!input.pressed_keys.is_empty()
                || !input.text_input.is_empty()
                || !input.ime_preedit.is_empty()
                || input.pointer_pressed_outside))
}

fn egui_input(
    ctx: &egui::Context,
    rect: egui::Rect,
    retained_pointer_capture: bool,
) -> UiInputState {
    ctx.input(|input| {
        let pointer_buttons_down = pointer_buttons_down(input);
        let pointer_pressed_buttons = pointer_pressed_buttons(input);
        let pointer_released_buttons = pointer_released_buttons(input);
        let pointer_pressed_outside = input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::PointerButton {
                    pos,
                    pressed: true,
                    button: egui::PointerButton::Primary,
                    ..
                } if !rect.contains(*pos)
            )
        });
        let press_origin_inside = input
            .pointer
            .press_origin()
            .is_some_and(|position| rect.contains(position));
        let frame_press_started_inside = input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::PointerButton {
                    pos,
                    pressed: true,
                    ..
                } if rect.contains(*pos)
            )
        });
        let owns_pointer_gesture = pointer_gesture_belongs_to_surface(
            retained_pointer_capture,
            press_origin_inside,
            frame_press_started_inside,
        );
        let foreign_pointer_activity = !owns_pointer_gesture
            && (!pointer_buttons_down.is_empty()
                || !pointer_pressed_buttons.is_empty()
                || !pointer_released_buttons.is_empty());
        let pointer_position = (!foreign_pointer_activity)
            .then(|| input.pointer.interact_pos())
            .flatten()
            .filter(|position| rect.contains(*position))
            .map(|position| [position.x - rect.min.x, position.y - rect.min.y]);
        let pointer_delta = input.pointer.delta();
        let mut pressed_keys = Vec::new();
        let mut text_input = String::new();
        for event in &input.events {
            match event {
                egui::Event::Text(text) | egui::Event::Paste(text) => text_input.push_str(text),
                egui::Event::Key {
                    key, pressed: true, ..
                } => pressed_keys.push(format!("{key:?}")),
                _ => {}
            }
        }
        UiInputState {
            pointer_position,
            pointer_delta: [pointer_delta.x, pointer_delta.y],
            time_seconds: input.time,
            scroll_delta: [-input.smooth_scroll_delta.x, -input.smooth_scroll_delta.y],
            pointer_down: owns_pointer_gesture && input.pointer.primary_down(),
            pointer_buttons_down: owns_pointer_gesture
                .then_some(pointer_buttons_down)
                .unwrap_or_default(),
            pointer_pressed_buttons: owns_pointer_gesture
                .then_some(pointer_pressed_buttons)
                .unwrap_or_default(),
            pointer_released_buttons: owns_pointer_gesture
                .then_some(pointer_released_buttons)
                .unwrap_or_default(),
            pointer_pressed_outside,
            pressed_keys,
            text_input,
            ime_preedit: String::new(),
            modifiers: raf_ui::UiModifiers {
                shift: input.modifiers.shift,
                control: input.modifiers.ctrl,
                alt: input.modifiers.alt,
                command: input.modifiers.mac_cmd,
            },
        }
    })
}

fn pointer_gesture_belongs_to_surface(
    retained_pointer_capture: bool,
    press_origin_inside: bool,
    frame_press_started_inside: bool,
) -> bool {
    retained_pointer_capture || press_origin_inside || frame_press_started_inside
}

fn pointer_buttons_down(input: &egui::InputState) -> Vec<UiPointerButton> {
    let mut buttons = Vec::new();
    if input.pointer.primary_down() {
        buttons.push(UiPointerButton::Primary);
    }
    if input.pointer.secondary_down() {
        buttons.push(UiPointerButton::Secondary);
    }
    if input.pointer.middle_down() {
        buttons.push(UiPointerButton::Middle);
    }
    buttons
}

fn pointer_pressed_buttons(input: &egui::InputState) -> Vec<UiPointerButton> {
    pointer_buttons_for(
        input,
        egui::PointerButton::Primary,
        UiPointerButton::Primary,
        true,
    )
    .into_iter()
    .chain(pointer_buttons_for(
        input,
        egui::PointerButton::Secondary,
        UiPointerButton::Secondary,
        true,
    ))
    .chain(pointer_buttons_for(
        input,
        egui::PointerButton::Middle,
        UiPointerButton::Middle,
        true,
    ))
    .collect()
}

fn pointer_released_buttons(input: &egui::InputState) -> Vec<UiPointerButton> {
    pointer_buttons_for(
        input,
        egui::PointerButton::Primary,
        UiPointerButton::Primary,
        false,
    )
    .into_iter()
    .chain(pointer_buttons_for(
        input,
        egui::PointerButton::Secondary,
        UiPointerButton::Secondary,
        false,
    ))
    .chain(pointer_buttons_for(
        input,
        egui::PointerButton::Middle,
        UiPointerButton::Middle,
        false,
    ))
    .collect()
}

fn pointer_buttons_for(
    input: &egui::InputState,
    source: egui::PointerButton,
    target: UiPointerButton,
    pressed: bool,
) -> Option<UiPointerButton> {
    let changed = if pressed {
        input.pointer.button_pressed(source)
    } else {
        input.pointer.button_released(source)
    };
    changed.then_some(target)
}

fn find_node<'a>(node: &'a raf_ui::UiNode, id: &str) -> Option<&'a raf_ui::UiNode> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_node(child, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_target_size_tracks_hidpi_without_changing_logical_layout() {
        let standard = UiEnvironment {
            viewport_size: [320.0, 180.0],
            scale_factor: 1.0,
            ..UiEnvironment::default()
        };
        let hidpi = UiEnvironment {
            scale_factor: 1.5,
            ..standard
        };
        assert_eq!(standard.physical_size(), [320, 180]);
        assert_eq!(hidpi.physical_size(), [480, 270]);
    }

    #[test]
    fn retained_text_raster_has_a_quality_floor() {
        let scale = |scale_factor| UiEnvironment {
            scale_factor,
            ..UiEnvironment::default()
        };
        assert_eq!(scale(1.0).raster_scale(), 1.0);
        assert_eq!(scale(1.25).raster_scale(), 1.25);
        assert_eq!(scale(1.5).raster_scale(), 1.5);
        assert_eq!(scale(3.0).raster_scale(), 3.0);
        assert_eq!(scale(5.0).raster_scale(), 4.0);
        assert_eq!(scale(1.0).text_raster_scale(), 1.25);
        assert_eq!(scale(1.25).text_raster_scale(), 1.25);
        assert_eq!(scale(1.5).text_raster_scale(), 1.5);
    }

    #[test]
    fn foreign_pointer_drag_does_not_process_a_retained_surface() {
        let input = UiInputState {
            pointer_buttons_down: vec![UiPointerButton::Primary],
            ..UiInputState::default()
        };

        assert!(!should_process_input(
            &input, false, false, None, false, false,
        ));
    }

    #[test]
    fn pointer_gesture_keeps_the_surface_where_it_started() {
        assert!(!pointer_gesture_belongs_to_surface(false, false, false));
        assert!(pointer_gesture_belongs_to_surface(false, true, false));
        assert!(pointer_gesture_belongs_to_surface(false, false, true));
        assert!(pointer_gesture_belongs_to_surface(true, false, false));
    }

    #[test]
    fn local_scroll_and_pointer_capture_still_process_input() {
        let scroll = UiInputState {
            pointer_position: Some([24.0, 32.0]),
            scroll_delta: [0.0, 12.0],
            ..UiInputState::default()
        };
        let outside_drag = UiInputState {
            pointer_position: None,
            ..UiInputState::default()
        };

        assert!(should_process_input(
            &scroll,
            false,
            false,
            Some([24.0, 32.0]),
            false,
            false,
        ));
        assert!(should_process_input(
            &outside_drag,
            false,
            true,
            Some([24.0, 32.0]),
            false,
            false,
        ));
    }

    #[test]
    fn outside_primary_press_processes_focus_release_without_surface_hit() {
        let input = UiInputState {
            pointer_pressed_outside: true,
            ..UiInputState::default()
        };

        assert!(should_process_input(
            &input, false, false, None, true, false,
        ));
    }
}
