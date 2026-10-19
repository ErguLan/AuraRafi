# Scripting internals

Audience: engine contributors. For writing a game, use [Programming guide](SCRIPTING_GUIDE.md).
This is a code map, not a second architecture specification: `Agent.md`,
`.ai/SYSTEM_TRUTH.md` and the live implementation take precedence.

Status (2026-10-10): the paths below are connected in source. The owner,
typed-parameter and prefab update has not been compiled or manually accepted
in this pass; the user owns functional testing. C++/WASM and online execution
are not implemented by this update.

## Execution path

```text
Authored SceneGraph + project settings + session Nodes
  -> isolated raf_runtime::RuntimeWorld snapshot
  -> raf_script::RhaiScriptRuntime
     -> shared compiled AST per canonical source file
     -> independent scope/schedule per entity attachment
     -> Rhai adapter -> ScriptContext / NodeHandle -> snapshot mutations
  -> raf_player presentation through the existing graphics contract
```

Play never runs Cargo. Rhai compilation builds an in-memory AST, not an
executable. Stop discards simulation changes; authoring commands and scene
history are a separate boundary.

## Source map

| Responsibility | Source |
| --- | --- |
| Simulation and hook order | `crates/raf_runtime/src/world.rs` |
| Attachment loading, AST sharing, hooks, reload | `crates/raf_script/src/runtime.rs` |
| Opt-in update intervals and event subscriptions | `crates/raf_script/src/runtime/schedule.rs` |
| Synchronous Rhai context guard and value conversion | `crates/raf_script/src/backends/rhai_backend.rs` |
| Public neutral operations and identity validation | `crates/raf_script/src/host_api.rs`, `node_handle.rs` |
| Parameters, tags, bounded queries | `crates/raf_script/src/behavior_api.rs` |
| Rhai vocabulary, not domain rules | `crates/raf_script/src/backends/rhai_behavior_api.rs` |
| Camera and hierarchy operations, including shared follow | `crates/raf_script/src/camera_api.rs`, `host_api.rs`; thin wrappers in `backends/rhai_scene_api.rs` |
| Optional compiler and executor interfaces | `crates/raf_script/src/backends/wasm_contract.rs` |
| Optional module preparation and lifecycle boundary | `crates/raf_script/src/backends/wasm_backend.rs` |
| Typed imports and per-attachment guest token table | `crates/raf_script/src/backends/wasm_host.rs` |
| Graph compilation to the same Rhai lifecycle | `crates/raf_nodes/src/runtime_compiler.rs` |
| Serialized values and per-source overrides | `crates/raf_core/src/scene/runtime.rs`, `graph.rs` |
| Prefab validation, fresh identities, reference remapping | `crates/raf_core/src/scene/prefab.rs` |
| Shared behavior/prefab authoring commands | `crates/raf_editor/src/commands/behavior.rs` |
| Inspector presentation / action translation | `panels/inspector_behavior_surface.rs`, `native_behavior_authoring.rs` in `raf_editor/src` |
| Optional source relationship indexing | `crates/raf_script/src/reference_analysis.rs`, `raf_editor/src/script_relationships.rs` |

## Ownership and references

`ScriptContext::owner()` identifies the attachment's entity. `get_parent`
identifies an actual scene hierarchy parent. The public recommended Rhai form
is `owner()`, not `script.parent`. `self_node()` and `script.parent` remain
compatibility aliases, without changing old projects' meaning.

Global session Nodes have no implicit owner. They must resolve an explicit
entity. The Entity Reference node calls `get_entity_reference(handle, name)`
to read a shared typed Entity variable on that entity.

Persistent references are `VariableValue::Entity(Option<Uuid>)`, never flat
scene indices or raw runtime handles. Resolution returns a scoped `NodeHandle`
only for a live, unique UUID. Missing/unassigned/deleted references return an
invalid Rhai handle; callers check `is_valid`. A wrong parameter type is an
error. Reparenting and renaming do not change a UUID. Duplicate UUIDs are
ambiguous, never resolved to the first match.

