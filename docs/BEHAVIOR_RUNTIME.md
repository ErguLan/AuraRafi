# Composable local behaviors and script relationships

Status: implementation checkpoint, not native acceptance or a networking release.

Start with [Programming guide](SCRIPTING_GUIDE.md) for authoring or
[Scripting internals](SCRIPTING_INTERNALS.md) for the code map. The 2026-10-10
owner/typed-parameter/prefab update is source-only, pending compilation and user testing.

## Ownership and authoring

There is one isolated Play world, one Rhai lifecycle and one Host API.
An entity's existing `scripts` list composes independently scoped behaviors.
The same script attached to different entities shares compilation, not state.
No character, controller, electricity rule, camera rig or behavior is injected.
Author scripts with the existing Inspector/Assets workflow or `/script.create`,
`/script.attach`, `/script.detach`, `/script.validate`. Scene edits remain
transaction-backed; Play modifications stay in the snapshot.

## Hierarchy references in Rhai

```rhai
let me = owner();                         // Entity owning this attachment.
let group = get_parent(me);               // Real scene parent; invalid for a root.
let marker = find_child(me, "Marker");    // Explicit authored child name.
let camera = entity_parameter("camera"); // Inspector-assigned UUID reference.
let stable = entity("uuid-or-World/Camera");
if is_valid(camera) && has_camera(camera) { activate_camera(camera); }
```

This is Rhai, not Luau. `game.Lamp` is not dynamic property syntax in this
adapter; use `game["Lamp"]` or `game.child("Lamp")`. Reserved `script` and
`game` bindings are opaque Host API facades, not new entities or script scene
children. `script.parent` and `self_node()` remain compatibility aliases; new
code should use `owner()`. Global session Nodes have no implicit owner; `owner()` is
invalid there. Duplicate names, missing children and deleted entities never
silently associate with another object. Prefer UUIDs for long-lived references.
Local/world transform and camera APIs remain unchanged.

## Parameters, state, capabilities and queries

`parameter("speed", 2.0)` reads the current attachment's override, then the
owner's shared Inspector variable, then the supplied default. Bool, finite
Number and bounded Text are supported; incompatible types report an error.
`entity_parameter("target")` reads an Entity UUID reference in the same order.
Missing, explicitly unassigned and deleted references return invalid handles;
they never search by name. Per-source overrides avoid parameter collisions.
Script `let` variables are private attachment
state and survive accepted hot reloads. `get_property`/`set_property` expose
scalar state shared by scripts on an entity. Stop does not commit runtime state.

`entity.set_capability("conductive", true)` defines a user semantic tag.
Tags use `capability.<name>` Bool variables, also editable in the Inspector.
They are not network authority or security permissions. No capability is required.

- `entity.has_capability(name)` checks a tag.
- `query_capability(name, limit)` returns scoped handles, in scene order.
- `query_nearby(name, [x,y,z], radius, limit)` filters by world-space origin.

Queries return at most 256 handles, inspect at most the configured entity
budget (capped at 10,000 entries), and check the shared deadline. This is a
bounded linear query, not a spatial tree or mesh-overlap query. Avoid per-object,
per-frame scans. A spatial index can later replace the implementation without
changing script calls.

## Scheduling and structured local events

```rhai
let behavior = #{
    update_interval: 0.25,    // Optional 0..60 seconds; zero means per-frame.
    enabled: true,            // Gates on_update only, not callbacks/cleanup.
    events: ["power.changed"] // Omit for legacy delivery; [] receives no events.
};
```

The runtime accumulates actual delta time and calls `on_update(dt)` once when
due, without unbounded catch-up. Map changes in hooks apply on the next pass.
`enabled=false` resets accumulated update time; events can reactivate it.
Event-only behaviors should omit `on_update`. Fixed-update, late-update, contacts
and destruction keep their existing lifecycle: intervals do not throttle physics
or cameras. Hot reload preserves scope/configuration and does not rerun top-level
setup. Invalid configuration is diagnosed and disables the attachment.

