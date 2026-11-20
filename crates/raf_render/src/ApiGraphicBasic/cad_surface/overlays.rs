//! Selection, hover, net emphasis and DRC overlay marks.

use glam::Vec2;
use raf_electronics::cad_scene::CadShape;
use raf_electronics::{CadObject, CadObjectKind, CadRect};

use crate::api_graphic_basic::command_list::BasicCommandList;

use super::paint::{
    expanded_shape, outset_rect, record_border, record_disc, record_line, record_quad,
    record_shape_outline, rotated_rect, with_alpha,
};
use super::pcb::{object_line_color, object_line_width, object_z};
use super::{CadSurfaceOptions, Density};

/// Alpha floor applied to every object outside the highlighted net, expressed
/// as an 8-bit factor. 115/255 is about 0.45, the readability floor requested
/// for the dimmed state.
pub(super) const NET_DIM_ALPHA: u8 = 115;

/// Extra logical stroke width of the halo drawn around a highlighted net.
pub(super) const NET_HALO_WIDTH_BONUS: f32 = 2.0;

/// Alpha of the net halo. Derived from `symbol_color` so the emphasis reads in
/// luminance instead of introducing a hue the editor does not own.
pub(super) const NET_HALO_ALPHA: u8 = 170;

/// Extra logical stroke width of the pointer affordance, kept clearly below the
/// selection halo so the two never read as the same state.
const HOVER_WIDTH_BONUS: f32 = 1.0;

/// Pointer affordance color.
///
/// Derived from the cold axis/grid token family already present in this file
/// (`axis_color` is `[220, 226, 240, ...]`) and used at full opacity, so hover
/// is the cold counterpart of the warm `selection_color` and the two stay
/// distinguishable when both are visible on the same object.
pub(super) const HOVER_COLOR: [u8; 4] = [150, 186, 224, 255];

/// Diameter of a DRC mark in physical target pixels, so markers keep a constant
/// on-screen size at any zoom.
const DRC_MARKER_PIXELS: f32 = 18.0;

/// Alpha of the filled part of a DRC mark; the outline carries the severity.
const DRC_MARKER_FILL_ALPHA: u8 = 96;

/// Logical stroke width of the DRC mark outline.
const DRC_MARKER_BORDER_WIDTH: f32 = 1.5;

/// How one CAD object is treated relative to the current pointer and net focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Emphasis {
    /// No cross-probing state is active.
    Plain,
    /// Committed selection. Its own outline pass runs after the body.
    Selected,
    /// Under the pointer but not selected.
    Hovered,
    /// Member of `highlight_net_id`.
    Highlighted,
    /// Outside `highlight_net_id` while a net is focused.
    Receding,
}

pub(super) fn object_is_selected(object: &CadObject, options: &CadSurfaceOptions) -> bool {
    if options.selected_object_ids.is_empty() && options.selected_source_ids.is_empty() {
        return false;
    }

    options
        .selected_object_ids
        .iter()
        .any(|selected| selected == &object.id)
        || object_is_hovered_source(object, &options.selected_source_ids)
}

fn object_is_hovered_source(object: &CadObject, sources: &[[u8; 16]]) -> bool {
    if sources.is_empty() {
        return false;
    }
    object
        .source_id
        .map(|source_id| {
            sources
                .iter()
                .any(|selected| selected == source_id.as_bytes())
        })
        .unwrap_or(false)
}

/// Resolves the emphasis one object gets from the pointer and net focus.
///
/// Net focus wins over hover: when a net is being followed, every other object
/// recedes regardless of where the pointer is, so the eye lands on the net and
/// not on the last thing it touched. Receding is only ever applied while a net
/// is actually focused, so a neutral canvas keeps full contrast.
pub(super) fn object_emphasis(object: &CadObject, options: &CadSurfaceOptions) -> Emphasis {
    if let Some(net_id) = options.highlight_net_id {
        return if object.net_id == Some(net_id) {
            Emphasis::Highlighted
        } else {
            Emphasis::Receding
        };
    }
    if object_is_selected(object, options) {
        Emphasis::Selected
    } else if object_is_hovered_source(object, &options.hovered_source_ids) {
        Emphasis::Hovered
    } else {
        Emphasis::Plain
    }
}

