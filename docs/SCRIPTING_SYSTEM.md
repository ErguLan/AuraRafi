# AuraRafi Scripting System

This document defines the canonical scripting architecture for AuraRafi. It
covers the three scripting tiers, the shared Host API, how visual nodes
execute, how commands create scripts, the security model, configuration
surfaces, and a phased mini-roadmap.

Status: **Architecture spec.** Runtime does not exist yet. This document
defines the contract so that when the runtime is built, every tier plugs in
without refactor.

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
| 3 | Visual Nodes | Non-programmers, prototyping | Full (executor is in-process) | Interpreted graph walk |

Tier 1 and Tier 3 ship first. Tier 2 is designed now, implemented when the
runtime is built and a WASM runtime dependency is approved.

---

## 2. Current State Audit

Three disconnected systems exist today. This section is the honest baseline.

### 2.1 Visual Nodes (`crates/raf_nodes/`)
- Full canvas UI with pan/zoom, palette, drag-to-connect, undo/redo, multi-flow (`crates/raf_editor/src/panels/node_editor.rs`).
- Working interpreter (`executor.rs`) that walks flow chains, evaluates data pins, handles If branches.
- **Gap**: `Spawn Entity`, `Destroy Entity`, `Set Position` nodes only log `"deferring to ECS Bridge"`. They do not touch `SceneGraph`. They are visually functional but inert in runtime.
- `compiler.rs` is a stub that validates connectivity only.

### 2.2 External Scripts (`crates/raf_editor/src/script_support.rs` + `panels/behaviors.rs`)
- Detects `.rs`, `.cpp`, `.rhai`, `.lua`, `.py`, `.js`, `.ts` by extension.
- `is_engine_supported()` returns true only for Rust, C++, Rhai.
- Scans `assets/` for script files, validates presence of `on_start`/`on_update` by text search.
- `SceneNode.scripts: Vec<String>` stores relative paths.
- `behaviors.rs` panel attaches/detaches scripts, shows validation status, opens VS Code.
- **Gap**: No runtime loads or executes these files. The paths are stored metadata only.

### 2.3 CommandBus (`crates/raf_core/src/command.rs`)
- Full serializable command bus with submit/flush/record/undo/redo.
- **Gap**: Removed from `app.rs` as dead code two sessions ago. The bus exists in `raf_core` but nothing wires it.

### 2.4 Console Commands (`crates/raf_editor/src/commands/`)
- Slash command parser with handlers for `game.*`, `electronics.*`, `pcb.*`, `workspace.*`.
- These handlers DO mutate the scene and are the only working "scripting" surface today.
- **Gap**: No `script.*` domain exists. Commands cannot create, attach, or run scripts.

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

The context is constructed per-frame by the (future) `ScriptRuntime` system
and passed to each script's `on_update(dt)`. Scripts receive it implicitly
(Rhai) or as a pointer (WASM).

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
ScriptRuntime collects all node.scripts paths
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

## 7. How Visual Nodes Run

### 7.1 Phase 1 (immediate): Interpreted graph walk
The existing `executor.rs` already walks the flow chain correctly. The only
change is: where it currently logs `"deferring to ECS Bridge"`, it calls the
Host API instead.

- `Spawn Entity` node -> `ctx.spawn_entity(name, primitive)`
- `Set Position` node -> `ctx.set_position(handle, x, y, z)`
- `Destroy Entity` node -> `ctx.destroy_entity(handle)`

The executor receives a `&mut ScriptContext` for the duration of the walk.
This is the minimal wiring to make nodes functional.

### 7.2 Phase 2 (future): Compile to Rhai source
`compiler.rs` (currently a stub) gains a `compile_to_rhai(graph) -> String`
pass. This produces readable Rhai source from a node graph, viewable in the
editor ("View as Code"). This unifies the execution path: a compiled node
graph runs through the same Rhai backend as a hand-written script.

### 7.3 Call Script Function node
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
    Compiles a node graph to Rhai source via compiler.rs Phase 2.
