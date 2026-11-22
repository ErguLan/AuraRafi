//! Direct retained CAD surface host for native presentation.

use glam::Vec2;
use raf_electronics::CadScene;

use super::cad_surface::{
    build_cad_surface_frame, CadSurfaceFrame, CadSurfaceHitRegion, CadSurfaceOptions,
};

/// Retained CAD frame plus the pick regions that native editors consume.
///
/// The host owns the last built frame so an idle surface does no work: geometry
/// is recorded only when a caller asks for a rebuild. Presentation belongs to
/// the shared editor compositor, which receives the frame through
/// `EditorCanvasLayer`; this type never opens a device or a target.
pub struct DirectCadSurfaceHost {
    frame: Option<CadSurfaceFrame>,
}

impl DirectCadSurfaceHost {
    pub fn new() -> Self {
        Self { frame: None }
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
}

impl Default for DirectCadSurfaceHost {
    fn default() -> Self {
        Self::new()
    }
}
