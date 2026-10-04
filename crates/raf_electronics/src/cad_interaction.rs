//! Renderer-neutral retained CAD picking and selection.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cad_scene::{CadLayerKind, CadObject, CadObjectKind, CadPickPriority, CadScene};

/// Longest gap between two clicks that may still be treated as one double click.
///
/// A pair has to be recognised fast enough not to be read as two unrelated
/// clicks, and slow enough not to swallow two deliberate clicks. 0.32 s is the
/// conventional desktop value: slower than the ~0.2 s a mouse double click
/// takes, faster than a comfortable deliberate second click.
const DOUBLE_CLICK_WINDOW_SECONDS: f64 = 0.32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CadSelection {
    pub object_id: String,
    pub source_id: Option<Uuid>,
    pub kind: CadObjectKind,
    pub layer: CadLayerKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CadPickHit {
    pub selection: CadSelection,
    pub priority: CadPickPriority,
    pub distance: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CadInteractionState {
    pub hovered: Option<CadSelection>,
    pub selected: Option<CadSelection>,
    /// Timestamp of the last primary release this surface saw, used as the first
    /// click of a possible double click pair.
    ///
    /// Deliberately not a counter. The raw input snapshot a CAD canvas consumes
    /// carries button state and a timestamp but no click count and no double
    /// click flag, so the pair is reconstructed here from two samples the surface
    /// already owns. `RafUI` surfaces get the same information for free through
    /// `UiEventKind::DoubleClick`, which the CAD canvas cannot receive because it
    /// is not a `RafUI` surface.
    pub last_primary_release_seconds: Option<f64>,
    /// World position of `last_primary_release_seconds`, which is what decides
    /// whether two clicks mean one double click or two different intentions.
    pub last_primary_release_world: Option<Vec2>,
}

impl CadInteractionState {
    pub fn update_hover(
        &mut self,
        scene: &CadScene,
        point: Vec2,
        tolerance: f32,
    ) -> Option<CadPickHit> {
        let hit = pick(scene, point, tolerance);
        self.hovered = hit.as_ref().map(|hit| hit.selection.clone());
        hit
    }

    pub fn update_hover_editable(
        &mut self,
        scene: &CadScene,
        point: Vec2,
        tolerance: f32,
    ) -> Option<CadPickHit> {
        let hit = pick_editable(scene, point, tolerance);
        self.hovered = hit.as_ref().map(|hit| hit.selection.clone());
        hit
    }

    pub fn select_at(
        &mut self,
        scene: &CadScene,
        point: Vec2,
        tolerance: f32,
    ) -> Option<CadPickHit> {
        let hit = self.update_hover(scene, point, tolerance);
        self.selected = hit.as_ref().map(|hit| hit.selection.clone());
        hit
    }

    /// Selection variant used by editing gestures: painted overlays never win.
    pub fn select_editable_at(
        &mut self,
        scene: &CadScene,
        point: Vec2,
        tolerance: f32,
    ) -> Option<CadPickHit> {
        let hit = self.update_hover_editable(scene, point, tolerance);
        self.selected = hit.as_ref().map(|hit| hit.selection.clone());
        hit
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Reports whether this primary release completes a double click, and
    /// consumes the pair when it does.
    ///
    /// Call it once per primary release, on the release itself. The release is
    /// both the sample that closes a pair and the sample that opens the next one,
    /// which is why this method records before it judges: the current click
    /// becomes the pending first click, and the click before it decides whether
    /// the two form a pair.
    ///
    /// `max_distance` is expressed in the same world units as `world`. A caller
    /// that thinks in screen pixels converts it with the camera zoom, the way
    /// every other tolerance on this canvas is authored, so the pair stays the
    /// same gesture at every zoom level.
    ///
    /// A detected pair clears the pending click, so three quick clicks cannot
    /// fire twice: the third one has no partner and simply becomes the pending
    /// click again. Any release that is too late, too far, or out of order is
    /// remembered as the new pending click instead, so a click that lands outside
    /// the window does not poison the click that follows it.
    ///
    /// The time comparison rejects a negative gap on purpose: a timestamp that
    /// goes backwards is a reordered or reset clock, not a double click.
    pub fn consume_double_click(
        &mut self,
        world: Vec2,
        time_seconds: f64,
        max_distance: f32,
    ) -> bool {
        let previous_time = self.last_primary_release_seconds;
        let previous_world = self.last_primary_release_world;
        self.last_primary_release_seconds = Some(time_seconds);
        self.last_primary_release_world = Some(world);
        let (Some(previous_time), Some(previous_world)) = (previous_time, previous_world) else {
            return false;
        };
        let elapsed = time_seconds - previous_time;
        if !(elapsed >= 0.0 && elapsed <= DOUBLE_CLICK_WINDOW_SECONDS) {
            return false;
        }
        if previous_world.distance(world) > max_distance.max(0.0) {
            return false;
        }
        self.last_primary_release_seconds = None;
        self.last_primary_release_world = None;
        true
    }

    /// Forgets the pending click without judging it.
    ///
    /// Used when the surface a click happened on is no longer the surface being
    /// routed to, so a release from one document cannot pair with a release from
    /// another one.
    pub fn clear_double_click(&mut self) {
        self.last_primary_release_seconds = None;
        self.last_primary_release_world = None;
    }
}

/// Picks the object the user visually aimed at, ignoring non-interactive overlays.
///
/// NetLabel, Airwire and DrcMarker are visual aids painted above the document and
/// must never win a click over the component, pin, wire or trace beneath them.
/// Among the remaining hits the closest candidate wins; pick priority is only used
/// to break exact-distance ties.
pub fn pick_editable(scene: &CadScene, point: Vec2, tolerance: f32) -> Option<CadPickHit> {
    best_candidate(scene, point, tolerance, |object| {
        !is_painted_overlay(object)
    })
}

pub fn pick(scene: &CadScene, point: Vec2, tolerance: f32) -> Option<CadPickHit> {
    best_candidate(scene, point, tolerance, |_| true)
}

/// Shared candidate search: closest candidate first, pick priority only as a tie
/// breaker.
///
/// Sorting by distance before priority is what makes the click match what the
/// user sees. Ranking by priority alone let a large, invisible net label sitting
/// on top of a symbol beat the pin the cursor was actually pointing at.
fn best_candidate(
    scene: &CadScene,
    point: Vec2,
    tolerance: f32,
    accept: impl Fn(&CadObject) -> bool,
) -> Option<CadPickHit> {
    let tolerance = tolerance.max(0.5);
    scene
        .objects
        .iter()
        .filter(|object| accept(*object))
        .filter_map(|object| pick_object(object, point, tolerance))
        // `min_by`, not `max_by`: the closest candidate has to win, so distance
        // ascends and priority descends. Ranking by priority alone let a large,
        // invisible net label sitting on top of a symbol beat the pin the cursor
        // was actually pointing at.
        .min_by(|left, right| {
            left.distance.total_cmp(&right.distance).then_with(|| {
                (right.priority, layer_order(right.selection.layer))
                    .cmp(&(left.priority, layer_order(left.selection.layer)))
            })
        })
}

/// Reports whether the object is painted above the document instead of being part
/// of it.
///
/// The three kinds are the diagnostic aids themselves; the layer check also
/// catches transient editor previews (placement ghost, wire rubber band, board
/// outline drag) that reuse document kinds but live on an overlay layer. Either
/// way the object is decoration, so a click must fall through to the real
/// geometry underneath.
fn is_painted_overlay(object: &CadObject) -> bool {
    matches!(
        object.kind,
        CadObjectKind::NetLabel | CadObjectKind::Airwire | CadObjectKind::DrcMarker
    ) || matches!(
        object.layer,
        CadLayerKind::Overlay | CadLayerKind::Airwire | CadLayerKind::Drc
    )
}

fn pick_object(object: &CadObject, point: Vec2, tolerance: f32) -> Option<CadPickHit> {
    let distance = if let Some(rect) = object.rect {
        let half = rect.size * 0.5;
        let min = rect.center - half;
        let max = rect.center + half;
        let closest = point.clamp(min, max);
        point.distance(closest)
    } else {
        object
            .points
            .windows(2)
            .map(|segment| point_segment_distance(point, segment[0], segment[1]))
            .fold(f32::INFINITY, f32::min)
    };
    if distance > tolerance {
        return None;
    }
    Some(CadPickHit {
        selection: CadSelection {
            object_id: object.id.clone(),
            source_id: object.source_id,
            kind: object.kind,
            layer: object.layer,
        },
        priority: object.pick_priority,
        distance,
    })
}

fn point_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let delta = end - start;
    let length_squared = delta.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let projection = ((point - start).dot(delta) / length_squared).clamp(0.0, 1.0);
    point.distance(start + delta * projection)
}

fn layer_order(layer: CadLayerKind) -> u8 {
    match layer {
        CadLayerKind::Schematic => 10,
        CadLayerKind::PcbTopCopper => 20,
        CadLayerKind::PcbBottomCopper => 19,
        CadLayerKind::PcbSilkscreen => 30,
        CadLayerKind::Airwire => 15,
        CadLayerKind::Overlay => 40,
        CadLayerKind::Drc => 50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cad_scene::{CadObject, CadRect, CadSurfaceKind};

    fn rect_object(
        id: &str,
        kind: CadObjectKind,
        layer: CadLayerKind,
        priority: CadPickPriority,
        center: Vec2,
        size: Vec2,
    ) -> CadObject {
        CadObject {
            id: id.to_string(),
            source_id: None,
            kind,
            layer,
            pick_priority: priority,
            rect: Some(CadRect::new(center, size)),
            points: Vec::new(),
            line_paths: Vec::new(),
            label: None,
            net: None,
            net_id: None,
            color_rgba: [0; 4],
        }
    }

    fn polyline_object(
        id: &str,
        kind: CadObjectKind,
        priority: CadPickPriority,
        points: Vec<Vec2>,
    ) -> CadObject {
        let mut object = rect_object(
            id,
            kind,
            CadLayerKind::Schematic,
            priority,
            Vec2::ZERO,
            Vec2::ZERO,
        );
        object.rect = None;
        object.points = points;
        object
    }

    #[test]
    fn pin_priority_wins_over_component_body() {
        let mut scene = CadScene {
            surface: CadSurfaceKind::Schematic,
            objects: Vec::new(),
        };
        scene.objects.push(CadObject {
            id: "component".to_string(),
            source_id: None,
            kind: CadObjectKind::Component,
            layer: CadLayerKind::Schematic,
            pick_priority: CadPickPriority::Component,
            rect: Some(CadRect::new(Vec2::ZERO, Vec2::splat(20.0))),
            points: Vec::new(),
            line_paths: Vec::new(),
            label: None,
            net: None,
            net_id: None,
            color_rgba: [0; 4],
        });
        scene.objects.push(CadObject {
            id: "pin".to_string(),
            source_id: None,
            kind: CadObjectKind::Pin,
            layer: CadLayerKind::Schematic,
            pick_priority: CadPickPriority::Pin,
            rect: Some(CadRect::new(Vec2::ZERO, Vec2::splat(4.0))),
            points: Vec::new(),
            line_paths: Vec::new(),
            label: None,
            net: None,
            net_id: None,
            color_rgba: [0; 4],
        });

        assert_eq!(
            pick(&scene, Vec2::ZERO, 2.0).unwrap().selection.object_id,
            "pin"
        );
    }

    #[test]
    fn polyline_pick_respects_tolerance() {
        let object = CadObject {
            id: "wire".to_string(),
            source_id: None,
            kind: CadObjectKind::Wire,
            layer: CadLayerKind::Schematic,
            pick_priority: CadPickPriority::Wire,
            rect: None,
            points: vec![Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0)],
            line_paths: Vec::new(),
            label: None,
            net: None,
            net_id: None,
            color_rgba: [0; 4],
        };

        assert!(pick_object(&object, Vec2::new(10.0, 2.0), 3.0).is_some());
        assert!(pick_object(&object, Vec2::new(10.0, 4.0), 3.0).is_none());
    }

    #[test]
    fn painted_net_label_does_not_steal_the_click_from_the_pin_underneath() {
        let mut scene = CadScene {
            surface: CadSurfaceKind::Schematic,
            objects: Vec::new(),
        };
        scene.objects.push(rect_object(
            "component",
            CadObjectKind::Component,
            CadLayerKind::Schematic,
            CadPickPriority::Component,
            Vec2::new(40.0, 40.0),
            Vec2::new(40.0, 28.0),
        ));
        scene.objects.push(rect_object(
            "pin",
            CadObjectKind::Pin,
            CadLayerKind::Schematic,
            CadPickPriority::Pin,
            Vec2::new(40.0, 40.0),
            Vec2::new(10.0, 10.0),
        ));
        scene.objects.push(rect_object(
            "net_label",
            CadObjectKind::NetLabel,
            CadLayerKind::Overlay,
            CadPickPriority::Overlay,
            Vec2::new(40.0, 40.0),
            Vec2::new(48.0, 16.0),
        ));

        assert_eq!(
            pick(&scene, Vec2::new(40.0, 40.0), 8.0)
                .unwrap()
                .selection
                .object_id,
            "net_label",
            "legacy pick keeps its priority-first behaviour"
        );
        assert_eq!(
            pick_editable(&scene, Vec2::new(40.0, 40.0), 8.0)
                .unwrap()
                .selection
                .object_id,
            "pin"
        );
    }

    #[test]
    fn drc_marker_and_airwire_never_win_an_editable_pick() {
        let mut scene = CadScene {
            surface: CadSurfaceKind::Pcb,
            objects: Vec::new(),
        };
        scene.objects.push(rect_object(
            "pad",
            CadObjectKind::Pad,
            CadLayerKind::PcbTopCopper,
            CadPickPriority::Pin,
            Vec2::ZERO,
            Vec2::splat(12.0),
        ));
        scene.objects.push(rect_object(
            "drc_marker",
            CadObjectKind::DrcMarker,
            CadLayerKind::Drc,
            CadPickPriority::Drc,
            Vec2::ZERO,
            Vec2::splat(16.0),
        ));
        scene.objects.push(rect_object(
            "airwire",
            CadObjectKind::Airwire,
            CadLayerKind::Airwire,
            CadPickPriority::Overlay,
            Vec2::new(1.0, 1.0),
            Vec2::splat(6.0),
        ));

        assert_eq!(
            pick(&scene, Vec2::ZERO, 8.0).unwrap().selection.object_id,
            "drc_marker"
        );
        assert_eq!(
            pick_editable(&scene, Vec2::ZERO, 8.0)
                .unwrap()
                .selection
                .object_id,
            "pad"
        );
    }

    #[test]
    fn editable_pick_prefers_the_closest_candidate_over_higher_priority() {
        let mut scene = CadScene {
            surface: CadSurfaceKind::Schematic,
            objects: Vec::new(),
        };
        scene.objects.push(rect_object(
            "component",
            CadObjectKind::Component,
            CadLayerKind::Schematic,
            CadPickPriority::Component,
            Vec2::ZERO,
            Vec2::splat(10.0),
        ));
        let wire = polyline_object(
            "wire",
            CadObjectKind::Wire,
            CadPickPriority::Wire,
            vec![Vec2::new(-20.0, 0.0), Vec2::new(20.0, 0.0)],
        );
        scene.objects.push(wire);

        let point = Vec2::new(8.0, 0.0);
        assert_eq!(
            pick_editable(&scene, point, 6.0)
                .unwrap()
                .selection
                .object_id,
            "wire",
            "the wire the cursor actually touches must win over the symbol body"
        );
    }

    #[test]
    fn a_single_click_is_never_a_double_click() {
        let mut state = CadInteractionState::default();
        let at = Vec2::new(12.0, 40.0);

        assert!(!state.consume_double_click(at, 1.0, 8.0));
    }

    #[test]
    fn two_releases_on_the_same_point_inside_the_window_are_one_pair() {
        let mut state = CadInteractionState::default();
        let at = Vec2::new(12.0, 40.0);

        assert!(!state.consume_double_click(at, 1.0, 8.0));
        assert!(state.consume_double_click(at, 1.2, 8.0));
    }

    #[test]
    fn three_quick_clicks_fire_the_pair_only_once() {
        let mut state = CadInteractionState::default();
        let at = Vec2::new(12.0, 40.0);

        assert!(!state.consume_double_click(at, 1.0, 8.0));
        assert!(state.consume_double_click(at, 1.1, 8.0));
        assert!(
            !state.consume_double_click(at, 1.15, 8.0),
            "the third click must not reuse the consumed pair"
        );
        assert!(
            state.consume_double_click(at, 1.2, 8.0),
            "and it becomes the first click of the next pair"
        );
    }

    #[test]
    fn two_clicks_on_two_pins_are_not_a_double_click() {
        let mut state = CadInteractionState::default();
        let first = Vec2::new(0.0, 0.0);
        let second = Vec2::new(80.0, 0.0);

        // This is the click-click wire route: one click per pin. It must create
        // the wire and stay in the tool.
        assert!(!state.consume_double_click(first, 1.0, 8.0));
        assert!(!state.consume_double_click(second, 1.05, 8.0));
    }

    #[test]
    fn two_clicks_inside_the_tolerance_are_a_pair_at_any_zoom() {
        let mut state = CadInteractionState::default();
        // One pixel of a zoomed-in view is a small world distance; the caller
        // converts the screen radius with the zoom before calling.
        let tolerance = 8.0 / 4.0;

        assert!(!state.consume_double_click(Vec2::new(50.0, 50.0), 1.0, tolerance));
        assert!(state.consume_double_click(Vec2::new(51.0, 50.5), 1.1, tolerance));
    }

    #[test]
    fn a_click_outside_the_time_window_does_not_poison_the_next_pair() {
        let mut state = CadInteractionState::default();
        let at = Vec2::new(12.0, 40.0);

        assert!(!state.consume_double_click(at, 1.0, 8.0));
        assert!(
            !state.consume_double_click(at, 2.0, 8.0),
            "a slow second click is two clicks, not a pair"
        );
        assert!(
            state.consume_double_click(at, 2.1, 8.0),
            "and it is the anchor of the next pair instead of a dead end"
        );
    }

    #[test]
    fn a_backwards_timestamp_is_not_a_double_click() {
        let mut state = CadInteractionState::default();
        let at = Vec2::new(12.0, 40.0);

        assert!(!state.consume_double_click(at, 5.0, 8.0));
        assert!(!state.consume_double_click(at, 4.0, 8.0));
    }

    #[test]
    fn clearing_the_pending_click_ends_the_pair() {
        let mut state = CadInteractionState::default();
        let at = Vec2::new(12.0, 40.0);

        assert!(!state.consume_double_click(at, 1.0, 8.0));
        state.clear_double_click();
        assert!(!state.consume_double_click(at, 1.1, 8.0));
    }
}
