# AuraRafi — System Truth (Source of Truth)

## Provider model selection (2026-10-30)

The shared provider picker separates API-key catalogs from authorized account
plan catalogs. OpenAI has no bundled model suggestions; account choices preserve
server display names, slugs and ordering. Refresh never replaces a saved model.
API fields and manual ids are hidden in plan mode. Favorites use the selected
credential lane. Catalog requests initialize their state, remain bounded to one
pending worker per provider, hide results from other credentials and wake the UI
on loading/error/empty transitions. Settings requests discovery when its picker
opens or the user explicitly refreshes, not while typing a credential.

Verification: final AI catalog tests 4/4; initial focused editor pass 10/10.
Final editor retest was blocked by unrelated pipeline signature errors in
`shadow_gpu.rs`; those files were not modified. No live login/inference,
application build or native-window acceptance was performed. Details and limits
are recorded in `docs/AI_SYSTEM.md`.

## Harness implementation pass (2026-10-23)

Native Agent receipts bypass wire-result truncation until postconditions have
been checked; provider output still uses the existing bounded model projection.
Parent verification resolves live node identity. Mixed script/build tool packs
prioritize scripting within the existing contextual budget, and unknown-tool
diagnostics enumerate only advertised routes. Step-guard exhaustion is an Error,
not successful completion. Workspace tool rows reuse the compact dock renderer,
including virtual height calculations, and failures expand automatically.
Native workspace/dock group multi-tool activity without hiding failures/final
answers and render bounded Markdown/code tiles. Composer image paste and file
drops use bounded worker ingestion, project-confined media references and actual
multimodal provider content parts; metadata chips are not thumbnails or proof of
model understanding. Vision requires explicit confirmation. Only latest-turn
images are sent. Automatic output omits the cap; reasoning uses endpoint-specific
overrides. Provider length/filter stops and incomplete streams fail explicitly.
Settings show one provider editor and truthful local setup status, not connection
validation. See `docs/AGENT_WORKSPACE.md` for limits and remaining work; automatic
context management, visual feedback from captures and generation recipes are not
implemented by this pass. Native-window/provider/performance validation is pending.

Focused verification on 2026-10-23: 52 editor tests, 44 `raf_ai` tests and 8 CLI/MCP
tests passed. A Spanish dark 760x640 CPU UI fixture was visually inspected; locale
JSON and scoped whitespace checks passed. The application build failed on a full
C: temporary volume; tests subsequently passed with process-local TEMP/TMP on D:.
No updated application executable is delivered, per the user's later tests-only
instruction. See `docs/AGENT_WORKSPACE.md` for verification scope and remaining work.

## Native Active execution permissions (2026-10-20)

`AgentPanel::sync_execution_permissions` makes Active authoritative over the
legacy review preference, for current and parked runs. Native workspace and
editor dock hide review controls/approval cards in Active. Disabling runtime
review resumes a suspended queue and drops only unapproved review snapshots;
explicit decisions keep their revision guards. Inspect/Plan, command gateway
validation, project confinement, durable saves and external-tool grants remain
unchanged. Focused runtime, panel-controller and workspace/dock presentation
regressions passed on 2026-10-23. Native-window interaction remains unverified.

The workspace presentation key also includes AgentMode, so changing the mode
through Settings invalidates cached permission controls even for an idle chat.
Revalidated on 2026-10-30: four focused Active-permission tests and the scoped
review revision-conflict regression passed. No application executable was built
and no running editor instance was modified by this verification pass.

## Agent scene preview (2026-10-19)

`agent_preview.rs` owns an optional read-only, demand-created scene presentation
and temporary camera; `agent_workspace_layout.rs` owns transient splitter geometry
and pointer capture. The Agent right rail projects these through RafUI chrome.
`native_agents.rs` resolves the conversation's service-owned or editor-mounted
scene without cloning/mutating it. ApiGraphicBasic retains rendering ownership
and shares the native graphics context/compositor; no editor UI or Runtime is
mounted for preview. Hidden previews stop drawing immediately, expire after five
seconds, and are destroyed on leaving Agents or changing document/project.
Compositor source release also drops CPU-upload/GPU-view references. See
`docs/AGENT_WORKSPACE.md`; native-window and performance acceptance remain separate.
Editor verification initially encountered unrelated compile errors in
`electronics_controller.rs::toggle_pcb_layer` (`?` in a bool-returning method)
and the runtime-settings error message in `native_application.rs` (`String + &String`).
The user explicitly authorized the two narrow corrections. Verification passed:
26 focused workspace tests, all 35 raf_ai unit tests, four semantic-icon tests,
and `cargo build -p aura_rafi_editor` (historical note: `--target-dir target_agent_workbench_20261014` was used once; that practice is now FORBIDDEN — use `target_gnu/`).
Real CPU scene/UI captures were generated for both themes and languages at
900/1440 px; visual inspection covered Spanish dark/wide and light/narrow.
This does not establish native GPU interaction or FPS/RAM/frame-time acceptance.

