# Runtime input, progress and world transitions

Status: source implementation. Native-window, filesystem failure-injection and
low-resource performance acceptance remain separate from static checks/tests.
This extends [Local Runtime](LOCAL_RUNTIME.md), not export or networking.

## Ownership

`raf_core::runtime_config` owns project policy and compatible defaults.
`raf_script::services` owns typed state/requests/receipts; `query_api` owns AABB
queries. Rhai registrations and generated Nodes delegate to these contracts.
`raf_runtime::storage` owns lazy bounded disk I/O, `session_loader` strict ingress,
and `RuntimeWorld` safe simulation boundaries. `raf_player` owns cursor leasing,
candidate preparation and presentation. The editor only launches manual Play and
commits settings through its existing path. No player/controller is injected.
Legacy `raf_core::save_system::GameState` is not Play's active progress format;
there is no automatic migration of legacy files.

## Input

Project Settings actions and axes remain editable JSON. For example:

```json
[{"name":"interact","keys":["e","mouse:0"]}]
```

```json
[
  {"name":"move_x","negative":"a","positive":"d","scale":1.0},
  {"name":"look_x","negative":"","positive":"relative_x","scale":0.1}
]
```

`input_axis("move_x")` reads data, not built-in movement. Sources accept ordinary
keys, `mouse:0` through `mouse:4`, `mouse_x/y`, `relative_x/y` and `wheel_x/y`.
Each pair is positive minus negative times scale. Limits: 128 actions, 128 axes,
16 bindings per action. Unknown platform keys remain inactive.

Additional Rhai calls:

- `mouse_position()` returns logical window coordinates, or `()` outside.
- `mouse_delta()`, `mouse_relative()`, `mouse_wheel()` return two numbers.
  Relative motion uses raw device counts; delta/position use logical pixels.
  Wheel uses the native snapshot's logical units, 24 per line.
- `was_mouse_just_pressed(button)`, `was_mouse_just_released(button)`,
  `was_key_just_released(key)` and `was_action_just_released(name)`.
  Buttons: 0 left, 1 right, 2 middle, 3 back, 4 forward.
- `set_cursor(mode, visible)` requests `free`, `locked` or `confined`.
  `cursor_capture()` reports the actual result, including a platform fallback
  from locked to confined. Server-role contexts reject local cursor capture.

Escape releases capture without consuming the game's Escape logic. Shift+Escape
stops Play; Escape cancels preparation. After focus loss, pause or Escape, a new
click inside the game permits capture again, and the script must request it.
The native cursor lease releases on Drop/Stop. Raw motion goes only to the active
local runtime. UI ownership suppresses the matching gameplay input, including
pending transients. Edges/motion accumulate until the first fixed tick, then clear
for catch-up ticks; held state remains. Update/fixed-update/late-update observe
the same tick: choose your controller hook rather than applying input in all three.

## Physics queries

```rhai
let hit = raycast([0,2,0], [0,0,-1], 100.0);
if hit != () { print(hit.distance); let target = hit.entity; }
let objects = overlap_aabb([-1,-1,-1], [1,1,1]);
let filtered = raycast([0,2,0], [0,0,-1], 100.0, owner(), "interactable", false);
```

Filtered overloads append ignored entity, semantic tag and include-triggers.
Tags are authored `SceneNode.tags`, not capability variables; empty accepts all.
Missing ignore handles mean no exclusion; stale nonempty handles are errors.
Ray results contain scoped `entity`, world `position`, `normal`, `distance`.
Starting inside returns zero distance/normal. Overlap fails above 256 results
rather than silently truncating. Both use authored AABB colliders, parent
transforms and offsets. Rotation expands world bounds; this is not exact mesh,
convex, slope or character collision. Visibility does not disable queries.
Queries and current physics share one bounds helper and script execution budgets.

## Progress location and policy

Project Settings > Game progress configures enablement, location, schema version,
per-slot KiB, slots per profile and optional autosave. Autosave defaults off.
It saves only explicitly authored store data when dirty, not the entire scene.
Intervals accumulate bounded running-frame time; paused worlds do not autosave.

Default Windows Play location:

```text
%LOCALAPPDATA%/AuraRafi/Games/<project UUID>/saves/development/play-1/main/manual.ron
```

Other platforms use their local application-data directory. Portable uses
`<project>/saves/development/play-N/`; custom uses
`<authorized folder>/<project UUID>/development/play-N/`. Scripts choose profile
and slot identifiers, never filesystem paths. Play lanes 1–4 isolate simultaneous
windows and are reused after Stop/Play. Different editor processes sharing a
lane use writer locks/generation conflicts, not automatic merging. No Play path
accesses production saves. Custom folder grants are per project, local to the
PC's EngineSettings, not shared project configuration. Revoke deletes no files.
Running players keep their launch snapshot; Stop/Play applies policy changes.

