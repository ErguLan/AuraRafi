//! Native placement/input wrapper for one retained RafUI document.
//!
//! This type never allocates a foreign UI texture or asks another toolkit to
//! paint it. The surface is encoded
//! directly into the editor swapchain by ApiGraphicBasic.

use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, UiDispatchedAction, UiSurface,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::UiRect;

use crate::editor_layout::EditorRect;

pub struct NativeRetainedSurface {
    owner: InputOwner,
    rect: EditorRect,
    host: DirectUiSurfaceHost,
}

impl NativeRetainedSurface {
    pub fn new(
        region: InputRegionId,
        rect: EditorRect,
        surface: UiSurface,
        graphics: &NativeGraphicsContext<'_>,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            owner: InputOwner::RetainedUi(region),
            rect,
            host: graphics.create_ui_host(surface, clear_color),
        }
    }

    pub fn owner(&self) -> InputOwner {
        self.owner
    }

    pub fn rect(&self) -> EditorRect {
        self.rect
    }

    pub fn set_rect(&mut self, rect: EditorRect) {
        self.rect = rect;
    }

    pub fn set_surface(&mut self, surface: UiSurface) {
        self.host.set_surface(surface);
    }

    pub fn host(&self) -> &DirectUiSurfaceHost {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut DirectUiSurfaceHost {
        &mut self.host
    }

    pub fn register_embedded_png(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        let decoded = image::load_from_memory(bytes)
            .map_err(|error| format!("Unable to decode retained UI image '{key}': {error}"))?
            .to_rgba8();
        self.host.images_mut().insert_rgba(
            key.to_string(),
            [decoded.width(), decoded.height()],
            decoded.into_raw(),
        )
    }

    pub fn process_input<F>(
        &mut self,
        native_input: &NativeUiInputBridge,
        router: &mut InputRouter,
        raster_scale: f32,
        resolve: F,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        let size = self.rect.logical_size();
        self.host.process_routed_input(
            size,
            raster_scale,
            resolve,
            native_input,
            router,
            self.owner,
            UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        )
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }
}
