//! Native Electronics canvas boundary.
//!
//! This adapter connects the live document controller to the neutral CAD frame
//! builder and the shared ApiGraphicBasic runtime. Document mutation and input
//! ownership live in `electronics_controller`; this type only owns the
//! presentation target and retained CAD frame.

use std::sync::Arc;

use glam::Vec2;
use raf_electronics::CadScene;
use raf_render::api_graphic_basic::cad_surface::{
    CadSurfaceFrame, CadSurfaceHitRegion, CadSurfaceOptions,
};
use raf_render::api_graphic_basic::cad_surface_host::DirectCadSurfaceHost;
use raf_render::api_graphic_basic::ui_surface::NativeGraphicsContext;
use raf_render::api_graphic_basic::{CanvasTargetRect, EditorCanvasLayer};
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime};

pub struct NativeElectronicsCanvas {
    surface: GraphicsSurfaceKind,
    host: DirectCadSurfaceHost,
    frame_revision: u64,
    size: [u32; 2],
}

impl NativeElectronicsCanvas {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        clear_color: [u8; 4],
        surface: GraphicsSurfaceKind,
    ) -> Self {
        debug_assert!(matches!(
            surface,
            GraphicsSurfaceKind::SchematicCanvas | GraphicsSurfaceKind::PcbCanvas
        ));
        Self {
            surface,
            host: graphics.create_cad_host(clear_color),
            frame_revision: 0,
            size: [1, 1],
        }
    }

    pub fn surface(&self) -> GraphicsSurfaceKind {
        self.surface
    }

    pub fn set_surface(&mut self, surface: GraphicsSurfaceKind) {
        self.surface = surface;
    }

    pub fn rebuild(
        &mut self,
        scene: &CadScene,
        size: [u32; 2],
        options: CadSurfaceOptions,
    ) -> &CadSurfaceFrame {
        self.size = [size[0].max(1), size[1].max(1)];
        self.frame_revision = self.frame_revision.wrapping_add(1).max(1);
        self.host
            .rebuild(scene, self.size[0], self.size[1], options)
    }

    pub fn frame(&self) -> Option<&CadSurfaceFrame> {
        self.host.frame()
    }

    pub fn hit_test(&self, point: Vec2) -> Option<&CadSurfaceHitRegion> {
        self.host.hit_test(point)
    }

    pub fn frame_revision(&self) -> u64 {
        self.frame_revision
    }

    /// Renders the retained CAD frame into the shared editor canvas layer.
    /// The compositor, rather than this adapter, owns the final swapchain
    /// presentation so Game and Electronics follow one low-overhead path.
    pub fn render_layer(
        &mut self,
        runtime: &mut RenderRuntime,
        target_rect: CanvasTargetRect,
    ) -> Option<EditorCanvasLayer> {
        let frame = self.host.frame()?;
        runtime.activate_surface(self.surface);
        let output = runtime.render_scene_frame(&frame.frame);
        Some(EditorCanvasLayer {
            output: Arc::new(output),
            source_size: [frame.frame.width, frame.frame.height],
            target_rect,
        })
    }
}
