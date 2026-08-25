# AuraRafi — System Truth (Source of Truth)

This file contains the definitive technical registry of AuraRafi. AI agents must rely on this file as the primary architectural mapping of the codebase.

## Current native migration truth (2026-08-20)

The active editor path is Winit + RafUI + ApiGraphicBasic. The retired widget
toolkit is not a runtime dependency or an ownership boundary. Any old panel
paths mentioned below are historical records, not active ownership. The
current presentation boundary is
`native_application.rs`, `native_workbench.rs`, `native_editor_runtime.rs`,
`native_surface.rs`, and `ApiGraphicBasic::editor_compositor`.

## 1. Core Architectural Pillars
AuraRafi is a unified sandbox engine. It handles both standard Game ECS Scene Graphs and CAD PCB Design Topologies inside a lightweight structural core.

* **ApiGraphicBasic Graphics Ownership**: `ApiGraphicBasic` is the sole public graphics owner. WGPU is the current private adapter/compatibility backend while owned DX12, Vulkan, and Metal backends mature behind the same contract. The authoritative migration rule is `.ai/APIGRAPHICBASIC.md`.
* **GPU Hardware Driver Priority**: Uses available GPU hardware as the normal execution path, including integrated GPUs for potato profiles. CPU software rasterization is recovery, headless, testing, or incompatibility; it is not the automatic definition of low-spec mode.
* **Native Graphics Ownership**: Responsibilities move into Rafi-owned ApiGraphicBasic contracts capability by capability. Upper layers never fork into separate WGPU/native versions; WGPU remains private until a validated executor replacement exists.
* **Viewport Integrity Baseline (2026-07-19)**: View2D is an orthographic view of the shared 3D scene. The active scene path batches adjacent lines, bounds persistent mesh reuse, renders physical pixels for high-DPI targets, and uses world-transform scale for culling/focus. `Sprite2D` is a legacy alias to `Plane`, not an active primitive.
* **Unified Event & Mutator Layers**: High-level commands flow through the `CommandBus` to support transaction replays, Undo/Redo historical stacks, and AI Tool Calling.
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
* **No retained tree in `app.rs`**: Keep `app.rs` as route coordinator,
  application-state owner, command/persistence boundary, and autosave owner.
* **Creating a RafUI surface**:
  1. Add a focused `*_surface.rs` document builder under the owning editor
     module or `panels/`.
  2. Add a `*_surface_host.rs` host when the surface needs texture lifecycle,
     input dispatch, session state, or action translation.
  3. Build `UiDocument` from stable IDs, classes, layout constraints, i18n
     keys, and typed event bindings.
  4. Map `UiAction` values to existing command, validation, undo, and
     persistence boundaries. Do not mutate models inside the document builder.
  5. Register the route/host in `app.rs` without embedding raw visual trees.
* **Native RafUI**: Retained surfaces own new menus, rails, fixed dock chrome,
  and editor overlays; ApiGraphicBasic owns their GPU/CPU composition.
* **Authoring source of truth**: Read `docs/RAF_UI.md`,
  `docs/EDITOR_RAFUI.md`, `docs/APIGRAPHICBASIC.md`, and
  `.ulpi/design/DESIGN.md` before UI or surface changes. They define exact
  menu, theme, canvas, resource, editor, and ownership behavior.

### B. Command Console Loop
* Manual commands typed with `/` in the Console flow to `parse_console_input` in `app.rs`.
* Parsed fields are mapped in `execute_console_command`. Every mutating command pushes an undo state copy first, mutates the state, and records the change in `record_immediate_document_change` (zero-latency auto-save when linear saving is enabled).

### C. Agent Retained Rendering Guardrails
* Agent history pagination is a view optimization, not backend loading. `Load
  older messages` and `Back to latest` change the visible page; they must not
  create a second persistence path or move history I/O into the render loop.
* The page size is persisted as `EngineSettings::agent_message_page_size`,
  defaults to 8, and is clamped to 4..32. Keep the current-page bound when
  changing the Agent surface; rendering the full runtime history defeats the
  retained text-atlas budget.
* When changing session or page, reset transient scroll/focus/hover/pointer
  capture state. Do not reuse a previous surface's interaction state against
  a new message document.
* Performance validation must inspect `render_needed`, target-size markers,
  layout/paint cache hits, atlas uploads, and GPU present time. Switching tabs
  is not proof that Agent is fixed; it only stops executing the Agent bridge.

### D. Game Engine Viewport / PCB Layout Unification
* A PCB is mapped into the `SceneGraph` container directly using its footprint's physical parameters. Primitives (fr4 boards as flat Cubes, IC chips as black Cubes, pads as Cylinders) represent the layout instantly in the 3D viewport, sharing the same render pipeline of standard game assets.

> Developed by Yoll. More info: [yoll.site](https://yoll.site).
