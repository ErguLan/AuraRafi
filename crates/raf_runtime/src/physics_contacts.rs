//! Bounded AABB contact history; no window or script execution state.
use super::physics::{
    shapes::{self, BoxShape},
    spatial::SpatialIndex,
};
use glam::Vec3;
use raf_core::scene::{Aabb, SceneGraph, SceneNodeId};
use raf_script::physics_api::{PhysicsEvent, PhysicsEventKind as K};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Pair {
    a: Uuid,
    b: Uuid,
    trigger: bool,
}
#[derive(Default)]
pub(super) struct ContactHistory {
    previous: Vec<Pair>,
    current: Vec<Pair>,
    candidates: Vec<SceneNodeId>,
    start: HashMap<SceneNodeId, BoxShape>,
    transient: Vec<Pair>,
    pub events: Vec<PhysicsEvent>,
}
impl ContactHistory {
    pub fn begin_step(
        &mut self,
        scene: &SceneGraph,
        colliders: &[SceneNodeId],
        active: &[SceneNodeId],
        spatial: &SpatialIndex,
        swept: bool,
    ) {
        self.start.clear();
        if swept
            && active
                .iter()
                .any(|&id| scene.get(id).is_some_and(is_sensor))
        {
            self.start.extend(
                colliders
                    .iter()
                    .filter_map(|&id| Some((id, spatial.shape(id)?))),
            );
        }
    }

    pub fn update(
        &mut self,
        scene: &SceneGraph,
        active: &[SceneNodeId],
        spatial: &SpatialIndex,
    ) -> Result<(), String> {
        self.current.clear();
        self.events.clear();
        self.transient.clear();
        if active.is_empty() && self.previous.is_empty() {
            return Ok(());
        }
        for &id in active {
            let node = scene.get(id).ok_or("Contact owner disappeared")?;
            if node.collider.collider_type != raf_core::scene::ColliderType::Aabb {
                continue;
            }
            let Some(a) = spatial.shape(id) else {
                continue;
            };
            let bounds = a.bounds();
            let margin = Vec3::splat(1e-5);
            spatial.query(
                Aabb {
                    min: bounds.min - margin,
                    max: bounds.max + margin,
                },
                &mut self.candidates,
            );
            for &other in &self.candidates {
                if id == other {
                    continue;
                }
                let Some(b) = spatial.shape(other) else {
                    continue;
                };
                let other_node = scene.get(other).ok_or("Contact collider disappeared")?;
                if !node.collider.interacts_with(&other_node.collider) {
                    continue;
                }
                let trigger = is_sensor(node) || is_sensor(other_node);
                let intersects = if let Some(character) = &node.character_controller {
                    let up = character.state.up.try_normalize().unwrap_or(Vec3::Y);
                    let tolerance = if trigger { 0.0 } else { 0.0002 };
                    crate::physics_character::capsule_overlaps_box(
                        a.center,
                        up,
                        character.radius + tolerance,
                        character.height + tolerance * 2.0,
                        b,
                    )
                    .map_err(str::to_owned)?
                } else if let Some(character) = &other_node.character_controller {
                    let up = character.state.up.try_normalize().unwrap_or(Vec3::Y);
                    let tolerance = if trigger { 0.0 } else { 0.0002 };
                    crate::physics_character::capsule_overlaps_box(
                        b.center,
                        up,
                        character.radius + tolerance,
                        character.height + tolerance * 2.0,
                        a,
                    )
                    .map_err(str::to_owned)?
                } else {
                    shapes::intersects(a, b, if trigger { -1e-6 } else { 1e-5 })
                };
                if !intersects {
                    continue;
                }
                let Some((a, b)) = contact_endpoints(scene, id, other) else {
                    continue;
                };
                let pair = Pair { a, b, trigger };
                self.current.push(pair);
                // Each pair can be discovered from at most two active endpoints.
                if self.current.len() > 32_768 {
                    return Err("Physics contact budget exceeded (16384 pairs)".into());
                }
            }
        }
        self.current.sort_unstable();
        self.current.dedup();
        self.swept_sensors(scene, active, spatial)?;
        if self.current.len() > 16_384 {
            return Err("Physics contact budget exceeded (16384 pairs)".into());
        }
        for &pair in &self.previous {
            if self.current.binary_search(&pair).is_err() {
                append(
                    &mut self.events,
                    pair,
                    if pair.trigger {
                        K::TriggerExit
                    } else {
                        K::CollisionExit
                    },
                )?;
            }
        }
        for &pair in &self.current {
            let kind = match (pair.trigger, self.previous.binary_search(&pair).is_ok()) {
                (true, true) => K::TriggerStay,
                (true, false) => K::TriggerEnter,
                (false, true) => K::CollisionStay,
                (false, false) => K::CollisionEnter,
            };
            append(&mut self.events, pair, kind)?;
        }
        for &pair in &self.transient {
            append(&mut self.events, pair, K::TriggerEnter)?;
            append(&mut self.events, pair, K::TriggerExit)?;
        }
        std::mem::swap(&mut self.previous, &mut self.current);
        Ok(())
    }

