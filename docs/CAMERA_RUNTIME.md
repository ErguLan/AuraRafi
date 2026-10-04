# Camera and Nodes runtime

Implementation date: 2026-10-03. Native visual acceptance and whole-machine
resource measurements must be recorded separately; compilation is not proof.

## Ownership

`GameCamera` is a component on an ordinary scene entity. Adding it to a Part
does not remove geometry, physics, children or scripts. Hierarchy can create
an Empty Camera; Inspector can add/remove the component and edit the lens.
These authoring actions use the Game command kernel and scene undo history.
Both EN/ES catalogs and the shared renderer-owned camera icon are used.

Project Settings / Inspector "Use as startup camera" stores one UUID.
Selecting B replaces A; other cameras remain inactive until a script selects
them. Each player's `RuntimeViewState` copies that starting UUID. Runtime
activation changes only that instance, not the authored scene/project.
No implicit first-camera fallback, player, character, rig or controller exists.
A missing camera shows the existing diagnostic; scripts may still run.

New Game projects now seed one editable root `Camera` in their Main session
and persist its UUID as the startup camera. The template uses a perspective
lens, position (0,2,6) meters and pitch -15 degrees. It attaches no script,
character or rig. This is project creation only: opening an existing project,
creating an empty scene or removing the camera never silently recreates it.
Electronics projects remain unchanged. The editor navigation camera is a
separate resource and never substitutes for a Game camera.

To configure an existing project manually, select a camera/entity in Hierarchy.
In Inspector enable the camera component and choose "Use as startup camera"
(Spanish: "Usar al iniciar Play"). Alternatively, select the entity first and
use Project Settings -> "Use selected object as game camera". Save the project.
A stationary camera needs no script; scripts may select another camera
explicitly for their runtime instance. The Game view follows this entity's
transform, not the editor's orbit/navigation pose.

The project-template update has a focused creation/persistence regression test.
Its execution was blocked before compilation by access denied to
`target_gnu/debug/.cargo-lock`; no editor build or alternate target was started.

CameraRig/Pivot/Camera is optional composition using ordinary entities.
Hierarchy does not require reserved names or mandatory rig nodes.

## Editor camera visibility and Play background

Camera components now have an editor-only SVG billboard at their world origin,
including cameras on Parts. The glyph stays 32 logical points across editor
orbit/zoom, DPI and adaptive render resolution, is clickable, and turns orange
when selected. It uses the embedded `raf_render/assets/ui_icons/svg/camera.svg`;
its fixed polyline source is compiled once into segments, then submitted as
batched screen triangles through ApiGraphicBasic. No SVG library, per-entity
image upload, or runtime camera mesh is added. Camera helper poses/lenses are
cached by scene revision instead of walking the entire hierarchy on each
editor-camera-only frame.

A short world-space arrow indicates the actual inherited -Z viewing direction;
selection also shows a lens guide (perspective cone or orthographic rectangle).
The guide uses the editor viewport aspect and is not a full far-clip-volume
preview or a guarantee of the separate Play window's aspect. Show gizmos hides
the helper and its billboard hit target. Scale handles remain scale controls,
not the camera's visual identity.

The current scene background is flat light gray `(240,240,242)`; it is not an
authored skybox or gradient. Both native editor and local Play now consume
`SCENE_BACKGROUND` and `SCENE_LIGHT_DIRECTION` from the shared scene renderer,
instead of Play forcing a near-black background and different lighting.
This does not change any camera pose, activation, geometry or scripts. An
empty or off-target view should still show the light scene background.
GPU/native-window visual acceptance remains a separate manual check.

## Shared scripting API

- `entity(reference)`: exact UUID, unambiguous name or root path
  (`/World/Camera`). Handles retain instance and identity validation.
- `get_parent`, `get_children`, `find_child`, `set_parent(parent, keep_world)`,
  `detach_parent(keep_world)`. Cycles and invalid references are errors.
- Explicit `get/set_world_position`, `get/set_local_position`,
  `get/set_world_rotation`, `get/set_local_rotation`. Positions are meters;
  script rotations are radians. The Inspector's stored rotations are degrees.
  Legacy `set_position` is world-space and `set_rotation` is local-space.
- `add_camera`, `remove_camera`, `has_camera`, `get_active_camera`,
  `activate_camera`, `clear_active_camera`, `spawn_camera` (explicit only).
- `get_camera_lens`, `set_fov`, `set_clip`, `set_orthographic`.
  Lens changes validate before mutation: finite FOV [1,179), positive near,
  far > near and <=100000, positive ortho half-height.