```

### 8.2 Domain
`script` commands live in domain `shared` (available in any project type).
Handler module: `crates/raf_editor/src/commands/script.rs`.

### 8.3 Catalog entry
Add to `assets/commands/catalog.json` following the existing schema:
name, aliases, domain, description_key, parameters, examples.

### 8.4 Command-to-script bridge
Commands that need to run script logic go through the Host API, not through
a parallel path. This means `/script.run` constructs a `ScriptContext`,
creates a Rhai engine, and calls `on_start` through the same code a future
runtime would use. No duplicate execution path.

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
| Visual Nodes | N/A (in-process) | N/A | N/A | Step limit in executor (already 10,000) |

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
| Visual Nodes | Already has auto-save (30s). On document change, re-walk graph next frame. |

Hot-reload is opt-in via `script_hot_reload` setting (default true).

---

## 12. Configuration

### 12.1 Engine-level settings (`EngineSettings` in `raf_core/src/config.rs`)

New fields with `#[serde(default)]`:

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `script_runtime_enabled` | `bool` | `false` | Master switch. Off until runtime exists. |
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

The crate compiles today with Rhai backend only. `wasm_backend.rs` is a
documented stub that returns `ScriptError::WasmNotImplemented`. `node_backend.rs`
calls into `raf_nodes::executor` and wires its output to the Host API.

---

## 14. Mini-Roadmap

### Phase A: Architecture scaffold (this session)
- Create `crates/raf_script/` with the structure above.
- Implement `ScriptContext`, `NodeHandle`, `ScriptValue`, `ScriptError`.
- Implement `host/` modules with real SceneGraph calls.
- Implement `rhai_backend.rs` with full Host API registration (compiles, does not run yet).
- `wasm_backend.rs` stub returning `WasmNotImplemented`.
- `node_backend.rs` wiring `raf_nodes::executor` to Host API.
- Add settings fields to `EngineSettings` and `ProjectSettings`.
- Add UI sections to `settings_panel.rs` and `project_settings.rs`.
- Add `script.*` command domain to `commands/` and `catalog.json`.
- i18n keys for all new strings.
- This document.

### Phase B: Editor Play mode (next)
- `ScriptRuntime` system in `app.rs` behind `script_runtime_enabled` flag.
- In editor Play mode: load scripts, call `on_start`, call `on_update(dt)`.
- Console output for script logs and errors.
- Hot-reload via `notify` watcher.
- `/script.run` command for one-shot testing.

### Phase C: Visual node wiring (next)
- Replace `executor.rs` "deferring to ECS Bridge" with Host API calls.
- Add `Call Script Function` node to palette.
- Node execution in Play mode via `node_backend.rs`.

### Phase D: WASM Native Modules (when runtime is built)
- Choose WASM runtime (wasmtime or lightweight alternative).
- Implement `wasm_backend.rs` for real.
- Write `aurarafi.h` C++ header and build instructions.
- WASM Host ABI version 1 spec document.
- Deprecate `docs/CPP_MODDING.md`.

### Phase E: Compile nodes to Rhai (future)
- `compiler.rs` gains `compile_to_rhai(graph) -> String`.
- "View as Code" button in node editor.
- `/script.compile_nodes` command.

### Phase F: Standalone runtime export (future)
- `ScriptContext` runs without editor dependencies.
- Same Host API, same backends, no `egui` in the path.
- Ship `.wasm` and `.rhai` in the export bundle.

---

## 15. Relationship to Existing Docs

| Document | Status |
|----------|--------|
| `docs/NODES_SYSTEM.md` | Updated to reference this doc for runtime behavior. |
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
- Visual Nodes are interpreted now, compilable to Rhai later, and can call Rhai functions.
- Commands can create, attach, validate, and run scripts.
- Everything is in SI units via `units.rs`.
- The `raf_script` crate is the single home for all scripting logic.
- Runtime does not exist yet; this architecture is the contract for when it does.