    fn swept_sensors(
        &mut self,
        scene: &SceneGraph,
        active: &[SceneNodeId],
        spatial: &SpatialIndex,
    ) -> Result<(), String> {
        if self.start.is_empty() {
            return Ok(());
        }
        // Sensors are usually sparse. Compare swept bounds before the more expensive relative cast.
        for &sensor in active {
            let Some(node) = scene.get(sensor).filter(|node| is_sensor(node)) else {
                continue;
            };
            let Some((&a, end_a)) = self.start.get(&sensor).zip(spatial.shape(sensor)) else {
                continue;
            };
            for (&other, &b) in &self.start {
                if sensor == other {
                    continue;
                }
                let Some(other_node) = scene.get(other) else {
                    continue;
                };
                let Some(end_b) = spatial.shape(other) else {
                    continue;
                };
                if (a.center == end_a.center && b.center == end_b.center)
                    || !node.collider.interacts_with(&other_node.collider)
                    || (is_sensor(other_node) && sensor.0 > other.0)
                    || !path_bounds(a, end_a).intersects(&path_bounds(b, end_b))
                {
                    continue;
                }
                let Some((first, second)) = contact_endpoints(scene, sensor, other) else {
                    continue;
                };
                let pair = Pair {
                    a: first,
                    b: second,
                    trigger: true,
                };
                if self.current.binary_search(&pair).is_ok()
                    || self.previous.binary_search(&pair).is_ok()
                {
                    continue;
                }
                let relative_motion = (end_a.center - a.center) - (end_b.center - b.center);
                let crossed = if let Some(character) = &other_node.character_controller {
                    let up = character.state.up.try_normalize().unwrap_or(Vec3::Y);
                    !crate::physics_character::capsule_overlaps_box(
                        b.center,
                        up,
                        character.radius,
                        character.height,
                        a,
                    )
                    .map_err(str::to_owned)?
                        && crate::physics_character::sweep_capsule_box(
                            b.center,
                            up,
                            character.radius,
                            character.height,
                            -relative_motion,
                            a,
                        )
                        .map_err(str::to_owned)?
                        .is_some()
                } else {
                    !shapes::intersects(a, b, -1e-6)
                        && shapes::sweep(a, b, relative_motion).is_some()
                };
                if crossed {
                    self.transient.push(pair);
                    if self.transient.len() > 16_384 {
                        return Err("Swept sensor contact budget exceeded".into());
                    }
                }
            }
        }
        self.transient.sort_unstable();
        self.transient.dedup();
        Ok(())
    }
}

fn contact_endpoints(scene: &SceneGraph, a: SceneNodeId, b: SceneNodeId) -> Option<(Uuid, Uuid)> {
    let a = scene
        .get(crate::physics_dynamics::collider_owner(scene, a))?
        .uuid;
    let b = scene
        .get(crate::physics_dynamics::collider_owner(scene, b))?
        .uuid;
    (a != b).then(|| if a < b { (a, b) } else { (b, a) })
}

fn is_sensor(node: &raf_core::scene::SceneNode) -> bool {
    node.rigid_body.enabled && node.rigid_body.is_trigger
}

fn path_bounds(start: BoxShape, end: BoxShape) -> Aabb {
    let start = start.bounds();
    let end = end.bounds();
    Aabb {
        min: start.min.min(end.min),
        max: start.max.max(end.max),
    }
}
fn append(events: &mut Vec<PhysicsEvent>, pair: Pair, kind: K) -> Result<(), String> {
    if events.len() + 2 > 65_536 {
        return Err("Physics event budget exceeded (65536 directed events)".into());
    }
    events.push(PhysicsEvent {
        receiver: pair.a,
        other: pair.b,
        kind,
    });
    events.push(PhysicsEvent {
        receiver: pair.b,
        other: pair.a,
        kind,
    });
    Ok(())
}
