//! CPU presentation host for retained UI surfaces.
//!
//! It has no WGPU dependency. A caller can upload its borrowed RGBA output
//! through `DirectCanvasPresenter` only when a CPU fallback is active.

use super::compilation::{UiSurfaceCompilationCache, UiSurfaceCompileMetrics};
use super::{
    UiDispatchedAction, UiInputState, UiSurface, UiSurfaceCpuMetrics, UiSurfaceCpuRenderer,
    UiSurfaceDiagnostics, UiSurfaceDrawList, UiSurfaceFrame, UiSurfaceImageStore, UiSurfaceSession,
};

pub struct CpuUiSurfaceHost {
    surface: UiSurface,
    session: UiSurfaceSession,
    compositor: UiSurfaceCpuRenderer,
    images: UiSurfaceImageStore,
    clear_color: [u8; 4],
    last_frame: Option<UiSurfaceFrame>,
    last_draw_list: Option<UiSurfaceDrawList>,
    last_metrics: UiSurfaceCpuMetrics,
    compilation: UiSurfaceCompilationCache,
}

impl CpuUiSurfaceHost {
    pub fn new(surface: UiSurface, clear_color: [u8; 4]) -> Self {
        Self {
            surface,
            session: UiSurfaceSession::default(),
            compositor: UiSurfaceCpuRenderer::new(),
            images: UiSurfaceImageStore::default(),
            clear_color,
            last_frame: None,
            last_draw_list: None,
            last_metrics: UiSurfaceCpuMetrics::default(),
            compilation: UiSurfaceCompilationCache::default(),
        }
    }

    pub fn surface(&self) -> &UiSurface {
        &self.surface
    }

    pub fn surface_mut(&mut self) -> &mut UiSurface {
        &mut self.surface
    }

    pub fn session(&self) -> &UiSurfaceSession {
        &self.session
    }

    /// Mutable access to transient focus, text, and scroll state. This keeps
    /// the CPU fallback behavior aligned with the direct GPU host.
    pub fn session_mut(&mut self) -> &mut UiSurfaceSession {
        &mut self.session
    }

    pub fn clear_color(&self) -> [u8; 4] {
        self.clear_color
    }

    pub fn images(&self) -> &UiSurfaceImageStore {
        &self.images
    }

    pub fn images_mut(&mut self) -> &mut UiSurfaceImageStore {
        &mut self.images
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

    /// Matches the direct host's HiDPI behavior while keeping input and
    /// retained layout coordinates in logical points.
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
            size,
            raster_scale,
            self.clear_color,
        );
        self.session.process_input(&self.surface, &frame, input)
    }

    pub fn render<F>(&mut self, size: [u32; 2], resolve: F) -> DirectUiSurfaceCpuFrame<'_>
    where
        F: FnMut(&str) -> String,
    {
        self.render_at_scale(size, size, 1.0, resolve)
    }

    /// Produces a dense CPU pixel buffer from a logical retained surface.
    pub fn render_at_scale<F>(
        &mut self,
        target_size: [u32; 2],
        logical_size: [u32; 2],
        raster_scale: f32,
        mut resolve: F,
    ) -> DirectUiSurfaceCpuFrame<'_>
    where
        F: FnMut(&str) -> String,
    {
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        let compiled = self.compilation.compile(
            &self.surface,
            &mut self.session,
            logical_size,
            raster_scale,
            self.clear_color,
            |key| resolve(key),
        );
        let draw_list = compiled.draw_list.scaled_for_output(
            target_size[0].max(1) as f32 / logical_size[0].max(1) as f32,
            target_size[1].max(1) as f32 / logical_size[1].max(1) as f32,
        );
        self.last_metrics = self.compositor.render(
            &draw_list,
            &self.session.text_atlas,
            &self.images,
            target_size,
            self.clear_color,
        );
        self.last_frame = Some((*compiled.frame).clone());
        self.last_draw_list = Some(draw_list);

        DirectUiSurfaceCpuFrame {
            frame: self.last_frame.as_ref().expect("CPU UI frame"),
            draw_list: self.last_draw_list.as_ref().expect("CPU UI draw list"),
            pixels: self.compositor.pixels(),
            size: self.compositor.size(),
            metrics: self.last_metrics,
            compilation: self.compilation.metrics(),
        }
    }
}

pub struct DirectUiSurfaceCpuFrame<'a> {
    pub frame: &'a UiSurfaceFrame,
    pub draw_list: &'a UiSurfaceDrawList,
    pub pixels: &'a [u8],
    pub size: [u32; 2],
    pub metrics: UiSurfaceCpuMetrics,
    pub compilation: UiSurfaceCompileMetrics,
}

impl DirectUiSurfaceCpuFrame<'_> {
    pub fn diagnostics(&self) -> UiSurfaceDiagnostics {
        UiSurfaceDiagnostics::from_frame(self.frame)
    }
}
