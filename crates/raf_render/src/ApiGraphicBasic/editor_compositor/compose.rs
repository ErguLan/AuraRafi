//! Pass recording and final composition of canvas and retained UI layers.
//!
//! One `ProductDevice` owns the target. The compositor turns the retained scene
//! frame (native handoff or CPU pixels) plus every retained UI layer into a
//! single product packet, prepares it by revision, and draws it. There is no
//! blit chain, no intermediate GPU texture, and no second renderer.

use std::sync::Arc;

use raf_agb::backends::native::NativeBackend;
use raf_agb::product::{FrameUniforms, PacketHandle, ProductDevice};

use crate::api_graphic_basic::canvas_presenter::{append_cpu_output, CanvasTargetRect};
use crate::api_graphic_basic::device::SceneFrameOutput;
use crate::api_graphic_basic::product_composition::{
    linear_rgba, prepare_product_scene, scene_frame_uniforms, ProductFrameBuilder,
};
use crate::api_graphic_basic::ui_surface::DirectUiSurfaceHost;

use super::{EditorCanvasLayer, EditorComposedFrame, EditorCompositorMetrics, EditorUiLayer};

pub struct NativeEditorCompositor {
    builder: ProductFrameBuilder,
    clear_color: [u8; 4],
    revision: u64,
    fingerprint: Option<CompositionFingerprint>,
    handle: Option<PacketHandle>,
    metrics: EditorCompositorMetrics,
}

impl NativeEditorCompositor {
    pub fn new(
        _graphics: &crate::api_graphic_basic::ui_surface::NativeGraphicsContext<'_>,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            builder: ProductFrameBuilder::default(),
            clear_color,
            revision: 0,
            fingerprint: None,
            handle: None,
            metrics: EditorCompositorMetrics::default(),
        }
    }

    /// Drops any retained canvas identity after a surface switch.
    pub fn release_canvas(&mut self) {
        self.fingerprint = None;
        self.handle = None;
        self.builder.clear();
    }

    pub fn metrics(&self) -> EditorCompositorMetrics {
        self.metrics
    }

    pub(crate) fn compose<F>(
        &mut self,
        device: &mut ProductDevice<NativeBackend>,
        target_size: [u32; 2],
        canvas_layer: Option<EditorCanvasLayer>,
        ui: &mut DirectUiSurfaceHost,
        logical_size: [u32; 2],
        raster_scale: f32,
        resolve: F,
    ) -> Result<EditorComposedFrame, String>
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
            target_size,
            canvas_layer,
            &mut ui_layers,
            resolve,
        )
    }

    pub(crate) fn compose_layers<F>(
        &mut self,
        device: &mut ProductDevice<NativeBackend>,
        target_size: [u32; 2],
        canvas_layer: Option<EditorCanvasLayer>,
        ui_layers: &mut [EditorUiLayer<'_>],
        mut resolve: F,
    ) -> Result<EditorComposedFrame, String>
    where
        F: FnMut(&str) -> String,
    {
        let clear = linear_clear(self.clear_color);
        let fingerprint = CompositionFingerprint::new(&canvas_layer, ui_layers, target_size);
        let changed = self.fingerprint != Some(fingerprint) || self.handle.is_none();

        // Camera and viewport come from the canvas regardless of whether this
        // frame reprepares the packet; an idle frame still has to draw with the
        // retained scene camera, not the default one.
        let mut uniforms = FrameUniforms::default();
        uniforms.viewport = [0.0, 0.0, 1.0, 1.0];
        if let Some(layer) = canvas_layer.as_ref() {
            if let SceneFrameOutput::Scene(frame) = layer.output.as_ref() {
                uniforms = scene_frame_uniforms(frame);
                uniforms.viewport = layer.target_rect.normalized(target_size);
            }
        }
        if uniforms.shadows.is_some() && !device.supports_shadows() {
            uniforms.shadows = None;
        }
        if uniforms.scene_filter && !device.supports_scene_filter() {
            uniforms.scene_filter = false;
        }
        let mut canvas_layers = 0;

        if changed {
            self.revision = self.revision.wrapping_add(1).max(1);
            self.builder.clear();
            match canvas_layer.as_ref() {
                Some(layer) => match layer.output.as_ref() {
                    SceneFrameOutput::Scene(frame) => {
                        let rect = layer.target_rect.normalized(target_size);
                        if rect[2] <= 0.0 || rect[3] <= 0.0 {
                            self.builder.append_empty(clear, self.revision)?;
                        } else {
                            let full = rect == [0.0, 0.0, 1.0, 1.0];
                            self.builder.append_scene_placed(
                                frame,
                                self.revision,
                                if full { None } else { Some(clear) },
                                (!full).then_some(rect),
                            )?;
                            canvas_layers = 1;
                        }
                    }
                    SceneFrameOutput::CpuPixels(_) => {
                        self.builder.append_empty(clear, self.revision)?;
                        let rect = layer.target_rect.normalized(target_size);
                        if append_cpu_output(
                            self.builder.packet_mut(),
                            rect,
                            layer.output.as_ref(),
                            layer.source_size,
                        )? {
                            canvas_layers = 1;
                        }
                    }
                },
                None => self.builder.append_empty(clear, self.revision)?,
            }

            let mut ui_frames = Vec::with_capacity(ui_layers.len());
            for layer in ui_layers.iter_mut() {
                let rect = layer.target_rect.normalized(target_size);
                let painted = layer.host.append_product_paint_frame(
                    self.builder.packet_mut(),
                    target_size,
                    [
                        rect[0] * target_size[0] as f32,
                        rect[1] * target_size[1] as f32,
                        rect[2] * target_size[0] as f32,
                        rect[3] * target_size[1] as f32,
                    ],
                    layer.logical_size,
                    layer.raster_scale,
                    |key| resolve(key),
                )?;
                ui_frames.push(painted);
            }

            let handle = prepare_product_scene(device, &mut self.builder)?;
            self.handle = Some(handle);
            self.fingerprint = Some(fingerprint);
            let mut metrics = EditorCompositorMetrics::default();
            metrics.command_submits = 1;
            metrics.canvas_layers = canvas_layers;
            metrics.ui_layers = ui_frames.len() as u32;
            self.metrics = metrics;

            let handle = self.handle.expect("product handle was just prepared");
            device
                .draw(handle, &uniforms)
                .map_err(|error| format!("product draw failed: {error}"))?;

            return Ok(EditorComposedFrame {
                ui_layers: ui_frames,
                metrics: self.metrics,
            });
        }

        let handle = self
            .handle
            .ok_or_else(|| "compositor has no prepared product packet".to_string())?;
        device
            .draw(handle, &uniforms)
            .map_err(|error| format!("product draw failed: {error}"))?;

        Ok(EditorComposedFrame {
            ui_layers: Vec::new(),
            metrics: self.metrics,
        })
    }
}

