//! Retained CAD surface facade.
//!
//! The public frame model and the frame builder live here; schematic, PCB,
//! overlay, primitive and picking details live in the sibling modules.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use raf_electronics::cad_scene::CadShape;
use raf_electronics::{
    CadLayerKind, CadObject, CadObjectKind, CadPickPriority, CadRect, CadScene, CadSurfaceKind,
};

use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::scene_renderer::{FrameStats, SceneRenderFrame};

mod hit_test;
mod overlays;
mod paint;
mod pcb;
mod schematic;

#[cfg(test)]
mod tests;

use hit_test::{cad_hit_region, cad_object_intersects_bounds, canvas_view_projection};
use overlays::{
    object_emphasis, object_is_selected, recede, record_drc_marker, record_hover_outline,
    record_selection_outline, Emphasis, NET_HALO_ALPHA, NET_HALO_WIDTH_BONUS,
};
use paint::{
    disc_mesh, record_border, record_dashed_path, record_line, record_quad, unit_quad_mesh,
    with_alpha,
};
use pcb::{
    body_backdrop_order, layer_order, object_border, object_border_width, object_fill,
    object_line_color, object_line_width, object_z, record_pad_geometry,
};
use schematic::{
    assign_junction_marks, record_grid, record_junction_marks, record_net_label_anchor,
    scene_junction_points, NET_LABEL_ANCHOR_ALPHA, NET_LABEL_ANCHOR_COLOR,
};

/// Smallest stroke the executors agree on, in physical target pixels.
///
/// `BasicCommandList::draw_line` already clamps to one pixel, so this only keeps
/// the intent explicit at the authoring site.
const MIN_DEVICE_WIDTH: f32 = 1.0;

/// Accepted range for `CadSurfaceOptions::raster_scale`.
///
/// The host owns density, so the surface only has to reject a degenerate value
/// instead of trusting it blindly.
const RASTER_SCALE_RANGE: [f32; 2] = [0.25, 4.0];

#[derive(Debug, Clone)]
pub struct CadSurfaceOptions {
    /// A bounded point grid for physical board drafting.
    pub dotted_grid: bool,
    pub clear_color: [u8; 4],
    pub world_bounds: Option<[f32; 4]>,
    pub show_grid: bool,
    pub show_labels: bool,
    pub show_drc_markers: bool,
    pub grid_step: f32,
    pub grid_color: [u8; 4],
    pub major_grid_color: [u8; 4],
    pub axis_color: [u8; 4],
    pub symbol_color: [u8; 4],
    pub trace_width: f32,
    pub wire_width: f32,
    /// Explicit render-object identities, for objects without a model UUID.
    ///
    /// Kept for hosts that address CAD objects by `CadObject::id`. The editor
    /// resolves selections through `source_id`, so this list is normally empty.
    pub selected_object_ids: Vec<String>,
    /// Stable model UUID bytes highlighted by the retained surface.
    pub selected_source_ids: Vec<[u8; 16]>,
    pub selection_color: [u8; 4],
    pub selection_width: f32,
    /// Stable model UUID bytes under the pointer, drawn with a hover treatment
    /// that is visually distinct from selection.
    pub hovered_source_ids: Vec<[u8; 16]>,
    /// Connectivity identity to emphasise; objects on other nets recede.
    pub highlight_net_id: Option<usize>,
    /// Target-pixels per logical point, so line work keeps its visual weight
    /// on high-DPI targets.
    pub raster_scale: f32,
    /// Whether `CadSurfaceFrame::hit_regions` is populated.
    ///
    /// Defaults to `true` so existing hosts keep working. The editor resolves
    /// picking through `raf_electronics::cad_interaction::pick_editable`, which
    /// already ignores painted overlays, so it can set this to `false` and skip
    /// four `String` clones per visible object on every frame.
    pub collect_hit_regions: bool,
}

