//! Native editor frame composition owned by ApiGraphicBasic.
//!
//! Scene/CAD canvases and retained RafUI chrome are encoded into one final
//! presentation command buffer. Upper editor layers provide owned outputs and
//! logical rectangles without importing WGPU types.

use super::canvas_presenter::{CanvasTargetRect, DirectCanvasPresenter, PreparedCanvasSource};
use super::device::SceneFrameOutput;
use super::ui_surface::{DirectUiSurfaceFrame, DirectUiSurfaceHost, NativeGraphicsContext};

pub struct EditorCanvasLayer {
    pub output: SceneFrameOutput,
    pub source_size: [u32; 2],
    pub target_rect: CanvasTargetRect,
}

pub struct EditorUiLayer<'a> {
    pub host: &'a mut DirectUiSurfaceHost,
    pub target_rect: CanvasTargetRect,
    pub logical_size: [u32; 2],
    pub raster_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditorCompositorMetrics {
    pub command_submits: u32,
    pub canvas_layers: u32,
    pub ui_layers: u32,
    pub presents: u32,
}

pub struct EditorComposedFrame {
    pub ui_layers: Vec<DirectUiSurfaceFrame>,
    pub metrics: EditorCompositorMetrics,
}

pub struct NativeEditorCompositor {
    canvas: DirectCanvasPresenter,
    clear_color: [u8; 4],
}

impl NativeEditorCompositor {
    pub fn new(graphics: &NativeGraphicsContext<'_>, clear_color: [u8; 4]) -> Self {
        Self {
            canvas: DirectCanvasPresenter::new(graphics.device(), graphics.color_format()),
            clear_color,
        }
    }

    pub(crate) fn compose<F>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        canvas_layer: Option<EditorCanvasLayer>,
        ui: &mut DirectUiSurfaceHost,
        logical_size: [u32; 2],
        raster_scale: f32,
        resolve: F,
    ) -> EditorComposedFrame
    where
        F: FnMut(&str) -> String,
    {
        let mut ui_layers = [EditorUiLayer {
            host: ui,
            target_rect: CanvasTargetRect::full(target_size),
            logical_size,
            raster_scale,
        }];
        self.compose_layers(
            device,
            queue,
            target,
            target_size,
            canvas_layer,
            &mut ui_layers,
            resolve,
        )
    }

    pub(crate) fn compose_layers<F>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        canvas_layer: Option<EditorCanvasLayer>,
        ui_layers: &mut [EditorUiLayer<'_>],
        mut resolve: F,
    ) -> EditorComposedFrame
    where
        F: FnMut(&str) -> String,
    {
        let prepared_canvas = canvas_layer.and_then(|layer| {
            self.canvas
                .prepare_output(device, queue, layer.output, layer.source_size)
                .map(|source| (source, layer.target_rect))
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ApiGraphicBasic.NativeEditorFrame"),
        });
        clear_target(&mut encoder, target, color_from_bytes(self.clear_color));

        let mut metrics = EditorCompositorMetrics::default();
        if let Some((source, target_rect)) = prepared_canvas {
            self.encode_canvas(
                device,
                &mut encoder,
                target,
                target_size,
                target_rect,
                source,
            );
            metrics.canvas_layers = 1;
        }

        let mut ui_frames = Vec::with_capacity(ui_layers.len());
        for layer in ui_layers {
            let frame = layer.host.encode_in_rect(
                device,
                queue,
                &mut encoder,
                target,
                target_size,
                layer.target_rect,
                layer.logical_size,
                layer.raster_scale,
                wgpu::LoadOp::Load,
                |key| resolve(key),
            );
            ui_frames.push(frame);
        }
        metrics.ui_layers = ui_frames.len() as u32;
        queue.submit(std::iter::once(encoder.finish()));
        metrics.command_submits = 1;

        EditorComposedFrame {
            ui_layers: ui_frames,
            metrics,
        }
    }

    fn encode_canvas(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        target_size: [u32; 2],
        target_rect: CanvasTargetRect,
        source: PreparedCanvasSource,
    ) {
        self.canvas.encode_prepared(
            device,
            encoder,
            target,
            target_size,
            target_rect,
            source,
            wgpu::LoadOp::Load,
        );
    }
}

fn clear_target(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    color: wgpu::Color,
) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ApiGraphicBasic.NativeEditorClear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
}

fn color_from_bytes(color: [u8; 4]) -> wgpu::Color {
    wgpu::Color {
        r: f64::from(color[0]) / 255.0,
        g: f64::from(color[1]) / 255.0,
        b: f64::from(color[2]) / 255.0,
        a: f64::from(color[3]) / 255.0,
    }
}
