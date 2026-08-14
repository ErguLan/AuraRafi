//! Renderer-neutral retained CAD picking and selection.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cad_scene::{CadLayerKind, CadObject, CadObjectKind, CadPickPriority, CadScene};

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

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }
}

pub fn pick(scene: &CadScene, point: Vec2, tolerance: f32) -> Option<CadPickHit> {
    let tolerance = tolerance.max(0.5);
    scene
        .objects
        .iter()
        .filter_map(|object| pick_object(object, point, tolerance))
        .max_by(|left, right| {
            (left.priority, layer_order(left.selection.layer))
                .cmp(&(right.priority, layer_order(right.selection.layer)))
                .then_with(|| right.distance.total_cmp(&left.distance))
        })
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
            color_rgba: [0; 4],
        };

        assert!(pick_object(&object, Vec2::new(10.0, 2.0), 3.0).is_some());
        assert!(pick_object(&object, Vec2::new(10.0, 4.0), 3.0).is_none());
    }
}
