//! Bounded AABB contact history; no window or script execution state.
use super::physics::world_bounds;
use raf_core::scene::{Aabb, SceneGraph, SceneNodeId};
use raf_script::physics_api::{PhysicsEvent, PhysicsEventKind as K};
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
    bounds: Vec<Option<Aabb>>,
    pub events: Vec<PhysicsEvent>,
}
impl ContactHistory {
    pub fn update(
        &mut self,
        scene: &SceneGraph,
        colliders: &[SceneNodeId],
        active: &[SceneNodeId],
    ) -> Result<(), String> {
        self.current.clear();
        self.events.clear();
        if active.is_empty() && self.previous.is_empty() {
            return Ok(());
        }
        // Project once per collider, not once per candidate pair.
        self.bounds.clear();
        self.bounds
            .extend(colliders.iter().map(|&id| world_bounds(scene, id)));
        for &id in active {
            let Some(i) = colliders.iter().position(|other| *other == id) else {
                continue;
            };
            let node = scene.get(id).ok_or("Contact owner disappeared")?;
            let Some(a) = self.bounds[i] else {
                continue;
            };
            for (j, &other) in colliders.iter().enumerate() {
                if id == other {
                    continue;
                }
                let Some(b) = self.bounds[j] else {
                    continue;
                };
                let other_node = scene.get(other).ok_or("Contact collider disappeared")?;
                let trigger = (node.rigid_body.enabled && node.rigid_body.is_trigger)
                    || (other_node.rigid_body.enabled && other_node.rigid_body.is_trigger);
                let intersects = if trigger {
                    (0..3).all(|axis| a.min[axis] < b.max[axis] && a.max[axis] > b.min[axis])
                } else {
                    (0..3).all(|axis| {
                        a.min[axis] <= b.max[axis] + 1e-5 && a.max[axis] + 1e-5 >= b.min[axis]
                    })
                };
                if !intersects {
                    continue;
                }
                let (a, b) = if node.uuid < other_node.uuid {
                    (node.uuid, other_node.uuid)
                } else {
                    (other_node.uuid, node.uuid)
                };
                let pair = Pair { a, b, trigger };
                if !self.current.contains(&pair) {
                    self.current.push(pair);
                }
                if self.current.len() > 128 {
                    return Err("Physics contact budget exceeded (128 pairs)".into());
                }
            }
        }
        self.current.sort_unstable();
        for &pair in &self.previous {
            if !self.current.contains(&pair) {
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
            let kind = match (pair.trigger, self.previous.contains(&pair)) {
                (true, true) => K::TriggerStay,
                (true, false) => K::TriggerEnter,
                (false, true) => K::CollisionStay,
                (false, false) => K::CollisionEnter,
            };
            append(&mut self.events, pair, kind)?;
        }
        std::mem::swap(&mut self.previous, &mut self.current);
        Ok(())
    }
}
fn append(events: &mut Vec<PhysicsEvent>, pair: Pair, kind: K) -> Result<(), String> {
    if events.len() + 2 > 256 {
        return Err("Physics event budget exceeded (256 directed events)".into());
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
