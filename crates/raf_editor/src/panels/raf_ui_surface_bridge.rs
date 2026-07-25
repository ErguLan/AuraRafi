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
    UiEnvironment, UiMotionSpec, UiOverlayLayer, UiOverlayManager, UiOverlayRequest, UiPlacement,
    UiTween,
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
    palette: Option<StudioUiPalette>,
    images: UiSurfaceImageStore,
    last_logical_size: Option<[u32; 2]>,
    tooltip_motion: UiTween,
    last_tooltip_time_seconds: f64,
    overlay_manager: UiOverlayManager,
}

impl RafUiSurfaceBridge {
    pub fn new(texture_name: &'static str) -> Self {
        Self {
            canvas: GpuCanvas::new(texture_name).with_retained_ui_sampling(),
            tooltip_canvas: GpuCanvas::new(format!("{texture_name}.tooltip"))
                .with_retained_ui_sampling(),
            gpu: None,
            cpu: None,
            tooltip_gpu: None,
            tooltip_cpu: None,
            surface: None,
            palette: None,
            images: UiSurfaceImageStore::default(),
            last_logical_size: None,
            tooltip_motion: UiTween::new(0.0, UiMotionSpec::tooltip()),
            last_tooltip_time_seconds: 0.0,
            overlay_manager: UiOverlayManager::default(),
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

    fn show_with_control_state_and_clear_color<F, S>(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: UiSurface,
        clear_color: [u8; 4],
        mut seed_control_state: S,
        mut resolve_text: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
        S: FnMut(&mut raf_ui::UiControlState),
    {
        let surface = surface.with_retained_tooltips(false);
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
            ..UiEnvironment::default()
        };
        let raster_scale = presentation.text_raster_scale();
        let target_size = presentation.physical_size();
        self.sync_surface(render_state, palette, surface, target_size, clear_color);
        self.reset_text_atlas_when_resized(logical_size);
        self.with_control_state(|controls| seed_control_state(controls));

        let input = egui_input(ui.ctx(), rect);
        let (actions, hover_changed) =
            if let (Some(render_state), Some(gpu)) = (render_state, self.gpu.as_mut()) {
                gpu.resize(render_state.device.as_ref(), target_size);
                let hovered_before = gpu.host.session().interaction.focus.hovered.clone();
                let actions = gpu.host.process_input_at_scale(
                    logical_size,
                    raster_scale,
                    |key| resolve_text(key),
                    &input,
                );
                gpu.host.render_at_scale(
                    render_state.device.as_ref(),
                    render_state.queue.as_ref(),
                    gpu.view.as_ref(),
                    target_size,
                    logical_size,
                    raster_scale,
                    |key| resolve_text(key),
                );
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
                (
                    actions,
                    hovered_before != gpu.host.session().interaction.focus.hovered,
                )
            } else {
                let cpu = self
                    .cpu
                    .as_mut()
                    .expect("CPU RafUI bridge must be prepared");
                let hovered_before = cpu.session().interaction.focus.hovered.clone();
                let actions = cpu.process_input_at_scale(
                    logical_size,
                    raster_scale,
                    |key| resolve_text(key),
                    &input,
                );
                let frame = cpu.render_at_scale(target_size, logical_size, raster_scale, |key| {
                    resolve_text(key)
                });
                self.canvas.present(
                    ui.ctx(),
                    None,
                    SceneFrameOutput::CpuPixels(frame.pixels.to_vec()),
                    frame.size[0],
                    frame.size[1],
                );
                (
                    actions,
                    hovered_before != cpu.session().interaction.focus.hovered,
                )
            };
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
        if let Some(gpu) = self.gpu.as_ref() {
            return gpu.host.session().hovered_tooltip_key(surface);
        }
        self.cpu
            .as_ref()
            .and_then(|cpu| cpu.session().hovered_tooltip_key(surface))
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
        let Some(tooltip_key) = self.current_tooltip_key() else {
            self.overlay_manager.remove("rafui.tooltip.overlay");
            self.tooltip_motion.set_target(0.0);
            self.tooltip_motion.advance(delta_seconds, false);
            return;
        };
        let Some(pointer_position) = pointer_position else {
            return;
        };
        self.tooltip_motion.set_target(1.0);
        let opacity = self.tooltip_motion.advance(delta_seconds, false);
        let text = resolve_text(&tooltip_key);
        let logical_size = raf_ui_tooltip::logical_size_for_text(&text);
        let pixels_per_point = ctx.pixels_per_point().clamp(0.5, 4.0);
        let presentation = UiEnvironment {
            viewport_size: [logical_size[0] as f32, logical_size[1] as f32],
            scale_factor: pixels_per_point,
            ..UiEnvironment::default()
        };
        let raster_scale = presentation.text_raster_scale();
        let target_size = presentation.physical_size();
        let tooltip_surface = raf_ui_tooltip::build_surface(palette, &text, opacity);

        if let Some(render_state) = render_state {
            let recreate = self
                .tooltip_gpu
                .as_ref()
                .map(|gpu| gpu.format != render_state.target_format)
                .unwrap_or(true);
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
                *gpu.host.surface_mut() = tooltip_surface.clone();
                gpu.resize(render_state.device.as_ref(), target_size);
            }
            let gpu = self
                .tooltip_gpu
                .as_mut()
                .expect("retained tooltip GPU surface must exist");
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
        } else {
            if self.tooltip_cpu.is_none() {
                self.tooltip_cpu =
                    Some(CpuUiSurfaceHost::new(tooltip_surface.clone(), [0, 0, 0, 0]));
            }
            let cpu = self
                .tooltip_cpu
                .as_mut()
                .expect("retained tooltip CPU surface must exist");
            *cpu.surface_mut() = tooltip_surface;
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
            ctx.request_repaint();
        }
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

    fn sync_surface(
        &mut self,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        surface: UiSurface,
        size: [u32; 2],
        clear_color: [u8; 4],
    ) {
        let surface_changed = self.surface.as_ref() != Some(&surface);
        let palette_changed = self.palette != Some(palette);
        self.surface = Some(surface.clone());
        self.palette = Some(palette);

        if let Some(render_state) = render_state {
            let recreate = self
                .gpu
                .as_ref()
                .map(|gpu| gpu.format != render_state.target_format || palette_changed)
                .unwrap_or(true);
            if recreate {
                let mut gpu = GpuSurface::new(surface.clone(), render_state, size, clear_color);
                *gpu.host.images_mut() = self.images.clone();
                self.gpu = Some(gpu);
            } else if surface_changed {
                let gpu = self.gpu.as_mut().expect("GPU RafUI bridge must exist");
                *gpu.host.surface_mut() = surface.clone();
                gpu.host.session_mut().text_atlas.clear();
            }
        }

        if render_state.is_none() {
            if self.cpu.is_none() || palette_changed {
                let mut cpu = CpuUiSurfaceHost::new(surface, clear_color);
                *cpu.images_mut() = self.images.clone();
                self.cpu = Some(cpu);
            } else if surface_changed {
                let cpu = self.cpu.as_mut().expect("CPU RafUI bridge must exist");
                *cpu.surface_mut() = surface;
                cpu.session_mut().text_atlas.clear();
            }
        }
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

fn egui_input(ctx: &egui::Context, rect: egui::Rect) -> UiInputState {
    ctx.input(|input| {
        let pointer_position = input
            .pointer
            .interact_pos()
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
            pointer_down: input.pointer.primary_down(),
            pointer_buttons_down: pointer_buttons_down(input),
            pointer_pressed_buttons: pointer_pressed_buttons(input),
            pointer_released_buttons: pointer_released_buttons(input),
            pressed_keys,
            text_input,
        }
    })
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
}
