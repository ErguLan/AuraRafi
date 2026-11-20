//! Canvas and retained UI layer definitions composed by the native editor.

use std::sync::Arc;

use crate::api_graphic_basic::canvas_presenter::CanvasTargetRect;
use crate::api_graphic_basic::device::SceneFrameOutput;
use crate::api_graphic_basic::ui_surface::DirectUiSurfaceHost;

#[derive(Clone)]
pub struct EditorCanvasLayer {
    pub output: Arc<SceneFrameOutput>,
    pub source_size: [u32; 2],
    pub target_rect: CanvasTargetRect,
}

pub struct EditorUiLayer<'a> {
    pub host: &'a mut DirectUiSurfaceHost,
    pub target_rect: CanvasTargetRect,
    pub logical_size: [u32; 2],
    pub raster_scale: f32,
}
