# Local physics scripting

Physics controls live once in `raf_script::physics_api`. Rhai registration is
a thin adapter, and visual Nodes generate the same API calls. Runtime physics
owns integration/contact history; scripts own behavior. No entity name,
character, camera, rig or gameplay controller is required or injected.
C++/WASM remains unimplemented: this shared contract is reusable, not a claim
that additional languages execute today. No new dependency is required.

## API and units

Use a stable entity reference or `self_node()` on an attached script:

```rhai
let body = entity("/World/Box");
if is_valid(body) {
    body.set_aabb_collider([-0.5,-0.5,-0.5], [0.5,0.5,0.5]);
    body.configure_body("dynamic", true, false, 2.0, 0.1);
    body.set_velocity([3.0,0.0,0.0]);
    body.apply_impulse([0.0,4.0,0.0]);
}
```

- `configure_body(type, gravity, trigger, mass, damping)` explicitly enables
  a body, preserving its velocity. Type is `static`, `dynamic` or `kinematic`.
  Mass is positive finite kilograms; damping is nonnegative finite 1/seconds.
- `get_velocity()`, `set_velocity([x,y,z])`, `add_velocity([x,y,z])` use
  world-space meters/second, including under transformed parents. Velocity
  setters do not enable a body. Arrays accept integers or floating-point values.
- `apply_impulse([x,y,z])` uses kg*m/s: delta velocity = impulse / mass.
  It requires an enabled dynamic body; this is not a collision impulse solver.
- `set_physics_enabled(bool)` controls body integration/trigger participation.
  Collider existence is independent: disabling a body leaves its static
  solid collider. Use `remove_collider()` to remove collision participation.
- `set_aabb_collider(min,max)` sets finite local bounds, strictly ordered on
  every axis, and resets the collider offset to zero. It never enables a body.
- `remove_collider()` does not remove the entity or its velocity/body.

Project `enable_physics` must be on for integration/events. Otherwise scripts
can configure their snapshot, but physics does not step. Script spawns and
configuration changes are collected at the next fixed physics boundary.

## Events

Attached entity scripts may declare any of these optional public hooks:

```rhai
fn on_collision_enter(other) { /* first touching fixed tick */ }
fn on_collision_stay(other)  { /* subsequent touching ticks */ }
fn on_collision_exit(other)  { /* first separated fixed tick */ }
fn on_trigger_enter(other)  { /* first overlapping fixed tick */ }
fn on_trigger_stay(other)   { /* subsequent overlapping ticks */ }
fn on_trigger_exit(other)   { /* first separated fixed tick */ }
```

They receive a scoped handle and run only for contacts involving the owner.
Pairs report both endpoints. If either collider is an enabled trigger, the
pair produces trigger events instead of collision events and is not blocking.
Static/static solid pairs do not generate events. An enabled static trigger
can overlap an ordinary static collider. Removing/changing a collider or
changing trigger mode exits the old pair on the next physics step.

`on_physics_event(event)` provides `kind` (for example `trigger_enter`),
`entity`, `other`, `entity_uuid` and `other_uuid`. On an entity attachment it
observes only that owner; a session graph observes all directed events.
Dedicated owner hooks execute before that attachment's generic physics hook.
Authored `emit_event`/`on_event` remain independent and cannot spoof these hooks.

Use `is_valid(other)` before dereferencing: an exit can refer to a deleted
object. UUID strings preserve identity even then. Events never redirect to an
unrelated/replacement entity. Callback edits affect subsequent callbacks and
the next integration step; contact generation for the current tick is fixed.

Order: update -> fixed-update/authored events -> physics -> physics callbacks
-> late-update. Callbacks share the script tick deadline and error policy;
they do not recursively execute events. Pause freezes contacts/time; Step
executes exactly one tick. Stop discards all snapshot changes.

## Visual Nodes

The shared catalog includes Get/Set/Add Velocity, Apply Impulse, Configure Body,
Set Physics Enabled, Set AABB Collider, Remove Collider and six contact events.
Vector/configuration inputs support wires or Inspector values. Physics event
nodes expose Entity/Other handles and an editable `entity` reference filter.
An empty filter explicitly observes every receiver; use a UUID/path/name to
avoid two executions for the two endpoints of one pair. Missing filters do
not silently match another entity. Delay retains the event payload; validate
handles again if the entity can disappear while waiting.

## Lightweight limits and known boundaries

- At most 64 moving bodies, 1024 colliders, 4096 active/collider candidates,
  128 touching pairs and 256 directed events per fixed tick. Limits fail
  visibly; no unbounded contact queue or silent truncation.
- Dynamic solid bodies use gravity/damping, axis-swept AABB blocking and
  minimum-translation initial-overlap correction (up to four passes).
  Unresolvable initial overlap reports an error; no infinite solver loop.
- Kinematic bodies integrate prescribed velocity, ignoring gravity, damping
  and blocking response. Scripts are responsible for their motion constraints.
- No angular dynamics, friction/restitution, full dynamic-body impulse solver,
  convex/mesh narrow phase, collision layers or continuous trigger sweep.
  A fast body can cross a thin trigger between ticks without overlap events.
- Rotated colliders are world-axis bounds, not oriented boxes. Nonfinite
  transforms and singular moving-body parents fail explicitly.
- No rollback of already-applied mutations within a failed tick; the existing
  runtime error policy applies. Scene changes never save into the project.

## Minimal manual acceptance

Copy `examples/runtime/physics_body.rhai` into your project's script folder
and attach it explicitly to a Box. Author a static floor with an AABB and a
separate enabled static trigger. Enable physics/scripting; manually use Play.
Check falling/blocking, script velocity/impulse, trigger enter/stay/exit and
deleting a target. Repeat Pause/Step/Stop in both window modes; verify the
authored scene remains unchanged. The example is optional, not engine logic.
Audio output and whole-machine resource measurements are separate checks.