## Precise construction, visual Assets and Hierarchy (2026-10-19)

Shared authoring geometry lives in `raf_core::scene::geometry`: shape descriptors,
local dimensions and world/subtree AABBs apply transforms once. Capsule has a
1 m diameter and 2 m total Y height. Optional `size` on game.add/update derives
scale and rejects simultaneous scale; no scene migration or duplicate size state.
Assets use a typed catalog projection and a bounded worker for 128 px image and
isolated ApiGraphicBasic shape/prefab thumbnails. Catalog refresh invalidates
previews; GLB/GLTF remain pending 0.12. Placement stages geometry outside the
live scene and commits one undoable edit. Hierarchy adds conjunctive field
queries, bounded badges, insertion parent, ordered drops and UUID session view
state in editor_hierarchy.ron. Script rows remain optional and virtual. Tab keeps
Object Mode with an explicit unavailable notice; persistent topology editing,
linked prefab packages and additional shapes stay in the 0.12 roadmap.
Verification status must distinguish focused tests from native DPI/interaction QA.

## Runtime services (2026-10-18, source implementation)

`runtime_config` owns input axes/progress policy. `raf_script::services` owns
typed progress requests/receipts, cursor intent and session requests; `query_api`
owns bounded AABB queries and the bounds helper shared with current physics.
`raf_runtime::{storage,session_loader}` own lazy bounded disk I/O and strict
saved-session ingress. `RuntimeWorld`/`raf_player` prepare candidate worlds,
retain the old world on failure, and transfer progress only after commit.
Project Settings exposes local folder grants separately from shared policy.
Rhai and generated service Nodes share the Host API; future WASM imports carry
scoped guest tokens and typed values, without installing or activating a provider.
No controller/player is injected. Export, animation, audio expansion and online
transport are excluded. See `docs/RUNTIME_SERVICES.md`; native interaction,
failure-injection and low-resource acceptance remain distinct from static checks.

## Camera helper and instrument presentation (2026-10-16)

Camera is a virtual Assets shortcut (`builtin://camera`) using existing camera
creation/history, not a file or implicit startup reassignment. The camera guide
no longer depends on billboard-origin visibility or a -0.99 x-ray depth offset.
`ApiGraphicBasic::line_projection` owns allocation-free homogeneous clipping
shared by GPU and CPU line execution. Biased depth is clamped inside 0..1;
CPU mesh/line/shadow reconstruction uses the same normalized depth convention.
WGPU remains the private executor, not the owner of editor helper policy.

Lighting and Runtime presentation follow the explicitly requested Creative
Instrument Craftsmanship guide, with generated semantic SVG/PNG icons and
retained interaction states. No new backend, dependency or idle animation loop
is introduced. Source/regression fixtures are written; compiled/native-window
acceptance and performance measurements have not been rerun for this pass.

## World Lighting deliveries 1–2 (2026-10-12)

`raf_core::environment` owns versioned world lighting in `SceneGraph`; old scenes
default to Neutral. `panels/lighting_surface.rs` and
`native_workbench_lighting.rs` integrate the Hierarchy Lighting tab and Inspector.
Sun, colored ambient, solid/gradient sky and solar disk are connected through
`raf_render::environment_frame` and the existing ApiGraphicBasic GPU/CPU executor.
Geometry revisions stay independent of clock changes. The Play snapshot owns
runtime lighting; transient view overrides live in `raf_script::view`.
`environment_api` is the shared Rhai/Nodes Host contract. Canonical
`lighting.inspect/set/reset` authoring commands are connected to native UI,
Console, Agent and attached CLI/MCP. Delivery 2 adds opt-in directional shadows:
`shadow_frame.rs` owns coverage, caster keys and capped CPU recovery;
`ApiGraphicBasic/shadow_gpu.rs` privately owns the retained GPU depth pass and
receiver pipelines. One 256/512/1024 map, hard/four-tap filtering, distance fade,
bounded work/memory, cast/receive flags and shared scripting properties. The CPU
map is capped at 256; overflow omits the entire effect and reports a metric.
Imported skyboxes, contact shadows, cascades, GI, volumetrics, network replication
and WASM environment imports are not implemented by this delivery.
See `docs/LIGHTING.md`; native visual/performance acceptance is distinct
from automated checks.

