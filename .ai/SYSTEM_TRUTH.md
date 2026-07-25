# AuraRafi — System Truth (Source of Truth)

This file contains the definitive technical registry of AuraRafi. AI agents must rely on this file as the primary architectural mapping of the codebase.

## 1. Core Architectural Pillars
AuraRafi is a unified sandbox engine. It handles both standard Game ECS Scene Graphs and CAD PCB Design Topologies inside a lightweight structural core.

* **ApiGraphicBasic Graphics Ownership**: `ApiGraphicBasic` is the sole public graphics owner. WGPU is the current private adapter/compatibility backend while owned DX12, Vulkan, and Metal backends mature behind the same contract. The authoritative migration rule is `.ai/APIGRAPHICBASIC.md`.
* **GPU Hardware Driver Priority**: Uses available GPU hardware as the normal execution path, including integrated GPUs for potato profiles. CPU software rasterization is recovery, headless, testing, or incompatibility; it is not the automatic definition of low-spec mode.
* **Controlled Hybrid Migration**: Responsibilities move from WGPU-facing implementation into Rafi-owned contracts capability by capability. Upper layers never fork into separate WGPU/native versions, and WGPU is removed only after parity and validation gates pass.
* **Viewport Integrity Baseline (2026-07-19)**: View2D is an orthographic view of the shared 3D scene. The active scene path batches adjacent lines, bounds persistent mesh reuse, renders physical pixels for high-DPI targets, and uses world-transform scale for culling/focus. `Sprite2D` is a legacy alias to `Plane`, not an active primitive.
* **Unified Event & Mutator Layers**: High-level commands flow through the `CommandBus` to support transaction replays, Undo/Redo historical stacks, and AI Tool Calling.
* **No MSVC Tooling Assumptions**: Always compiled via `stable-x86_64-pc-windows-gnu` using MinGW/MSYS2 toolchains on Windows. All compiled outputs are routed to `target_gnu/`.
* **Canonical Unit System**: 1 world unit = 1 meter (games viewport 3D). 1 schematic unit = 1 millimeter (schematic/PCB canvas). All internal calculations in SI. `DisplayUnit` enum only changes UI display, never computation. Constants in `raf_core::units`.
* **Scripts Never Touch Engine Internals**: All scripting (Rhai, WASM, Visual Nodes) calls the shared Host API in `raf_script::host_api::ScriptContext`. No tier accesses `SceneGraph`, `InputState`, or audio directly. Scripts hold `NodeHandle` (opaque IDs), not references. See `docs/SCRIPTING_SYSTEM.md`.

---

## 2. Definitive Directory Map

### Main Entry Points
* **`editor/`**: Binary wrapping crate.
  * `src/main.rs`: Entry point. Prepares the desktop window and transitional
    `eframe` context, allocates custom icon textures (`icon.png`), and mounts
    `AuraRafiApp`. RafUI native-host work remains behind the same application
    boundary.

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
* `src/event.rs`: Universal string-keyed typeless pub/sub event pipeline.
* `src/world_state.rs`: Observer snapshot recording game parameters (time, camera, resources, biome) for AI Directorship.

#### 2. `raf_editor` (Graphical Interface Controls)
* `src/lib.rs`: Re-exports editor layouts and assets.
* `src/app.rs`: central state router sheet; handles loading gates, Hub menus, project setup views, status telemetry, auto-saves, and global window close traps.
* `src/theme.rs`: Solid color tokens and warm orange accent configurations (`#D4771A`).
* `src/ui_icons.rs`: Load-budgeted asynchronous icon atlas dispatcher.
* `src/script_support.rs`: Code checking, hot-reload notifications, and routing scripts to external IDEs.
* `src/panels/viewport.rs`: Transitional egui-hosted central canvas shell. Renderer ownership, camera/edit state, frame caching, and presentation must continue moving behind renderer-side hosts rather than growing this panel.
* `src/panels/viewport_grid.rs`: Draws infinite 3D coordinate grids on the canvas.
* `src/panels/viewport_edit.rs`: Vertex/face subquery editing drawing routines.
* `src/panels/schematic_view.rs`: Interactive electrical wiring workspace.
* `src/panels/node_editor.rs`: Logic scripting canvas using custom Bezier curves.
* `src/panels/hierarchy.rs`: Scene Graph tree node inspector.
* `src/panels/properties.rs`: selected node attribute forms (colors, visibility, coordinates).
* `src/panels/asset_browser.rs`: Asset tracking and dynamic filesystem monitor.
* `src/panels/console.rs`: Panel supporting debugging and manual console input line.
* `src/panels/ai_chat.rs`: Multi-provider LLM interface (supports OpenClaw).

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
* `docs/RAF_UI_AUTHORING.md`: Required practical authoring contract.

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
* `.ai/APIGRAPHICBASIC.md`: Mandatory controlled-hybrid and native-backend migration rule.
* `src/projection.rs`: Projects 3D vectors onto the 2D egui coordinate plane.
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
* **Transitional Egui**: Existing `show(&mut self, ui: &mut egui::Ui)` bodies
  are temporary adapters. New menus, rails, fixed dock chrome, and renderer
  canvas ownership must use RafUI/ApiGraphicBasic contracts instead.
* **Authoring source of truth**: Read `docs/RAF_UI_AUTHORING.md`,
  `docs/APIGRAPHICBASIC.md`, and `.ulpi/design/DESIGN.md` before UI or surface
  changes. They define exact menu, theme, canvas, resource, and ownership
  behavior.

### B. Command Console Loop
* Manual commands typed with `/` in the Console flow to `parse_console_input` in `app.rs`.
* Parsed fields are mapped in `execute_console_command`. Every mutating command pushes an undo state copy first, mutates the state, and records the change in `record_immediate_document_change` (zero-latency auto-save when linear saving is enabled).

### C. Game Engine Viewport / PCB Layout Unification
* A PCB is mapped into the `SceneGraph` container directly using its footprint's physical parameters. Primitives (fr4 boards as flat Cubes, IC chips as black Cubes, pads as Cylinders) represent the layout instantly in the 3D viewport, sharing the same render pipeline of standard game assets.
