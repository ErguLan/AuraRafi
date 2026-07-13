//! Direct retained UI host.
//!
//! The host owns retained UI state and the WGPU compositor but not a window.
//! A platform shell supplies the target texture view, allowing the editor to
//! migrate away from `eframe` without changing UI documents or event logic.

use super::{
    UiDispatchedAction, UiInputState, UiSurface, UiSurfaceDrawList, UiSurfaceFrame,
    UiSurfaceGpuMetrics, UiSurfaceGpuRenderer, UiSurfaceSession,
};

pub struct DirectUiSurfaceHost {
    surface: UiSurface,
    session: UiSurfaceSession,
    compositor: UiSurfaceGpuRenderer,
    clear_color: [u8; 4],
}

impl DirectUiSurfaceHost {
    pub fn new(
        surface: UiSurface,
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            surface,
            session: UiSurfaceSession::default(),
            compositor: UiSurfaceGpuRenderer::new(device, color_format),
            clear_color,
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

    pub fn build_frame<F>(&mut self, size: [u32; 2], resolve: F) -> UiSurfaceFrame
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

    pub fn render<F>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        mut resolve: F,
    ) -> DirectUiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        let frame = self.build_frame(size, |key| resolve(key));
        let draw_list =
            UiSurfaceDrawList::build(&frame, &self.session.text_atlas, |key| resolve(key));
        let metrics = self.compositor.render(
            device,
            queue,
            target,
            size,
            &draw_list,
            &mut self.session.text_atlas,
            self.clear_color,
        );
        DirectUiSurfaceFrame {
            frame,
            draw_list,
            metrics,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectUiSurfaceFrame {
    pub frame: UiSurfaceFrame,
    pub draw_list: UiSurfaceDrawList,
    pub metrics: UiSurfaceGpuMetrics,
}