Lighting optimized modes (2026-10-19): `EnvironmentSettings.technology` selects
exactly `optimized_current` (legacy default) or `optimized_improved`, exposed in
Lighting > Technologies. Improved reuses spare light/sky uniform channels for
hemispheric ambient, exposure shoulder and softened cel transitions. The private
ApiGraphicBasic executor admits one five-tap scene-only spatial filter and one
RGBA8 target within min(64 MiB, graphics budget / 16), or explicitly skips it on
budget denial; this is not FXAA/MSAA and never filters RafUI/CAD. Improved fits
the existing single shadow map to camera receivers and bounds Soft filtering at
nine comparisons; Current retains four. There are no cascades, temporal history
or extra background jobs. See `docs/LIGHTING.md` for opt-in read-only project A/B
captures and the distinction between offscreen QA and native/low-end acceptance.

This file contains the definitive technical registry of AuraRafi. AI agents must rely on this file as the primary architectural mapping of the codebase.

## Current native migration truth (2026-08-24)

The active editor path is Winit + RafUI + ApiGraphicBasic. The retired widget
toolkit is not a runtime dependency or an ownership boundary. Any old panel
paths mentioned below are historical records, not active ownership. The
current presentation boundary is
`native_application.rs`, `native_workbench.rs`, `native_editor_runtime.rs`,
`native_surface.rs`, and `ApiGraphicBasic::editor_compositor`.

## Current decisions absorbed here

Local Runtime implementation: `raf_runtime` owns isolated scene snapshots,
fixed-step simulation and bounded Rhai execution, independently of windows.
`raf_player` owns native presentation using the existing RafUI/ApiGraphicBasic
contracts. `native_game_runtime.rs` is the manual editor launch/control adapter;
the executable's `--runtime` route skips editor/Hub/Agent initialization.
Play never invokes Cargo and never writes simulation changes into the project.
Both window modes reuse the startup loading document. Runtime never implicitly
creates a character/camera. New Game projects seed an editable Camera template
with its UUID configured as startup; loading existing scenes never injects it.
Attached Agent/CLI/MCP runtime activation stays forbidden.
See `docs/LOCAL_RUNTIME.md` for acceptance status and explicit limitations;
build success is not proof of live window behavior or low-resource performance.

Camera/Nodes update (2026-10-03): a camera is an optional component on any
scene entity, including a visible Part. Startup selection is one project
UUID, copied into each player's transient view state; Rhai and Nodes may
override it without editing project settings. Other than the explicit new-Game
Camera template, no camera/character/rig is injected. Stable reference lookup, explicit local/world
transforms, hierarchy changes, lens validation and camera selection live in
the shared ScriptContext/NodeHandle API. Late-update runs after physics.
The active session Nodes snapshot compiles through
`raf_nodes::runtime_compiler` into a global Rhai attachment with the same
sandbox and Host API; the legacy executor is not the Play path. C++/WASM and
server networking remain unimplemented adapters. See CAMERA_RUNTIME.md.

Physics scripting: `raf_script::physics_api` owns explicit velocity, impulse,
body and collider controls for Rhai/Nodes. `raf_runtime::physics` owns bounded
integration, `physics_contacts` owns UUID contact history and
`physics_response` owns bounded translation-only depenetration. Physics hooks
dispatch under the existing script deadline before late-update. No gameplay
controller or new physics dependency is injected. See PHYSICS_RUNTIME.md;
native acceptance and resource measurement remain separate gates.

Composable behavior checkpoint (2026-10-06): existing multi-script attachments
remain the composition boundary, with per-attachment scope state and scalar
Inspector parameters. `raf_script::behavior_api` supplies user capability tags
and bounded queries; the lifecycle supplies opt-in update intervals/event
subscriptions and structured, bounded local messages. Legacy `script.parent`
is the attachment owner and `game["Name"]` is unambiguous root lookup. Seven
behavior Nodes compile to the same Host API. No gameplay controller is injected.
`raf_editor::script_relationships` owns an opt-in background source index;
Hierarchy renders attached/reference rows as presentation only, never scene
children. See BEHAVIOR_RUNTIME.md. Native acceptance, typed behavior manifests,
live prefab inheritance/package authoring and online replication are not claimed complete.

