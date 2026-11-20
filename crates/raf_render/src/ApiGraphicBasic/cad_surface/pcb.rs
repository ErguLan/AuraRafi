//! PCB board styling: layers, pads, drills and object paint resolution.

use raf_electronics::cad_scene::{CadDrillShape, CadShape};
use raf_electronics::{CadLayerKind, CadObject, CadObjectKind, CadRect};

use crate::api_graphic_basic::command_list::BasicCommandList;

use super::paint::{
    record_shape_fill, record_shape_outline, rotate_vec2, with_alpha, AIRWIRE_LOGICAL_WIDTH,
};
use super::{CadSurfaceOptions, Density};

/// Alpha of a PCB component body.
///
/// A 2D board view has no depth: copper is read through the silkscreen outline,
/// so the body is a silhouette rather than an opaque lid over the netlist.
pub(super) const PCB_BODY_ALPHA: u8 = 150;

/// Sub-order inside one CAD layer.
///
/// A component body is a silhouette backdrop, not copper. Sorting only by
/// `(layer, pick_priority)` put components *after* the `PcbTopCopper` traces
/// (priority `Component` 30 > `Copper` 10) and the opaque body rect then hid
/// the netlist under every footprint. Painter order alone cannot fix it: the
/// body keeps its own z, so the depth test also let the later rect win. Drawing
/// the body first is enough, because CAD line work uses the no-depth pipeline
/// and therefore submission order *is* the visible order, while pads keep their
/// higher priority and still land on top of their own body.
pub(super) fn body_backdrop_order(object: &CadObject) -> u8 {
    if object.kind == CadObjectKind::Component {
        0
    } else {
        1
    }
}

pub(super) fn record_pad_geometry(
    commands: &mut BasicCommandList,
    quad_id: usize,
    disc_id: Option<usize>,
    object: &CadObject,
    z: f32,
    density: Density,
    fill: [u8; 4],
    border: [u8; 4],
    border_width: f32,
    bore_color: [u8; 4],
) {
    let Some(rect) = object.rect else {
        return;
    };
    let rotation = object.rotation_degrees.to_radians();
    record_shape_fill(
        commands,
        quad_id,
        disc_id,
        rect.center,
        rect.size,
        object.shape,
        z,
        rotation,
        fill,
    );
    record_shape_outline(
        commands,
        rect,
        object.shape,
        rotation,
        z - 0.01,
        border_width,
        border,
        density,
    );

    if let Some(drill) = object.drill {
        let drill_center = rect.center + rotate_vec2(drill.offset, rotation);
        let drill_shape = match drill.shape {
            CadDrillShape::Circle => CadShape::Circle,
            CadDrillShape::Oval => CadShape::Oval,
        };
        record_shape_fill(
            commands,
            quad_id,
            disc_id,
            drill_center,
            drill.size,
            drill_shape,
            z - 0.012,
            rotation,
            bore_color,
        );
        let bore_border = if drill.plated && border[3] != 0 {
            with_alpha(border, 210)
        } else {
            [116, 126, 134, 220]
        };
        record_shape_outline(
            commands,
            CadRect::new(drill_center, drill.size),
            drill_shape,
            rotation,
            z - 0.014,
            0.7,
            bore_border,
            density,
        );
    }
}

/// Authoring width of one conductor, in logical points.
///
/// Physical PCB widths convert through the same camera density as pad geometry.
/// Legacy and schematic marks retain their screen-space presentation width.
pub(super) fn object_line_width(object: &CadObject, options: &CadSurfaceOptions, density:Density) -> f32 {
    if let Some(width)=object.stroke_width_world.filter(|w|w.is_finite() && *w>0.0) {
        return width/(density.world_per_pixel*density.raster_scale);
    }
    match object.kind {
        CadObjectKind::Trace => options.trace_width,
        CadObjectKind::Airwire => AIRWIRE_LOGICAL_WIDTH,
        CadObjectKind::BoardOutline => 1.4,
        _ => options.wire_width,
    }
}

pub(super) fn object_fill(object: &CadObject) -> [u8; 4] {
    match object.kind {
        // Silhouette, not a lid. Copper is read through the footprint outline.
        CadObjectKind::Component => with_alpha(object.color_rgba, PCB_BODY_ALPHA),
        CadObjectKind::Pin => [212, 119, 26, 255],
        CadObjectKind::Pad => object.color_rgba,
        CadObjectKind::NetLabel => [18, 18, 20, 220],
        CadObjectKind::DrcMarker => object.color_rgba,
        CadObjectKind::BoardOutline
        | CadObjectKind::Wire
        | CadObjectKind::Trace
        | CadObjectKind::Airwire => [0, 0, 0, 0],
    }
}

pub(super) fn object_line_color(object: &CadObject) -> [u8; 4] {
    object.color_rgba
}

pub(super) fn object_border(object: &CadObject) -> [u8; 4] {
    match object.kind {
        CadObjectKind::Component => [245, 245, 246, 180],
        CadObjectKind::Pin => [245, 180, 65, 255],
        CadObjectKind::Pad if object.color_rgba[3] == 0 => [0, 0, 0, 0],
        CadObjectKind::Pad => copper_edge_color(object.color_rgba),
        CadObjectKind::NetLabel => [212, 119, 26, 220],
        CadObjectKind::DrcMarker => [245, 245, 246, 220],
        _ => object.color_rgba,
    }
}

pub(super) fn object_border_width(object: &CadObject) -> f32 {
    match object.kind {
        CadObjectKind::DrcMarker => 2.0,
        CadObjectKind::Component | CadObjectKind::Pin => 1.25,
        CadObjectKind::Pad => 0.85,
        CadObjectKind::NetLabel => 1.0,
        _ => 0.0,
    }
}

fn copper_edge_color(copper: [u8; 4]) -> [u8; 4] {
    [
        copper[0].saturating_add(34),
        copper[1].saturating_add(29),
        copper[2].saturating_add(22),
        copper[3],
    ]
}

pub(super) fn object_z(object: &CadObject) -> f32 {
    // The CAD canvas uses an orthographic projection with a clip-space depth
    // range of 0..=1. Negative z values are clipped before the line shader
    // runs, which made GPU symbols disappear while a hover overlay
    // still made them look intermittently present.
    (0.82 - 0.05 * layer_order(object.layer) as f32 - 0.0005 * object.pick_priority as i32 as f32)
        .clamp(0.05, 0.95)
}

pub(super) fn layer_order(layer: CadLayerKind) -> u8 {
    match layer {
        CadLayerKind::PcbSilkscreen => 0,
        CadLayerKind::PcbBottomCopper => 1,
        CadLayerKind::PcbTopCopper => 2,
        CadLayerKind::Schematic => 3,
        CadLayerKind::Airwire => 4,
        CadLayerKind::Overlay => 5,
        CadLayerKind::Drc => 6,
    }
}
