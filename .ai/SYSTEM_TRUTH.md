# AuraRafi — System Truth (Source of Truth)

This file contains the definitive technical registry of AuraRafi. AI agents must rely on this file as the primary architectural mapping of the codebase.

## 1. Core Architectural Pillars
AuraRafi is a unified sandbox engine. It handles both standard Game ECS Scene Graphs and CAD PCB Design Topologies inside a lightweight structural core.

* **GPU Hardware Driver Priority**: Uses high-performance GPU hardware rendering (via private `wgpu` integration mapped to host `eframe` context) as the primary execution path, automatically fallback-routing to CPU software rasterization only on low-spec potato devices.
* **Unified Event & Mutator Layers**: High-level commands flow through the `CommandBus` to support transaction replays, Undo/Redo historical stacks, and AI Tool Calling.
* **No MSVC Tooling Assumptions**: Always compiled via `stable-x86_64-pc-windows-gnu` using MinGW/MSYS2 toolchains on Windows. All compiled outputs are routed to `target_gnu/`.
* **Canonical Unit System**: 1 world unit = 1 meter (games viewport 3D). 1 schematic unit = 1 millimeter (schematic/PCB canvas). All internal calculations in SI. `DisplayUnit` enum only changes UI display, never computation. Constants in `raf_core::units`.
* **Scripts Never Touch Engine Internals**: All scripting (Rhai, WASM, Visual Nodes) calls the shared Host API in `raf_script::host_api::ScriptContext`. No tier accesses `SceneGraph`, `InputState`, or audio directly. Scripts hold `NodeHandle` (opaque IDs), not references. See `docs/SCRIPTING_SYSTEM.md`.

---

## 2. Definitive Directory Map

### Main Entry Points
* **`editor/`**: Binary wrapping crate.
  * `src/main.rs`: Entry point. Prepares the viewport window, boots `eframe` context, allocates custom icon textures (`icon.png`), and mounts `AuraRafiApp`.

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
* `src/panels/viewport.rs`: Central 2D/3D work canvas, containing camera orbit matrices and gizmo hit testing.
* `src/panels/viewport_grid.rs`: Draws infinite 3D coordinate grids on the canvas.
* `src/panels/viewport_edit.rs`: Vertex/face subquery editing drawing routines.
* `src/panels/schematic_view.rs`: Interactive electrical wiring workspace.
* `src/panels/node_editor.rs`: Logic scripting canvas using custom Bezier curves.
* `src/panels/hierarchy.rs`: Scene Graph tree node inspector.
* `src/panels/properties.rs`: selected node attribute forms (colors, visibility, coordinates).
* `src/panels/asset_browser.rs`: Asset tracking and dynamic filesystem monitor.
* `src/panels/console.rs`: Panel supporting debugging and manual console input line.
* `src/panels/ai_chat.rs`: Multi-provider LLM interface (supports OpenClaw).

#### 3. `raf_render` (Graphics Runtime)
* `src/renderer.rs`: Main interface for GPU `wgpu` pipelines and CPU Painters.
* `src/render_config.rs`: Houses quality configuration structures and GPU capabilities tags.
* `src/ApiGraphicBasic/`: Holds mesh recipe generators, line arrays, and grid parameters.
* `src/projection.rs`: Projects 3D vectors onto the 2D egui coordinate plane.
* `src/camera.rs`: Matrix calculation for camera orientation, zoom, Orbit, and flyover.
* `src/depth_sort.rs`: Quick-Sort implementation sorting polygons back-to-front (Painter's algorithm).
* `src/post_process.rs`: CPU shaders for bloom, saturation, vignette, tone-mapping, and FXAA.
* `src/picking.rs`: Traces mouse rays against entity bounds for precise item selection.

#### 4. `raf_electronics` (Electronic Core)
* `src/component.rs`: Defines properties, pins, and simulation models (`SimModel`).
* `src/library.rs`: Hardcoded base component models.
* `src/schematic.rs`: Contains schematic state, active wire list, and connection intersections.
* `src/netlist.rs`: Performs Union-Find connectivity analysis to extract node networks.
* `src/drc.rs`: Runs Design Rule Checks (validates disconnected nets, float inputs, overlaps).
* `src/simulation.rs`: Modified Nodal Analysis (MNA), solving DC node voltages.
* `src/pcb/layout.rs`: Tracks footprint offsets, layers, drill holes, and routes airwires.

#### 5. `raf_nodes` (Visual Scripting)
* `src/node.rs`: Struct definition for pins and parameters.
* `src/compiler.rs` & `src/executor.rs`: Compiles graphs and runs visual scripting flows.

#### 6. `raf_script` (Scripting Runtime + Host API)
* `src/host_api.rs`: `ScriptContext`, `InputSnapshot`, `AudioCommandQueue`, `TimeInfo`. The single entry point for all script execution.
* `src/node_handle.rs`: `NodeHandle` opaque entity reference. Roblox-style API (`set_position`, `set_color`, `move_by`). `HOST_API_VERSION = 1`.
* `src/value.rs`: `ScriptValue` dynamic type (Bool/Int/Float/String/Vec3/Color/Handle/List).
* `src/backends/rhai_backend.rs`: Tier 1. Rhai engine with full Host API registered via thread-local context.
* `src/backends/wasm_backend.rs`: Tier 2. WASM Native Module loading (stub, Phase D).
* `src/backends/node_backend.rs`: Tier 3. Bridges `raf_nodes` executor to Host API.
* `src/host/`: Operation modules (scene, transform, property, audio, input, time, interop).

#### 7. `raf_hardware` (IoT Systems Interface)
* `src/serial.rs`: Handles serial port interfaces and device connections (ESP32/Arduino).
* `src/ml.rs`: Extracts model metrics for sensor-based neural network tasks.
* `src/robot.rs`: High-level system descriptions tracking actuator telemetry.

---

## 3. Crucial Workflows & Systems

### A. UI Modification Protocol
* **No UI in `app.rs`**: Keep `app.rs` slim. It acts solely as an orchestrator.
* **Creating a New Panel**:
  1. Add source file inside `crates/raf_editor/src/panels/`.
  2. Implement `show(&mut self, ui: &mut egui::Ui)` on a custom struct implementing `Default`.
  3. Register inside `crates/raf_editor/src/panels/mod.rs`.
  4. Mount as a field on `AuraRafiApp` in `src/app.rs`.

### B. Command Console Loop
* Manual commands typed with `/` in the Console flow to `parse_console_input` in `app.rs`.
* Parsed fields are mapped in `execute_console_command`. Every mutating command pushes an undo state copy first, mutates the state, and records the change in `record_immediate_document_change` (zero-latency auto-save when linear saving is enabled).

### C. Game Engine Viewport / PCB Layout Unification
* A PCB is mapped into the `SceneGraph` container directly using its footprint's physical parameters. Primitives (fr4 boards as flat Cubes, IC chips as black Cubes, pads as Cylinders) represent the layout instantly in the 3D viewport, sharing the same render pipeline of standard game assets.
