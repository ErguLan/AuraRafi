use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use glam::{Mat4, Quat, Vec2, Vec3};
use raf_electronics::{
    CadLayerKind, CadObject, CadObjectKind, CadPickPriority, CadRect, CadScene, CadSurfaceKind,
};

use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::scene_renderer::{FrameStats, SceneRenderFrame};

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

/// Airwire guide stroke width, in logical points.
const AIRWIRE_LOGICAL_WIDTH: f32 = 1.25;

/// Airwire dash period, in physical target pixels, before the stroke width is
/// taken into account.
const AIRWIRE_DASH_PIXELS: f32 = 6.0;

/// Gap between two airwire dashes as a fraction of the dash itself.
const AIRWIRE_GAP_RATIO: f32 = 0.9;

/// Hard cap on the number of dashes one segment may emit.
///
/// A one-pixel zoom on a large board would otherwise ask for hundreds of
/// thousands of line instances in a single frame.
const AIRWIRE_MAX_DASHES: usize = 2048;

/// Junction dot diameter relative to the wire stroke it marks.
const JUNCTION_DIAMETER_RATIO: f32 = 1.7;

/// Wire endpoint coincidence tolerance used for junction marks.
///
/// This is deliberately the netlist tolerance (`POSITION_TOLERANCE` in
/// `raf_electronics::netlist`), not the wire self-check epsilon
/// (`WIRE_POINT_EPSILON` in `raf_electronics::schematic`, 0.001). A junction dot
/// is a promise that two wires are electrically joined, and the netlist only
/// unions wires whose endpoints are within 2.0 grid units, so a tighter epsilon
/// would draw fewer dots than the netlist actually connects.
const JUNCTION_TOLERANCE: f32 = 2.0;

/// Alpha floor applied to every object outside the highlighted net, expressed
/// as an 8-bit factor. 115/255 is about 0.45, the readability floor requested
/// for the dimmed state.
const NET_DIM_ALPHA: u8 = 115;

/// Extra logical stroke width of the halo drawn around a highlighted net.
const NET_HALO_WIDTH_BONUS: f32 = 2.0;

/// Alpha of the net halo. Derived from `symbol_color` so the emphasis reads in
/// luminance instead of introducing a hue the editor does not own.
const NET_HALO_ALPHA: u8 = 170;

/// Extra logical stroke width of the pointer affordance, kept clearly below the
/// selection halo so the two never read as the same state.
const HOVER_WIDTH_BONUS: f32 = 1.0;

/// Pointer affordance color.
///
/// Derived from the cold axis/grid token family already present in this file
/// (`axis_color` is `[220, 226, 240, ...]`) and used at full opacity, so hover
/// is the cold counterpart of the warm `selection_color` and the two stay
/// distinguishable when both are visible on the same object.
const HOVER_COLOR: [u8; 4] = [150, 186, 224, 255];

/// Alpha of the net label plate. The overlay owns the text; the canvas only has
/// to lift it off the wire it names without hiding that wire.
const NET_LABEL_PLATE_ALPHA: u8 = 40;

/// Alpha of the net label anchor tick, derived from the `NetLabel` border token.
const NET_LABEL_ANCHOR_COLOR: [u8; 4] = [212, 119, 26, 255];
const NET_LABEL_ANCHOR_ALPHA: u8 = 200;

/// Alpha of a PCB component body.
///
/// A 2D board view has no depth: copper is read through the silkscreen outline,
/// so the body is a silhouette rather than an opaque lid over the netlist.
const PCB_BODY_ALPHA: u8 = 150;

/// Diameter of a DRC mark in physical target pixels, so markers keep a constant
/// on-screen size at any zoom.
const DRC_MARKER_PIXELS: f32 = 18.0;

/// Alpha of the filled part of a DRC mark; the outline carries the severity.
const DRC_MARKER_FILL_ALPHA: u8 = 96;

/// Logical stroke width of the DRC mark outline.
const DRC_MARKER_BORDER_WIDTH: f32 = 1.5;

#[derive(Debug, Clone)]
pub struct CadSurfaceOptions {
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

/// How one CAD object is treated relative to the current pointer and net focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Emphasis {
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
        || wire_count >= 2;
    let disc_id = needs_disc.then(|| commands.register_mesh(disc_mesh()));

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

