# Programming in AuraRafi

This guide covers the current local Rhai workflow. Engine contributors should
use [Scripting internals](SCRIPTING_INTERNALS.md). For exact function contracts,
see [Camera](CAMERA_RUNTIME.md), [Physics](PHYSICS_RUNTIME.md) and
[Behavior runtime](BEHAVIOR_RUNTIME.md).

Status (2026-10-10): the new owner/typed-parameter/prefab workflow is connected
in source, pending compilation and user testing. Rhai is the executable language
in local Play. Nodes compile to Rhai. C++/WASM is not executable in this player.
Examples are ordinary files to copy into a project, never injected engine logic.

## 1. Create and attach a behavior

Create a Game project and an entity in Hierarchy. It can be a visible Part,
an Empty controller, or a camera entity: the engine does not choose a gameplay
role from its name.

Create a Rhai file from Assets, then edit it in your external editor and save.
For example, put `mover.rhai` under the project's `assets/scripts`:

```rhai
fn on_update(dt) {
    let me = owner();
    if !is_valid(me) { return; }
    let speed = parameter("speed", 2.0);
    if is_key_pressed("d") { me.move_by(speed * dt, 0.0, 0.0); }
    if is_key_pressed("a") { me.move_by(-speed * dt, 0.0, 0.0); }
}
```

Select your entity, open Inspector > Components > Behaviors, and choose the
Rhai file in Attach behavior. Under that attachment, open Parameters and add
`speed` as Number. Press Enter to commit a name or scalar value. The type
button cycles Number → Text → Bool → Entity and resets the value; Undo restores
an unintended change. No parameter is required here: absent `speed` uses `2.0`.

With both Console command switches enabled, the equivalent commands are:

```text
/behavior.attach target=/World/Block source=scripts/mover.rhai
/behavior.parameter target=/World/Block source=scripts/mover.rhai name=speed type=Number value=3
```

Those paths describe your authored hierarchy. Replace them with your own paths
or UUIDs. Commands reject ambiguous names. Attachment `source` paths are relative
to `assets`, not to the repository or executable folder.

Press Play manually to run an isolated scene; Stop leaves the authored scene
unchanged. Runtime input requires focus in the runtime view. Editing source is
not the same as starting Play, and Play does not rebuild the Rust engine.

## 2. Owner is not parent

```rhai
let me = owner();
let group = get_parent(me);
let marker = find_child(me, "Marker");
if is_valid(marker) { marker.set_visible(true); }
```

`owner()` is the entity this behavior is attached to. `get_parent(me)` is its
real hierarchy parent and may be invalid at the root. Child names in this
example are your convention, not an engine requirement. For reusable objects,
prefer an Inspector reference over embedding a name in the source.

`entity("UUID")` and `entity("/World/Camera")` are available for explicit lookup.
`self_node()` and `script.parent` still work for old files, but new code should
use `owner()`. These functions are AuraRafi's API, not native Rhai syntax.

## 3. Assign another object without hardcoded names

Add a parameter called `target` to the attachment and change its type to Entity.
Choose an object from the reference selector. The selector shows up to 256 scene
objects plus the current assignment; use Console with a UUID/path for larger
scenes. Select Unassigned to deliberately leave it empty.

```rhai
fn on_update(dt) {
    let target = entity_parameter("target");
    if !is_valid(target) { return; }
    target.move_by(parameter("speed", 2.0) * dt, 0.0, 0.0);
}
```

The parameter stores a UUID. Renaming or reparenting the object keeps the link;
deleting it makes the handle invalid. A new object with the same name is not
automatically substituted. A Text value containing a name is **not** an Entity
parameter. Wrong types produce an error rather than a hidden fallback.

Use per-attachment parameters when several behaviors use the same parameter
names. Inspector > Components > Variables exposes shared entity values.
An attachment override wins over a shared value; a missing override can read
that shared value. Private `let` state belongs to one attachment.

## 4. Configure a runtime camera

For a fixed camera, add the Camera component to an authored entity and use
**Use as startup camera**. This sets one project reference, not a special class
or an immutable camera priority. Scripts may change the active camera later.
A Part can carry the component without losing its visible geometry.

For script-driven selection/follow:

1. Copy [camera_controller.rhai](../examples/runtime/camera_controller.rhai)
   to the project's `assets/scripts`.
