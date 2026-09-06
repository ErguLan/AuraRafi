# AuraRafi — System Truth (Source of Truth)

This file contains the definitive technical registry of AuraRafi. AI agents must rely on this file as the primary architectural mapping of the codebase.

## Current native migration truth (2026-08-24)

The active editor path is Winit + RafUI + ApiGraphicBasic. The retired widget
toolkit is not a runtime dependency or an ownership boundary. Any old panel
paths mentioned below are historical records, not active ownership. The
current presentation boundary is
`native_application.rs`, `native_workbench.rs`, `native_editor_runtime.rs`,
`native_surface.rs`, and `ApiGraphicBasic::editor_compositor`.

## Current decisions absorbed here

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
  `stable-x86_64-pc-windows-gnu` with outputs under `target_gnu/`.

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
* **No MSVC Tooling Assumptions**: Always compiled via `stable-x86_64-pc-windows-gnu` using MinGW/MSYS2 toolchains on Windows. All compiled outputs are routed to `target_gnu/`.
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
* `src/panels/editor_bottom_dock_surface.rs`: Console, Assets, Project and
  Agent bottom-dock surfaces.
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
* `src/node.rs`: Struct definition for pins and parameters.
* `src/compiler.rs` & `src/executor.rs`: Compiles graphs and runs visual scripting flows.

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