impl Default for CadSurfaceOptions {
    fn default() -> Self {
        Self {
            dotted_grid:false,
            clear_color: [10, 10, 11, 255],
            world_bounds: None,
            show_grid: true,
            show_labels: true,
            show_drc_markers: true,
            grid_step: 20.0,
            // These are the surface's own defaults, used when a host creates a
            // surface without configuring it. They are deliberately more
            // present than the near-invisible values the editor controller
            // ships, because a host that has not expressed an opinion should
            // still get a grid it can see.
            grid_color: [180, 186, 200, 30],
            major_grid_color: [220, 226, 240, 58],
            axis_color: [220, 226, 240, 82],
            symbol_color: [245, 245, 246, 255],
            trace_width: 3.0,
            wire_width: 2.0,
            selected_object_ids: Vec::new(),
            selected_source_ids: Vec::new(),
            selection_color: [255, 172, 64, 255],
            selection_width: 1.0,
            hovered_source_ids: Vec::new(),
            highlight_net_id: None,
            raster_scale: 1.0,
            collect_hit_regions: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CadSurfaceFrame {
    pub frame: SceneRenderFrame,
    pub objects: Vec<CadSurfaceObject>,
    /// Retained AGB pick regions.
    ///
    /// Deprecated as an *editing* path: the netlist-aware picker lives in
    /// `raf_electronics::cad_interaction::pick_editable`, which also refuses to
    /// let a painted `NetLabel`, `Airwire` or `DrcMarker` win a click over the
    /// geometry underneath. Kept, and still populated by default, because it is
    /// public API of `raf_render`; hosts that moved to `pick_editable` can set
    /// `CadSurfaceOptions::collect_hit_regions` to `false` to stop paying for it.
    pub hit_regions: Vec<CadSurfaceHitRegion>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CadSurfaceObject {
    pub id: String,
    /// Stable source identity propagated from the retained CAD object.
    pub source_id: Option<String>,
    pub kind: CadObjectKind,
    pub layer: CadLayerKind,
    pub pick_priority: CadPickPriority,
    pub rect: Option<CadRect>,
    pub label: Option<String>,
    pub net: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CadSurfaceHitRegion {
    pub id: String,
    /// Stable source identity used by editors for selection lookup.
    pub source_id: Option<String>,
    pub kind: CadObjectKind,
    pub layer: CadLayerKind,
    pub pick_priority: CadPickPriority,
    pub bounds: CadRect,
    pub label: Option<String>,
    pub net: Option<String>,
}

impl CadSurfaceFrame {
    pub fn hit_test(&self, point: Vec2) -> Option<&CadSurfaceHitRegion> {
        self.hit_regions
            .iter()
            .filter(|region| region.bounds.contains(point))
            .max_by_key(|region| (region.pick_priority, layer_order(region.layer)))
    }
}

/// Physical-pixel to world-unit conversions resolved once per frame.
///
/// `world_bounds` is computed with a *logical* canvas size while the framebuffer
/// is *physical*, so one world unit is not one logical point. Every mark whose
/// size is authored in target pixels (junction dots, DRC marks) converts through
/// this struct, and every stroke width authored in logical points converts
/// through [`Density::pixels`].
#[derive(Debug, Clone, Copy)]
struct Density {
    /// Target pixels per logical point.
    raster_scale: f32,
    /// World units covered by one physical target pixel.
    world_per_pixel: f32,
}

impl Density {
    fn new(options: &CadSurfaceOptions, width: u32, height: u32, bounds: [f32; 4]) -> Self {
        let [left, right, top, bottom] = bounds;
        let span_x = (right - left).max(0.001);
        let span_y = (bottom - top).max(0.001);
        let pixels_per_world = (width as f32 / span_x)
            .min(height as f32 / span_y)
            .max(0.0001);
        Self {
            raster_scale: options
                .raster_scale
                .clamp(RASTER_SCALE_RANGE[0], RASTER_SCALE_RANGE[1]),
            world_per_pixel: 1.0 / pixels_per_world,
        }
    }

    /// Logical points to physical target pixels.
    ///
    /// `BasicLine::width` is documented as target pixels and the GPU and CPU
    /// executors must receive the same number, so this is the *only* place a CAD
    /// stroke width is converted. Applying it here, once, at the single point
    /// where geometry reaches the backend-neutral command list is what keeps the
    /// grid, object borders, selection and hover halos from drifting apart on a
    /// high-DPI target: at 200% a 1.0 grid stroke was half a logical point wide
    /// and disappeared into subsampling.
    fn pixels(&self, logical: f32) -> f32 {
        (logical * self.raster_scale).max(MIN_DEVICE_WIDTH)
    }

    /// Physical target pixels to world units.
    fn world(&self, pixels: f32) -> f32 {
        pixels * self.world_per_pixel
    }
}

pub fn build_cad_surface_frame(
    scene: &CadScene,
    width: u32,
    height: u32,
    options: CadSurfaceOptions,
) -> CadSurfaceFrame {
    let mut commands = BasicCommandList::new();
    commands.clear(options.clear_color);
    // CAD is a two-dimensional instrument. The Y-down projection must not
    // cull its filled pads and silhouettes as back-facing 3D geometry.
    commands.set_pipeline(crate::api_graphic_basic::pipeline::BasicPipelineKind::FlatColorTwoSided);
    let quad_id = commands.register_mesh(unit_quad_mesh());
    let bounds = options
        .world_bounds
        .unwrap_or([0.0, width as f32, 0.0, height as f32]);
    let density = Density::new(&options, width, height, bounds);
    let collect_hit_regions = options.collect_hit_regions;
    let mut objects = Vec::with_capacity(scene.objects.len());
    let mut hit_regions = Vec::new();
    if collect_hit_regions {
        hit_regions = Vec::with_capacity(scene.objects.len());
    }

    let schematic = scene.surface == CadSurfaceKind::Schematic;
    let mut sorted = scene.objects.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|object| {
        (
            layer_order(object.layer),
            body_backdrop_order(object),
            object.pick_priority,
        )
    });

    // The disc mesh only exists for the round marks: DRC severities and wire
    // junctions. A frame with neither does not carry an unused registration.
    let wire_count = if schematic {
        sorted
            .iter()
            .filter(|object| object.kind == CadObjectKind::Wire)
            .count()
    } else {
        0
    };
    let needs_disc = (options.show_drc_markers
        && scene
            .objects
            .iter()
            .any(|object| object.kind == CadObjectKind::DrcMarker))
        || wire_count >= 2
        || scene.objects.iter().any(|object| {
            object.kind == CadObjectKind::Pad
                && (matches!(
                    object.shape,
                    CadShape::Circle | CadShape::Oval | CadShape::RoundRect { .. }
                ) || object.drill.is_some())
        });
    let disc_id = needs_disc.then(|| commands.register_mesh(disc_mesh()));

    if !schematic {
        if let Some(board) = scene.objects.iter().find(|object| object.id == "board_outline" && object.kind == CadObjectKind::BoardOutline) {
            super::cad_board_surface::record_board_material(&mut commands, &board.points, bounds, [width, height], options.clear_color);
        }
    }
    if options.show_grid {
        record_grid(&mut commands, &options, bounds, density);
    }

    // Junction marks are pure reading geometry, and they only exist where the
    // netlist actually joins wires, so the clustering pass runs once per scene.
    let junction_marks = if schematic && disc_id.is_some() {
        assign_junction_marks(&sorted, &scene_junction_points(scene))
    } else {
        HashMap::new()
    };

    let mut anchored_net_ids = std::collections::HashSet::new();
    let mut anchored_net_names = std::collections::HashSet::new();
    let mut visible_entities = 0u32;
    for (index, object) in sorted.iter().enumerate() {
        if !cad_object_intersects_bounds(object, bounds) {
            continue;
        }
        visible_entities = visible_entities.saturating_add(1);
        let emphasis = object_emphasis(object, &options);
        let draw_net_label_anchor = if object.kind != CadObjectKind::NetLabel {
            false
        } else if let Some(net_id) = object.net_id {
            anchored_net_ids.insert(net_id)
        } else {
            object
                .net
                .as_deref()
                .or(object.label.as_deref())
                .is_some_and(|name| anchored_net_names.insert(name.to_string()))
        };
        record_object(
            object,
            quad_id,
            disc_id,
            &options,
            &mut commands,
            schematic,
            density,
            emphasis,
            draw_net_label_anchor,
        );
        if object_is_selected(object, &options) {
            record_selection_outline(object, &options, &mut commands, density);
        } else if emphasis == Emphasis::Hovered {
            record_hover_outline(object, &options, &mut commands, density);
        }
        record_junction_marks(
            &junction_marks,
            index,
            disc_id,
            object,
            emphasis,
            &mut commands,
            density,
            &options,
        );
        objects.push(CadSurfaceObject {
            id: object.id.clone(),
            source_id: object.source_id.map(|id| id.to_string()),
            kind: object.kind,
            layer: object.layer,
            pick_priority: object.pick_priority,
            rect: object.rect,
            label: object.label.clone(),
            net: object.net.clone(),
        });
        if collect_hit_regions {
            if let Some(hit_region) = cad_hit_region(object, &options) {
                hit_regions.push(hit_region);
            }
        }
    }

    CadSurfaceFrame {
        frame: SceneRenderFrame {
            commands,
            view_proj: canvas_view_projection(bounds[0], bounds[1], bounds[2], bounds[3]),
            light_dir: Vec3::Z,
            width,
            height,
            texture_cache_budget_bytes: 0,
            stats: FrameStats {
                total_entities: scene.objects.len() as u32,
                visible_entities,
                triangles_rendered: 0,
                triangles_culled: 0,
                ..FrameStats::default()
            },
        },
        objects,
        hit_regions,
    }
}

fn record_object(
    object: &CadObject,
    quad_id: usize,
    disc_id: Option<usize>,
    options: &CadSurfaceOptions,
    commands: &mut BasicCommandList,
    schematic: bool,
    density: Density,
    emphasis: Emphasis,
    draw_net_label_anchor: bool,
) {
    if object.kind == CadObjectKind::DrcMarker && !options.show_drc_markers {
        return;
    }

    let z = object_z(object);
    // A DRC marker is the one object that keeps full strength while a net is
    // focused: dimming a violation would hide the very thing the user may be
    // about to route into.
    let recede_paint = emphasis == Emphasis::Receding && object.kind != CadObjectKind::DrcMarker;
    let paint = |color: [u8; 4]| {
        if recede_paint {
            recede(color)
        } else {
            color
        }
    };

    if object.kind == CadObjectKind::DrcMarker {
        record_drc_marker(commands, quad_id, disc_id, object, z, density);
        return;
    }

    // Schematic text belongs to the retained editor overlay. Symbol strokes,
    // wires and anchors stay in AGB so they share the canvas camera and scale.
    let body_owned_by_overlay =
        schematic && matches!(object.kind, CadObjectKind::Component | CadObjectKind::Pin);
    if !body_owned_by_overlay {
        if let Some(rect) = object.rect {
            if object.kind == CadObjectKind::NetLabel {
                if options.show_labels && draw_net_label_anchor {
                    let anchor = paint(with_alpha(NET_LABEL_ANCHOR_COLOR, NET_LABEL_ANCHOR_ALPHA));
                    record_net_label_anchor(commands, rect, z, anchor, density);
                }
            } else if object.kind == CadObjectKind::Pad {
                record_pad_geometry(
                    commands,
                    quad_id,
                    disc_id,
                    object,
                    z,
                    density,
                    paint(object_fill(object)),
                    paint(object_border(object)),
                    object_border_width(object),
                    options.clear_color,
                );
            } else {
                let rotation = object.rotation_degrees.to_radians();
                record_quad(
                    commands,
                    quad_id,
                    rect.center,
                    rect.size,
                    z,
                    rotation,
                    paint(object_fill(object)),
                );
                let border = object_border(object);
                if border[3] != 0 {
                    record_border(
                        commands,
                        rect,
                        z - 0.01,
                        object_border_width(object),
                        paint(border),
                        density,
                        rotation,
                    );
                }
            }
        }
    }

    if object.points.len() >= 2 {
        let logical_width = object_line_width(object, options, density);
        let line_color = paint(object_line_color(object));
        let line_z = z - 0.02;
        if object.kind == CadObjectKind::Airwire {
            record_dashed_path(
                commands,
                &object.points,
                line_z,
                logical_width,
                line_color,
                density,
            );
        } else {
            if emphasis == Emphasis::Highlighted {
                // A halo in the file's own symbol white, not a new hue, so the
                // focused net reads in luminance against the receding board.
                let halo = with_alpha(options.symbol_color, NET_HALO_ALPHA);
                for segment in object.points.windows(2) {
                    record_line(
                        commands,
                        segment[0],
                        segment[1],
                        line_z - 0.004,
                        logical_width + NET_HALO_WIDTH_BONUS,
                        halo,
                        density,
                    );
                }
            }
            for segment in object.points.windows(2) {
                record_line(
                    commands,
                    segment[0],
                    segment[1],
                    line_z,
                    logical_width,
                    line_color,
                    density,
                );
            }
        }
    }

    // Schematic symbols are independent strokes, not one connected path.
    // Keep them in the same backend-neutral line stream as wires so AGB/WGPU
    // can batch them without the editor repainting the static geometry.
    for path in &object.line_paths {
        for segment in path.windows(2) {
            let base = if schematic
                && object.color_rgba[3] == u8::MAX
                && !matches!(object.kind, CadObjectKind::Pin | CadObjectKind::Pad)
            {
                options.symbol_color
            } else {
                object_line_color(object)
            };
            record_line(
                commands,
                segment[0],
                segment[1],
                z - 0.015,
                2.0,
                paint(base),
                density,
            );
        }
    }
}