fn linear_clear(color: [u8; 4]) -> [f32; 4] {
    linear_rgba(color)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompositionFingerprint {
    target_size: [u32; 2],
    canvas: CanvasFingerprint,
    ui: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CanvasFingerprint {
    None,
    Scene {
        ptr: usize,
        rect: [u32; 4],
        source_size: [u32; 2],
    },
    Cpu {
        ptr: usize,
        len: usize,
        rect: [u32; 4],
        source_size: [u32; 2],
    },
}

impl CompositionFingerprint {
    fn new(
        canvas: &Option<EditorCanvasLayer>,
        ui_layers: &[EditorUiLayer<'_>],
        target_size: [u32; 2],
    ) -> Self {
        let canvas = match canvas {
            None => CanvasFingerprint::None,
            Some(layer) => {
                let rect = [
                    layer.target_rect.x,
                    layer.target_rect.y,
                    layer.target_rect.width,
                    layer.target_rect.height,
                ];
                match layer.output.as_ref() {
                    SceneFrameOutput::Scene(frame) => CanvasFingerprint::Scene {
                        ptr: Arc::as_ptr(&layer.output) as *const () as usize,
                        rect,
                        source_size: [frame.width, frame.height],
                    },
                    SceneFrameOutput::CpuPixels(pixels) => CanvasFingerprint::Cpu {
                        ptr: Arc::as_ptr(&layer.output) as *const () as usize,
                        len: pixels.len(),
                        rect,
                        source_size: layer.source_size,
                    },
                }
            }
        };
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for layer in ui_layers {
            hash = fnv(hash, layer.host.paint_revision());
            hash = fnv(hash, u64::from(layer.target_rect.x));
            hash = fnv(hash, u64::from(layer.target_rect.y));
            hash = fnv(hash, u64::from(layer.target_rect.width));
            hash = fnv(hash, u64::from(layer.target_rect.height));
            hash = fnv(hash, u64::from(layer.logical_size[0]));
            hash = fnv(hash, u64::from(layer.logical_size[1]));
            hash = fnv(hash, u64::from(layer.raster_scale.to_bits()));
        }
        Self {
            target_size,
            canvas,
            ui: hash,
        }
    }
}

fn fnv(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x0000_0100_0000_01b3)
}
