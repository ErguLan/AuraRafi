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
    cached_layer: Option<CachedCadLayer>,
}

struct CachedCadLayer {
    frame_revision: u64,
    device_generation: u64,
    surface_generation: u64,
    layer: EditorCanvasLayer,
}

impl CachedCadLayer {
    fn reuse(
        &self,
        frame_revision: u64,
        device_generation: u64,
        surface_generation: u64,
        target_rect: CanvasTargetRect,
    ) -> Option<EditorCanvasLayer> {
        if self.frame_revision != frame_revision
            || self.device_generation != device_generation
            || self.surface_generation != surface_generation
        {
            return None;
        }
        let mut layer = self.layer.clone();
        layer.target_rect = target_rect;
        Some(layer)
    }
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
            cached_layer: None,
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
        runtime.activate_surface(self.surface);
        let snapshot = runtime.snapshot();
        if let Some(layer) = self.cached_layer.as_ref().and_then(|cached| {
            cached.reuse(
                self.frame_revision,
                snapshot.device_generation,
                snapshot.surface_generation,
                target_rect,
            )
        }) {
            return Some(layer);
        }
        let frame = self.host.frame()?;
        let output = runtime.render_scene_frame(&frame.frame);
        let layer = EditorCanvasLayer {
            output: Arc::new(output),
            source_size: [frame.frame.width, frame.frame.height],
            target_rect,
        };
        self.cached_layer = Some(CachedCadLayer {
            frame_revision: self.frame_revision,
            device_generation: snapshot.device_generation,
            surface_generation: snapshot.surface_generation,
            layer: layer.clone(),
        });
        Some(layer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_render::api_graphic_basic::device::SceneFrameOutput;

    #[test]
    fn electronics_ui_frames_reuse_the_executed_cad_output() {
        let cached = CachedCadLayer {
            frame_revision: 2,
            device_generation: 3,
            surface_generation: 4,
            layer: EditorCanvasLayer {
                output: Arc::new(SceneFrameOutput::CpuPixels(vec![0; 16])),
                source_size: [2, 2],
                target_rect: CanvasTargetRect::new(0, 0, 2, 2),
            },
        };
        let moved = CanvasTargetRect::new(52, 80, 2, 2);
        let reused = cached.reuse(2, 3, 4, moved).unwrap();
        assert!(Arc::ptr_eq(&cached.layer.output, &reused.output));
        assert_eq!(reused.target_rect, moved);
        assert!(
            cached.reuse(5, 3, 4, moved).is_none(),
            "CAD geometry/camera changed"
        );
        assert!(
            cached.reuse(2, 5, 4, moved).is_none(),
            "graphics device changed"
        );
        assert!(
            cached.reuse(2, 3, 5, moved).is_none(),
            "another surface used the shared target"
        );
    }
}
