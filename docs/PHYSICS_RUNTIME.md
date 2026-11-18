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
  a body. Changing a moving body to static clears its velocity. Type is `static`, `dynamic` or `kinematic`.
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
- `set_character_body(bool)` marks a body for simulation in basic mode.
- `set_world_gravity([x,y,z])` changes the runtime instance's gravity in
  advanced mode, at the next physics boundary. Values are finite m/s².
- `apply_force([x,y,z])`, `apply_torque([x,y,z])` and `apply_impulse_at(impulse, point)`
  queue world-space force, torque and an impulse at a world position for the
  next fixed tick on an advanced dynamic body.
- `set_collision_filter(layer,mask)` assigns bitsets. Both sides must accept
  the other's layer. `attach_collider_to_parent(true)` explicitly assigns a
  child collider to its direct parent body.

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

## Physics modes and known boundaries

- Project Runtime settings offer advanced physics and a world gravity vector.
  Basic mode uses 9.81 m/s² downward and simulates marked character bodies,
  kinematic bodies and sensors. Other dynamic bodies stay in place. Existing
  scripts must mark their character explicitly in basic mode.
- A retained bounding-volume tree selects candidates. Topology is rebuilt when
  the collider roster changes. Pose changes refit bounds in the existing tree.
  Runtime metrics count builds, refits, visits and candidates. Contact history admits up to
  16384 pairs and 65536 directed events; overflow is reported explicitly.
- Dynamic solid bodies use gravity/damping, swept oriented box blocking and
  minimum-translation initial-overlap correction (up to four passes).
  Unresolvable initial overlap reports an error; no infinite solver loop.
- Kinematic bodies integrate prescribed velocity, ignoring gravity, damping
  and blocking response. Scripts are responsible for their motion constraints.
- Advanced dynamics resolves linear and angular impulses, contact materials,
  gravity scale, per-axis locks and sleeping. The narrow phase uses oriented
  boxes. Convex and triangle meshes require a separate narrow phase. Swept
  tests use translation and the initial orientation. A fast traversal that
  starts and ends outside an advanced sensor emits enter and exit in the tick.
- Rotation and scale are applied once to local collider bounds. Raycasts use
  the transformed box; overlap_aabb remains a conservative world-box query.
  Nonfinite transforms and singular moving-body parents fail explicitly.
- No rollback of already-applied mutations within a failed tick; the existing
  runtime error policy applies. Scene changes never save into the project.

## Capsule controllers and joints

`configure_character(radius, height)` creates an explicit capsule, in meters,
using the project jump height and slope limit. `character_move(direction)`
uses the project movement speed. `character_set_velocity(velocity)` requests
an explicit world velocity. The controller projects movement onto the gravity
plane. `character_jump()` requests a grounded jump and `character_grounded()`
reports the last tick's support. The controller carries the character with a
translating support and can climb a step within its configured limit. In advanced
mode, side contacts can push dynamic bodies. Dynamic boxes test collisions
against the curved capsule instead of its enclosing bounds.

The Physics Inspector creates fixed, hinge, slider, ball and spring joints
between two selected bodies. The joint editor takes local anchors and axes,
angle limits in radians or slider limits in meters, optional motor and break
threshold. Scripts can use `add_physics_joint`, `configure_joint_motion` and
`configure_joint_spring`. Joints solve at fixed ticks in the advanced mode.
The player can show physics shapes and capsule ground normals through the
project debug setting in Runtime.

## Minimal manual acceptance

Copy `examples/runtime/physics_body.rhai` into your project's script folder
and attach it explicitly to a Box. Author a static floor with an AABB and a
separate enabled static trigger. Enable physics/scripting; manually use Play.
Check falling/blocking, script velocity/impulse, trigger enter/stay/exit and
deleting a target. Repeat Pause/Step/Stop in both window modes; verify the
authored scene remains unchanged. The example is optional, not engine logic.
Audio output and whole-machine resource measurements are separate checks.
