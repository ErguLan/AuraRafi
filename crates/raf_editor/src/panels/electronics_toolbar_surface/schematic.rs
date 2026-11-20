//! Schematic tools of the Electronics toolbar band.

use raf_electronics::CadSurfaceKind;
use raf_render::api_graphic_basic::ui_surface::UiIconId;

use super::ToolbarControl;

/// The schematic option of the document segment.
pub(super) fn mode_option() -> (ToolbarControl, CadSurfaceKind) {
    (
        ToolbarControl::segment(
            "electronics.mode.schematic",
            UiIconId::Schematic,
            Some("electronics.toolbar.schematic"),
        ),
        CadSurfaceKind::Schematic,
    )
}

/// Schematic tools, appended after the shared selection and pan tools.
pub(super) fn document_tools(controls: &mut Vec<ToolbarControl>) {
    controls.push(ToolbarControl::tool(
        "electronics.wire",
        UiIconId::Wire,
        Some("electronics.toolbar.wire"),
    ));
    controls.push(ToolbarControl::tool(
        "electronics.place",
        UiIconId::CadPlace,
        Some("electronics.toolbar.place"),
    ));
}