2. Attach it to an authored Controller or another entity you choose.
3. Add an Entity parameter `camera` pointing to an entity with Camera component.
4. Optionally add `target` and `alternate` as Entity parameters.
5. Optionally set Number parameters `fov`, `offset_x/y/z`, `aim_height`,
   `sharpness`, and Text `switch_key`. Missing scalars use the example's defaults.

The example activates the configured camera on start, follows the target after
physics, and switches to/from the alternate using the configured key. An absent
target leaves the camera still; no player, rig or replacement object is spawned.
An absent camera leaves the runtime's existing startup selection untouched.
For direct movement, [free_camera.rhai](../examples/runtime/free_camera.rhai)
is a separate opt-in example to attach to a camera entity.

Units: positions/offsets are meters, time is seconds, scripting rotations are
radians, camera FOV is degrees. Do not pass Inspector Euler degrees as radians.
For a black/empty view, first inspect the active camera, position/orientation,
clip range and authored background; a valid reference alone proves none of those.

## 5. Compose behaviors and reactions

Attach multiple sources to one entity. Each has independent private state;
shared properties and local events are explicit communication channels.

```rhai
let behavior = #{ update_interval: 0.25, events: ["power.changed"] };
fn on_event(name, data) {
    if type_of(data) != "map" || !data.contains("enabled") { return; }
    owner().set_visible(data.enabled);
}
```

Another authored behavior can send
`send_event(target, "power.changed", #{ enabled: true })` after validating its
target. These are queued local messages, not online replication. Keep message
names, meanings and gameplay rules in your project.

Copy the optional signal emitter/receiver examples for a complete pair. Assign
the emitter's `receiver` Entity parameter to your receiver; neither file searches
for a mandatory object name. See [Behavior runtime](BEHAVIOR_RUNTIME.md) for
payload limits, tags, queries and scheduling. Avoid scene-wide scans every tick.

## 6. Reuse a hierarchy as a prefab

Select the root of your authored subtree. In Components > Behaviors > Reusable
subtree, enter an assets-relative filename ending in `.prefab.ron` and choose
Save subtree as prefab. A plain `rig.prefab.ron` saves at the assets root.
Refresh Assets after creation if the selector has not picked up the new file.
Choose it in Instantiate prefab to create an independent root copy.

```text
/prefab.capture target=/World/Rig file=rig.prefab.ron
/prefab.instantiate file=rig.prefab.ron parent=/World
```

Each copy gets new UUIDs. Typed references to entities inside the copied subtree
follow that copy, including per-behavior parameters. References outside it stay
external. Raw UUID/name strings embedded in Rhai are not rewritten. Include the
controller and camera under the same prefab root if they must copy together.

You can edit each copy's parameters independently. Updating the prefab file does
not update existing copies: live inheritance/override propagation is not part
of this implementation. Instantiation participates in scene Undo; capturing an
asset file does not. Existing files are never overwritten. Startup-camera project
selection is not changed by creating a copy.

## 7. Nodes and C++

Global session Nodes have no implicit owner. Resolve an explicit entity with
Get Entity. To read its shared Entity variable, connect it to Entity Reference,
set Property to the variable name, then connect Value to camera/hierarchy
operations. These compile to the same Host API; they do not run another engine.

[camera_controller_contract.cpp](../examples/runtime/camera_controller_contract.cpp)
shows the intended neutral ownership/parameter contract. It is not a shipping
SDK, a working native behavior or a file you can attach to local Play today.
The existing native-mod/plugin discussion is a different integration boundary.

## Troubleshooting and boundaries

- Nothing happens: confirm the file is saved, attached, Rhai enabled, and inspect
  runtime diagnostics. Empty optional hooks are valid and do nothing.
- Parameter seems ignored: check exact source path, spelling, type and scope.
  Stop and Play again to copy authoring changes into a fresh runtime snapshot.
- New top-level variable after hot reload: Stop and Play; accepted reload keeps
  old scope and does not rerun top-level initialization.
- Missing object: check `is_valid`; assign a new reference explicitly.
- Prefab cannot save: choose a new filename in an existing folder under assets.
- Do not treat source compilation, native-window acceptance, low-end performance
  and v0.8 completion as the same check. This pass did not run those checks.
