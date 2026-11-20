//! Schematic grid, wires, junctions and net label anchors.

use std::cell::RefCell;
use std::collections::HashMap;

use glam::Vec2;
use raf_electronics::{CadObject, CadObjectKind, CadRect, CadScene};

use crate::api_graphic_basic::command_list::BasicCommandList;

use super::overlays::{recede, Emphasis};
use super::paint::{record_disc, record_line};
use super::pcb::{object_line_color, object_line_width, object_z};
use super::{CadSurfaceOptions, Density, MIN_DEVICE_WIDTH};

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

/// Alpha of the short leader connecting a net name to its wire.
pub(super) const NET_LABEL_ANCHOR_COLOR: [u8; 4] = [212, 119, 26, 255];
pub(super) const NET_LABEL_ANCHOR_ALPHA: u8 = 172;

pub(super) fn record_grid(
    commands: &mut BasicCommandList,
    options: &CadSurfaceOptions,
    bounds: [f32; 4],
    density: Density,
) {
    let base_step = options.grid_step.max(0.05);
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

    if options.dotted_grid {
        // visual_grid_step enforces >=12px spacing: a 4K canvas stays bounded.
        for ix in start_x..=end_x {
            for iy in start_y..=end_y {
                let point=Vec2::new(ix as f32*step,iy as f32*step);
                let major=ix.rem_euclid(5)==0 && iy.rem_euclid(5)==0;
                let color=if major {options.major_grid_color}else{options.grid_color};
                record_line(commands,point,point+Vec2::new(0.75*density.world_per_pixel,0.0),0.95,if major {1.7}else{1.0},color,density);
            }
        }
        return;
    }
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

pub(super) fn visual_grid_step(base_step: f32, screen_step: f32) -> f32 {
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
    (base_step.max(0.05) * multiplier).max(base_step.max(0.05))
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

/// Small leader between a net name and the wire it identifies.
///
/// Net names render above the wire in the retained text overlay. Keeping only
/// this short line avoids painting a card over every orthogonal wire segment.
pub(super) fn record_net_label_anchor(
    commands: &mut BasicCommandList,
    rect: CadRect,
    z: f32,
    anchor_color: [u8; 4],
    density: Density,
) {
    if anchor_color[3] == 0 || rect.size.x <= 0.0 || rect.size.y <= 0.0 {
        return;
    }
    let leader_world = density.world(density.pixels(4.0));
    record_line(
        commands,
        rect.center,
        rect.center - Vec2::Y * leader_world,
        z - 0.005,
        1.0,
        anchor_color,
        density,
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
pub(super) fn assign_junction_marks(sorted: &[&CadObject], points: &[Vec2]) -> HashMap<usize, Vec<Vec2>> {
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
pub(super) fn scene_junction_points(scene: &CadScene) -> Vec<Vec2> {
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
pub(super) fn record_junction_marks(
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
        .world(density.pixels(object_line_width(object, options, density)) * JUNCTION_DIAMETER_RATIO)
        .max(density.world(MIN_DEVICE_WIDTH));
    let z = object_z(object) - 0.024;
    for point in points {
        record_disc(commands, disc_id, *point, diameter * 0.5, z, color);
    }
}