/// Scales one channel down to the readability floor used for non-focused nets.
pub(super) fn recede(color: [u8; 4]) -> [u8; 4] {
    if color[3] == 0 {
        return color;
    }
    let alpha = (u32::from(color[3]) * u32::from(NET_DIM_ALPHA) / 255) as u8;
    [color[0], color[1], color[2], alpha]
}

pub(super) fn record_selection_outline(
    object: &CadObject,
    options: &CadSurfaceOptions,
    commands: &mut BasicCommandList,
    density: Density,
) {
    let z = object_z(object) - 0.08;
    let halo = options.selection_width.max(1.0);
    if !object.line_paths.is_empty() {
        let width = match object.kind {
            CadObjectKind::Pin | CadObjectKind::Pad => 2.0,
            _ => halo,
        };
        for path in &object.line_paths {
            for segment in path.windows(2) {
                record_line(
                    commands,
                    segment[0],
                    segment[1],
                    z,
                    width,
                    options.selection_color,
                    density,
                );
            }
        }
        return;
    }
    if let Some(rect) = object.rect {
        record_object_outline(
            object,
            rect,
            z,
            halo,
            options.selection_color,
            density,
            commands,
        );
        return;
    }

    let base_width = object_line_width(object, options, density);
    let width = base_width.max(1.0) + halo * 2.0;
    for segment in object.points.windows(2) {
        record_line(
            commands,
            segment[0],
            segment[1],
            z,
            width,
            options.selection_color,
            density,
        );
        // Keep the conductor visible inside its thin selection halo.
        record_line(
            commands,
            segment[0],
            segment[1],
            z - 0.01,
            base_width,
            object_line_color(object),
            density,
        );
    }
}

/// Pointer affordance for the object under the cursor.
///
/// Deliberately different from `record_selection_outline` in three ways: it
/// never runs on a selected object, it uses the cold `HOVER_COLOR` instead of
/// the warm selection token, and its halo is half as thick. A cold thin outline
/// and a warm thick halo are separable at a glance even when they overlap.
pub(super) fn record_hover_outline(
    object: &CadObject,
    options: &CadSurfaceOptions,
    commands: &mut BasicCommandList,
    density: Density,
) {
    let z = object_z(object) - 0.06;
    if !object.line_paths.is_empty() {
        for path in &object.line_paths {
            for segment in path.windows(2) {
                record_line(
                    commands,
                    segment[0],
                    segment[1],
                    z,
                    1.0,
                    HOVER_COLOR,
                    density,
                );
            }
        }
        return;
    }
    if let Some(rect) = object.rect {
        let outset = density.world(2.0);
        let expanded = outset_rect(rect, outset);
        if object.kind == CadObjectKind::Pad && object.shape != CadShape::Rectangle {
            record_shape_outline(
                commands,
                expanded,
                expanded_shape(object.shape, outset),
                object.rotation_degrees.to_radians(),
                z,
                1.0,
                HOVER_COLOR,
                density,
            );
        } else {
            record_border(
                commands,
                expanded,
                z,
                1.0,
                HOVER_COLOR,
                density,
                object.rotation_degrees.to_radians(),
            );
        }
        return;
    }

    let base_width = object_line_width(object, options, density);
    for segment in object.points.windows(2) {
        record_line(
            commands,
            segment[0],
            segment[1],
            z,
            base_width + HOVER_WIDTH_BONUS,
            HOVER_COLOR,
            density,
        );
        record_line(
            commands,
            segment[0],
            segment[1],
            z - 0.01,
            base_width,
            object_line_color(object),
            density,
        );
    }
}

fn record_object_outline(
    object: &CadObject,
    rect: CadRect,
    z: f32,
    width: f32,
    color: [u8; 4],
    density: Density,
    commands: &mut BasicCommandList,
) {
    let rotation = object.rotation_degrees.to_radians();
    if object.kind == CadObjectKind::Pad && object.shape != CadShape::Rectangle {
        record_shape_outline(
            commands,
            rect,
            object.shape,
            rotation,
            z,
            width,
            color,
            density,
        );
    } else {
        record_border(commands, rect, z, width, color, density, rotation);
    }
}

