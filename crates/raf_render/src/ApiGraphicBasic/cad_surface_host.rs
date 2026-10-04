//! Direct retained CAD surface host for native presentation.

use glam::Vec2;
use raf_electronics::CadScene;

use crate::api_graphic_basic::canvas_presenter::DirectSceneSurfaceHost;
use crate::api_graphic_basic::device::BasicDevice;

use super::cad_surface::{
    build_cad_surface_frame, CadSurfaceFrame, CadSurfaceHitRegion, CadSurfaceOptions,
};

/// Retained CAD frame plus the presentation target that draws it.
///
/// The host owns the last built frame so an idle surface does no work: geometry
/// is recorded only when a caller asks for a rebuild, and presentation reuses
/// whatever frame is already resident.
pub struct DirectCadSurfaceHost {
    scene_host: DirectSceneSurfaceHost,
    frame: Option<CadSurfaceFrame>,
}

impl DirectCadSurfaceHost {
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            scene_host: DirectSceneSurfaceHost::new(device, color_format, clear_color),
            frame: None,
        }
    }

    /// Records a new CAD frame for the given physical target size.
    pub fn rebuild(
        &mut self,
        scene: &CadScene,
        width: u32,
        height: u32,
        options: CadSurfaceOptions,
    ) -> &CadSurfaceFrame {
        self.frame = Some(build_cad_surface_frame(scene, width, height, options));
        self.frame.as_ref().expect("CAD frame was just built")
    }

    /// The retained frame, if one was ever built.
    pub fn frame(&self) -> Option<&CadSurfaceFrame> {
        self.frame.as_ref()
    }

    /// Retained AGB pick regions of the last built frame.
    ///
    /// This is the legacy CAD pick path. Editing gestures belong to
    /// `raf_electronics::cad_interaction::pick_editable`, which also refuses to
    /// let a painted `NetLabel`, `Airwire` or `DrcMarker` win a click over the
    /// geometry underneath, and which resolves selection back to the document.
    /// Hosts that resolve picking there can build with
    /// `CadSurfaceOptions::collect_hit_regions = false` and stop paying for the
    /// per-object string clones; kept here because it is public API of
    /// `raf_render`.
    pub fn hit_test(&self, point: Vec2) -> Option<&CadSurfaceHitRegion> {
        self.frame.as_ref()?.hit_test(point)
    }

    /// Renders the retained frame with an explicit `BasicDevice`.
    ///
    /// The editor does not use this path: `NativeElectronicsCanvas::render_layer`
    /// hands the same `SceneRenderFrame` to the shared `RenderRuntime`, so
    /// Schematic, PCB, Scene and RafUI all present through one executor. This
    /// remains for a host that owns a concrete `BasicDevice` and wants to draw
    /// the CAD frame without the shared runtime.
    pub fn render(
        &mut self,
        basic_device: &mut BasicDevice,
        presentation_device: &wgpu::Device,
        presentation_queue: &wgpu::Queue,
        target: &wgpu::TextureView,
    ) -> bool {
        let Some(frame) = self.frame.as_ref() else {
            return false;
        };
        self.scene_host.render_frame(
            basic_device,
            presentation_device,
            presentation_queue,
            target,
            &frame.frame,
        );
        true
    }

    /// Presents an output already rendered by the shared RenderRuntime.
    /// Electronics stays behind the same native canvas boundary as Game and
    /// does not need to access a concrete BasicDevice.
    pub fn present_output(
        &mut self,
        presentation_device: &wgpu::Device,
        presentation_queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        output: crate::api_graphic_basic::device::SceneFrameOutput,
        source_size: [u32; 2],
    ) {
        self.scene_host.present_output(
            presentation_device,
            presentation_queue,
            target,
            output,
            source_size,
        );
    }
}
