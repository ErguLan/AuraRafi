//! Editor-facing platform input adapter.
//!
//! The editor exposes one native input boundary shared by RafUI and the
//! viewport. Keeping these aliases here gives integrations a stable editor
//! API without coupling them to a widget toolkit.

pub use raf_core::{
    CaptureMode, InputKey, InputKeySet, InputModifiers, InputOwner, InputRegionId, InputRouter,
    InputSnapshot, PointerButton, PointerButtonSet, PointerCapture,
};
pub use raf_render::api_graphic_basic::ui_surface::NativeUiInputBridge;

pub type EditorInputBridge = NativeUiInputBridge;