Runtime handles validate the runtime instance, slot and UUID. User code cannot
borrow `SceneGraph`, forge a handle from an integer, or retain a Rust reference.
The Rhai adapter's thread-local context is scoped to synchronous execution;
the guard clears access on return and rejects reentrant mutable access.

## Parameters, state and reuse

`SceneNode.scripts` remains the attachment list; there is no parallel behavior
executor. The same source on two entities shares compilation, not mutable scope.
At most one effective attachment of the same canonical file exists per entity.

`SceneNode.behavior_parameters[source]` stores typed overrides for that exact
attachment path. Reading `parameter(name, default)` or `entity_parameter(name)`
uses the following order:

1. The current attachment's override, if present.
2. The owner's shared `variables`, if present.
3. Scalar caller default, or an invalid handle for a missing Entity reference.

An explicit `Entity(None)` blocks a shared reference; it is not a request for
automatic discovery. Incompatible present types do not silently use defaults.
Scalar parameters are Bool, finite Number and bounded Text. Parameter names
are 1..128 bytes. Authoring allows 256 parameters per scope and 64 behaviors
per entity. These are authored values, not a schema extracted from Rhai.

Use scope variables for private behavior state and `get_property/set_property`
for shared entity state. Overrides are configuration, not network authority.
Changing authoring values during Play does not update an already isolated world.

## Lifecycle, reload and failures

Initialization evaluates top-level code once per attachment, then calls optional
`on_start()`. Each simulation tick runs `on_update(dt)`, `on_fixed_update(dt)`,
physics, contact hooks, then `on_late_update(dt)`. Local queued messages are
delivered at a safe fixed-update boundary, not recursively. Stop performs
bounded `on_destroy()` cleanup.

`behavior.update_interval` throttles only `on_update`, accumulating elapsed
simulation time without catch-up loops. It does not throttle physics or late
camera updates. All hooks are optional; an empty project needs no controller.

Hot reload is host-triggered at a safe boundary. Accepted source replaces the
AST while preserving attachment scope; top-level initialization does not rerun.
Rejected source keeps the previous AST. New top-level state may require Stop
and Play. See [Behavior runtime](BEHAVIOR_RUNTIME.md) for scheduling details.

Operation counts, shared deadlines, cancellation, instance/entity/source limits,
event queue/payload limits and bounded queries are enforced by the existing
runtime. They are not permission to do unlimited work across many attachments.
Errors are reported with source, owner and hook; failing attachments can be
disabled. No C++ sandbox or online security boundary is implied.

## Authoring and prefabs

Inspector, Console, internal Agent and attached adapters converge on
`behavior.attach`, `behavior.detach`, `behavior.parameter`, `prefab.capture`
and `prefab.instantiate`. Scene mutations use the existing snapshot history;
external requests retain gateway revision, preview and idempotency handling.
Preview operates on disposable scene state and never creates a prefab file.

A `.prefab.ron` is a versioned subtree snapshot, not a live inherited asset.
Validation checks identities, parent-before-child order, link agreement,
connectivity and budgets before scene allocation. Instantiation assigns fresh
UUIDs, clears ECS indices/reconciliation keys and remaps internal typed
references in shared variables and attachment overrides. External UUIDs stay
external, even when absent. Ordinary hierarchy duplication uses the same
reference-remapping helper. No name strings inside source code are rewritten.

Limits: 1024 nodes / 8MiB per prefab; 10000 flat scene slots when instantiating.
The root's local transform is preserved. Instantiating under another parent
therefore uses that parent's coordinate space. Startup camera project settings
are not automatically reassigned. Capture refuses overwrites and requires an
existing destination folder inside `assets`. Scene Undo removes an instantiated
copy but does not delete a captured asset file. There is no prefab inheritance,
live propagation, package export or automatic external-reference rebinding.

## Adding an operation

