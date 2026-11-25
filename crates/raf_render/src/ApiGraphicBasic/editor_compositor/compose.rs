//! Pass recording and final composition of canvas and retained UI layers.
//!
//! One `ProductDevice` owns the target. Executors that support layers keep two
//! retained packets on the same device: a scene packet (canvas + scene commands
//! + scene environment) and a UI packet (retained RafUI paint). Each half is
//! rebuilt only when its own fingerprint changes and both are drawn in one
//! target acquisition and one present. Executors without layered support keep
//! the single combined packet used before this path existed.
//!
//! There is no blit chain, no intermediate GPU texture, and no second renderer.

use std::sync::{Arc, Once};

use raf_agb::product::{
    Command, FrameUniforms, PacketHandle, PacketKey, ProductDevice, ProductPacket,
};
use raf_agb::Backend;

use crate::api_graphic_basic::canvas_presenter::{append_cpu_output, CanvasTargetRect};
use crate::api_graphic_basic::device::SceneFrameOutput;
use crate::api_graphic_basic::product_composition::{
    linear_rgba, prepare_product_packet, prepare_product_scene, scene_frame_uniforms,
    ProductFrameBuilder,
};
use crate::api_graphic_basic::ui_surface::DirectUiSurfaceHost;

use super::{EditorCanvasLayer, EditorComposedFrame, EditorCompositorMetrics, EditorUiLayer};

/// Retained packet key for the RafUI half. The scene keeps
/// `PRODUCT_SCENE_PACKET_KEY` (1) through `ProductFrameBuilder::default`.
const UI_PACKET_KEY: PacketKey = PacketKey(2);

pub struct NativeEditorCompositor {
    // Single-packet path used by every executor without layered support.
    builder: ProductFrameBuilder,
    revision: u64,
    fingerprint: Option<CompositionFingerprint>,
    handle: Option<PacketHandle>,
    // Two retained packets for executors that draw both halves in one pass.
    scene_builder: ProductFrameBuilder,
    scene_revision: u64,
    scene_fingerprint: Option<SceneFingerprint>,
    scene_handle: Option<PacketHandle>,
    ui_builder: ProductFrameBuilder,
    ui_revision: u64,
    ui_fingerprint: Option<UiFingerprint>,
    ui_handle: Option<PacketHandle>,
    layered: Option<bool>,
    clear_color: [u8; 4],
    metrics: EditorCompositorMetrics,
}

impl NativeEditorCompositor {
    pub fn new(
        _graphics: &crate::api_graphic_basic::ui_surface::NativeGraphicsContext<'_>,
        clear_color: [u8; 4],
    ) -> Self {
        Self::empty(clear_color)
    }

    fn empty(clear_color: [u8; 4]) -> Self {
        Self {
            builder: ProductFrameBuilder::default(),
            revision: 0,
            fingerprint: None,
            handle: None,
            scene_builder: ProductFrameBuilder::default(),
            scene_revision: 0,
            scene_fingerprint: None,
            scene_handle: None,
            ui_builder: ProductFrameBuilder::new(UI_PACKET_KEY),
            ui_revision: 0,
            ui_fingerprint: None,
            ui_handle: None,
            layered: None,
            clear_color,
            metrics: EditorCompositorMetrics::default(),
        }
    }

    /// Drops every retained packet identity after a surface or device switch.
    /// The next frame rebuilds both halves from their sources.
    pub fn release_canvas(&mut self) {
        self.fingerprint = None;
        self.handle = None;
        self.builder.clear();
        self.scene_fingerprint = None;
        self.scene_handle = None;
        self.scene_builder.clear();
        self.ui_fingerprint = None;
        self.ui_handle = None;
        self.ui_builder.clear();
        self.layered = None;
    }

    pub fn metrics(&self) -> EditorCompositorMetrics {
        self.metrics
    }

    pub(crate) fn compose<B, F>(
        &mut self,
        device: &mut ProductDevice<B>,
        target_size: [u32; 2],
        canvas_layer: Option<EditorCanvasLayer>,
        ui: &mut DirectUiSurfaceHost,
        logical_size: [u32; 2],
        raster_scale: f32,
        resolve: F,
    ) -> Result<EditorComposedFrame, String>
    where
        B: Backend,
        F: FnMut(&str) -> String,
    {
        let mut ui_layers = [EditorUiLayer {
            host: ui,
            target_rect: CanvasTargetRect::full(target_size),
            logical_size,
            raster_scale,
        }];
        self.compose_layers(device, target_size, canvas_layer, &mut ui_layers, resolve)
    }