Owner and reuse update (2026-10-10, source implementation only): `owner()` is
the canonical public attachment reference; hierarchy parent remains distinct.
Typed Entity UUID parameters and per-source overrides are edited through the
native Inspector and shared `behavior.*` commands. Entity Reference Nodes reuse
the Host API. Versioned `.prefab.ron` subtree capture/instantiation creates
independent copies, with fresh UUIDs and internal typed-reference remapping;
ordinary duplication uses the same remapper. No live inheritance or automatic
gameplay association is implied. This update has not been compiled, run or
functionally accepted in this pass. See SCRIPTING_INTERNALS.md and SCRIPTING_GUIDE.md.

Optional compiled-language preparation (2026-10-10): `raf_script::backends`
separates dependency-free `WasmToolchain` build inputs from `WasmExecutor`,
shared compiled modules and per-attachment instances. `wasm_host` owns a
bounded guest-token table and typed imports delegating to the shared Host API.
`wasm_backend` is explicitly unavailable by default; no tool download,
dependency, provider registration or automatic process launch is added.
The Play loader remains Rhai-only. Rust is the motor's implementation language,
not direct `.rs` attachment support. Concrete VM/toolchain adapters, finalized
guest ABI/SDK and scheduler/reload/contact integration are future work. No
compiled-language execution, sandbox acceptance or tests are claimed here.

This section is the active decision register. Historical ADR files are not a
second authority. When an older note conflicts with this section or with the
active code map below, this document and the live code win.

* **Native editor ownership**: Winit, RafUI, ApiGraphicBasic and the native
  workbench own the active editor path. The old monolithic `app.rs`/retired
  widget descriptions are historical and must not be used as implementation
  targets.
* **Command boundary**: `raf_core::CommandBus` remains the transactional
  primitive for history and undo/redo. Native UI, Agent, CLI and MCP converge
  through the shared command protocol and `raf_editor::commands::CommandGateway`;
  no adapter may create a parallel mutation path.
* **Electronics presentation**: schematic and PCB authoring are 2D-first
  documents. The native CAD surface uses shared ApiGraphicBasic contracts;
  3D inspection is future/prepared capability, not the current PCB ownership
  model.
* **Component extensibility**: the electrical library intentionally combines
  built-in templates from `default_library()`, external `.ron` assets, and
  registered Rust extension hooks. “Data-driven” does not mean “no built-ins”.
* **Persistence and scene data**: project documents use RON; the core scene
  stores nodes in contiguous vectors with index relationships. Do not replace
  those boundaries with ad-hoc JSON/TOML or pointer-owned trees.
* **Translations**: visible UI text uses the embedded JSON i18n catalogs and
  `raf_core::i18n::t()` with entries in both `en.json` and `es.json`; inline
  language branches are deprecated.
* **Windows toolchain**: the repository's documented build target remains
  `stable-x86_64-pc-windows-gnu` with outputs under `target_gnu/` (set in
  `.cargo/config.toml`). FORBIDDEN to create other `target_*` (`target_ai`,
  `target_check`, `target_fix`, ...). Agents: do not use a custom `--target-dir`;
  isolation only via `CARGO_TARGET_DIR` outside the repo.

* **Visual design governance**: `.ai/STUDIO_GRADE_UI.md` is the active
  product-wide visual guide. It supplies defaults rather than immutable
  architecture. A current user brief or screenshot may override a visual
  default while preserving the technical, accessibility, localization, and
  performance contracts below.

## 1. Core Architectural Pillars
AuraRafi is a unified sandbox engine. It handles both standard Game ECS Scene Graphs and CAD PCB Design Topologies inside a lightweight structural core.