/// Canvas-side shape of a DRC mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DrcShape {
    ErrorSquare,
    WarningDiamond,
    InfoDisc,
}

/// Classifies a DRC mark from the only severity signal the canvas receives.
///
/// `CadObject` does not carry `DrcSeverity`; `cad_scene::push_drc_issue` bakes
/// it into `color_rgba` through `drc_color`, which maps Error to
/// `[220, 66, 58]`, Warning to `[245, 180, 65]` and Info to `[170, 170, 178]`.
/// The canvas therefore takes the nearest of those three RGB triples. Color
/// alone is not enough here: the label text of every mark is painted in the same
/// orange by the overlay, so severity has to survive without it. The unknown
/// case falls back to `ErrorSquare` because a marker whose severity cannot be
/// read must never be downgraded to an advisory.
pub(super) fn drc_shape(object: &CadObject) -> DrcShape {
    const ERROR_RGB: [u8; 3] = [220, 66, 58];
    const WARNING_RGB: [u8; 3] = [245, 180, 65];
    const INFO_RGB: [u8; 3] = [170, 170, 178];

    let distance = |reference: [u8; 3]| -> i32 {
        (0..3usize)
            .map(|channel| {
                let delta = i32::from(object.color_rgba[channel]) - i32::from(reference[channel]);
                delta * delta
            })
            .sum()
    };

    let error = distance(ERROR_RGB);
    let warning = distance(WARNING_RGB);
    let info = distance(INFO_RGB);
    if error <= warning && error <= info {
        DrcShape::ErrorSquare
    } else if warning <= info {
        DrcShape::WarningDiamond
    } else {
        DrcShape::InfoDisc
    }
}

/// Paints one DRC mark, encoding severity in shape first and color second.
pub(super) fn record_drc_marker(
    commands: &mut BasicCommandList,
    quad_id: usize,
    disc_id: Option<usize>,
    object: &CadObject,
    z: f32,
    density: Density,
) {
    let Some(center) = object.rect.map(|rect| rect.center) else {
        return;
    };
    // Keep the physical pad visible under a finding. The short leader still
    // identifies its exact location at every zoom level.
    let location = center;
    let center = center + Vec2::new(density.world(10.0), -density.world(14.0));
    record_line(commands, location, center, z + 0.005, 0.8, with_alpha(object.color_rgba, 150), density);
    let shape = drc_shape(object);
    let size = Vec2::splat(density.world(DRC_MARKER_PIXELS));
    let fill = with_alpha(object.color_rgba, DRC_MARKER_FILL_ALPHA);
    let stroke = object.color_rgba;
    let radius = size.x * 0.5;

    match shape {
        DrcShape::ErrorSquare => {
            let rect = CadRect::new(center, size);
            record_quad(commands, quad_id, center, size, z, 0.0, fill);
            record_border(
                commands,
                rect,
                z - 0.005,
                DRC_MARKER_BORDER_WIDTH,
                stroke,
                density,
                0.0,
            );
        }
        DrcShape::WarningDiamond => {
            let rotation = std::f32::consts::FRAC_PI_4;
            let rect = rotated_rect(center, size, rotation);
            record_quad(commands, quad_id, center, size, z, rotation, fill);
            record_border(
                commands,
                rect,
                z - 0.005,
                DRC_MARKER_BORDER_WIDTH,
                stroke,
                density,
                rotation,
            );
        }
        DrcShape::InfoDisc => {
            // A ring reads better than a solid dot for the lowest severity: it
            // stays visible over a bright pad without competing with an error.
            if let Some(disc_id) = disc_id {
                record_disc(commands, disc_id, center, radius, z, stroke);
                record_disc(commands, disc_id, center, radius * 0.55, z - 0.005, fill);
            } else {
                record_quad(commands, quad_id, center, size, z, 0.0, fill);
            }
        }
    }
}
