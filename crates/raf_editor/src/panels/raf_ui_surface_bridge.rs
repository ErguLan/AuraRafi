//! Compatibility name for native retained surfaces.
//!
//! This module exists so downstream editor features can migrate their host
//! imports one at a time. It is not a legacy widget bridge: the implementation is the
//! direct AGB/Winit placement wrapper.

use raf_core::{InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    NativeGraphicsContext, NativeUiInputBridge, UiDispatchedAction, UiSurface,
};
use raf_render::api_graphic_basic::EditorUiLayer;

use crate::editor_layout::EditorRect;
use crate::native_surface::NativeRetainedSurface;

pub struct RafUiSurfaceBridge {
    surface: NativeRetainedSurface,
}

impl RafUiSurfaceBridge {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        region: InputRegionId,
        rect: EditorRect,
        surface: UiSurface,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            surface: NativeRetainedSurface::new(region, rect, surface, graphics, clear_color),
        }
    }

    pub fn set_rect(&mut self, rect: EditorRect) {
        self.surface.set_rect(rect);
    }

    pub fn set_surface(&mut self, surface: UiSurface) {
        self.surface.set_surface(surface);
    }

    pub fn register_embedded_png(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        self.surface.register_embedded_png(key, bytes)
    }

    pub fn process_input<F>(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
        raster_scale: f32,
        resolve: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        self.surface
            .process_input(input, router, raster_scale, resolve)
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        self.surface.compositor_layer(scale_factor, target_size)
    }
}