    pub(crate) fn compose_layers<B, F>(
        &mut self,
        device: &mut ProductDevice<B>,
        target_size: [u32; 2],
        canvas_layer: Option<EditorCanvasLayer>,
        ui_layers: &mut [EditorUiLayer<'_>],
        resolve: F,
    ) -> Result<EditorComposedFrame, String>
    where
        B: Backend,
        F: FnMut(&str) -> String,
    {
        let layered = device.supports_layers();
        if self.layered != Some(layered) {
            self.reset_for_path(layered);
        }
        if layered {
            self.compose_layered(device, target_size, canvas_layer, ui_layers, resolve)
        } else {
            self.compose_single(device, target_size, canvas_layer, ui_layers, resolve)
        }
    }

    /// Drops the retained identity of the previous composition path so a
    /// backend switch never mixes packets. Revision counters stay monotonic;
    /// the freshly cleared builders accept the next value.
    fn reset_for_path(&mut self, layered: bool) {
        // Key 1 is shared by the combined packet and the scene packet. Keep the
        // next revision above whichever path prepared it last; key 2 tracks its
        // own counter across switches.
        if layered {
            self.scene_revision = self.scene_revision.max(self.revision);
        } else {
            self.revision = self.revision.max(self.scene_revision);
        }
        self.fingerprint = None;
        self.handle = None;
        self.builder.clear();
        self.scene_fingerprint = None;
        self.scene_handle = None;
        self.scene_builder.clear();
        self.ui_fingerprint = None;
        self.ui_handle = None;
        self.ui_builder.clear();
        self.layered = Some(layered);
    }

    fn compose_single<B, F>(
        &mut self,
        device: &mut ProductDevice<B>,
        target_size: [u32; 2],
        canvas_layer: Option<EditorCanvasLayer>,
        ui_layers: &mut [EditorUiLayer<'_>],
        mut resolve: F,
    ) -> Result<EditorComposedFrame, String>
    where
        B: Backend,
        F: FnMut(&str) -> String,
    {
        self.metrics.begin_frame();
        self.metrics.layered = false;
        let clear = linear_clear(self.clear_color);
        let fingerprint = CompositionFingerprint::new(&canvas_layer, &ui_layers[..], target_size);
        let changed = self.fingerprint != Some(fingerprint) || self.handle.is_none();

        // Camera and viewport come from the canvas regardless of whether this
        // frame reprepares the packet; an idle frame still has to draw with the
        // retained scene camera, not the default one.
        let mut uniforms = composition_uniforms(canvas_layer.as_ref(), target_size);
        // Clear features the active backend cannot execute. Never draw with a
        // shadow or filter request that the device silently ignores.
        let dropped = drop_unsupported_features(
            &mut uniforms,
            device.supports_shadows(),
            device.supports_scene_filter(),
            device.info().backend,
        );
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

            if trace_enabled() {
                trace_packet("single", self.builder.packet());
            }
            let handle = prepare_product_scene(device, &mut self.builder)?;
            self.handle = Some(handle);
            self.fingerprint = Some(fingerprint);
            self.metrics.canvas_layers = canvas_layers;
            self.metrics.ui_layers = ui_frames.len() as u32;
            self.metrics.scene_packet_revision = self.revision;
            self.metrics.ui_packet_revision = self.revision;
            self.metrics.scene_packet_rebuilds = 1;
            self.metrics.ui_packet_rebuilds = 1;
            self.metrics.layer_count = 1;
            if trace_enabled() {
                trace_product_frame(
                    &uniforms,
                    ui_layers.len(),
                    dropped,
                    "single",
                    self.metrics.scene_packet_revision,
                    self.metrics.ui_packet_revision,
                    1,
                );
            }

            let handle = self.handle.expect("product handle was just prepared");
            let draw = device
                .draw(handle, &uniforms)
                .map_err(|error| format!("product draw failed: {error}"))?;
            self.metrics.record_draw(draw);

            return Ok(EditorComposedFrame {
                ui_layers: ui_frames,
                metrics: self.metrics,
            });
        }

        let handle = self
            .handle
            .ok_or_else(|| "compositor has no prepared product packet".to_string())?;
        let draw = device
            .draw(handle, &uniforms)
            .map_err(|error| format!("product draw failed: {error}"))?;
        self.metrics.layer_count = 1;
        self.metrics.record_draw(draw);

        Ok(EditorComposedFrame {
            ui_layers: Vec::new(),
            metrics: self.metrics,
        })
    }

    fn compose_layered<B, F>(
        &mut self,
        device: &mut ProductDevice<B>,
        target_size: [u32; 2],
        canvas_layer: Option<EditorCanvasLayer>,
        ui_layers: &mut [EditorUiLayer<'_>],
        mut resolve: F,
    ) -> Result<EditorComposedFrame, String>
    where
        B: Backend,
        F: FnMut(&str) -> String,
    {
        self.metrics.begin_frame();
        self.metrics.layered = true;
        let clear = linear_clear(self.clear_color);
        // Both packets draw with the scene camera. When there is no canvas the
        // UI packet is alone and its clear becomes the editor background.
        let mut uniforms = composition_uniforms(canvas_layer.as_ref(), target_size);
        let dropped = drop_unsupported_features(
            &mut uniforms,
            device.supports_shadows(),
            device.supports_scene_filter(),
            device.info().backend,
        );

        let scene_fingerprint = SceneFingerprint::new(&canvas_layer, target_size);
        let ui_fingerprint = UiFingerprint::new(&ui_layers[..], target_size);
        let scene_changed = canvas_layer.is_some()
            && (self.scene_fingerprint != Some(scene_fingerprint) || self.scene_handle.is_none());
        let ui_changed = self.ui_fingerprint != Some(ui_fingerprint) || self.ui_handle.is_none();

        self.metrics.canvas_layers = canvas_layer_count(canvas_layer.as_ref(), target_size);
        self.metrics.ui_layers = ui_layers.len() as u32;

        if scene_changed {
            self.scene_revision = self.scene_revision.wrapping_add(1).max(1);
            self.scene_builder.clear();
            let layer = canvas_layer
                .as_ref()
                .expect("a scene fingerprint change requires a canvas layer");
            match layer.output.as_ref() {
                SceneFrameOutput::Scene(frame) => {
                    let rect = layer.target_rect.normalized(target_size);
                    if rect[2] <= 0.0 || rect[3] <= 0.0 {
                        self.scene_builder
                            .append_empty(clear, self.scene_revision)?;
                    } else {
                        let full = rect == [0.0, 0.0, 1.0, 1.0];
                        self.scene_builder.append_scene_placed(
                            frame,
                            self.scene_revision,
                            if full { None } else { Some(clear) },
                            (!full).then_some(rect),
                        )?;
                    }
                }
                SceneFrameOutput::CpuPixels(_) => {
                    self.scene_builder
                        .append_empty(clear, self.scene_revision)?;
                    let rect = layer.target_rect.normalized(target_size);
                    append_cpu_output(
                        self.scene_builder.packet_mut(),
                        rect,
                        layer.output.as_ref(),
                        layer.source_size,
                    )?;
                }
            }
            if trace_enabled() {
                trace_packet("scene", self.scene_builder.packet());
            }
            let handle = prepare_product_scene(device, &mut self.scene_builder)?;
            self.scene_handle = Some(handle);
            self.scene_fingerprint = Some(scene_fingerprint);
            self.metrics.scene_packet_revision = self.scene_revision;
            self.metrics.scene_packet_rebuilds = 1;
        }

        let mut ui_frames = Vec::new();
        if ui_changed {
            self.ui_revision = self.ui_revision.wrapping_add(1).max(1);
            self.ui_builder.clear();
            // The retained UI packet is one clear plus the paint commands; the
            // compositor owns the background and the host never emits Clear.
            self.ui_builder.append_empty(clear, self.ui_revision)?;
            ui_frames.reserve(ui_layers.len());
            for layer in ui_layers.iter_mut() {
                let rect = layer.target_rect.normalized(target_size);
                let painted = layer.host.append_product_paint_frame(
                    self.ui_builder.packet_mut(),
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
            if trace_enabled() {
                trace_packet("ui", self.ui_builder.packet());
            }
            let handle = prepare_product_packet(device, &mut self.ui_builder, "UI")?;
            self.ui_handle = Some(handle);
            // Freeze the fingerprint after the append: atlas or image residency
            // can advance the host revision while the packet is produced, and
            // the next idle frame must match the produced commands.
            self.ui_fingerprint = Some(UiFingerprint::new(&ui_layers[..], target_size));
            self.metrics.ui_packet_revision = self.ui_revision;
            self.metrics.ui_packet_rebuilds = 1;
        }

        let mut handles = Vec::with_capacity(2);
        if canvas_layer.is_some() {
            if let Some(handle) = self.scene_handle {
                handles.push(handle);
            }
        }
        if let Some(handle) = self.ui_handle {
            handles.push(handle);
        }
        if handles.is_empty() {
            return Err("compositor has no prepared product packet".to_string());
        }
        self.metrics.layer_count = handles.len() as u32;
        if trace_enabled() && (scene_changed || ui_changed) {
            trace_product_frame(
                &uniforms,
                ui_layers.len(),
                dropped,
                "layered",
                self.metrics.scene_packet_revision,
                self.metrics.ui_packet_revision,
                handles.len(),
            );
        }

        let draw = device
            .draw_layers(&handles, &uniforms)
            .map_err(|error| format!("product draw failed: {error}"))?;
        self.metrics.record_draw(draw);

        Ok(EditorComposedFrame {
            ui_layers: ui_frames,
            metrics: self.metrics,
        })
    }
}

fn linear_clear(color: [u8; 4]) -> [f32; 4] {
    linear_rgba(color)
}

/// Camera, lighting, shadows and canvas viewport shared by every packet.
fn composition_uniforms(
    canvas: Option<&EditorCanvasLayer>,
    target_size: [u32; 2],
) -> FrameUniforms {
    let mut uniforms = FrameUniforms::default();
    uniforms.viewport = [0.0, 0.0, 1.0, 1.0];
    if let Some(layer) = canvas {
        if let SceneFrameOutput::Scene(frame) = layer.output.as_ref() {
            uniforms = scene_frame_uniforms(frame);
            uniforms.viewport = layer.target_rect.normalized(target_size);
        }
    }
    uniforms
}

fn canvas_layer_count(canvas: Option<&EditorCanvasLayer>, target_size: [u32; 2]) -> u32 {
    let Some(layer) = canvas else {
        return 0;
    };
    let rect = layer.target_rect.normalized(target_size);
    if rect[2] <= 0.0 || rect[3] <= 0.0 {
        return 0;
    }
    match layer.output.as_ref() {
        SceneFrameOutput::Scene(_) => 1,
        SceneFrameOutput::CpuPixels(pixels) => u32::from(!pixels.is_empty()),
    }
}

fn trace_enabled() -> bool {
    std::env::var_os("RAF_AGB_TRACE_FRAME").is_some()
}

/// Graphics features that were requested by the frame but removed because the
/// active backend cannot execute them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DroppedFeatures {
    shadows: bool,
    scene_filter: bool,
}

/// Per-process warning gates. These live beside the compositor that performs
/// the capability check; each feature warns exactly once per process.
static SHADOWS_UNSUPPORTED_WARNED: Once = Once::new();
static SCENE_FILTER_UNSUPPORTED_WARNED: Once = Once::new();

/// Removes features the device cannot execute before the draw and reports the
/// first occurrence per process. The frame never pretends the feature ran.
fn drop_unsupported_features(
    uniforms: &mut FrameUniforms,
    supports_shadows: bool,
    supports_scene_filter: bool,
    backend: &'static str,
) -> DroppedFeatures {
    let mut dropped = DroppedFeatures::default();
    if uniforms.shadows.is_some() && !supports_shadows {
        uniforms.shadows = None;
        dropped.shadows = true;
        warn_unsupported_feature(
            &SHADOWS_UNSUPPORTED_WARNED,
            "shadows",
            "graphics.unsupported.shadows",
            backend,
        );
    }
    if uniforms.scene_filter && !supports_scene_filter {
        uniforms.scene_filter = false;
        dropped.scene_filter = true;
        warn_unsupported_feature(
            &SCENE_FILTER_UNSUPPORTED_WARNED,
            "scene_filter",
            "graphics.unsupported.scene_filter",
            backend,
        );
    }
    dropped
}

fn warn_unsupported_feature(
    once: &'static Once,
    feature: &'static str,
    code: &'static str,
    backend: &'static str,
) {
    once.call_once(|| {
        let detail = format!("{backend} does not support {feature}; the frame draws without it");
        tracing::warn!(
            feature = feature,
            code = code,
            detail = %detail,
            "graphics feature dropped"
        );
    });
}

/// One retained packet summary for the live native host.
fn trace_packet(role: &'static str, packet: &ProductPacket) {
    let mut clear = None;
    let mut sky = None;
    let mut background = 0;
    let mut paints = 0;
    for command in &packet.commands {
        match command {
            Command::Clear(color) => clear = Some(*color),
            Command::SceneBackground(_) => background += 1,
            Command::DrawSky(backdrop) => sky = Some(*backdrop),
            Command::Paint(_) | Command::StyledPaint { .. } => paints += 1,
            Command::Cad { .. } | Command::CadStyled { .. } => paints += 1,
            _ => {}
        }
    }
    tracing::info!(
        role,
        commands = packet.commands.len(),
        meshes = packet.meshes.len(),
        textures = packet.textures.len(),
        paints,
        background,
        ?clear,
        sky = ?sky.map(|sky| (
            sky.zenith,
            sky.horizon,
            sky.ground,
            sky.haze,
            sky.sun_disk_cos,
            sky.halo,
            sky.halo_cos,
            sky.stars_density,
            sky.stars_intensity,
        )),
        "agb.compose.packet"
    );
}

/// Bounded composition trace for the live native host.
///
/// Enable with `RAF_AGB_TRACE_FRAME=1`. It reports the chosen path, both packet
/// revisions and the camera, lighting style, and viewport the shaders receive,
/// which is the first thing to check when the editor and a headless capture
/// disagree.
fn trace_product_frame(
    uniforms: &FrameUniforms,
    ui_layers: usize,
    dropped: DroppedFeatures,
    path: &'static str,
    scene_revision: u64,
    ui_revision: u64,
    layers: usize,
) {
    tracing::info!(
        path,
        scene_revision,
        ui_revision,
        layer_count = layers,
        ui_layers,
        viewport = ?uniforms.viewport,
        eye = ?uniforms.camera.world_position,
        view_projection = ?uniforms.camera.view_projection,
        toward_light = ?uniforms.light.toward_light,
        light_color = ?uniforms.light.color,
        exposure_ev = uniforms.environment.exposure_ev,
        ambient = uniforms.environment.ambient,
        ambient_tint = ?uniforms.environment.ambient_tint,
        authored = uniforms.environment.authored,
        improved = uniforms.environment.improved,
        toon_steps = uniforms.environment.toon_steps,
        toon_softness = uniforms.environment.toon_softness,
        shadows = uniforms.shadows.is_some(),
        shadow_dropped = dropped.shadows,
        filter_dropped = dropped.scene_filter,
        "agb.compose.trace"
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompositionFingerprint {
    scene: SceneFingerprint,
    ui: UiFingerprint,
}

impl CompositionFingerprint {
    fn new(
        canvas: &Option<EditorCanvasLayer>,
        ui_layers: &[EditorUiLayer<'_>],
        target_size: [u32; 2],
    ) -> Self {
        Self {
            scene: SceneFingerprint::new(canvas, target_size),
            ui: UiFingerprint::new(ui_layers, target_size),
        }
    }
}

/// Scene identity: a new canvas Arc, kind, source size, placement or target
/// size. Camera-only changes never advance it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SceneFingerprint {
    target_size: [u32; 2],
    canvas: CanvasFingerprint,
}

impl SceneFingerprint {
    fn new(canvas: &Option<EditorCanvasLayer>, target_size: [u32; 2]) -> Self {
        Self {
            target_size,
            canvas: CanvasFingerprint::new(canvas),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CanvasFingerprint {
    None,
    Scene {
        content: u64,
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

impl CanvasFingerprint {
    fn new(canvas: &Option<EditorCanvasLayer>) -> Self {
        let Some(layer) = canvas else {
            return Self::None;
        };
        let rect = [
            layer.target_rect.x,
            layer.target_rect.y,
            layer.target_rect.width,
            layer.target_rect.height,
        ];
        match layer.output.as_ref() {
            SceneFrameOutput::Scene(frame) => Self::Scene {
                content: frame.commands.content_hash(),
                rect,
                source_size: [frame.width, frame.height],
            },
            SceneFrameOutput::CpuPixels(pixels) => Self::Cpu {
                ptr: Arc::as_ptr(&layer.output) as *const () as usize,
                len: pixels.len(),
                rect,
                source_size: layer.source_size,
            },
        }
    }
}

/// UI identity: paint revisions, target rects, logical size, raster scale and
/// the physical target they resolve against. Scene changes never advance it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UiFingerprint {
    target_size: [u32; 2],
    hash: u64,
}

impl UiFingerprint {
    fn new(ui_layers: &[EditorUiLayer<'_>], target_size: [u32; 2]) -> Self {
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
        Self { target_size, hash }
    }
}

fn fnv(hash: u64, value: u64) -> u64 {
    (hash ^ value).wrapping_mul(0x0000_0100_0000_01b3)
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    use glam::{Mat4, Vec3};
    use raf_agb::product::{Layer, ProductDevice};
    use raf_agb::{
        AdapterInfo, Backend, Error as AgbError, Frame, FrameMetrics, FramePlan, Output,
        Result as AgbResult, Workload,
    };
    use raf_ui::{StudioUiPalette, UiNode, UiNodeKind};

    use super::*;
    use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;
    use crate::api_graphic_basic::command_list::BasicCommandList;
    use crate::api_graphic_basic::mesh::{BasicMesh, BasicVertex};
    use crate::api_graphic_basic::pipeline::BasicPipelineKind;
    use crate::api_graphic_basic::ui_surface::{DirectUiSurfaceHost, UiSurface};
    use crate::scene_renderer::{FrameStats, SceneRenderFrame};

    #[derive(Default)]
    struct ProbeState {
        next: u32,
        uploads: u32,
        draws: Vec<Vec<u32>>,
        clears: Vec<[f32; 4]>,
    }

    /// Records translated residents and the selected clear without a GPU.
    #[derive(Clone)]
    struct LayerProbe {
        info: AdapterInfo,
        state: Rc<RefCell<ProbeState>>,
        layered: bool,
    }

    impl Backend for LayerProbe {
        type Resident = u32;

        fn info(&self) -> &AdapterInfo {
            &self.info
        }

        fn supports_layers(&self) -> bool {
            self.layered
        }

        fn configure(&mut self, _: &FramePlan) -> AgbResult<()> {
            Ok(())
        }

        fn upload(&mut self, _: &Workload) -> AgbResult<u32> {
            let mut state = self.state.borrow_mut();
            state.uploads += 1;
            state.next += 1;
            Ok(state.next)
        }

        fn draw(
            &mut self,
            resident: &u32,
            plan: &FramePlan,
            frame: &Frame,
        ) -> AgbResult<FrameMetrics> {
            self.draw_layers(&[Layer { resident, scene: true }], plan, frame)
        }

        fn draw_layers(
            &mut self,
            layers: &[Layer<'_, u32>],
            _: &FramePlan,
            frame: &Frame,
        ) -> AgbResult<FrameMetrics> {
            if layers.len() > 1 && !self.layered {
                return Err(AgbError::Unsupported("layered composition"));
            }
            let mut state = self.state.borrow_mut();
            let ids: Vec<u32> = layers.iter().map(|layer| *layer.resident).collect();
            state.draws.push(ids.clone());
            state.clears.push(frame.clear);
            Ok(FrameMetrics {
                draw_calls: ids.len() as u32,
                submits: 1,
                ..Default::default()
            })
        }

        fn wait(&mut self) -> AgbResult<()> {
            Ok(())
        }

        fn capture(&mut self) -> AgbResult<Vec<u8>> {
            Err(AgbError::Invalid("the layer probe has no pixels"))
        }
    }

    fn open_probe(layered: bool) -> (ProductDevice<LayerProbe>, Rc<RefCell<ProbeState>>) {
        let state = Rc::new(RefCell::new(ProbeState::default()));
        let probe = LayerProbe {
            info: AdapterInfo {
                backend: "layer-probe",
                name: "CPU".into(),
                driver: String::new(),
                hardware: false,
                max_texture: 16384,
                max_buffer: u32::MAX.into(),
                gpu_timestamps: false,
            },
            state: Rc::clone(&state),
            layered,
        };
        let mut device = ProductDevice::new(probe, GraphicsMemoryBudget::potato()).unwrap();
        device.configure([320, 200], Output::Capture).unwrap();
        (device, state)
    }

    fn retained_canvas(size: [u32; 2]) -> Arc<SceneFrameOutput> {
        retained_canvas_with_clear(size, [12, 18, 24, 255])
    }

    fn retained_canvas_with_clear(size: [u32; 2], clear: [u8; 4]) -> Arc<SceneFrameOutput> {
        let mut commands = BasicCommandList::new();
        commands.clear(clear);
        let mesh = Arc::new(BasicMesh::new(
            vec![
                BasicVertex {
                    position: Vec3::ZERO,
                    normal: Vec3::Y,
                    uv: [0.0, 0.0],
                },
                BasicVertex {
                    position: Vec3::X,
                    normal: Vec3::Y,
                    uv: [1.0, 0.0],
                },
                BasicVertex {
                    position: Vec3::Y,
                    normal: Vec3::Y,
                    uv: [0.0, 1.0],
                },
            ],
            vec![0, 1, 2],
        ));
        let mesh_id = commands.register_mesh(mesh);
        commands.set_pipeline(BasicPipelineKind::FlatColor);
        commands.draw_mesh(mesh_id, Mat4::IDENTITY, [200, 40, 30, 255]);
        Arc::new(SceneFrameOutput::Scene(Arc::new(SceneRenderFrame {
            commands,
            view_proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            width: size[0],
            height: size[1],
            texture_cache_budget_bytes: 0,
            stats: FrameStats::default(),
        })))
    }

    fn ui_host() -> DirectUiSurfaceHost {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_style(StudioUiPalette::IndustrialDark.panel_style());
        DirectUiSurfaceHost::new(
            UiSurface::new("layers", StudioUiPalette::IndustrialDark, root),
            [9, 9, 10, 255],
        )
    }

    fn compose(
        compositor: &mut NativeEditorCompositor,
        device: &mut ProductDevice<LayerProbe>,
        canvas: Option<Arc<SceneFrameOutput>>,
        ui: &mut DirectUiSurfaceHost,
        size: [u32; 2],
    ) -> EditorComposedFrame {
        let canvas_layer = canvas.map(|output| EditorCanvasLayer {
            output,
            source_size: size,
            target_rect: CanvasTargetRect::full(size),
        });
        let mut layers = [EditorUiLayer {
            host: ui,
            target_rect: CanvasTargetRect::full(size),
            logical_size: [160, 100],
            raster_scale: 1.0,
        }];
        compositor
            .compose_layers(
                device,
                size,
                canvas_layer,
                &mut layers,
                |key| key.to_string(),
            )
            .expect("composition")
    }

    #[test]
    fn ui_revision_keeps_the_scene_packet_and_scene_revision_keeps_the_ui_packet() {
        let (mut device, state) = open_probe(true);
        let mut ui = ui_host();
        let mut compositor = NativeEditorCompositor::empty([9, 9, 10, 255]);
        let size = [320, 200];
        let canvas = retained_canvas(size);

        let first = compose(
            &mut compositor,
            &mut device,
            Some(canvas.clone()),
            &mut ui,
            size,
        );
        assert!(first.metrics.layered);
        assert_eq!(first.metrics.scene_packet_rebuilds, 1);
        assert_eq!(first.metrics.ui_packet_rebuilds, 1);
        assert_eq!(first.metrics.layer_count, 2);
        assert_eq!(first.metrics.command_submits, 1);
        assert_eq!(state.borrow().uploads, 2);
        let draw = state.borrow().draws.last().unwrap().clone();
        assert_eq!(draw.len(), 2);
        let first_scene = draw[0];
        let first_ui = draw[1];
        assert_ne!(first_scene, first_ui);
        let scene_revision = first.metrics.scene_packet_revision;
        let ui_revision = first.metrics.ui_packet_revision;

        let idle = compose(
            &mut compositor,
            &mut device,
            Some(canvas.clone()),
            &mut ui,
            size,
        );
        assert_eq!(idle.metrics.scene_packet_rebuilds, 0);
        assert_eq!(idle.metrics.ui_packet_rebuilds, 0);
        assert_eq!(idle.metrics.scene_packet_revision, scene_revision);
        assert_eq!(idle.metrics.ui_packet_revision, ui_revision);
        assert_eq!(state.borrow().uploads, 2);
        assert_eq!(
            state.borrow().draws.last().unwrap().clone(),
            vec![first_scene, first_ui]
        );

        ui.surface_mut();
        let ui_frame = compose(
            &mut compositor,
            &mut device,
            Some(canvas.clone()),
            &mut ui,
            size,
        );
        assert_eq!(ui_frame.metrics.scene_packet_rebuilds, 0);
        assert_eq!(ui_frame.metrics.ui_packet_rebuilds, 1);
        assert_eq!(ui_frame.metrics.scene_packet_revision, scene_revision);
        assert!(ui_frame.metrics.ui_packet_revision > ui_revision);
        assert_eq!(state.borrow().uploads, 3);
        let draw = state.borrow().draws.last().unwrap().clone();
        assert_eq!(draw[0], first_scene, "the scene packet is not re-uploaded");
        assert_ne!(draw[1], first_ui);
        let second_ui = draw[1];

        let replacement = retained_canvas_with_clear(size, [30, 18, 24, 255]);
        let scene_frame = compose(
            &mut compositor,
            &mut device,
            Some(replacement),
            &mut ui,
            size,
        );
        assert_eq!(scene_frame.metrics.scene_packet_rebuilds, 1);
        assert_eq!(scene_frame.metrics.ui_packet_rebuilds, 0);
        assert!(scene_frame.metrics.scene_packet_revision > scene_revision);
        assert_eq!(
            scene_frame.metrics.ui_packet_revision,
            ui_frame.metrics.ui_packet_revision
        );
        assert_eq!(state.borrow().uploads, 4);
        let draw = state.borrow().draws.last().unwrap().clone();
        assert_ne!(draw[0], first_scene);
        assert_eq!(draw[1], second_ui, "the UI packet is not re-uploaded");
    }

    #[test]
    fn layered_frame_without_a_canvas_draws_only_the_ui_packet() {
        let (mut device, state) = open_probe(true);
        let mut ui = ui_host();
        let mut compositor = NativeEditorCompositor::empty([9, 9, 10, 255]);
        let size = [320, 200];
        let canvas = retained_canvas(size);
        compose(
            &mut compositor,
            &mut device,
            Some(canvas),
            &mut ui,
            size,
        );
        let ui_resident = state.borrow().draws.last().unwrap()[1];

        let ui_only = compose(&mut compositor, &mut device, None, &mut ui, size);
        assert_eq!(ui_only.metrics.layer_count, 1);
        assert_eq!(ui_only.metrics.canvas_layers, 0);
        assert_eq!(ui_only.metrics.scene_packet_rebuilds, 0);
        assert_eq!(ui_only.metrics.ui_packet_rebuilds, 0);
        assert_eq!(
            state.borrow().draws.last().unwrap().clone(),
            vec![ui_resident]
        );
        assert_eq!(
            state.borrow().clears.last().copied(),
            Some(linear_rgba([9, 9, 10, 255])),
            "the UI packet owns the editor background when there is no scene"
        );
    }

    #[test]
    fn release_canvas_forces_both_packets_on_a_reopened_device() {
        let (mut device, _) = open_probe(true);
        let mut ui = ui_host();
        let mut compositor = NativeEditorCompositor::empty([9, 9, 10, 255]);
        let size = [320, 200];
        let canvas = retained_canvas(size);
        let first = compose(
            &mut compositor,
            &mut device,
            Some(canvas.clone()),
            &mut ui,
            size,
        );
        assert_eq!(first.metrics.layer_count, 2);

        compositor.release_canvas();
        let (mut reopened, state) = open_probe(true);
        let second = compose(&mut compositor, &mut reopened, Some(canvas), &mut ui, size);
        assert_eq!(second.metrics.scene_packet_rebuilds, 1);
        assert_eq!(second.metrics.ui_packet_rebuilds, 1);
        assert_eq!(second.metrics.layer_count, 2);
        assert_eq!(second.metrics.command_submits, 1);
        assert_eq!(state.borrow().uploads, 2);
    }

    #[test]
    fn devices_without_layers_keep_the_single_packet_path() {
        let (mut device, state) = open_probe(false);
        let mut ui = ui_host();
        let mut compositor = NativeEditorCompositor::empty([9, 9, 10, 255]);
        let size = [320, 200];
        let canvas = retained_canvas(size);
        let first = compose(
            &mut compositor,
            &mut device,
            Some(canvas.clone()),
            &mut ui,
            size,
        );
        assert!(!first.metrics.layered);
        assert_eq!(first.metrics.layer_count, 1);
        assert_eq!(state.borrow().uploads, 1, "one combined packet");
        assert_eq!(state.borrow().draws.last().unwrap().len(), 1);

        ui.surface_mut();
        let second = compose(&mut compositor, &mut device, Some(canvas), &mut ui, size);
        assert_eq!(second.metrics.layer_count, 1);
        assert_eq!(
            state.borrow().uploads,
            2,
            "a UI change rebuilds the combined packet"
        );
        assert_eq!(second.metrics.scene_packet_rebuilds, 1);
        assert_eq!(second.metrics.ui_packet_rebuilds, 1);
    }

    fn shadow_uniforms() -> raf_agb::product::ShadowUniforms {
        raf_agb::product::ShadowUniforms {
            light_view_projection: glam::Mat4::IDENTITY,
            bias: 0.001,
            resolution: 512,
            depth_key: 7,
            distance: 32.0,
            strength: 1.0,
            filter_taps: 1,
            slope_bias_per_texel: 0.0,
        }
    }

    #[test]
    fn unsupported_features_are_cleared_and_reported() {
        let mut uniforms = FrameUniforms::default();
        uniforms.shadows = Some(shadow_uniforms());
        uniforms.scene_filter = true;
        let dropped = drop_unsupported_features(&mut uniforms, false, false, "recorder");
        assert!(dropped.shadows);
        assert!(dropped.scene_filter);
        assert!(uniforms.shadows.is_none());
        assert!(!uniforms.scene_filter);
    }

    #[test]
    fn supported_features_are_retained() {
        let mut uniforms = FrameUniforms::default();
        uniforms.shadows = Some(shadow_uniforms());
        uniforms.scene_filter = true;
        let dropped = drop_unsupported_features(&mut uniforms, true, true, "vulkan");
        assert!(!dropped.shadows);
        assert!(!dropped.scene_filter);
        assert!(uniforms.shadows.is_some());
        assert!(uniforms.scene_filter);
    }
}
