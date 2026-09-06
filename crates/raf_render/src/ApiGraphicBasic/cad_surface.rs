use std::sync::Arc;

use glam::{Mat4, Quat, Vec2, Vec3};
use raf_electronics::{CadLayerKind, CadObject, CadObjectKind, CadPickPriority, CadRect, CadScene};

use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::scene_renderer::{FrameStats, SceneRenderFrame};

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
    pub selected_object_ids: Vec<String>,
    /// Stable model UUID bytes highlighted by the retained surface.
    pub selected_source_ids: Vec<[u8; 16]>,
    pub selection_color: [u8; 4],
    pub selection_width: f32,
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
            grid_color: [180, 186, 200, 14],
            major_grid_color: [220, 226, 240, 30],
            axis_color: [220, 226, 240, 40],
            symbol_color: [245, 245, 246, 255],
            trace_width: 3.0,
            wire_width: 2.0,
            selected_object_ids: Vec::new(),
            selected_source_ids: Vec::new(),
            selection_color: [255, 172, 64, 255],
            selection_width: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CadSurfaceFrame {
    pub frame: SceneRenderFrame,
    pub objects: Vec<CadSurfaceObject>,
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

pub fn build_cad_surface_frame(
    scene: &CadScene,
    width: u32,
    height: u32,
    options: CadSurfaceOptions,
) -> CadSurfaceFrame {
    let mut commands = BasicCommandList::new();
    commands.clear(options.clear_color);
    let quad_id = commands.register_mesh(unit_quad_mesh());
    let mut objects = Vec::with_capacity(scene.objects.len());
    let mut hit_regions = Vec::with_capacity(scene.objects.len());
    let bounds = options
        .world_bounds
        .unwrap_or([0.0, width as f32, 0.0, height as f32]);

    if options.show_grid {
        record_grid(
            &mut commands,
            &options,
            bounds,
            [width as f32, height as f32],
        );
    }

    let mut sorted = scene.objects.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|object| (layer_order(object.layer), object.pick_priority));

    let mut visible_entities = 0u32;
    for object in sorted {
        if !cad_object_intersects_bounds(object, bounds) {
            continue;
        }
        visible_entities = visible_entities.saturating_add(1);
        record_object(
            object,
            quad_id,
            &options,
            &mut commands,
            scene.surface == raf_electronics::CadSurfaceKind::Schematic,
        );
        if object_is_selected(object, &options) {
            record_selection_outline(object, &options, &mut commands);
        }
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
        if let Some(hit_region) = cad_hit_region(object, &options) {
            hit_regions.push(hit_region);
        }
    }

    CadSurfaceFrame {
        frame: SceneRenderFrame {
            commands,
            view_proj: canvas_view_projection(bounds[0], bounds[1], bounds[2], bounds[3]),
            light_dir: Vec3::Z,
            width,
            height,
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
        || object
            .source_id
            .map(|source_id| {
                options
                    .selected_source_ids
                    .iter()
                    .any(|selected| selected == source_id.as_bytes())
            })
            .unwrap_or(false)
}

fn record_selection_outline(
    object: &CadObject,
    options: &CadSurfaceOptions,
    commands: &mut BasicCommandList,
) {
    let z = object_z(object) - 0.08;
    if !object.line_paths.is_empty() {
        let width = match object.kind {
            CadObjectKind::Pin | CadObjectKind::Pad => 2.0,
            _ => options.selection_width.max(1.0),
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
            options.selection_width.max(1.0),
            options.selection_color,
        );
        return;
    }

    let base_width = match object.kind {
        CadObjectKind::Trace => options.trace_width,
        CadObjectKind::Airwire => 1.25,
        _ => options.wire_width,
    };
    let width = base_width.max(1.0) + options.selection_width.max(1.0) * 2.0;
    for segment in object.points.windows(2) {
        record_line(
            commands,
            segment[0],
            segment[1],
            z,
            width,
            options.selection_color,
        );
        // Keep the conductor visible inside its thin selection halo.
        record_line(
            commands,
            segment[0],
            segment[1],
            z - 0.01,
            base_width,
            object_line_color(object),
        );
    }
}

fn record_grid(
    commands: &mut BasicCommandList,
    options: &CadSurfaceOptions,
    bounds: [f32; 4],
    viewport_size: [f32; 2],
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
    let world_span = Vec2::new(right - left, bottom - top);
    let pixels_per_world =
        (viewport_size[0].max(1.0) / world_span.x).min(viewport_size[1].max(1.0) / world_span.y);
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
    options: &CadSurfaceOptions,
    commands: &mut BasicCommandList,
    schematic: bool,
) {
    if object.kind == CadObjectKind::DrcMarker && !options.show_drc_markers {
        return;
    }

    let z = object_z(object);
    if !schematic
        || !matches!(
            object.kind,
            CadObjectKind::Component | CadObjectKind::Pin | CadObjectKind::NetLabel
        )
    {
        if let Some(rect) = object.rect {
            let fill = object_fill(object);
            record_rect(commands, quad_id, rect, z, fill);
            record_border(
                commands,
                rect,
                z - 0.01,
                object_border_width(object),
                object_border(object),
            );
        }
    }

    if object.points.len() >= 2 {
        let width = match object.kind {
            CadObjectKind::Trace => options.trace_width,
            CadObjectKind::Airwire => 1.25,
            _ => options.wire_width,
        };
        let line_color = object_line_color(object);
        for segment in object.points.windows(2) {
            record_line(
                commands,
                segment[0],
                segment[1],
                z - 0.02,
                width,
                line_color,
            );
        }
    }

    // Schematic symbols are independent strokes, not one connected path.
    // Keep them in the same backend-neutral line stream as wires so AGB/WGPU
    // can batch them without the editor repainting the static geometry.
    for path in &object.line_paths {
        for segment in path.windows(2) {
            let line_color = if schematic
                && object.color_rgba[3] == u8::MAX
                && !matches!(object.kind, CadObjectKind::Pin | CadObjectKind::Pad)
            {
                options.symbol_color
            } else {
                object_line_color(object)
            };
            record_line(commands, segment[0], segment[1], z - 0.015, 2.0, line_color);
        }
    }

    // Text is supplied by the retained editor overlay for schematics. AGB
    // keeps the CAD scene geometry here; drawing opaque placeholder rectangles
    // for labels made the schematic look like a stack of cards and hid text.
    if !schematic && options.show_labels {
        record_label_placeholder(object, quad_id, commands, z - 0.03);
    }
}

fn record_label_placeholder(
    object: &CadObject,
    quad_id: usize,
    commands: &mut BasicCommandList,
    z: f32,
) {
    let Some(label) = object.label.as_ref() else {
        return;
    };
    if label.trim().is_empty() {
        return;
    }

    let center = object
        .rect
        .map(|rect| rect.center + Vec2::new(0.0, rect.size.y * 0.5 + 10.0))
        .or_else(|| object.points.first().copied())
        .unwrap_or(Vec2::ZERO);
    let width = (label.len() as f32 * 7.0).clamp(28.0, 180.0);
    let rect = CadRect::new(center, Vec2::new(width, 16.0));
    record_rect(commands, quad_id, rect, z, [18, 18, 20, 210]);
    record_border(commands, rect, z - 0.01, 1.0, [56, 56, 62, 220]);
}

fn record_rect(
    commands: &mut BasicCommandList,
    quad_id: usize,
    rect: CadRect,
    z: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || rect.size.x <= 0.0 || rect.size.y <= 0.0 {
        return;
    }
    let min = rect.center - rect.size * 0.5;
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::new(rect.size.x, rect.size.y, 1.0),
        Quat::IDENTITY,
        Vec3::new(min.x, min.y, z),
    );
    commands.draw_mesh(quad_id, transform, color);
}

fn record_border(
    commands: &mut BasicCommandList,
    rect: CadRect,
    z: f32,
    width: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || width <= 0.0 || rect.size.x <= 0.0 || rect.size.y <= 0.0 {
        return;
    }
    let min = rect.center - rect.size * 0.5;
    let max = rect.center + rect.size * 0.5;
    record_line(commands, min, Vec2::new(max.x, min.y), z, width, color);
    record_line(commands, Vec2::new(max.x, min.y), max, z, width, color);
    record_line(commands, max, Vec2::new(min.x, max.y), z, width, color);
    record_line(commands, Vec2::new(min.x, max.y), min, z, width, color);
}

fn record_line(
    commands: &mut BasicCommandList,
    start: Vec2,
    end: Vec2,
    z: f32,
    width: f32,
    color: [u8; 4],
) {
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

fn object_fill(object: &CadObject) -> [u8; 4] {
    match object.kind {
        CadObjectKind::Component => object.color_rgba,
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

fn unit_quad_mesh() -> Arc<BasicMesh> {
    Arc::new(BasicMesh::from_positions(
        &[
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        &[0, 1, 2, 0, 2, 3],
    ))
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
    use raf_electronics::{CadObjectKind, ElectronicComponent, Schematic};

    use super::*;

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
        let frame = build_cad_surface_frame(
            &scene,
            160,
            120,
            CadSurfaceOptions {
                world_bounds: Some([0.0, 160.0, 0.0, 120.0]),
                ..CadSurfaceOptions::default()
            },
        );

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
        let frame = build_cad_surface_frame(
            &scene,
            160,
            120,
            CadSurfaceOptions {
                world_bounds: Some([0.0, 160.0, 0.0, 120.0]),
                ..CadSurfaceOptions::default()
            },
        );

        let wire_segments = frame
            .frame
            .commands
            .commands()
            .iter()
            .filter_map(|command| match command {
                crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                    lines,
                    ..
                } => Some(lines),
                _ => None,
            })
            .flatten()
            .filter(|line| line.color == [216, 221, 227, 255])
            .count();

        // The diagonal wire is represented by the same deterministic elbow
        // route used by the editor, so it contributes two GPU segments.
        assert_eq!(wire_segments, 4);
    }

    #[test]
    fn cad_surface_leaves_schematic_symbol_artwork_to_the_native_overlay() {
        let mut schematic = Schematic::new("Native symbol overlay");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(80.0, 60.0);
        schematic.add_component(resistor);

        let scene = CadScene::from_schematic(&schematic);
        let frame = build_cad_surface_frame(
            &scene,
            160,
            120,
            CadSurfaceOptions {
                world_bounds: Some([0.0, 160.0, 0.0, 120.0]),
                ..CadSurfaceOptions::default()
            },
        );

        let symbol_lines = frame
            .frame
            .commands
            .commands()
            .iter()
            .filter_map(|command| match command {
                crate::api_graphic_basic::command_list::GraphicCommand::DrawLineBatch {
                    lines,
                    ..
                } => Some(lines),
                _ => None,
            })
            .flatten()
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
}