* **ApiGraphicBasic Graphics Ownership**: `ApiGraphicBasic` is the sole public graphics owner. WGPU is the current private adapter/compatibility backend while owned DX12, Vulkan, and Metal backends mature behind the same contract. The authoritative migration rule is `.ai/APIGRAPHICBASIC.md`.
* **GPU Hardware Driver Priority**: Uses available GPU hardware as the normal execution path, including integrated GPUs for potato profiles. CPU software rasterization is recovery, headless, testing, or incompatibility; it is not the automatic definition of low-spec mode.
* **Native Graphics Ownership**: Responsibilities move into Rafi-owned ApiGraphicBasic contracts capability by capability. Upper layers never fork into separate WGPU/native versions; WGPU remains private until a validated executor replacement exists.
* **Viewport Integrity Baseline (2026-07-19)**: View2D is an orthographic view of the shared 3D scene. The active scene path batches adjacent lines, bounds persistent mesh reuse, renders physical pixels for high-DPI targets, and uses world-transform scale for culling/focus. `Sprite2D` is a legacy alias to `Plane`, not an active primitive.
* **Unified Event & Mutator Layers**: `CommandBus` supplies transactional
  history; the editor `CommandGateway` and shared command protocol are the
  boundary used by native UI, Agent, CLI and MCP for project mutations.
* **No MSVC Tooling Assumptions**: Always compiled via `stable-x86_64-pc-windows-gnu` using MinGW/MSYS2 toolchains on Windows. All compiled outputs are routed to `target_gnu/` (the only one allowed). Never create an alternate `target/` or `target_*` inside the repo.
* **Canonical Unit System**: 1 world unit = 1 meter (games viewport 3D). 1 schematic unit = 1 millimeter (schematic/PCB canvas). All internal calculations in SI. `DisplayUnit` enum only changes UI display, never computation. Constants in `raf_core::units`.
* **Scripts Never Touch Engine Internals**: All scripting (Rhai, WASM, Visual Nodes) calls the shared Host API in `raf_script::host_api::ScriptContext`. No tier accesses `SceneGraph`, `InputState`, or audio directly. Scripts hold `NodeHandle` (opaque IDs), not references. See `docs/SCRIPTING_SYSTEM.md`.

---

## 2. Definitive Directory Map

### Main Entry Points
* **`editor/`**: Binary wrapping crate.
  * `src/main.rs`: Entry point. Starts the native Winit application boundary;
    RafUI surfaces and ApiGraphicBasic own presentation below it.

### Crates Directory (`crates/`)

#### 1. `raf_core` (Base Infrastructure)
* `src/lib.rs`: Exports core modules and basic types.
* `src/config.rs`: Houses `EngineSettings` (Auto-save limits, languages, input settings, potato preset gates, `DisplayUnit` for UI) and `Theme` enums.
* `src/units.rs`: Canonical unit constants (`METERS_PER_UNIT`, `MM_PER_SCHEMATIC_UNIT`, `SCHEMATIC_TO_WORLD`) and `DisplayUnit` enum (Metric/Imperial/Game). Public for future FFI/scripting import.
* `src/project.rs`: Formulates the `Project` model (RON metadata, project directories, type designations: Game or Electronics).
* `src/save_system.rs`: Low-level serialization helper functions.
* `src/i18n.rs`: The custom JSON translation engine. Compiles locale mappings directly into the static executable.
* `locales/en.json` & `locales/es.json`: Static bilingual locale files (Spanish and English).
* `src/command.rs`: Governs the transactional `CommandBus`.
* `src/command_protocol.rs`, `src/capabilities.rs`, `src/ipc.rs`: Shared
  request/response frames, canonical capability catalog, and the project-scoped
  local attach handshake used by the CLI, MCP and editor Agent.
* `src/event.rs`: Universal string-keyed typeless pub/sub event pipeline.
* `src/world_state.rs`: Observer snapshot recording game parameters (time, camera, resources, biome) for AI Directorship.

#### 2. `raf_editor` (Graphical Interface Controls)
* `src/lib.rs`: Re-exports editor layouts and assets.
* `src/attached.rs`: Loopback attach listener. It owns transport/authentication
  only; requests are queued to the native editor host so scene history, session and
  persistence remain editor-authoritative. Attached scene writes return a
  scoped `UndoToken` backed by `SceneHistory`; stale project/session/revision
  tokens are rejected.
* `src/native_application.rs`: Winit lifecycle and native composition root.
* `src/native_editor_runtime.rs`: neutral input, command registry, history,
  frame pacing, dynamic resolution, and canvas orchestration.
* `src/native_editor_commands.rs`: RafUI/menu intent translation into runtime
  scene operations.
* `src/commands/gateway.rs`: Shared editor command boundary used by native UI,
  Agent and attached adapters.
