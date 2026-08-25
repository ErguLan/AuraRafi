//! Lightweight metadata for renderer-owned viewport overlays.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportOverlayState {
    pub grid_visible: bool,
    pub labels_visible: bool,
    pub polygons_visible: bool,
    pub gizmo_visible: bool,
}

impl Default for ViewportOverlayState {
    fn default() -> Self {
        Self {
            grid_visible: true,
            labels_visible: true,
            polygons_visible: false,
            gizmo_visible: true,
        }
    }
}