    let mut visible_entities = 0u32;
    for (index, object) in sorted.iter().enumerate() {
        if !cad_object_intersects_bounds(object, bounds) {
            continue;
        }
        visible_entities = visible_entities.saturating_add(1);
        let emphasis = object_emphasis(object, &options);
        record_object(
            object,
            quad_id,
            disc_id,
            &options,
            &mut commands,
            schematic,
            density,
            emphasis,
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
fn body_backdrop_order(object: &CadObject) -> u8 {
    if object.kind == CadObjectKind::Component {
        0
    } else {
        1
    }
}

/// Reject CAD objects that cannot contribute to the current presentation.
///
/// Schematic and PCB documents can contain many off-screen objects. Keeping
/// them in the model is required for editing, but recording their geometry and
/// hit regions every frame is wasted work on low-end machines. The test is
/// intentionally conservative: a polyline uses its full bounds, so a cable
/// crossing the viewport is never culled accidentally.
fn cad_object_intersects_bounds(object: &CadObject, bounds: [f32; 4]) -> bool {
    let [left, right, top, bottom] = bounds;
    if right <= left || bottom <= top {
        return false;
    }

    let mut bounds: Option<(glam::Vec2, glam::Vec2)> = object.rect.map(|rect| {
        let half = rect.size.abs() * 0.5;
        (rect.center - half, rect.center + half)
    });

    let mut include_point = |point: glam::Vec2| {
        if let Some((min, max)) = &mut bounds {
            *min = min.min(point);
            *max = max.max(point);
        } else {
            bounds = Some((point, point));
        }
    };
    for point in &object.points {
        include_point(*point);
    }
    for path in &object.line_paths {
        for point in path {
            include_point(*point);
        }
    }

    let Some((mut min, mut max)) = bounds else {
        return false;
    };

    let pad = match object.kind {
        CadObjectKind::Trace => 3.0,
        CadObjectKind::Airwire => 6.0,
        CadObjectKind::Pin | CadObjectKind::Pad => 6.0,
        _ => 2.0,
    };
    min -= glam::Vec2::splat(pad);
    max += glam::Vec2::splat(pad);
    max.x >= left && min.x <= right && max.y >= top && min.y <= bottom
}

fn object_is_selected(object: &CadObject, options: &CadSurfaceOptions) -> bool {
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
fn object_emphasis(object: &CadObject, options: &CadSurfaceOptions) -> Emphasis {
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
fn recede(color: [u8; 4]) -> [u8; 4] {
    if color[3] == 0 {
        return color;
    }
    let alpha = (u32::from(color[3]) * u32::from(NET_DIM_ALPHA) / 255) as u8;
    [color[0], color[1], color[2], alpha]
}

fn with_alpha(color: [u8; 4], alpha: u8) -> [u8; 4] {
    [color[0], color[1], color[2], alpha]
}

fn record_selection_outline(
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
        record_border(
            commands,
            rect,
            z,
            halo,
            options.selection_color,
            density,
            0.0,
        );
        return;
    }

    let base_width = object_line_width(object, options);
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
fn record_hover_outline(
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
        record_border(
            commands,
            outset_rect(rect, outset),
            z,
            1.0,
            HOVER_COLOR,
            density,
            0.0,
        );
        return;
    }

    let base_width = object_line_width(object, options);
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

fn outset_rect(rect: CadRect, margin: f32) -> CadRect {
    CadRect::new(rect.center, rect.size + Vec2::splat(margin * 2.0))
}

fn record_grid(
    commands: &mut BasicCommandList,
    options: &CadSurfaceOptions,
    bounds: [f32; 4],
    density: Density,
) {
    let base_step = options.grid_step.max(1.0);
    let [left, right, top, bottom] = bounds;
    if right <= left || bottom <= top {
        return;
    }

    // Keep the grid readable at long range. The snap step remains unchanged;
    // only the visual guide collapses to a stable 1/2/5/10 sequence once
    // adjacent lines would occupy less than twelve physical pixels. This
    // avoids the irregular visual density caused by arbitrary ceil factors.
    let pixels_per_world = 1.0 / density.world_per_pixel;
    let screen_step = base_step * pixels_per_world.max(0.0);
    let step = visual_grid_step(base_step, screen_step);

    let start_x = (left / step).floor() as i32 - 1;
    let end_x = (right / step).ceil() as i32 + 1;
    let start_y = (top / step).floor() as i32 - 1;
    let end_y = (bottom / step).ceil() as i32 + 1;

    for ix in start_x..=end_x {
        let x = ix as f32 * step;
        let color = grid_line_color(ix, options);
        record_line(
            commands,
            Vec2::new(x, top),
            Vec2::new(x, bottom),
            0.95,
            1.0,
            color,
            density,
        );
    }

    for iy in start_y..=end_y {
        let y = iy as f32 * step;
        let color = grid_line_color(iy, options);
        record_line(
            commands,
            Vec2::new(left, y),
            Vec2::new(right, y),
            0.95,
            1.0,
            color,
            density,
        );
    }
}

fn visual_grid_step(base_step: f32, screen_step: f32) -> f32 {
    let mut multiplier = 1.0;
    while screen_step.max(0.01) * multiplier < 12.0 {
        multiplier = if multiplier < 2.0 {
            2.0
        } else if multiplier < 5.0 {
            5.0
        } else {
            multiplier * 2.0
        };
    }
    (base_step.max(1.0) * multiplier).max(base_step.max(1.0))
}

fn grid_line_color(index: i32, options: &CadSurfaceOptions) -> [u8; 4] {
    if index == 0 {
        options.axis_color
    } else if index % 5 == 0 {
        options.major_grid_color
    } else {
        options.grid_color
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

    // Schematic component bodies and pin plates belong to the retained editor
    // overlay: it draws the symbol artwork and the labels with the same text
    // compositor as the rest of the chrome. Net labels are the exception, they
    // need the subtle plate below so the overlay text does not sit directly on
    // the wire it names.
    let body_owned_by_overlay =
        schematic && matches!(object.kind, CadObjectKind::Component | CadObjectKind::Pin);
    if !body_owned_by_overlay {
        if let Some(rect) = object.rect {
            if object.kind == CadObjectKind::NetLabel {
                let anchor = paint(with_alpha(NET_LABEL_ANCHOR_COLOR, NET_LABEL_ANCHOR_ALPHA));
                record_net_label_plate(
                    commands,
                    quad_id,
                    rect,
                    z,
                    paint(object.color_rgba),
                    anchor,
                    density,
                );
            } else {
                record_quad(
                    commands,
                    quad_id,
                    rect.center,
                    rect.size,
                    z,
                    0.0,
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
                        0.0,
                    );
                }
            }
        }
    }

    if object.points.len() >= 2 {
        let logical_width = object_line_width(object, options);
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

/// Subtle plate under a net label.
///
/// The overlay owns the text and draws it centred on this rect, but it draws no
/// background of its own, so the canvas supplies a barely-there plate plus an
/// anchor tick. An opaque card here would cover the wire underneath, which is
/// the one thing the label exists to identify.
fn record_net_label_plate(
    commands: &mut BasicCommandList,
    quad_id: usize,
    rect: CadRect,
    z: f32,
    color: [u8; 4],
    anchor_color: [u8; 4],
    density: Density,
) {
    if color[3] == 0 || rect.size.x <= 0.0 || rect.size.y <= 0.0 {
        return;
    }
    let plate = with_alpha(color, NET_LABEL_PLATE_ALPHA.min(color[3]));
    record_quad(commands, quad_id, rect.center, rect.size, z, 0.0, plate);

    let min = rect.center - rect.size * 0.5;
    let max = rect.center + rect.size * 0.5;
    record_line(
        commands,
        Vec2::new(min.x, max.y),
        Vec2::new(max.x, max.y),
        z - 0.005,
        1.0,
        anchor_color,
        density,
    );
}

/// Renders a polyline as a dashed guide.
///
/// Airwires are connections that still have to be routed. Drawing them like a
/// trace with only a lower alpha meant "missing copper" and "copper" differed by
/// a channel that disappears on a bad display or for a color-blind reader. A
/// dash pattern makes the pending state legible on its own.
fn record_dashed_path(
    commands: &mut BasicCommandList,
    points: &[Vec2],
    z: f32,
    logical_width: f32,
    color: [u8; 4],
    density: Density,
) {
    let stroke_px = density.pixels(logical_width);
    let period = (AIRWIRE_DASH_PIXELS.max(stroke_px * 2.0)) * density.world_per_pixel;
    if period <= f32::EPSILON {
        return;
    }
    let dash = period / (1.0 + AIRWIRE_GAP_RATIO);
    let gap = period - dash;
    if dash <= f32::EPSILON {
        return;
    }

    for segment in points.windows(2) {
        let (start, end) = (segment[0], segment[1]);
        let length = start.distance(end);
        if length <= f32::EPSILON {
            continue;
        }
        let direction = (end - start) / length;
        let mut travelled = 0.0;
        let mut dashes = 0usize;
        while travelled < length && dashes < AIRWIRE_MAX_DASHES {
            let dash_end = (travelled + dash).min(length);
            record_line(
                commands,
                start + direction * travelled,
                start + direction * dash_end,
                z,
                logical_width,
                color,
                density,
            );
            travelled = dash_end + gap;
            dashes += 1;
        }
    }
}

/// Canvas-side shape of a DRC mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrcShape {
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
fn drc_shape(object: &CadObject) -> DrcShape {
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
fn record_drc_marker(
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

/// A filled, optionally rotated quad. One unit mesh covers every box-like mark:
/// object bodies, pads, net label plates, junction dots and the square/diamond
/// DRC shapes.
fn record_quad(
    commands: &mut BasicCommandList,
    quad_id: usize,
    center: Vec2,
    size: Vec2,
    z: f32,
    rotation: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let origin = center - rotate_vec2(size * 0.5, rotation);
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::from_rotation_z(rotation),
        Vec3::new(origin.x, origin.y, z),
    );
    commands.draw_mesh(quad_id, transform, color);
}

/// A filled n-gon disc, used for the informational DRC ring and for junction
/// dots so a meeting point reads as a node rather than as a fourth wire.
fn record_disc(
    commands: &mut BasicCommandList,
    disc_id: usize,
    center: Vec2,
    radius: f32,
    z: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || radius <= 0.0 {
        return;
    }
    let size = Vec2::splat(radius * 2.0);
    let min = center - Vec2::splat(radius);
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::IDENTITY,
        Vec3::new(min.x, min.y, z),
    );
    commands.draw_mesh(disc_id, transform, color);
}

/// The axis-aligned footprint a rotated quad actually covers.
///
/// `record_quad` rotates about the rect center, so a caller that wants the
/// border of a diamond needs the rotated extent, not the original rect.
fn rotated_rect(center: Vec2, size: Vec2, rotation: f32) -> CadRect {
    let half = rotate_vec2(size * 0.5, rotation).abs();
    CadRect::new(center, half * 2.0)
}

fn rotate_vec2(value: Vec2, radians: f32) -> Vec2 {
    let (sin, cos) = radians.sin_cos();
    Vec2::new(value.x * cos - value.y * sin, value.x * sin + value.y * cos)
}

fn record_border(
    commands: &mut BasicCommandList,
    rect: CadRect,
    z: f32,
    logical_width: f32,
    color: [u8; 4],
    density: Density,
    rotation: f32,
) {
    if color[3] == 0 || logical_width <= 0.0 || rect.size.x <= 0.0 || rect.size.y <= 0.0 {
        return;
    }
    let half = rect.size.abs() * 0.5;
    let corner = |offset: Vec2| rect.center + rotate_vec2(offset, rotation);
    let top_left = corner(Vec2::new(-half.x, -half.y));
    let top_right = corner(Vec2::new(half.x, -half.y));
    let bottom_right = corner(Vec2::new(half.x, half.y));
    let bottom_left = corner(Vec2::new(-half.x, half.y));
    record_line(
        commands,
        top_left,
        top_right,
        z,
        logical_width,
        color,
        density,
    );
    record_line(
        commands,
        top_right,
        bottom_right,
        z,
        logical_width,
        color,
        density,
    );
    record_line(
        commands,
        bottom_right,
        bottom_left,
        z,
        logical_width,
        color,
        density,
    );
    record_line(
        commands,
        bottom_left,
        top_left,
        z,
        logical_width,
        color,
        density,
    );
}

/// The one place a CAD stroke reaches the backend-neutral command list.
///
/// `BasicLine::width` is target pixels for both executors (see
/// `command_list::BasicLine`), so the logical widths authored in
/// `CadSurfaceOptions` are converted once, here. See [`Density::pixels`] for
/// why the width is a logical quantity and not a physical one.
fn record_line(
    commands: &mut BasicCommandList,
    start: Vec2,
    end: Vec2,
    z: f32,
    logical_width: f32,
    color: [u8; 4],
    density: Density,
) {
    let width = density.pixels(logical_width);
    if color[3] == 0 || width <= 0.0 || start.distance(end) <= f32::EPSILON {
        return;
    }
    commands.draw_line(
        Vec3::new(start.x, start.y, z),
        Vec3::new(end.x, end.y, z),
        color,
        width,
        true,
        0.0,
    );
}

// Memoized junction pass, keyed by `CadScene::stable_fingerprint`.
//
// Safe to reuse because the fingerprint already hashes every wire point and the
// net identity, so an unchanged document cannot produce different junction
// geometry. It only skips the endpoint clustering pass; it never changes what is
// drawn, and any fingerprint change rebuilds it. It is deliberately *not* used
// as a frame cache key: the fingerprint says nothing about `world_bounds`, the
// framebuffer size, or the options, so a zoom or a hover change would replay a
// stale frame.
thread_local! {
    static JUNCTION_MEMO: RefCell<Option<(u64, Vec<Vec2>)>> = RefCell::new(None);
}

/// Assigns every junction point to exactly one wire in the sorted draw order.
///
/// Keying by sorted index, not by identity, is what keeps the mark correct while
/// the object loop culls off-screen geometry: a meeting point owned by a culled
/// wire is culled with it, and a four-way meeting paints one dot instead of one
/// per incident wire.
fn assign_junction_marks(sorted: &[&CadObject], points: &[Vec2]) -> HashMap<usize, Vec<Vec2>> {
    let mut marks: HashMap<usize, Vec<Vec2>> = HashMap::new();
    let mut claimed = vec![false; points.len()];
    for (index, &object) in sorted.iter().enumerate() {
        if object.kind != CadObjectKind::Wire {
            continue;
        }
        for endpoint in wire_endpoints(object) {
            let Some(point_index) = points
                .iter()
                .position(|point| point.distance(endpoint) <= JUNCTION_TOLERANCE)
            else {
                continue;
            };
            if claimed[point_index] {
                continue;
            }
            claimed[point_index] = true;
            marks.entry(index).or_default().push(points[point_index]);
        }
    }
    marks
}

/// The two real endpoints of a wire, ignoring routing elbows.
///
/// `cad_scene::orthogonal_wire_points` inserts a corner for a diagonal wire, so
/// `points[1]` is a bend, not a connection site. Only `first` and `last` are
/// endpoints the netlist can join.
fn wire_endpoints(object: &CadObject) -> [Vec2; 2] {
    let first = object.points.first().copied().unwrap_or(Vec2::ZERO);
    let last = object.points.last().copied().unwrap_or(first);
    [first, last]
}

/// Meeting points where two or more wire endpoints coincide.
///
/// This is a *reading* signal, not new connectivity: the netlist already unions
/// every pair of wires that shares an endpoint, and the criterion here is
/// exactly that pair, so a dot only ever appears where the netlist really did
/// join. A wire endpoint that merely lands on the interior of another segment is
/// deliberately not marked, because the netlist does not connect that case and a
/// dot there would be a false lead. A crossing without a shared endpoint gets no
/// mark either, which is what removes the "is this a T or a cross?" ambiguity.
fn junction_points(sorted: &[&CadObject]) -> Vec<Vec2> {
    let wires = sorted
        .iter()
        .filter(|object| object.kind == CadObjectKind::Wire)
        .count();
    if wires < 2 {
        return Vec::new();
    }

    struct Cluster {
        point: Vec2,
        count: usize,
    }

    let cell = JUNCTION_TOLERANCE.max(f32::EPSILON);
    let cell_of = |point: Vec2| -> (i32, i32) {
        (
            (point.x / cell).round() as i32,
            (point.y / cell).round() as i32,
        )
    };
    let mut clusters: Vec<Cluster> = Vec::with_capacity(wires);
    let mut occupied: HashMap<(i32, i32), usize> = HashMap::with_capacity(wires);

    for &object in sorted.iter() {
        if object.kind != CadObjectKind::Wire {
            continue;
        }
        let [first, last] = wire_endpoints(object);
        if first.distance(last) <= JUNCTION_TOLERANCE {
            // A degenerate wire cannot meet anything; counting it twice would
            // invent a junction out of its own endpoints.
            continue;
        }
        for endpoint in [first, last] {
            let base = cell_of(endpoint);
            let mut merged = None;
            'cells: for dx in -1..=1i32 {
                for dy in -1..=1i32 {
                    let Some(&index) = occupied.get(&(base.0 + dx, base.1 + dy)) else {
                        continue;
                    };
                    if clusters[index].point.distance(endpoint) <= JUNCTION_TOLERANCE {
                        merged = Some(index);
                        break 'cells;
                    }
                }
            }
            match merged {
                Some(index) => clusters[index].count += 1,
                None => {
                    let index = clusters.len();
                    clusters.push(Cluster {
                        point: endpoint,
                        count: 1,
                    });
                    occupied.insert(cell_of(endpoint), index);
                }
            }
        }
    }

    clusters
        .into_iter()
        .filter(|cluster| cluster.count >= 2)
        .map(|cluster| cluster.point)
        .collect()
}

/// Junction points of the current scene, memoized per document fingerprint.
fn scene_junction_points(scene: &CadScene) -> Vec<Vec2> {
    let fingerprint = scene.stable_fingerprint();
    let cached = JUNCTION_MEMO.with(|memo| {
        memo.borrow()
            .as_ref()
            .filter(|(key, _)| *key == fingerprint)
            .map(|(_, points)| points.clone())
    });
    if let Some(points) = cached {
        return points;
    }

    let sorted = scene.objects.iter().collect::<Vec<_>>();
    let points = junction_points(&sorted);
    JUNCTION_MEMO.with(|memo| {
        *memo.borrow_mut() = Some((fingerprint, points.clone()));
    });
    points
}

/// Emits the junction marks owned by the wire now being recorded.
///
/// The mark is a disc in the wire's own color, sized from the wire stroke and
/// from the same logical-to-physical conversion as every other stroke, and it
/// goes out right after the conductor so the wire network stays one continuous
/// line stream for the executors to batch.
fn record_junction_marks(
    marks: &HashMap<usize, Vec<Vec2>>,
    index: usize,
    disc_id: Option<usize>,
    object: &CadObject,
    emphasis: Emphasis,
    commands: &mut BasicCommandList,
    density: Density,
    options: &CadSurfaceOptions,
) {
    let (Some(points), Some(disc_id)) = (marks.get(&index), disc_id) else {
        return;
    };
    if object.kind != CadObjectKind::Wire {
        return;
    }
    let color = if emphasis == Emphasis::Receding {
        recede(object_line_color(object))
    } else {
        object_line_color(object)
    };
    let diameter = density
        .world(density.pixels(object_line_width(object, options)) * JUNCTION_DIAMETER_RATIO)
        .max(density.world(MIN_DEVICE_WIDTH));
    let z = object_z(object) - 0.024;
    for point in points {
        record_disc(commands, disc_id, *point, diameter * 0.5, z, color);
    }
}

fn cad_hit_region(object: &CadObject, options: &CadSurfaceOptions) -> Option<CadSurfaceHitRegion> {
    let bounds = if let Some(rect) = object.rect {
        rect
    } else {
        polyline_bounds(object, object_pick_width(object, options))?
    };
    Some(CadSurfaceHitRegion {
        id: object.id.clone(),
        source_id: object.source_id.map(|id| id.to_string()),
        kind: object.kind,
        layer: object.layer,
        pick_priority: object.pick_priority,
        bounds,
        label: object.label.clone(),
        net: object.net.clone(),
    })
}

fn polyline_bounds(object: &CadObject, width: f32) -> Option<CadRect> {
    let first = object.points.first().copied()?;
    let mut min = first;
    let mut max = first;
    for point in &object.points {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    let pad = width.max(6.0);
    let size = (max - min).abs() + Vec2::splat(pad * 2.0);
    Some(CadRect::new((min + max) * 0.5, size))
}

fn object_pick_width(object: &CadObject, options: &CadSurfaceOptions) -> f32 {
    match object.kind {
        CadObjectKind::Trace => options.trace_width,
        CadObjectKind::Airwire => 4.0,
        CadObjectKind::BoardOutline => 6.0,
        _ => options.wire_width,
    }
}

/// Authoring width of one conductor, in logical points.
///
/// `PcbTrace::width` still lives only in the PCB document: `CadObject` has no
/// width field, so every trace is drawn with `options.trace_width` and a 6-unit
/// power run is indistinguishable from a 6-unit signal run. The single match arm
/// below is where the per-object width becomes authoritative once the field
/// exists; nothing else in this module needs to change for that.
fn object_line_width(object: &CadObject, options: &CadSurfaceOptions) -> f32 {
    match object.kind {
        CadObjectKind::Trace => options.trace_width,
        CadObjectKind::Airwire => AIRWIRE_LOGICAL_WIDTH,
        _ => options.wire_width,
    }
}

fn object_fill(object: &CadObject) -> [u8; 4] {
    match object.kind {
        // Silhouette, not a lid. Copper is read through the footprint outline.
        CadObjectKind::Component => with_alpha(object.color_rgba, PCB_BODY_ALPHA),
        CadObjectKind::Pin | CadObjectKind::Pad => [212, 119, 26, 255],
        CadObjectKind::NetLabel => [18, 18, 20, 220],
        CadObjectKind::DrcMarker => object.color_rgba,
        CadObjectKind::BoardOutline
        | CadObjectKind::Wire
        | CadObjectKind::Trace
        | CadObjectKind::Airwire => [0, 0, 0, 0],
    }
}

fn object_line_color(object: &CadObject) -> [u8; 4] {
    object.color_rgba
}

fn object_border(object: &CadObject) -> [u8; 4] {
    match object.kind {
        CadObjectKind::Component => [245, 245, 246, 180],
        CadObjectKind::Pin | CadObjectKind::Pad => [245, 180, 65, 255],
        CadObjectKind::NetLabel => [212, 119, 26, 220],
        CadObjectKind::DrcMarker => [245, 245, 246, 220],
        _ => object.color_rgba,
    }
}

fn object_border_width(object: &CadObject) -> f32 {
    match object.kind {
        CadObjectKind::DrcMarker => 2.0,
        CadObjectKind::Component | CadObjectKind::Pin | CadObjectKind::Pad => 1.25,
        CadObjectKind::NetLabel => 1.0,
        _ => 0.0,
    }
}

fn object_z(object: &CadObject) -> f32 {
    // The CAD canvas uses an orthographic projection with a clip-space depth
    // range of 0..=1. Negative z values are clipped before the line shader
    // runs, which made GPU symbols disappear while a hover overlay
    // still made them look intermittently present.
    (0.82 - 0.05 * layer_order(object.layer) as f32 - 0.0005 * object.pick_priority as i32 as f32)
        .clamp(0.05, 0.95)
}

fn layer_order(layer: CadLayerKind) -> u8 {
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

/// Shared unit quad, allocated once.
///
/// The GPU mesh cache is keyed by `Arc::as_ptr` (see `BasicDevice::draw_mesh`),
/// so a fresh `Arc` every frame makes residency depend on allocator address
/// reuse: a hit is possible for different geometry and an eviction is arbitrary.
/// A `OnceLock` keeps one strong reference alive for the process, which makes
/// both the frame-local dedup key and the GPU cache key stable.
fn unit_quad_mesh() -> Arc<BasicMesh> {
    static UNIT_QUAD: OnceLock<Arc<BasicMesh>> = OnceLock::new();
    UNIT_QUAD
        .get_or_init(|| {
            Arc::new(BasicMesh::from_positions(
                &[
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                &[0, 1, 2, 0, 2, 3],
            ))
        })
        .clone()
}

/// Shared unit disc, allocated once, for the same cache-key reason as the quad.
///
/// A 16-gon fan wound counter-clockwise in local XY, matching the unit quad, so
/// back-face culling keeps behaving identically on both executors.
fn disc_mesh() -> Arc<BasicMesh> {
    const SEGMENTS: usize = 16;
    static UNIT_DISC: OnceLock<Arc<BasicMesh>> = OnceLock::new();
    UNIT_DISC
        .get_or_init(|| {
            let mut positions = Vec::with_capacity(SEGMENTS + 1);
            positions.push(Vec3::new(0.5, 0.5, 0.0));
            for step in 0..SEGMENTS {
                let angle = (step as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
                positions.push(Vec3::new(
                    0.5 + angle.cos() * 0.5,
                    0.5 + angle.sin() * 0.5,
                    0.0,
                ));
            }
            let mut indices = Vec::with_capacity(SEGMENTS * 3);
            for step in 0..SEGMENTS {
                let rim = step as u32 + 1;
                let next = ((step + 1) % SEGMENTS) as u32 + 1;
                indices.extend_from_slice(&[0, rim, next]);
            }
            Arc::new(BasicMesh::from_positions(&positions, &indices))
        })
        .clone()
}

fn canvas_view_projection(left: f32, right: f32, top: f32, bottom: f32) -> Mat4 {
    let width = (right - left).max(0.001);
    let height = (bottom - top).max(0.001);
    let scale_x = 2.0 / width;
    let scale_y = -2.0 / height;
    let translate_x = -(right + left) / width;
    let translate_y = (bottom + top) / height;

    Mat4::from_cols_array(&[
        scale_x,
        0.0,
        0.0,
        0.0,
        0.0,
        scale_y,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        translate_x,
        translate_y,
        0.0,
        1.0,
    ])
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use raf_electronics::{
        CadLayerKind, CadPickPriority, ElectronicComponent, PcbComponentPlacement, PcbLayer,
        PcbLayout, PcbTrace, Schematic,
    };

    use super::*;
    use crate::api_graphic_basic::command_list::BasicLine;

    fn options_at(width: u32, height: u32) -> CadSurfaceOptions {
        CadSurfaceOptions {
            world_bounds: Some([0.0, width as f32, 0.0, height as f32]),
            ..CadSurfaceOptions::default()
        }
    }

    /// Grid off, so a submitted frame contains only document geometry. Line
    /// work merges into one batch while it stays adjacent, so a grid batch would
    /// otherwise absorb the copper lines and hide the ordering under test.
    fn bare_options(width: u32, height: u32) -> CadSurfaceOptions {
        CadSurfaceOptions {
            show_grid: false,
            ..options_at(width, height)
        }
    }

    fn lines_of(frame: &CadSurfaceFrame) -> Vec<BasicLine> {
        frame
            .frame
            .commands
            .commands()
            .iter()
            .filter_map(|command| match command {
                crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                    lines,
                    ..
                } => Some(lines.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn mesh_colors(frame: &CadSurfaceFrame) -> Vec<[u8; 4]> {
        frame
            .frame
            .commands
            .commands()
            .iter()
            .flat_map(|command| match command {
                crate::api_graphic_basic::command_list::GraphicCommand::DrawMesh {
                    color, ..
                } => {
                    vec![*color]
                }
                crate::api_graphic_basic::command_list::GraphicCommand::DrawMeshBatch {
                    instances,
                    ..
                } => instances.iter().map(|instance| instance.color).collect(),
                _ => Vec::new(),
            })
            .collect()
    }

    fn manual_object(id: &str, kind: CadObjectKind, color: [u8; 4]) -> CadObject {
        CadObject {
            id: id.to_string(),
            source_id: None,
            kind,
            layer: CadLayerKind::Overlay,
            pick_priority: CadPickPriority::Overlay,
            rect: None,
            points: Vec::new(),
            line_paths: Vec::new(),
            label: None,
            net: None,
            net_id: None,
            color_rgba: color,
        }
    }

    #[test]
    fn cad_surface_records_rects_and_lines() {
        let mut schematic = Schematic::new("Render CAD");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(60.0, 64.0);
        let resistor_id = resistor.id;
        let resistor_id_text = resistor_id.to_string();
        schematic.add_component(resistor);
        schematic.add_wire(Vec2::new(12.0, 20.0), Vec2::new(80.0, 20.0), "N001");

        let scene = CadScene::from_schematic(&schematic);
        let frame = build_cad_surface_frame(&scene, 320, 200, CadSurfaceOptions::default());

        assert_eq!(frame.frame.width, 320);
        assert_eq!(frame.frame.height, 200);
        assert!(frame
            .objects
            .iter()
            .any(|object| object.kind == CadObjectKind::Component));
        assert!(frame
            .hit_regions
            .iter()
            .any(|region| region.kind == CadObjectKind::Component));
        assert_eq!(
            frame
                .hit_test(Vec2::new(60.0, 64.0))
                .map(|region| region.kind),
            Some(CadObjectKind::Component)
        );
        assert_eq!(
            frame
                .hit_test(Vec2::new(60.0, 64.0))
                .and_then(|region| region.source_id.as_deref()),
            Some(resistor_id_text.as_str())
        );
        assert!(!frame.frame.commands.commands().is_empty());
        assert!(frame.frame.commands.commands().iter().any(|command| {
            matches!(
                command,
                crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                    lines,
                    ..
                } if lines.len() >= 8
            )
        }));
        assert!(frame.frame.commands.commands().iter().any(|command| {
            matches!(
                command,
                crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                    lines,
                    ..
                } if lines.iter().any(|line| line.color == [112, 224, 136, 255])
            )
        }));

        let selected_frame = build_cad_surface_frame(
            &scene,
            320,
            200,
            CadSurfaceOptions {
                selected_source_ids: vec![*resistor_id.as_bytes()],
                ..CadSurfaceOptions::default()
            },
        );
        assert!(selected_frame
            .frame
            .commands
            .commands()
            .iter()
            .any(|command| {
                matches!(
                    command,
                    crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                        lines,
                        ..
                    } if lines.iter().any(|line| line.color == [255, 172, 64, 255])
                )
            }));
    }

    #[test]
    fn cad_surface_culls_offscreen_objects_before_recording_geometry() {
        let mut schematic = Schematic::new("CAD Culling");
        let mut visible = ElectronicComponent::resistor("10k");
        visible.position = Vec2::new(40.0, 40.0);
        let mut offscreen = ElectronicComponent::resistor("1M");
        offscreen.position = Vec2::new(900.0, 900.0);
        schematic.add_component(visible);
        schematic.add_component(offscreen);

        let scene = CadScene::from_schematic(&schematic);
        let frame = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));

        assert!(frame.objects.iter().all(|object| object
            .rect
            .map(|rect| rect.center.x < 200.0)
            .unwrap_or(true)));
        assert_eq!(frame.frame.stats.visible_entities, 3);
    }

    #[test]
    fn cad_surface_keeps_every_visible_schematic_wire_in_the_line_stream() {
        let mut schematic = Schematic::new("Wire visibility");
        schematic.add_wire(Vec2::new(10.0, 10.0), Vec2::new(90.0, 10.0), "N1");
        schematic.add_wire(Vec2::new(20.0, 20.0), Vec2::new(90.0, 70.0), "N2");
        schematic.add_wire(Vec2::new(30.0, 90.0), Vec2::new(110.0, 90.0), "N3");

        let scene = CadScene::from_schematic(&schematic);
        let frame = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));

        let wire_segments = lines_of(&frame)
            .into_iter()
            .filter(|line| line.color == [216, 221, 227, 255])
            .count();

        // The diagonal wire is represented by the same deterministic elbow
        // route used by the editor, so it contributes two GPU segments. None of
        // these three wires shares an endpoint, so no junction mark is added.
        assert_eq!(wire_segments, 4);
    }

    #[test]
    fn cad_surface_leaves_schematic_symbol_artwork_to_the_native_overlay() {
        let mut schematic = Schematic::new("Native symbol overlay");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(80.0, 60.0);
        schematic.add_component(resistor);

        let scene = CadScene::from_schematic(&schematic);
        let frame = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));

        let symbol_lines = lines_of(&frame)
            .into_iter()
            .filter(|line| line.color == [245, 245, 246, 255])
            .collect::<Vec<_>>();

        assert!(symbol_lines.is_empty());
    }

    #[test]
    fn visual_grid_step_uses_stable_density_levels() {
        assert_eq!(visual_grid_step(20.0, 20.0), 20.0);
        assert_eq!(visual_grid_step(20.0, 6.0), 40.0);
        assert_eq!(visual_grid_step(20.0, 2.5), 100.0);
        assert!(visual_grid_step(20.0, 0.4) >= 200.0);
    }

    #[test]
    fn raster_scale_multiplies_every_emitted_stroke_width() {
        let mut schematic = Schematic::new("Raster scale");
        schematic.add_wire(Vec2::new(10.0, 10.0), Vec2::new(90.0, 10.0), "N1");

        let scene = CadScene::from_schematic(&schematic);
        let single = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));
        let double = build_cad_surface_frame(
            &scene,
            160,
            120,
            CadSurfaceOptions {
                raster_scale: 2.0,
                ..options_at(160, 120)
            },
        );

        let single_max = lines_of(&single)
            .into_iter()
            .map(|line| line.width)
            .fold(0.0f32, f32::max);
        let double_max = lines_of(&double)
            .into_iter()
            .map(|line| line.width)
            .fold(0.0f32, f32::max);
        assert!(single_max > 1.0, "the frame must contain real strokes");
        assert!(
            (double_max - single_max * 2.0).abs() < 0.001,
            "every stroke must scale, got {single_max} then {double_max}"
        );
    }

    #[test]
    fn airwires_render_as_dashed_guides_instead_of_solid_conductor() {
        let scene = CadScene {
            surface: CadSurfaceKind::Pcb,
            objects: vec![CadObject {
                points: vec![Vec2::new(10.0, 10.0), Vec2::new(300.0, 10.0)],
                label: Some("N1".to_string()),
                net: Some("N1".to_string()),
                ..manual_object("airwire:0", CadObjectKind::Airwire, [245, 245, 246, 180])
            }],
        };
        let frame = build_cad_surface_frame(&scene, 400, 200, options_at(400, 200));
        let airwire_lines = lines_of(&frame)
            .into_iter()
            .filter(|line| line.color == [245, 245, 246, 180])
            .count();

        assert!(
            airwire_lines >= 4,
            "a long pending connection must be dashed, got {airwire_lines} segments"
        );
    }

    #[test]
    fn pcb_traces_paint_after_the_component_body_that_used_to_hide_them() {
        let seed = ElectronicComponent::resistor("10k").id;
        let mut layout = PcbLayout::new("Z order");
        layout.components.push(PcbComponentPlacement {
            component_id: seed,
            designator: "R1".to_string(),
            value: "10k".to_string(),
            footprint: "0805".to_string(),
            position: Vec2::new(100.0, 80.0),
            rotation: 0.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["A".to_string(), "B".to_string()],
        });
        layout.traces.push(PcbTrace {
            id: seed,
            net: "A".to_string(),
            layer: PcbLayer::TopCopper,
            width: 6.0,
            points: vec![Vec2::new(60.0, 80.0), Vec2::new(140.0, 80.0)],
        });

        let scene = CadScene::from_pcb(&layout);
        let frame = build_cad_surface_frame(&scene, 320, 240, bare_options(320, 240));
        let body_index = frame
            .frame
            .commands
            .commands()
            .iter()
            .position(|command| {
                matches!(
                    command,
                    crate::api_graphic_basic::command_list::GraphicCommand::DrawMesh {
                        color: [34, 34, 38, alpha],
                        ..
                    } if *alpha == PCB_BODY_ALPHA
                )
            })
            .expect("component body mesh");
        let trace_index = frame
            .frame
            .commands
            .commands()
            .iter()
            .position(|command| {
                matches!(
                    command,
                    crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch { lines, .. }
                    if lines.iter().any(|line| line.color == [207, 150, 91, 255])
                )
            })
            .expect("copper trace lines");

        assert!(
            body_index < trace_index,
            "the silhouette must be the backdrop of the copper, not its lid"
        );
    }

    #[test]
    fn pcb_component_bodies_are_no_longer_opaque_copper_lids() {
        let seed = ElectronicComponent::resistor("10k").id;
        let mut layout = PcbLayout::new("Body alpha");
        layout.components.push(PcbComponentPlacement {
            component_id: seed,
            designator: "R1".to_string(),
            value: "10k".to_string(),
            footprint: "0805".to_string(),
            position: Vec2::new(100.0, 80.0),
            rotation: 0.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["A".to_string(), "B".to_string()],
        });

        let scene = CadScene::from_pcb(&layout);
        let frame = build_cad_surface_frame(&scene, 320, 240, options_at(320, 240));
        assert!(mesh_colors(&frame)
            .iter()
            .any(|color| *color == [34, 34, 38, PCB_BODY_ALPHA]));
        assert!(!mesh_colors(&frame)
            .iter()
            .any(|color| *color == [34, 34, 38, 255]));
    }

    #[test]
    fn net_highlight_emphasises_one_net_and_recedes_the_rest() {
        let mut schematic = Schematic::new("Net highlight");
        schematic.add_wire(Vec2::new(10.0, 10.0), Vec2::new(90.0, 10.0), "FOCUS");
        schematic.add_wire(Vec2::new(10.0, 60.0), Vec2::new(90.0, 60.0), "OTHER");
        let scene = CadScene::from_schematic(&schematic);
        let focus_net = scene
            .objects
            .iter()
            .find(|object| {
                object.kind == CadObjectKind::Wire && object.net.as_deref() == Some("FOCUS")
            })
            .and_then(|object| object.net_id)
            .expect("focus net id");

        let frame = build_cad_surface_frame(
            &scene,
            160,
            120,
            CadSurfaceOptions {
                highlight_net_id: Some(focus_net),
                ..options_at(160, 120)
            },
        );
        let lines = lines_of(&frame);
        let focused = lines
            .iter()
            .filter(|line| line.color[3] == 255 && line.color == [216, 221, 227, 255])
            .count();
        let receded = lines
            .iter()
            .filter(|line| line.color[0] == 216 && line.color[3] < 255)
            .count();

        assert!(focused >= 1, "the focused net keeps full contrast");
        assert!(receded >= 1, "the other net must recede");
        assert!(
            lines
                .iter()
                .filter(|line| line.color[0] == 216)
                .all(|line| line.color[3] == 255 || line.color[3] == NET_DIM_ALPHA),
            "only the focused net may keep its full alpha"
        );
    }

    #[test]
    fn hover_uses_a_cold_outline_and_selection_a_warm_halo() {
        let mut schematic = Schematic::new("Hover vs selection");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(60.0, 60.0);
        let resistor_id = resistor.id;
        schematic.add_component(resistor);

        let scene = CadScene::from_schematic(&schematic);
        let hovered = build_cad_surface_frame(
            &scene,
            160,
            120,
            CadSurfaceOptions {
                hovered_source_ids: vec![*resistor_id.as_bytes()],
                ..options_at(160, 120)
            },
        );
        let selected = build_cad_surface_frame(
            &scene,
            160,
            120,
            CadSurfaceOptions {
                selected_source_ids: vec![*resistor_id.as_bytes()],
                ..options_at(160, 120)
            },
        );

        assert!(lines_of(&hovered)
            .iter()
            .any(|line| line.color == HOVER_COLOR));
        assert!(!lines_of(&hovered)
            .iter()
            .any(|line| line.color == [255, 172, 64, 255]));
        assert!(lines_of(&selected)
            .iter()
            .any(|line| line.color == [255, 172, 64, 255]));
        assert!(!lines_of(&selected)
            .iter()
            .any(|line| line.color == HOVER_COLOR));
    }

    #[test]
    fn drc_marks_encode_severity_in_shape_not_only_color() {
        let mark = |color: [u8; 4]| CadScene {
            surface: CadSurfaceKind::Pcb,
            objects: vec![CadObject {
                rect: Some(CadRect::new(Vec2::new(40.0, 40.0), Vec2::new(16.0, 16.0))),
                label: Some("violation".to_string()),
                ..manual_object("drc:rule", CadObjectKind::DrcMarker, color)
            }],
        };
        assert_eq!(
            drc_shape(&mark([220, 66, 58, 255]).objects[0]),
            DrcShape::ErrorSquare
        );
        assert_eq!(
            drc_shape(&mark([245, 180, 65, 255]).objects[0]),
            DrcShape::WarningDiamond
        );
        assert_eq!(
            drc_shape(&mark([170, 170, 178, 255]).objects[0]),
            DrcShape::InfoDisc
        );
        // An unreadable severity must never be downgraded to an advisory.
        assert_eq!(
            drc_shape(&mark([0, 0, 0, 255]).objects[0]),
            DrcShape::ErrorSquare
        );

        let frame =
            build_cad_surface_frame(&mark([220, 66, 58, 255]), 160, 120, options_at(160, 120));
        assert!(!frame.frame.commands.commands().is_empty());
    }

    #[test]
    fn shared_wire_endpoints_get_exactly_one_junction_mark() {
        let junction_scene = |shared: bool| CadScene {
            surface: CadSurfaceKind::Schematic,
            objects: vec![
                CadObject {
                    points: vec![Vec2::new(20.0, 20.0), Vec2::new(60.0, 20.0)],
                    net: Some("N1".to_string()),
                    ..manual_object("wire:a", CadObjectKind::Wire, [216, 221, 227, 255])
                },
                CadObject {
                    points: if shared {
                        vec![Vec2::new(60.0, 20.0), Vec2::new(60.0, 60.0)]
                    } else {
                        vec![Vec2::new(80.0, 20.0), Vec2::new(80.0, 60.0)]
                    },
                    net: Some("N1".to_string()),
                    ..manual_object("wire:b", CadObjectKind::Wire, [216, 221, 227, 255])
                },
            ],
        };

        let shared_frame =
            build_cad_surface_frame(&junction_scene(true), 160, 120, options_at(160, 120));
        let split_frame =
            build_cad_surface_frame(&junction_scene(false), 160, 120, options_at(160, 120));

        let shared_meshes = shared_frame.frame.commands.stats().mesh_instances;
        let split_meshes = split_frame.frame.commands.stats().mesh_instances;
        assert_eq!(
            shared_meshes - split_meshes,
            1,
            "one shared endpoint paints one disc; a near miss paints none"
        );
    }

    #[test]
    fn net_labels_get_a_subtle_plate_instead_of_an_opaque_card() {
        let mut schematic = Schematic::new("Net label plate");
        schematic.add_wire(Vec2::new(10.0, 20.0), Vec2::new(80.0, 20.0), "SIGNAL_A");
        let scene = CadScene::from_schematic(&schematic);
        let frame = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));
        let colors = mesh_colors(&frame);

        assert!(
            colors.iter().any(|color| color[3] == NET_LABEL_PLATE_ALPHA),
            "the overlay text needs a plate to sit on"
        );
        assert!(
            !colors.iter().any(|color| *color == [18, 18, 20, 220]),
            "the old opaque label card must be gone"
        );
        assert!(
            !colors.iter().any(|color| *color == [18, 18, 20, 210]),
            "the displaced label placeholder must be gone"
        );
    }

    #[test]
    fn hit_regions_are_skippable_without_touching_the_painted_frame() {
        let mut schematic = Schematic::new("Hit regions");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(40.0, 40.0);
        schematic.add_component(resistor);

        let scene = CadScene::from_schematic(&schematic);
        let with_regions = build_cad_surface_frame(&scene, 160, 120, options_at(160, 120));
        let without_regions = build_cad_surface_frame(
            &scene,
            160,
            120,
            CadSurfaceOptions {
                collect_hit_regions: false,
                ..options_at(160, 120)
            },
        );

        assert!(!with_regions.hit_regions.is_empty());
        assert!(without_regions.hit_regions.is_empty());
        assert_eq!(
            with_regions.frame.commands.stats(),
            without_regions.frame.commands.stats(),
            "skipping pick regions must not change a single submitted command"
        );
    }

    #[test]
    fn unit_quad_and_disc_meshes_are_shared_across_frames() {
        assert!(Arc::ptr_eq(&unit_quad_mesh(), &unit_quad_mesh()));
        assert!(Arc::ptr_eq(&disc_mesh(), &disc_mesh()));
    }
}
