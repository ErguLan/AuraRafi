//! Presentation composition helpers: surfaced layer drawing and the
//! Electronics canvas cache.

use raf_render::api_graphic_basic::ui_surface::{NativeUiWindowHost, PresentationHealth};

use crate::electronics_controller::NativeElectronicsEditor;
use crate::native_electronics::NativeElectronicsCanvas;

/// Reports an exhausted native presentation surface once per failure episode.
///
/// `NativeUiWindowHost` already reconfigures and retries transient surface
/// faults. The editor only needs a diagnostic when recovery is exhausted, and
/// it must not repeat that line for every redraw while the surface stays down.
pub(super) fn observe_editor_presentation<T, E: std::fmt::Debug>(
    host: &NativeUiWindowHost,
    result: &Result<T, E>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static REPORTED: AtomicBool = AtomicBool::new(false);
    match result {
        Ok(_) => REPORTED.store(false, Ordering::Relaxed),
        Err(error) => {
            if let PresentationHealth::Failed { fault } = host.presentation_health() {
                if !REPORTED.swap(true, Ordering::Relaxed) {
                    tracing::error!(
                        fault = fault.label(),
                        ?error,
                        "native editor presentation failed"
                    );
                }
            }
        }
    }
}

pub(super) fn sync_electronics_canvas(
    editor: Option<&mut NativeElectronicsEditor>,
    canvas: Option<&mut NativeElectronicsCanvas>,
    frame_key: &mut Option<(u64, [u32; 2])>,
    logical_rect: crate::editor_layout::EditorRect,
    target_rect: raf_render::api_graphic_basic::CanvasTargetRect,
    palette: raf_render::api_graphic_basic::ui_surface::StudioUiPalette,
    scale_factor: f32,
) {
    let Some(editor) = editor else {
        return;
    };
    let key = (
        editor.revision(),
        [target_rect.width.max(1), target_rect.height.max(1)],
    );
    if *frame_key == Some(key) {
        return;
    }
    let Some(canvas) = canvas else {
        return;
    };
    canvas.set_surface(match editor.active_surface() {
        raf_electronics::CadSurfaceKind::Schematic => {
            raf_render::bridge::GraphicsSurfaceKind::SchematicCanvas
        }
        raf_electronics::CadSurfaceKind::Pcb => raf_render::bridge::GraphicsSurfaceKind::PcbCanvas,
    });
    let options = editor.render_options(logical_rect, palette, scale_factor);
    canvas.rebuild(editor.scene(), key.1, options);
    *frame_key = Some(key);
}