* `src/agent_artifacts.rs`: Project-scoped evidence artifacts produced from
  the last ApiGraphicBasic Game frame; no renderer or scene ownership.
* `src/agent_executor.rs`: Native Agent tool layer and direct typed gateway
  routing, including the visual observation tool.
* `src/agent_service.rs`: Project-scoped native Agent supervision outside redraw,
  optional headless document hosts, and attached/editor execution handoffs.
  Hub startup is lazy. Explicit bulk loading uses one project worker; confirmed
  project closure cancels Agent work and waits for document/history saves before
  releasing hosts and authoring leases. Failures retain the project for retry.
* `src/agent_project_store.rs`: Worker save receipts and recoverable multi-file
  document journals; applied tool results wait for successful publication.
* `src/agent_workspace_surface.rs` and `src/agent_workspace_surface_host.rs`:
  Hub-linked project/conversation/evidence UI and its native input adapter.
* `src/native_agents.rs`: Hub/editor navigation and single-owner document handoffs.
* `src/native_workbench.rs`: Native Agent lifecycle projection, including the
  bounded task snapshots/events exposed to attached CLI and MCP clients.
* `src/native_attached_executor.rs`: CLI/MCP attached command boundary.
* `src/native_surface.rs`: Native retained RafUI placement/input wrapper.
* `src/native_workbench.rs`: Game editor chrome state, lifecycle and retained
  lifecycle coordinator.
* `src/native_workbench_input.rs`: retained-surface input dispatch and intent
  routing for the workbench.
* `src/native_workbench_surface.rs`: retained composition for the shell,
  navigator, inspector, toolbar, dock and status bar.
* `src/native_workbench_electronics.rs`: Electronics navigator and inspector
  plus analysis projections used by the native workbench compositor.
* `src/native_studio.rs`: Native project Hub and create/open flow.
* `src/native_electronics.rs`: Direct CAD canvas adapter over the shared
  ApiGraphicBasic compositor.
* `src/electronics_controller.rs`: Authoritative Electronics/PCB document,
  selection, history, persistence and analysis state.
* `src/electronics_controller_interaction.rs`: native CAD input, gestures,
  placement, routing, editing and persistence mechanics over the live editor.
* `src/native_attached_executor.rs`: Shared CommandGateway adapters for native
  UI, Agent and attached CLI/MCP Electronics commands.
* `src/panels/viewport_controller.rs`: Native central canvas interaction,
  camera/edit state, picking, and renderer-owned gizmo state.
* `src/panels/hierarchy_model.rs`, `hierarchy_surface.rs`: Scene tree model and
  retained hierarchy surface.
* `src/panels/inspector_surface.rs`: Selected-node forms and transform commits.
* `src/panels/editor_bottom_dock_surface.rs`: Bottom-dock chrome, tab strip,
  splitter and context-menu surfaces. Panel content is split into focused
  surfaces, including Console, Assets, Project, Nodes and Agent.
* `src/panels/nodes_surface.rs`: Retained Nodes authoring surface for the
  session-scoped graph.
* `src/panels/nodes_canvas.rs`: Nodes grid, typed-pin geometry and wire paint
  helpers using ordinary RafUI quads.
* `src/panels/nodes_catalog.rs`, `nodes_surface_host.rs`: UI catalog mapping
  and transient query/property/pin interaction state.
* `src/nodes_history.rs`: Bounded authoring-only graph undo/redo history.
* `src/panels/primitive_create.rs`, `search_surface.rs`,
  `viewport_toolbar_surface.rs`: focused retained authoring surfaces.

Retired panel paths are intentionally omitted from this active map; they remain
recoverable through version history and the stabilization archive.

#### 2a. `raf_cli` (External Agent Adapter)
* `src/main.rs`: Headless CLI, JSONL endpoint, MCP stdio adapter, and attached
  mode routing. It never exposes Play, Stop or Runtime.
* `src/attached.rs`: Project-scoped loopback client using the shared IPC
  handshake and command frames.

* `.ai/skills/raf-game-authoring/`: Reusable AI workflow for attached scene and
  scripting authoring. `docs/CLI_MCP_QUICKSTART.md` is the human-facing setup.

#### 3. `raf_ui` (Retained UI Model)
* `src/node.rs`, `src/layout.rs`, `src/style.rs`: Serializable retained nodes,
  CSS-like layout data, semantic classes, theme-aware style rules, and text
  keys.
