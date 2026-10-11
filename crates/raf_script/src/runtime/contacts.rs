//! Identity-safe physics dispatch under the existing aggregate tick deadline.
use super::*;
use crate::physics_api::{PhysicsEvent, PhysicsEventKind};
use crate::NodeHandle;

impl RhaiScriptRuntime {
    pub fn dispatch_physics_events(
        &mut self,
        scene: &mut SceneGraph,
        input: &InputSnapshot,
        dt: f32,
        events: &[PhysicsEvent],
    ) -> ScriptRuntimeReport {
        let mut report = ScriptRuntimeReport::default();
        if !self.started || self.stopped || events.is_empty() {
            return report;
        }
        if !self.bindings.iter().any(|binding| {
            binding.started
                && !binding.disabled
                && !binding.destroyed
                && (binding.compiled.has_on_physics_event
                    || binding
                        .compiled
                        .physics_hooks
                        .iter()
                        .any(|enabled| *enabled))
        }) {
            return report;
        }
        if events.len() > 256 {
            report.error(
                "physics",
                "Physics event budget exceeded (256 directed events)",
            );
            return report;
        }
        let deadline = self.tick_deadline.unwrap_or_else(|| {
            Instant::now() + Duration::from_millis(self.options.max_time_ms.clamp(1, 100))
        });
        // Keep lookup allocation bounded by the event batch, not the entire scene.
        let wanted: HashSet<Uuid> = events
            .iter()
            .flat_map(|event| [event.receiver, event.other])
            .collect();
        // Resolve UUIDs once per batch; callbacks still revalidate each live handle.
        let identities: HashMap<Uuid, SceneNodeId> = scene
            .iter()
            .filter(|(id, node)| scene.is_valid_node(*id) && wanted.contains(&node.uuid))
            .map(|(id, n)| (n.uuid, id))
            .collect();
        'events: for event in events {
            if !identities.contains_key(&event.receiver) {
                continue;
            }
            for binding in &mut self.bindings {
                if !binding.started || binding.disabled || binding.destroyed {
                    continue;
                }
                if !binding.global
                    && (!scene.is_valid_node(binding.owner)
                        || scene.get(binding.owner).map(|n| n.uuid) != Some(binding.identity))
                {
                    binding.disabled = true;
                    binding.destroyed = true;
                    continue;
                }
                let index = PhysicsEventKind::ALL
                    .iter()
                    .position(|k| *k == event.kind)
                    .expect("Known event kind");
                let owned = !binding.global && binding.identity == event.receiver;
                if !(owned && binding.compiled.physics_hooks[index])
                    && !binding.compiled.has_on_physics_event
                {
                    continue;
                }
                if !binding.global && !owned {
                    continue;
                }
                if Instant::now() >= deadline {
                    report.error(
                        "physics",
                        "Physics callbacks exceeded the shared script tick deadline",
                    );
                    break 'events;
                }
                let mut ctx = ScriptContext {
                    view: &mut self.view,
                    scene,
                    input,
                    audio: &mut self.audio,
                    events: &mut self.events,
                    time: TimeInfo {
                        elapsed: self.elapsed,
                        delta_time: dt,
                    },
                    instance_id: self.instance_id,
                    owner: (!binding.global).then_some(binding.owner),
                    behavior_source: (!binding.global).then_some(binding.path.as_str()),
                    entity_limit: self.options.max_entities,
                    cancellation: Some(&self.options.cancellation),
                    deadline: Some(deadline),
                };
                // Deletion during an earlier callback does not redirect this event to a replacement.
                let receiver = identities
                    .get(&event.receiver)
                    .filter(|id| {
                        ctx.scene.is_valid_node(**id)
                            && ctx
                                .scene
                                .get(**id)
                                .is_some_and(|n| n.uuid == event.receiver)
                    })
                    .map(|id| NodeHandle::scoped(*id, &ctx))
                    .unwrap_or_else(NodeHandle::invalid);
                if !receiver.is_valid(&ctx) {
                    continue;
                }
                let other = identities
                    .get(&event.other)
                    .filter(|id| {
                        ctx.scene.is_valid_node(**id)
                            && ctx.scene.get(**id).is_some_and(|n| n.uuid == event.other)
                    })
                    .map(|id| NodeHandle::scoped(*id, &ctx))
                    .unwrap_or_else(NodeHandle::invalid);
                if owned && binding.compiled.physics_hooks[index] {
                    let result = rhai_backend::call_physics_event(
                        &self.engine,
                        &binding.compiled,
                        &mut ctx,
                        &mut binding.scope,
                        event.kind.hook(),
                        rhai::Dynamic::from(other),
                    );
                    if !report.push_execution(
                        &binding.path,
                        binding.owner,
                        event.kind.hook(),
                        result,
                    ) {
                        binding.disabled = true;
                        continue;
                    }
                }
                if binding.compiled.has_on_physics_event && receiver.is_valid(&ctx) {
                    let mut value = rhai::Map::new();
                    value.insert("kind".into(), event.kind.name().into());
                    value.insert("entity".into(), rhai::Dynamic::from(receiver));
                    value.insert("other".into(), rhai::Dynamic::from(other));
                    value.insert("entity_uuid".into(), event.receiver.to_string().into());
                    value.insert("other_uuid".into(), event.other.to_string().into());
                    let result = rhai_backend::call_physics_event(
                        &self.engine,
                        &binding.compiled,
                        &mut ctx,
                        &mut binding.scope,
                        "on_physics_event",
                        rhai::Dynamic::from_map(value),
                    );
                    if !report.push_execution(
                        &binding.path,
                        binding.owner,
                        "on_physics_event",
                        result,
                    ) {
                        binding.disabled = true;
                    }
                }
            }
        }
        report
    }
}
