# AuraRafi Scripting System

This document defines the canonical scripting architecture for AuraRafi. It
covers the three scripting tiers, the shared Host API, how visual nodes
execute, how commands create scripts, the security model, configuration
surfaces, and a phased mini-roadmap.

Status: **Local Rhai runtime implementation.** Manual Play connects
`raf_runtime` and `raf_player`; acceptance evidence is maintained in
[Local Runtime](LOCAL_RUNTIME.md). Each attachment has persistent isolated
scope/owner state; shared files compile once. Public hooks are `on_start()`,
`on_update(dt)`, `on_fixed_update(dt)`, `on_event(name, value)` and
`on_destroy()` and `on_late_update(dt)`. Update hooks execute at the configured
fixed simulation frequency, independently of display pacing; late-update
follows physics. Supported Nodes compile to Rhai in the local runtime.
WASM remains reserved. See [Camera Runtime](CAMERA_RUNTIME.md).

---

## 1. Vision

AuraRafi offers three ways to write behavior. All three call the same Host
API. This is the same model used by Unity (C# + Visual Scripting share one
API), Unreal (C++ + Blueprint share the reflection layer), and Godot
(GDScript + C# + Visual share the Object API).

| Tier | Language | Audience | Sandbox | Performance |
|------|----------|----------|---------|-------------|
| 1 | Rhai | Beginners, game designers | Full | Interpreted (~10-70x native on tight numeric loops, fine for game logic) |
| 2 | WASM Native Module (C++, Rust, Zig, AssemblyScript) | Advanced users, performance-critical code | Full (WASM sandbox) | Near-native (~2-5x native) |
| 3 | Visual Nodes | Non-programmers, prototyping | Same bounded Rhai sandbox in Play | Graph-to-Rhai compilation |

Tier 1 is the executable script path. Tier 3 emits Rhai through
`raf_nodes::runtime_compiler` and uses the same Host API.
Tier 2 is designed now and waits for an approved WASM runtime dependency.

---

## 2. Current State

The native Nodes authoring document and isolated local Play world are
separate ownership layers connected by a snapshot/compile boundary.

### 2.1 Visual Nodes authoring (`crates/raf_nodes/` + `raf_editor`)

- `NodeGraph` is the session-scoped persisted document. It currently contains
  one named graph with nodes and connections; a collection of runtime flows is
  not exposed by the native editor contract.
- The native surface is
  `crates/raf_editor/src/panels/nodes_surface.rs`, composed through RafUI and
  ApiGraphicBasic. Geometry, grid, ports, and wire quads are isolated in
  `nodes_canvas.rs`; transient query, drafts, and pending-pin state live in
  `nodes_surface_host.rs`.
- The shared `raf_nodes::catalog` owns stable built-in node slugs and factories
  for Game and Electronics categories. The surface does not keep a second
  preset table.
- Nodes can be added, selected, moved, deleted, connected through checked
  typed pins, disconnected, edited in the right Inspector, searched, and
  validated. Node authoring has a bounded local undo/redo history.
- `nodes.ron` is loaded and saved with the project session. New persisted
  properties use serde defaults so older documents remain readable.
- `compiler.rs` is currently a graph validator. It reports connectivity and
  typing diagnostics; it does not compile executable code.
- The legacy executor/node_backend are compatibility preparation, not the
  Play path. `runtime_compiler` generates a global Rhai attachment; the
  unsaved active graph or saved startup-session graph is included in Play.

### 2.2 External Scripts (`crates/raf_editor/src/script_support.rs` + `panels/behaviors.rs`)
- Detects `.rs`, `.cpp`, `.rhai`, `.lua`, `.py`, `.js`, `.ts` by extension.
- `is_engine_supported()` returns true only for Rhai execution; native source
  files remain authoring assets, not executable runtime modules.
- Rhai validation uses the actual compiler/AST and project-confined paths.
- `SceneNode.scripts: Vec<String>` stores relative paths.
- `behaviors.rs` panel attaches/detaches scripts, shows validation status, opens VS Code.
- `raf_script::runtime::RhaiScriptRuntime` can load attached `.rhai` files
  from project or assets script folders and execute lifecycle functions on a
  cloned scene for isolated runtime-prep tests.
- Manual native Play consumes these Rhai attachments. Attached Agent/CLI/MCP
  activation remains blocked; native/WASM adapters are future work.

### 2.3 Command boundary (`raf_editor`)

The native Nodes surface emits command names through the workbench input
boundary. `native_editor_commands.rs` translates them into
`NativeEditorRuntime` graph methods. This is the active authoring path for
selection, graph mutations, validation, and persistence. It must not be
replaced with a second surface-local mutation path.

### 2.4 Console Commands (`crates/raf_editor/src/commands/`)
- Slash command parser with handlers for `game.*`, `electronics.*`, `pcb.*`, `workspace.*`.
- These handlers DO mutate the scene and remain the command surface for agent
  and console operations.
- `script.*` commands exist for create, attach, detach, list, validate, run,
  and node compile stubs.
- `/script.run` uses the same Rhai runtime session for one-shot `on_start`
  tests against a cloned scene.

### 2.5 C++ Modding (`docs/CPP_MODDING.md`)
- Documents a `.dll` + JSON command bus approach.
- C++ submits JSON strings through a function pointer callback.
- **Gap**: Verbose, not beginner-friendly, no sandbox (a bad `.dll` crashes the engine), no hot-reload story. Superseded by the WASM Native Module approach in this document.

---

## 3. Why Rhai (and How Slow Is It Really)

Rhai is a tree-walking interpreter written in pure Rust. It has no native
code, no JIT, no external C dependencies.

### Performance characteristics
- Tight numeric loop (e.g. summing 1M integers): ~10-70x slower than native Rust.
- Game logic dispatch (if key pressed, call `set_position`, branch on state): negligible. The cost is dominated by the Host API call into the scene graph, not by Rhai's interpretation overhead.
- Unreal Blueprint is also interpreted and ships commercial games. Godot GDScript is interpreted. The bottleneck for game logic is never the script interpreter; it is rendering and asset I/O.

### When Rhai is not enough
- Procedural mesh generation, dense particle simulation, custom physics solvers: these want native speed. That is what Tier 2 (WASM) exists for.
- The split is: 95% of game logic in Rhai, 5% hot inner loops in WASM. This matches the Unity C# + C++ plugin split.

### Why Rhai over Lua (mlua)
- Pure Rust: zero C dependency, builds with `stable-x86_64-pc-windows-gnu` without friction.
- Sandboxed by design: no filesystem, no network, no `eval` escape.
- Registers Rust functions natively: `engine.register_fn("set_position", ctx.set_position)`.
- Smaller attack surface for a lightweight engine.
- Trade-off: Luau migrants from Roblox need to learn Rhai syntax. Rhai is C-like and close enough that the learning curve is one afternoon.

---

## 4. Why WASM Native Modules Instead of Raw C++ FFI

The previous approach (`docs/CPP_MODDING.md`) loaded `.dll` files directly.
That approach has problems:

1. **No sandbox**: a bad C++ pointer dereference crashes the engine. No recovery.
2. **ABI fragility**: struct layouts, calling conventions, and name mangling differ across compilers and platforms.
3. **No hot-reload**: Windows locks loaded `.dll` files; swapping requires `FreeLibrary` + file rename dance.
4. **Single language**: only C++ (and Rust with `extern "C"`) can target it.
5. **Security risk**: a malicious `.dll` has full process memory access.

### The AuraRafi Host ABI (our own approach)

Tier 2 uses **WebAssembly** as the execution mechanism. The "propio" (our
own) part is the **AuraRafi Host ABI**: the versioned set of WASM import
functions that constitute our API. We define it, we version it, we own it.

```
User writes C++ (or Rust, Zig, AssemblyScript)
        |
        v
  Compiles to .wasm (clang --target=wasm32, rustc --target=wasm32-wasi, etc.)
        |
        v
  Engine loads .wasm via embedded WASM runtime
        |
        v
  WASM module imports from "aurarafi" import module:
    - aurarafi.get_node(name_ptr, name_len) -> u64 (NodeHandle)
    - aurarafi.set_position(handle, x, y, z) -> ()
    - aurarafi.set_color(handle, r, g, b, a) -> ()
    - aurarafi.get_delta_time() -> f32
    - aurarafi.is_key_pressed(keycode) -> i32
    - ... (mirrors the Rhai Host API exactly)
        |
        v
  WASM calls exports: on_start(), on_update(dt)
```

### Advantages over raw FFI
- **Sandboxed**: WASM cannot access host memory, filesystem, or network without explicit grants. A bad module traps, it does not crash the engine.
- **Multi-language**: C++, Rust, Zig, AssemblyScript, Grimoire, any language that compiles to WASM.
- **Hot-reload**: drop-in replacement of the `.wasm` file. No OS file locks.
- **Stable ABI**: WASM defines its own ABI. No struct layout issues, no calling convention surprises.
- **Cross-platform**: the same `.wasm` runs on Windows, Linux, Web, mobile.
- **Our own spec**: the AuraRafi Host ABI is versioned (`HOST_ABI_VERSION = 1`). Modules declare their target version. Breaking changes are explicit, not silent.

### WASM runtime choice
- To be decided when Tier 2 is implemented. Candidates: `wasmtime` (JIT, heavy), `wasmer` (JIT, heavy), or a lightweight pure-Rust interpreter.
- The Host ABI spec is independent of the runtime. We can switch runtimes without breaking modules.

---

## 5. The Host API

The Host API is the single choke point. Every tier calls the same functions.
No tier touches `SceneGraph`, `AudioCommandQueue`, or `InputState` directly.

### 5.1 ScriptContext

```rust
pub struct ScriptContext<'a> {
    scene: &'a mut SceneGraph,
    input: &'a InputState,
    audio: &'a mut AudioCommandQueue,
    time: TimeInfo,
    delta_time: f32,
    elapsed: f32,
}
```

The context is constructed per frame by `raf_script::runtime` and passed to
each script's `on_update(dt)`. Scripts receive it implicitly in Rhai. WASM will
receive it through the Host ABI once Tier 2 is wired.

### 5.2 NodeHandle

Scripts never hold `&mut SceneNode`. They hold a `NodeHandle`:

```rust
pub struct NodeHandle {
    id: SceneNodeId,
    // No reference to SceneGraph. Methods take &mut ScriptContext.
}
```

This is the Roblox `Part` equivalent. `script.Parent.Part1` becomes
`get_node("Part1")`. The handle is cheap to copy, safe to store, and cannot
dangle because the context validates the ID on every call.

### 5.3 Function surface (shared by all tiers)

Scene operations:
- `get_node(name: &str) -> Option<NodeHandle>`
- `spawn_entity(name: &str, primitive: &str) -> NodeHandle`
- `destroy_entity(handle: NodeHandle)`
- `find_child(parent: NodeHandle, name: &str) -> Option<NodeHandle>`
- `get_parent(child: NodeHandle) -> Option<NodeHandle>`
- `get_children(parent: NodeHandle) -> Vec<NodeHandle>`

Transform operations (all in meters, SI):
- `set_position(handle, x, y, z)`
- `set_rotation(handle, x, y, z)` (euler radians)
- `set_scale(handle, x, y, z)`
- `get_position(handle) -> (f32, f32, f32)`
- `move_by(handle, dx, dy, dz)`
- `rotate_by(handle, dx, dy, dz)`

Property operations:
- `set_color(handle, r, g, b, a)` (0-255)
- `set_visible(handle, bool)`
- `set_name(handle, name)`
- `get_property(handle, key) -> ScriptValue`
- `set_property(handle, key, value)`

Input:
- `is_key_pressed(key: &str) -> bool`
- `was_key_just_pressed(key: &str) -> bool`
- `is_mouse_pressed(button: i32) -> bool`

Audio:
- `play_audio(name: &str)`
- `stop_audio(name: &str)`
- `set_volume(name: &str, volume: f32)`

Time:
- `get_delta_time() -> f32` (seconds)
- `get_elapsed_time() -> f32` (seconds since scene load)

Script interop:
- `call_script_function(script_path: &str, function: &str, args: Vec<ScriptValue>) -> ScriptValue`

All values are in SI units (meters, seconds, radians, 0-255 RGBA) as defined
in `crates/raf_core/src/units.rs`. Scripts never convert units manually.

---

## 6. Script Lifecycle

```
Scene loaded
    |
    v
Prepared script harness collects all node.scripts paths
    |
    v
For each script:
    - Rhai: create Engine, register Host API, compile source
    - WASM: instantiate module, wire import table
    |
    v
Call on_start() once per script
    |
    v
Every frame:
    - Build ScriptContext (scene, input, audio, time, dt)
    - Call on_update(dt) per script
    - WASM: pass dt as f32 parameter
    - Rhai: pass dt as function argument
    |
    v
Scene unloaded / Play mode stopped:
    - Call on_destroy() per script (if present)
    - Drop all engines/modules
```

Functions are optional. A script with only `on_start` runs once. A script
with only `on_update` runs every frame. A script with neither is a no-op
(validation warns the user).

---

## 7. Visual Nodes execution boundary

The native Nodes authoring validator
validates graph structure, pin direction, duplicate links, occupied inputs,
and compatible data types. The authoring surface saves `NodeGraph` in
`nodes.ron`. Local Play consumes an isolated snapshot compiled to Rhai.

The legacy executor/node backend below remain historical preparation. The
active backend and its restrictions are defined in CAMERA_RUNTIME.md.
The authoring boundary remains:

- `Spawn Entity`, `Set Position`, and `Destroy Entity` do not mutate the live
  editor `SceneGraph` from the Nodes surface.
- `compiler.rs` does not emit Rhai or native executable code.
- Opening the Nodes tab does not start an execution loop.

### 7.1 Planned interpreted graph walk

When runtime integration is explicitly resumed, the executor may call the
shared Host API for operations such as:

- `Spawn Entity` -> `ctx.spawn_entity(...)`
- `Set Position` -> `ctx.set_position(...)`
- `Destroy Entity` -> `ctx.destroy_entity(...)`

That work belongs to the runtime/Host API contract, not to the retained UI
surface or its command adapter.

### 7.2 Active graph-to-Rhai compilation
`runtime_compiler::to_rhai` emits bounded source and explicit unsupported-node
errors. Native Compile returns that source; Play attaches the graph snapshot
through the same Rhai backend as a hand-written script. A dedicated code
viewer remains future work.

### 7.3 Call Script Function node (future, not implemented)
A new visual node `Call Script Function` lets a node graph invoke a function
defined in a `.rhai` script:

- Input pins: `script_path` (String), `function_name` (String), `args` (Any, variadic)
- Output pin: `return_value` (Any)

This bridges the two tiers. A graph can delegate complex logic to a Rhai
function, and a Rhai script can be invoked from a no-code flow. The call goes
through `ScriptContext::call_script_function`, the same Host API function.

---

## 8. Command Integration

Following the pattern in `docs/COMMANDS.md`, a new `script` domain is added.

### 8.1 New commands

```
/script.create lang=rhai name=player_controller
    Creates assets/scripts/player_controller.rhai from template.
    Also supports lang=cpp (creates .cpp + Makefile stub).

/script.attach file=player_controller.rhai entity="Player"
    Attaches script to entity by name. Uses SceneNode.scripts.

/script.detach file=player_controller.rhai entity="Player"
    Removes script from entity.

/script.list
    Lists all scripts in assets/scripts/ with language and entry points.

/script.validate file=player_controller.rhai
    Checks syntax (Rhai: engine.parse). Returns errors.

/script.run file=player_controller.rhai
    Executes on_start once in editor (testing, not runtime).

/script.compile_nodes flow=Main output=player_controller.rhai
    Historical syntax; use /script.compile_nodes file=nodes.ron.
    Returns generated Rhai in structured output; no file write or execution.
```

### 8.2 Domain
`script` commands live in domain `shared` (available in any project type).
Handler module: `crates/raf_editor/src/commands/script.rs`.

### 8.3 Catalog entry
Add to `assets/commands/catalog.json` following the existing schema:
name, aliases, domain, description_key, parameters, examples.

### 8.4 Command-to-script bridge
Commands that need to run script logic must go through the Host API, not
through a parallel path. `/script.run` reuses `raf_script::runtime` and runs
against a cloned scene so command testing does not mutate the editor document.

---

## 9. Anti-Spaghetti Principles

These are rules, not suggestions. They apply to all scripting code.

1. **One API, three callers.** Rhai, WASM, and Visual Nodes all call the Host API. No tier has a shortcut to SceneGraph. If a new operation is needed, it goes into `host_api.rs` once, and all three tiers gain it.

2. **NodeHandle is opaque.** Scripts hold an ID, not a reference. The context validates the ID on every call. If the scene graph is refactored to ECS, SoA, or anything else, scripts do not break.

3. **No script touches engine internals.** Scripts cannot import `raf_core`, `raf_render`, or `raf_editor`. They see only what the Host API exposes. This is enforced by sandbox (Rhai/WASM) and by construction (node executor only calls host functions).

4. **Commands are the programmatic path.** Anything a user can do in the UI (create script, attach, run) is also a command. Commands go through the CommandBus for undo/redo. Scripts do NOT go through the CommandBus for per-frame logic (too slow); they call the Host API directly. The CommandBus is for editor operations, not runtime ticks.

5. **Versioning is explicit.** `HOST_API_VERSION = 1`. Scripts declare their target version. WASM modules declare their `HOST_ABI_VERSION`. Breaking changes bump the version and old scripts fail with a clear error, not silent corruption.

6. **Units are SI everywhere.** The Host API operates in meters, seconds, radians, 0-255 RGBA. Scripts never convert. `units.rs` constants are imported by the Host API and by the WASM ABI header. No magic numbers.

7. **Editor and runtime share the path.** The `ScriptContext` and Host API are designed so the same code runs in the editor (Play mode) and in a future standalone runtime export. No `#[cfg(feature = "editor")]` in the Host API.

8. **Failure is loud.** A script that errors stops executing that frame and logs to the console. It does not silently continue. A WASM trap stops the module and logs. The engine never crashes from a script error.

---

## 10. Security Model

| Tier | Filesystem | Network | Memory | CPU |
|------|-----------|---------|--------|-----|
| Rhai | Blocked | Blocked | Sandbox | `set_max_operations` timeout per frame |
| WASM | Blocked unless granted | Blocked unless granted | Sandbox (linear memory) | Fuel/timeout via runtime |
| Visual Nodes | Same Rhai limits in Play | Same Rhai limits | Same Rhai limits | 4096 flow steps/hook, plus Rhai time/operation budgets |

Rhai and WASM cannot crash the engine. Visual Nodes cannot either (the
executor is Rust code that validates every call). The only way to crash the
engine is a bug in the Host API itself, which is engine code and engine
responsibility.

---

## 11. Hot-Reload

| Tier | Mechanism |
|------|-----------|
| Rhai | `notify` crate (already a workspace dep) watches `assets/scripts/`. On change, re-create `Engine`, re-register Host API, re-compile, call `on_start`. |
| WASM | Watch `assets/scripts/*.wasm`. On change, drop instance, re-instantiate module, re-wire imports, call `on_start`. |
| Visual Nodes | Persisted with the authoring session. Changes require Stop/Play; the runtime snapshot is immutable. |

Hot-reload is opt-in via `script_hot_reload` setting (default true).

---

## 12. Configuration

### 12.1 Engine-level settings (`EngineSettings` in `raf_core/src/config.rs`)

New fields with `#[serde(default)]`:

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `script_runtime_enabled` | `bool` | `false` | Master switch for the prepared script lifecycle harness. Keep off until Play/runtime is connected. |
| `default_script_language` | `ScriptLanguage` | `Rhai` | Template language for "New Script". |
| `script_hot_reload` | `bool` | `true` | Reload scripts on file change. |
| `script_timeout_ms` | `u32` | `100` | Max execution time per frame (Rhai `set_max_operations`). |
| `script_external_editor_cmd` | `String` | `"code"` | Command to open `.rhai`/`.cpp` files. |

UI: new `CollapsingHeader` "Scripting" in `settings_panel.rs`, after Editor.

### 12.2 Project-level settings (`ProjectSettings` in `raf_core/src/project.rs`)

New fields with `#[serde(default)]`:

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `enable_scripting` | `bool` | `true` | Per-project scripting toggle. |
| `allowed_script_languages` | `ScriptLanguageFlags` | `Rhai \| Nodes` | Bitflags of allowed tiers. C++ (WASM) requires explicit opt-in. |
| `script_execution_mode` | `ScriptExecutionMode` | `EditorOnly` | `Disabled` / `EditorOnly` / `Runtime`. |
| `auto_attach_scripts` | `bool` | `false` | Attach default script to new entities. |

UI: new card "Scripting" in `project_settings.rs`, after Runtime card.

### 12.3 New enums

```rust
pub enum ScriptLanguage {
    Rhai,
    Cpp,    // WASM target
    Nodes,  // Visual
}

bitflags! {
    pub struct ScriptLanguageFlags: u8 {
        const RHAII = 0x01;
        const CPP   = 0x02;
        const NODES = 0x04;
    }
}

pub enum ScriptExecutionMode {
    Disabled,
    EditorOnly,
    Runtime,
}
```

All enum variants and settings get i18n keys in `en.json` and `es.json`.

---

## 13. Crate Structure

New workspace member: `crates/raf_script/`

```
crates/raf_script/
  Cargo.toml
  src/
    lib.rs                    -- public exports
    host_api.rs               -- ScriptContext, all Host API functions
    node_handle.rs            -- NodeHandle (opaque ID wrapper)
    value.rs                  -- ScriptValue (dynamic value type)
    errors.rs                 -- ScriptError, ScriptResult
    prelude.rs                -- convenience re-exports for script authors
    lifetime.rs               -- on_start/on_update/on_destroy contract

    backends/
      mod.rs
      rhai_backend.rs         -- Rhai engine setup, fn registration, compile, call
      wasm_backend.rs         -- WASM module load, instantiate, call (stub for now)
      node_backend.rs         -- Bridges raf_nodes executor to host_api

    runtime.rs                -- Rhai script session: load attached scripts,
                                  call on_start/on_update on runtime scenes

    host/
      mod.rs
      scene_ops.rs            -- get_node, spawn, destroy, find_child, get_parent
      transform_ops.rs        -- set_position, set_rotation, set_scale, move_by
      property_ops.rs         -- set_color, set_visible, set_property, get_property
      audio_ops.rs            -- play_audio, stop_audio, set_volume
      input_ops.rs            -- is_key_pressed, was_key_just_pressed
      time_ops.rs             -- get_delta_time, get_elapsed_time
      interop_ops.rs          -- call_script_function
```

Dependencies:
- `raf_core` (for SceneGraph, units, config, i18n)
- `rhai` (for Tier 1 backend)
- `serde` (for ScriptValue serialization)
- WASM runtime dependency deferred to Tier 2 implementation.

The crate compiles and tests the Rhai backend plus the first runtime session.
`wasm_backend.rs` is a documented stub that returns
`ScriptError::WasmNotImplemented`. `node_backend.rs` calls into
`raf_nodes::executor` and wires its output to the Host API.

---

## 14. Mini-Roadmap

### Phase A: Architecture scaffold (done)
- Create `crates/raf_script/` with the structure above.
- Implement `ScriptContext`, `NodeHandle`, `ScriptValue`, `ScriptError`.
- Implement `host/` modules with real SceneGraph calls.
- Implement `rhai_backend.rs` with full Host API registration.
- `wasm_backend.rs` stub returning `WasmNotImplemented`.
- `node_backend.rs` wiring `raf_nodes::executor` to Host API.
- Add settings fields to `EngineSettings` and `ProjectSettings`.
- Add UI sections to `settings_panel.rs` and `project_settings.rs`.
- Add `script.*` command domain to `commands/` and `catalog.json`.
- i18n keys for all new strings.
- This document.

### Phase B: Prepared script lifecycle harness (in progress)
- `raf_script::runtime::RhaiScriptRuntime` loads attached scripts, calls
  `on_start`, and calls `on_update(dt)` on a cloned scene.
- `GameRuntimeState` owns the editor preparation facade and consumes the
  engine-agnostic `InputSnapshot` produced by the native input contract.
- Keep the top-level Play button guarded until runtime state, console logs,
  physics, nodes, and scene locking are validated together.
- Console output for script logs and errors.
- Hot-reload via `notify` watcher.
- `/script.run` command for one-shot testing through the same runtime session.

### Phase C: Visual node runtime wiring (implemented locally)
- Supported graphs compile through `runtime_compiler.rs` and attach as Rhai
  to the isolated Play world, not through the legacy `executor.rs`.
- Add `Call Script Function` only with a concrete persisted-node contract and
  lifecycle/error policy.
- Keep scene mutation, logs, locking, and runtime ownership outside the RafUI
  authoring surface.

### Phase D: WASM Native Modules (when runtime is built)
- Choose WASM runtime (wasmtime or lightweight alternative).
- Implement `wasm_backend.rs` for real.
- Write `aurarafi.h` C++ header and build instructions.
- WASM Host ABI version 1 spec document.
- Deprecate `docs/CPP_MODDING.md`.

### Phase E: Compile nodes to Rhai (implemented; dedicated code viewer future)
- `runtime_compiler::to_rhai(graph)` generates bounded runtime source.
- Nodes Compile returns source in the native console.
- `/script.compile_nodes file=nodes.ron` compiles saved project-local graphs.

### Phase F: Standalone runtime export (future)
- `ScriptContext` runs without editor dependencies.
- Same Host API, same backends, and no editor UI toolkit in the runtime path.
- Ship `.wasm` and `.rhai` in the export bundle.

---

## 15. Relationship to Existing Docs

| Document | Status |
|----------|--------|
| `docs/NODES_SYSTEM.md` | Current authoring contract; CAMERA_RUNTIME.md defines active execution and limits. |
| `docs/CPP_MODDING.md` | Superseded by Section 4 (WASM). Kept for historical reference until Phase D. |
| `docs/COMMANDS.md` | Extended with `script.*` domain (Section 8). |
| `docs/ARCHITECTURE.md` | New "Scripting" section pointing here. |
| `.ai/SYSTEM_TRUTH.md` | New pillar: "Scripts never touch engine internals; they call the Host API." |
| `docs/STABILIZATION_STATUS.md` | Entry for this session. |

---

## 16. Summary

- Three tiers (Rhai, WASM, Visual Nodes) share one Host API.
- Rhai is the primary beginner language. Its speed is fine for game logic.
- WASM replaces raw C++ FFI. It is sandboxed, multi-language, hot-reloadable, and our own Host ABI makes it "propio".
- Supported Visual Nodes execute through Rhai in isolated manual Play.
- Commands can create, attach, validate, and run scripts.
- Everything is in SI units via `units.rs`.
- The `raf_script` crate is the single home for all scripting logic.
- The local runtime exists; server transport and C++/WASM remain future work.