- `look_at(x,y,z)` or `look_at(target, aim_offset)`: -Z forward, +Y up.
- `follow(target, world_offset, aim_offset, sharpness, dt)`: exponential
  frame-rate-independent smoothing; sharpness 0 snaps. Use late-update.
- `runtime_role` / `is_client`: local is client-capable. A reserved Server
  role cannot activate a client camera. No server/networking is shipped.

`on_update` -> `on_fixed_update` -> physics -> `on_late_update` execute once
per fixed tick. Camera display interpolates pose between fixed samples;
camera/lens switches do not blend accidentally across unrelated views.
Pause/Step/Resume reset interpolation. Missing targets should be checked with
`is_valid`; they never address another entity. Scripts can choose freeze,
clear, stop or an explicit alternate view.

World rotation/look-at under non-uniform, mirrored or sheared parents is
rejected explicitly; use a positive uniform-scale pivot or a root camera.
Reparent with keep_world refuses transformations requiring unsupported shear.
There is no automatic camera-wall collision or cinematic transition system.

Camera-space authored UI must bind an authored camera and its document ID;
switching away hides its rendering and input. Screen-space UI is independent.
World-space UI remains unsupported.

## Nodes in Play

The launch manifest captures the unsaved active graph. A different selected
startup session loads its project-confined saved nodes file (<=1 MiB).
`runtime_compiler::to_rhai` validates and generates code; the graph becomes
one isolated global Rhai attachment with no implicit `self_node`.
Entity scripts initialize first, graph initialization follows. If several
controllers activate cameras, the last explicit activation wins.

New nodes expose stable entity lookup, parent/child access, positions, local
rotation, safe parenting, camera component/activation/lens/follow/look-at,
late-update, reference validity and events. Lens/follow inputs can be wired
or use Inspector properties. Follow Camera skips invalid/deleted targets.
On Event receives broadcast/UI events; Send Event can target a script owner
or broadcast with its property. No arbitrary script-function invocation.

Legacy Game nodes include start/update, key edges, mouse-button edges,
If/math/comparison, spawn/destroy/position, For (exclusive End), While,
Print and nonblocking Delay. Hardware nodes and mouse X/Y are rejected:
the Game runtime does not expose those services. No silent pass-through.
Budget: 64 nodes, 256 links, data depth 16, 4096 flow steps/hook, 128 delayed
continuations and <=3600 delay seconds, plus the Rhai operation/time limits.
Flow outputs have one successor; use explicit branching/loops, not fan-out.
Delay runs on simulation time, pauses with the world and does not sleep a
thread. Graph changes require Stop/Play; Rhai file hot reload remains separate.

Nodes Compile validates generated Rhai and returns source in the console.
`/script.compile_nodes file=<project-relative nodes.ron>` returns real source
in structured output without writing files or executing a scene.

## Examples

For the smallest manual smoke test:

1. Enable project scripting and allow Rhai. Author an Empty or Part named
   `FreeCam`; it does not need a character, rig or startup-camera assignment.
2. Copy `examples/runtime/free_camera.rhai` into the project's
   `assets/scripts/free_camera.rhai` and attach it to `FreeCam` through the
   scripting Inspector or `/script.attach file=scripts/free_camera.rhai entity=FreeCam`.
3. Press native Play manually. The script adds and activates its camera only
   in the snapshot. WASDQE moves; arrow keys rotate. Test Pause, Step and Stop.
4. Repeat using both window modes and confirm that Stop leaves the authored
   transform/component unchanged. This is a manual acceptance checklist, not
   a claim that these native-window checks have already passed.

For the follow example, explicitly author the named `/World/Camera` and
optional `/World/Player` and `/World/Overview` entities. Attach the controller
once to an ordinary Controller entity. The alternate view requires an authored
camera component; no target or alternate view is created implicitly.

- `examples/runtime/camera_controller.rhai`: explicit /World/Camera,
  optional /World/Player, WASD, post-physics follow, C -> /World/Overview.
- `examples/runtime/free_camera.rhai`: attach to a camera entity, WASDQE
  world-axis movement and arrow-key local yaw/pitch; no required character.
- `examples/runtime/camera_controller_contract.cpp`: a C++ contract sketch
  only, NOT an engine-executable script or a shipping SDK. Native/WASM
  isolation, ABI and toolchain remain separate work. No large dependency added.

Stop discards runtime state. It never saves script-generated camera changes,
parenting or graph execution into the source project.
