//! Surface acquisition, reconfiguration, and presentation health.

use super::super::presentation_lifecycle::{
    classify_surface_error, PresentationFault, PresentationHealth,
};
use super::super::{DirectUiSurfaceFrame, DirectUiSurfaceHost};
use super::NativeUiWindowHost;
use crate::api_graphic_basic::canvas_presenter::{DirectCanvasPresenter, DirectSceneSurfaceHost};
use crate::api_graphic_basic::device::{BasicDevice, SceneFrameOutput};
use crate::api_graphic_basic::{
    EditorCanvasLayer, EditorComposedFrame, EditorUiLayer, NativeEditorCompositor,
};
use crate::engine_time::Instant;
use crate::scene_renderer::SceneRenderFrame;

impl NativeUiWindowHost {
    /// Reports whether the surface is presenting, retrying, or exhausted.
    ///
    /// Transient faults are repaired inside the host before this state is
    /// observed, so callers see `Recovering` only when a retry is still due.
    pub fn presentation_health(&self) -> PresentationHealth {
        self.presentation.health()
    }

    /// Most recent surface fault observed by this host, including one that
    /// already recovered. Successful presentation clears the pending recovery
    /// state but keeps this diagnostic history until the next fault replaces
    /// it or the host is reconstructed.
    pub fn last_presentation_fault(&self) -> Option<PresentationFault> {
        self.presentation.last_fault()
    }

    /// Reconfigures the windowed swapchain after an external surface change.
    ///
    /// The host already reconfigures and retries acquisition when a transient
    /// fault is detected. This entry point exists for OS events that invalidate
    /// the surface before the next frame, and for hosts that suspend and resume
    /// presentation. A headless host owns a fixed target and is a no-op.
    pub fn recover_surface(&mut self) -> Result<(), String> {
        if self.is_headless() {
            return Ok(());
        }
        let Some(surface) = self.surface.as_ref() else {
            return Err("native presentation surface is unavailable".to_string());
        };
        surface.configure(&self.device, &self.configuration);
        self.presentation.record_success(Instant::now());
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if self.configuration.width == width && self.configuration.height == height {
            return;
        }
        if let Some(target) = self.headless.as_mut() {
            if target.resize(&self.device, [width, height]).is_ok() {
                self.configuration.width = width;
                self.configuration.height = height;
            }
            return;
        }
        self.configuration.width = width;
        self.configuration.height = height;
        if let Some(surface) = self.surface.as_ref() {
            surface.configure(&self.device, &self.configuration);
        }
    }

    /// Runs `encode` against the next presentation view.
    ///
    /// Windowed hosts acquire and present a swapchain image. Headless hosts
    /// compose into their owned target and never present, which is why the
    /// returned metrics keep `presents = 0` there.
    pub(super) fn with_presentation_target<R>(
        &mut self,
        encode: impl FnOnce(&wgpu::Device, &wgpu::Queue, &wgpu::TextureView, [u32; 2]) -> R,
    ) -> Result<R, wgpu::SurfaceError> {
        if let Some(target) = self.headless.as_ref() {
            let size = target.size();
            return Ok(encode(
                self.device.as_ref(),
                self.queue.as_ref(),
                target.view(),
                size,
            ));
        }
        // Public constructors always install a swapchain or a headless target.
        let frame = self.acquire_surface_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let size = [self.configuration.width, self.configuration.height];
        let output = encode(self.device.as_ref(), self.queue.as_ref(), &view, size);
        frame.present();
        Ok(output)
    }