Implement domain semantics in the Host API first; register thin Rhai wrappers
and compile Nodes to the same operation. Add authoring actions to the canonical
command catalog and all relevant adapters, not a panel-only mutation path.
Keep examples opt-in. Document units, limits, missing-reference behavior and
verification status. A future C++ adapter must reuse this boundary; the current
default WASM backend returns `WasmNotImplemented`.

## Future compiled languages: prepared seams, no installed executor

Decision (2026-10-10): do not download, install, compile or test a WASM engine
or C++ toolchain now. Rhai remains the local Play executor; Rust is the engine
implementation language. Attaching a `.rs` file is not Rust scripting support.
C++ and Rust guest sources can later target the same optional WASM boundary.

Three separate responsibilities are implemented as dependency-free contracts:

1. `WasmToolchain` accepts bounded in-memory `WasmBuildRequest` sources tagged
   Cpp or Rust and produces bytes. `build_with` validates input and the bounded
   artifact header. No concrete compiler/process adapter ships. Build input
   paths reject traversal, absolute paths and case-insensitive duplicates.
2. `WasmExecutor` validates/compiles a module; its immutable code can be shared.
   `WasmCompiledModule` creates independent `WasmInstance` objects. Register an
   engine-owned provider explicitly through `WasmBackend::with_executor`.
   `WasmBackend::default()` is unavailable and does no filesystem/PATH probing.
3. `GuestImports` exposes typed `HostOperation` requests through `HostBridge`.
   Owner, parameters, hierarchy, transforms, camera/follow, scalar properties,
   input/time and local events delegate to existing Host API operations.
   Rhai follow also delegates to `NodeHandle::follow`; there is one behavior
   implementation, not separate language-specific follow algorithms.

The module wrapper checks the actual provider-reported API version, known
optional lifecycle exports, hook arguments, one-time Start, Stop/failure state
and attachment identity (runtime + owner UUID + source). Missing exports are
no-ops. Guest entity zero means unassigned. Other guest values index a bounded
per-attachment table of scoped handles; never decode them via
`NodeHandle::from_raw`. Tokens remain local and are not event/network identities.

Default limits: 4MiB module, 256 memory pages, 100000 instructions per call and
1024 entity tokens. Provider-side enforcement is mandatory; these Rust values
do not themselves implement a VM sandbox. The provider must fully validate
modules/imports/signatures, check linear-memory ranges/UTF-8, deny ambient
WASI/filesystem/network access, bound compilation/instantiation/stack/tables,
and enforce fuel and cancellation/deadline checks even in loops with no imports.
Header validation alone is not proof that bytes are a valid or safe module.

Future registration sequence (engine integration, not a runnable game script):

```text
explicit request.build_with(optional toolchain) -> bounded WASM bytes
registered WasmExecutor -> WasmBackend.compile -> shared CompiledWasm
per-attachment ScriptContext -> CompiledWasm.instantiate -> WasmModuleHandle
existing lifecycle scheduler -> handle.invoke(hook, args, context)
typed guest imports -> existing Host API -> isolated runtime snapshot
```

The live Play attachment loader still accepts Rhai only and explicitly rejects
compiled-language sources/modules. Installing a compiler or toggling Allow WASM
does not register a provider or change that loader. Remaining integration:
choose/implement the provider and wire module bindings into the existing single
runtime scheduler, contact/event routing and reload/preparation flow. Do not
create another simulation loop or let authoring Agent/CLI/MCP start Play.

`HostOperation` and the traits are an internal Rust integration contract, not a
stable serialized WASM ABI or a shipping C++/Rust guest SDK. Final import/export
signatures, memory encoding and guest declarations must be specified with the
chosen provider; the historical raw-slot ABI sketches are not safe to implement
literally. Physics, audio and additional operations must extend the same Host
API bridge rather than bypass it. Native-window/security/performance acceptance
and executable guest examples remain future checks. This pass runs no tests.