## Progress scripting

`progress_set(key,value)`, `progress_get(key,fallback)`, `progress_remove(key)`
and `progress_clear()` manipulate memory only. Values use ScriptValue's finite
scalars, strings, vectors/colors, lists and records; raw handles and opaque
process/scene data are rejected. Durable entity references are UUID strings.
Limits: 256 keys, existing per-value depth/item/byte budgets, 16 profiles, 1–128
slots/profile and 1 KiB–4 MiB/slot, including a conservative metadata reserve.

```rhai
progress_set("score",42);
let request = progress_save("manual");
// On later ticks, inspect progress_status().status == "saved".
```

`progress_save(slot)`, `progress_load(slot)` and `progress_load_backup(slot)`
queue one request at a time and return its ID. Busy requests fail explicitly.
`progress_status()` returns request, status, error, busy, dirty, profile,
expected schema and loaded schema. Status identifiers are API values: `idle`,
`pending`, `saved`, `loaded`, `missing`, `schema_mismatch`, `restored`, `discarded`,
`error`. Edits made during a write remain dirty after the older write completes.

Load only stages data. `progress_apply_loaded()` explicitly replaces the memory
store if the schema matches. The developer restores actual entities separately.
`progress_loaded_get(key,fallback)` supports explicit scripted migration: read
old fields, transform chosen values and save the current schema yourself.
No automatic partial migration or empty-success fallback occurs.

`progress_profile(name)` requires clean, idle data. `progress_discard()` explicitly
abandons memory edits before switching without saving. It deletes no files.
Clear means modified empty data; discard means abandon edits. Applying a loaded
document intentionally discards current memory edits.

Existing slots must be loaded before overwrite. Under an OS writer lock, the
worker checks the observed generation; another writer causes a conflict. Corrupt
or unknown-format files remain intact. Explicitly read a backup and save to a
new slot to recover a corrupt primary. Leftover `.pending` files require
inspection; the engine does not automatically delete an unknown previous write.

The worker flushes a temporary file, preserves the old primary as
`<slot>.backup.ron`, publishes the new primary, then reports success. Failed
publication attempts rollback. This is recoverable publication, not immunity
to power loss/filesystem failure. Stop waits up to two seconds for pending work;
unconfirmed writes are reported, never called saved. Stop does not implicitly
save. Only a completed `saved` receipt confirms persistence: do not depend on
starting an asynchronous write when forcibly closing the whole process.

See the optional [progress checkpoint example](../examples/runtime/progress_checkpoint.rhai).
Nothing in the engine attaches that example automatically.

## World transitions

Read a saved world UUID with `/session.list`, then call `request_session(uuid)`.
`session_status()` returns current, status and error. Finish outstanding progress
I/O first, otherwise the transition reports busy. Nodes use the same API.

At a tick boundary the host freezes the old world, reuses the loading surface,
and prepares scene, graph, scripts and authored UI on one worker. Files are
bounded/project-confined; missing/corrupt worlds never become empty successes.
A bounded candidate on_start runs before commit without polling disk requests.
Failure retains the old world, progress and prior pause/running state. Successful
commit retires the old world, resets scoped references/input/UI caches and
transfers the progress service/worker. Use destroy hooks for cleanup, not
cross-world persistence requests after their service has been retired.

Transitions read saved sessions, not live unsaved editor data. They do not save
or load progress, choose another camera or associate a player. Editor project
persistence and Undo are untouched.

## Nodes and future compiled languages

The catalog adds input/pointer/cursor, AABB, progress and session operations.
Read Runtime Field extracts result fields, such as status or a ray's entity.
Generated graphs call the same Rhai Host API, not another Nodes executor.
`wasm_host::{HostOperation, InputQuery, ProgressImport, GuestRayHit}` supplies
typed imports for future C++/Rust adapters. Entity replies use scoped guest
table tokens, never raw scene indices. Progress remains ScriptValue and exposes
no arbitrary file paths. These are provider contracts, not a shipping C++ SDK
or working Play backend. No compiler, WASM executor, dependency download,
native plugin loader or online transport is added.

## Live acceptance

Still exercise both window modes, DPI/themes, compact Settings, focus/cursor
release, repeated Stop/Play, two save lanes, corrupt/missing saves, migration,
valid/invalid session transitions and low-resource measurements. Automated
checks are not proof of these interactions or completion of v0.8.
