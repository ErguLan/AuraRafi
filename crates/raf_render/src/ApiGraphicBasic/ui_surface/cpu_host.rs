//! CPU presentation host for retained UI surfaces.
//!
//! It has no WGPU dependency. A caller can upload its borrowed RGBA output
//! through `DirectCanvasPresenter` only when a CPU fallback is active.

use super::{
    UiDispatchedAction, UiInputState, UiSurface, UiSurfaceCpuMetrics, UiSurfaceCpuRenderer,
    UiSurfaceDrawList, UiSurfaceFrame, UiSurfaceSession,
};

pub struct CpuUiSurfaceHost {
    surface: UiSurface,
    session: UiSurfaceSession,
    compositor: UiSurfaceCpuRenderer,
    clear_color: [u8; 4],
    last_frame: Option<UiSurfaceFrame>,
    last_draw_list: Option<UiSurfaceDrawList>,
    last_metrics: UiSurfaceCpuMetrics,
}

impl CpuUiSurfaceHost {
    pub fn new(surface: UiSurface, clear_color: [u8; 4]) -> Self {
        Self {
            surface,
            session: UiSurfaceSession::default(),
            compositor: UiSurfaceCpuRenderer::new(),
            clear_color,
            last_frame: None,
            last_draw_list: None,
            last_metrics: UiSurfaceCpuMetrics::default(),
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

    pub fn clear_color(&self) -> [u8; 4] {
        self.clear_color
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
        let frame = self.build_frame(size, resolve);
        self.session.process_input(&self.surface, &frame, input)
    }

    pub fn render<F>(&mut self, size: [u32; 2], mut resolve: F) -> DirectUiSurfaceCpuFrame<'_>
    where
        F: FnMut(&str) -> String,
    {
        let frame = self.build_frame(size, |key| resolve(key));
        let draw_list =
            UiSurfaceDrawList::build(&frame, &self.session.text_atlas, |key| resolve(key));
        self.last_metrics =
            self.compositor
                .render(&draw_list, &self.session.text_atlas, size, self.clear_color);
        self.last_frame = Some(frame);
        self.last_draw_list = Some(draw_list);

        DirectUiSurfaceCpuFrame {
            frame: self.last_frame.as_ref().expect("CPU UI frame"),
            draw_list: self.last_draw_list.as_ref().expect("CPU UI draw list"),
            pixels: self.compositor.pixels(),
            size: self.compositor.size(),
            metrics: self.last_metrics,
        }
    }

    fn build_frame<F>(&mut self, size: [u32; 2], resolve: F) -> UiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        self.session.build_frame_with_resolved_text(
            &self.surface,
            size[0].max(1),
            size[1].max(1),
            self.clear_color,
            resolve,
        )
    }
}

pub struct DirectUiSurfaceCpuFrame<'a> {
    pub frame: &'a UiSurfaceFrame,
    pub draw_list: &'a UiSurfaceDrawList,
    pub pixels: &'a [u8],
    pub size: [u32; 2],
    pub metrics: UiSurfaceCpuMetrics,
}