* `src/docking.rs`: Persistable dock descriptors and workspace policy.
* `src/hit_test.rs`, `src/focus.rs`, `src/events.rs`: Renderer-neutral pointer,
  keyboard, focus, and typed action contracts.
* `src/text.rs`: Bounded text-atlas request metadata. Rasterization and GPU
  uploads stay in ApiGraphicBasic.
* `src/overlays.rs`: Window-coordinate overlay placement with flip/shift
  behavior for menus, popovers, tooltips, drag previews, and modals.
* `src/motion.rs`: Time-based tween/easing primitives shared by GPU and CPU
  retained hosts.
* `src/components.rs`: Semantic recipes for icon buttons, panel headers, tree
  rows, and compact tooltip nodes.
* `src/environment.rs`: Logical/physical density conversion and bounded
  raster-scale policy.
* `docs/RAF_UI.md` and `docs/EDITOR_RAFUI.md`: Active RafUI technical and editor authoring contracts.

#### 4. `raf_render` (Graphics Runtime)
* `src/bridge/render_runtime.rs`: Canonical shared runtime and surface/backend state for Scene, CAD, and renderer-owned surfaces.
  It also exposes a backend-neutral readback of the last rendered scene frame
  for Agent evidence; it does not initiate a second render.
* `src/render_config.rs`: Houses quality configuration structures and GPU capabilities tags.
* `src/ApiGraphicBasic/`: Rafi-owned graphics contract, devices, command lists, resources, Scene/CAD/RafUI surface presentation, WGPU adapter implementation, and CPU recovery. It must not expose WGPU types to upper layers long-term.
  * `handles.rs`: Generational backend-neutral resource and surface handles.
  * `capabilities.rs`: Backend identity, adapter preference, and potato/desktop budgets.
  * `command_list.rs`: Backend-neutral meshes and `DrawLineBatch`; adjacent line
    commands are merged without reordering.
* `src/ApiGraphicBasic/ui_surface/`: Compositors and hosts for retained RafUI draw data. RafUI documents remain backend-neutral.
  * `diagnostics.rs`: Data-only frame inspection for layout, clipping, hit regions, text requests, zero-size nodes, and z-order.
  * `render.rs`: Intrinsic-size aware layout boxes, shared hit regions, and paint input.
  * `compilation.rs`: Separate layout/paint invalidation keyed by controls, focus, raster density, motion, and resolved text.