    /// Acquires the next swapchain image with one bounded recovery attempt.
    ///
    /// Lost, outdated, and timed-out surfaces are reconfigured with the current
    /// configuration and retried exactly once. Out-of-memory is never retried.
    /// Every acquisition attempt records its outcome in the presentation
    /// recovery state so callers can distinguish a recovered frame from an
    /// exhausted surface without parsing an error string.
    fn acquire_surface_texture(&mut self) -> Result<wgpu::SurfaceTexture, wgpu::SurfaceError> {
        // Constructors always install a windowed surface or a headless target,
        // and the headless path never reaches acquisition.
        let Some(surface) = self.surface.as_ref() else {
            return Err(wgpu::SurfaceError::Lost);
        };
        match surface.get_current_texture() {
            Ok(texture) => {
                self.presentation.record_success(Instant::now());
                Ok(texture)
            }
            Err(error) => {
                let fault = classify_surface_error(error.clone());
                self.presentation.record_fault(fault, Instant::now());
                if !matches!(
                    fault,
                    PresentationFault::Lost
                        | PresentationFault::Outdated
                        | PresentationFault::Timeout
                ) {
                    return Err(error);
                }
                surface.configure(&self.device, &self.configuration);
                match surface.get_current_texture() {
                    Ok(texture) => {
                        self.presentation.record_success(Instant::now());
                        Ok(texture)
                    }
                    Err(retry_error) => {
                        self.presentation.record_fault(
                            classify_surface_error(retry_error.clone()),
                            Instant::now(),
                        );
                        Err(retry_error)
                    }
                }
            }
        }
    }

    pub fn render<F>(
        &mut self,
        host: &mut DirectUiSurfaceHost,
        resolve: F,
    ) -> Result<DirectUiSurfaceFrame, wgpu::SurfaceError>
    where
        F: FnMut(&str) -> String,
    {
        self.with_presentation_target(|device, queue, view, size| {
            host.render(device, queue, view, size, resolve)
        })
    }

    pub fn render_editor_frame<F>(
        &mut self,
        compositor: &mut NativeEditorCompositor,
        canvas_layer: Option<EditorCanvasLayer>,
        ui: &mut DirectUiSurfaceHost,
        logical_size: [u32; 2],
        raster_scale: f32,
        resolve: F,
    ) -> Result<EditorComposedFrame, wgpu::SurfaceError>
    where
        F: FnMut(&str) -> String,
    {
        let headless = self.is_headless();
        let mut composed = self.with_presentation_target(|device, queue, view, size| {
            compositor.compose(
                device,
                queue,
                view,
                size,
                canvas_layer,
                ui,
                logical_size,
                raster_scale,
                resolve,
            )
        })?;
        composed.metrics.presents = if headless { 0 } else { 1 };
        Ok(composed)
    }

    pub fn render_editor_layers<F>(
        &mut self,
        compositor: &mut NativeEditorCompositor,
        canvas_layer: Option<EditorCanvasLayer>,
        ui_layers: &mut [EditorUiLayer<'_>],
        resolve: F,
    ) -> Result<EditorComposedFrame, wgpu::SurfaceError>
    where
        F: FnMut(&str) -> String,
    {
        let headless = self.is_headless();
        let mut composed = self.with_presentation_target(|device, queue, view, size| {
            compositor.compose_layers(device, queue, view, size, canvas_layer, ui_layers, resolve)
        })?;
        composed.metrics.presents = if headless { 0 } else { 1 };
        Ok(composed)
    }

    pub fn present_canvas(
        &mut self,
        presenter: &mut DirectCanvasPresenter,
        output: SceneFrameOutput,
        source_size: [u32; 2],
        clear_color: [u8; 4],
    ) -> Result<(), wgpu::SurfaceError> {
        self.with_presentation_target(|device, queue, view, _| {
            presenter.present(device, queue, view, output, source_size, clear_color);
        })
    }

    pub fn render_scene(
        &mut self,
        host: &mut DirectSceneSurfaceHost,
        basic_device: &mut BasicDevice,
        frame: &SceneRenderFrame,
    ) -> Result<(), wgpu::SurfaceError> {
        self.with_presentation_target(|device, queue, view, _| {
            host.render_frame(basic_device, device, queue, view, frame);
        })
    }
}