`send_event(target, name, data)` targets scripts on an entity;
`emit_event(name, data)` broadcasts within this world. Delivery is at the next
safe fixed tick, never recursive. Subscriptions filter authored events only;
collision/trigger hooks retain their explicit contracts.

Payloads support finite scalars, Vec3, Color, lists and named records. Limits:
depth 8, 256 values, 8192 string/key bytes, 4096 per string and 128 per nonempty
key. The queue holds at most 256 events. Raw handles and pointers are rejected.
Include a UUID string and resolve `entity(uuid)` in the receiving context.
These are transport-neutral contracts, but events are **not replicated online**.

Optional `examples/runtime/signal_emitter.rhai` and `signal_receiver.rhai`
demonstrate composition. Copy them into the project's `assets/scripts`, create
your own emitter and receiver, and attach them manually. Assign the emitter's
`receiver` Entity parameter explicitly. `signal_amount` is an optional Number
parameter. Colors and reaction rules belong only to examples.

## Visual Nodes

The catalog adds Has/Set Capability, Query Capability, Query Nearby,
Get/Set Entity Property and Event Record. Event Record accepts a bounded JSON
object; connect Value to Send Event's new Value input. Old Send Event nodes
without that connection keep their Text property fallback. All seven nodes
compile through the existing Rhai/Host API path with bilingual labels, not a
second executor. Entity Reference additionally reads a shared typed Entity
variable from an explicit entity, through `get_entity_reference(handle, name)`.

## Optional Hierarchy script links

Enable **Settings > Editor > Hierarchy > Show script links** and Apply/Save.
The preference defaults off and follows EngineSettings RON persistence.
Virtual child rows show the shared Script icon and filename:

- Attached: this entity lists the script, even if missing/unsupported. Not
  proof of successful execution.
- Reference: an attached Rhai source contains a statically resolvable reference
  to this entity. Not proof that a branch executed or the entity changed.

Activate a row to open its confined project file with the existing external
editor launcher. Rows cannot be transformed, reparented, renamed, deleted as
entities or included in range selection. They are not persisted, exported,
counted as entities or added to scene `children`. Search/collapse, scrolling,
selection reveal and virtualization operate on the presentation projection.

One background worker watches save/rename/removal events using the existing
workspace `notify` dependency. Notifications/results are single-slot coalesced,
with a 150ms debounce. No per-frame filesystem scans or polling repaint loop.
Disabling tears down the worker/watcher. Limits: 256 attached source files,
1024 attachments, 256KiB per source, 8MiB per scan and 8192 projected links.
Resolution indexes the current scene once per change. Errors go to Console;
unreadable/removed sources lose stale reference links.

The extractor ignores comments, quoted text and dynamic/generated references.
It recognizes literal entity/UUID/path calls, `owner()`, per-source literal
`entity_parameter("name")`, owner/parent/child chains, root
indexing and simple uniquely assigned aliases. This is conservative lexical
analysis, not a whole-program execution/type/control-flow graph. External
unsaved buffers are invisible; detection happens after saving, without Play
or compilation. Unattached scripts, C++ and Nodes relationship indexing are
not claimed in this pass.

## Remaining expansion

This is not a completed open-world framework or giant new ECS. Typed behavior
asset manifests, live prefab inheritance/package authoring, a spatial index and full
per-script profiler remain future extensions. C++/WASM execution, enforced
server authority, replication, prediction, interest management and online
transport remain unimplemented. Do not mark all v0.8 complete from this alone.

Independent `.prefab.ron` subtree snapshots and per-source Inspector parameters
are now implemented in source. Capture does not overwrite existing files;
instantiation assigns fresh UUIDs and remaps internal typed references. These
are independent copies, not live inherited prefab instances. See the guides
above for limits, scene Undo and file-write boundaries.