* `.ai/APIGRAPHICBASIC.md`: Mandatory ApiGraphicBasic ownership and backend-evolution rule.
* `src/projection.rs`: Projects 3D vectors onto the native logical canvas.
* `src/camera.rs`: Matrix calculation for camera orientation, zoom, Orbit, and flyover.
* `src/depth_sort.rs`: Quick-Sort implementation sorting polygons back-to-front (Painter's algorithm).
* `src/post_process.rs`: CPU shaders for bloom, saturation, vignette, tone-mapping, and FXAA.
* `src/picking.rs`: Traces mouse rays against entity bounds for precise item selection.

#### 5. `raf_electronics` (Electronic Core)
* `src/component.rs`: Defines properties, pins, and simulation models (`SimModel`).
* `src/library.rs`: Hardcoded base component models.
* `src/schematic.rs`: Contains schematic state, active wire list, and connection intersections.
* `src/netlist.rs`: Performs Union-Find connectivity analysis to extract node networks.
* `src/drc.rs`: Runs Design Rule Checks (validates disconnected nets, float inputs, overlaps).
* `src/simulation.rs`: Modified Nodal Analysis (MNA), solving DC node voltages.
* `src/pcb/layout.rs`: Tracks footprint offsets, layers, drill holes, and routes airwires.

#### 6. `raf_nodes` (Visual Scripting)
* `src/node.rs`: Struct definition for pins, categories, and persisted authoring properties.
* `src/catalog.rs`: Stable built-in node descriptors and factories shared by Game and Electronics authoring.
* `src/graph.rs`: Session graph, checked connections, and authoring diagnostics.
* `src/compiler.rs`: Current validation-only compiler boundary; it does not execute or emit runtime code.
* `src/runtime_compiler.rs`: Bounded Game graph-to-Rhai compilation for the isolated Play snapshot and source-only authoring checks.
* `src/scene_nodes.rs`: Camera, hierarchy, reference, late-update and event node descriptors shared by the catalog.
* `src/executor.rs`: Prepared runtime infrastructure, outside the active native Play contract.

#### 7. `raf_script` (Scripting Runtime + Host API)
* `src/host_api.rs`: `ScriptContext`, `InputSnapshot`, `AudioCommandQueue`, `TimeInfo`. The single entry point for all script execution.
* `src/node_handle.rs`: `NodeHandle` opaque entity reference. Roblox-style API (`set_position`, `set_color`, `move_by`). `HOST_API_VERSION = 1`.
* `src/value.rs`: `ScriptValue` dynamic type (Bool/Int/Float/String/Vec3/Color/Handle/List).
* `src/backends/rhai_backend.rs`: Tier 1. Rhai engine with full Host API registered via thread-local context.
* `src/backends/wasm_backend.rs`: Tier 2. WASM Native Module loading (stub, Phase D).
* `src/backends/node_backend.rs`: Tier 3. Bridges `raf_nodes` executor to Host API.
* `src/host/`: Operation modules (scene, transform, property, audio, input, time, interop).

#### 8. `raf_hardware` (IoT Systems Interface)
* `src/serial.rs`: Handles serial port interfaces and device connections (ESP32/Arduino).
* `src/ml.rs`: Extracts model metrics for sensor-based neural network tasks.
* `src/robot.rs`: High-level system descriptions tracking actuator telemetry.

---

## 3. Crucial Workflows & Systems

### A. UI Modification Protocol
* **No monolithic editor coordinator**: Keep composition and lifecycle in
  `native_application.rs`, `native_workbench.rs` and their focused hosts. Do
  not reintroduce a retired `app.rs` ownership boundary.
* **Creating a RafUI surface**:
  1. Add a focused `*_surface.rs` document builder under the owning editor
     module or `panels/`.
  2. Add a `*_surface_host.rs` host when the surface needs texture lifecycle,
     input dispatch, session state, or action translation.
  3. Build `UiDocument` from stable IDs, classes, layout constraints, i18n
     keys, and typed event bindings.
  4. Map `UiAction` values to existing command, validation, undo, and
     persistence boundaries. Do not mutate models inside the document builder.
  5. Register the route/host through the native workbench/composition owner
     without embedding raw visual trees in a monolithic coordinator.
* **Native RafUI**: Retained surfaces own new menus, rails, fixed dock chrome,
  and editor overlays; ApiGraphicBasic owns their GPU/CPU composition.
* **Authoring source of truth**: Read `docs/RAF_UI.md`,
  `docs/EDITOR_RAFUI.md`, `docs/APIGRAPHICBASIC.md`, and
  `.ai/STUDIO_GRADE_UI.md` before UI or surface changes. The technical
  documents define menu, canvas, resource, editor, and ownership behavior;
  Studio Grade UI defines visual defaults and quality criteria. A brief under
  `.ulpi/design/` is task-specific context, not a second global authority.

### B. Command Console Loop
* Manual commands typed with `/` in the Console flow through
  `commands::parser::parse_console_input` and
  `native_editor_commands.rs`/the domain gateway.
* Parsed fields are mapped in the command executors. Every mutating command
  must preserve the command/history/persistence boundary rather than mutate a
  document silently.

### C. Agent Retained Rendering Guardrails
* The native Agent transcript is a continuous scroll surface. History I/O stays
  in the runtime and outside the render loop; the retained surface must not
  create a second persistence path.
* `EngineSettings::agent_message_page_size` is legacy persisted compatibility
  data. Its current bounds are 20..64 with a default of 24; it is not a live
  pagination contract.
* When changing session, reset transient scroll/focus/hover/pointer capture
  state. Do not reuse a previous surface's interaction state against a new
  message document.
* Performance validation must inspect `render_needed`, target-size markers,
  layout/paint cache hits, atlas uploads, and GPU present time. Switching tabs
  is not proof that Agent is fixed; it only stops executing the Agent bridge.

### D. Electronics / PCB Boundary
* Schematic and PCB documents are authored and persisted as 2D CAD data. The
  active PCB model lives under `raf_electronics::pcb::layout`; the native
  electronics controller and ApiGraphicBasic CAD surface own presentation and
  interaction. Do not map the current PCB editor into game `SceneGraph`
  primitives as an implementation shortcut.

> Developed by Yoll. More info: [yoll.site](https://yoll.site).
