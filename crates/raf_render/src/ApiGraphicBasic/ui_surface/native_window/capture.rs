//! CPU pixel capture and upload for retained UI presentation.

use super::NativeUiWindowHost;
use crate::api_graphic_basic::canvas_presenter::DirectCanvasPresenter;
use crate::api_graphic_basic::ui_surface::{CpuUiSurfaceHost, UiSurfaceCpuMetrics};

impl NativeUiWindowHost {
    /// Presents a retained surface through the CPU compositor and uploads the
    /// borrowed RGBA result only for the final native presentation step.
    pub fn render_cpu_ui<F>(
        &mut self,
        presenter: &mut DirectCanvasPresenter,
        host: &mut CpuUiSurfaceHost,
        resolve: F,
    ) -> Result<UiSurfaceCpuMetrics, wgpu::SurfaceError>
    where
        F: FnMut(&str) -> String,
    {
        self.with_presentation_target(|device, queue, view, size| {
            let clear_color = host.clear_color();
            let ui_frame = host.render(size, resolve);
            let metrics = ui_frame.metrics;
            presenter.present_cpu_pixels(
                device,
                queue,
                view,
                ui_frame.pixels,
                ui_frame.size,
                clear_color,
            );
            metrics
        })
    }
}
