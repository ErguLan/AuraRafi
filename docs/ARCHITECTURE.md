# AuraRafi Architecture

This document describes the high-level architecture of the AuraRafi engine.

## Overview

AuraRafi is a modular, open-source engine written in Rust, designed for both
video game development and electronic hardware design. The engine is structured
as a Cargo workspace with 9 independent crates plus a main editor binary.

The repository contains a mix of already-active editor workflows and systems
that are intentionally prepared for later runtime integration. This document
tries to distinguish those states instead of flattening everything into
"implemented" or "placeholder".

## Sessions And User UI

Projects can contain multiple sessions without becoming multiple projects. The
registry is `sessions/index.ron`; a session owns scene, node graph, UI document,
schematic and PCB document paths under `sessions/<uuid>/`. Assets and scripts
remain shared at the project root. Existing projects retain legacy root paths
until they are saved through the session flow.

Each game session also persists an `editor_camera.ron` resource. It stores the
editor camera block outside `SceneGraph`, so navigation state and bookmarks are
session-specific without creating an invisible or hardcoded hierarchy node.

Commands and Agent tools can list, create, open, duplicate and remove sessions.
Opening a session saves dirty work first, restores the selected documents and
clears undo history at the session boundary. Removing a non-active session
removes its registry entry but retains files for recovery.

Game UI is stored as an empty `raf_ui::UiDocument`, not as camera children. A
camera can opt into a document reference only when the author selects Camera
space.

RafUI documents, their session state, and their platform hosts have separate
ownership. A document declares layout, styles, semantic text keys, and typed
actions; a surface host maps those actions to existing backend operations; the
application routes typed intents and owns persistence. The precise authoring
contract, including menus and responsive layout, is maintained in
[`RAF_UI.md`](RAF_UI.md) and [`EDITOR_RAFUI.md`](EDITOR_RAFUI.md).

### RafUI Frontier Core Ownership

The retained UI core is split by responsibility instead of growing one
bridge-shaped file:

| Responsibility | Owner |
| --- | --- |
| Semantic document tree and component recipes | `crates/raf_ui/src/node.rs`, `components.rs` |
| Axis sizing and responsive layout data | `crates/raf_ui/src/layout.rs` |
| Overlay placement and edge flipping | `crates/raf_ui/src/overlays.rs` |
| Pointer, focus, hover intent, and typed actions | `crates/raf_ui/src/interaction.rs`, `focus.rs` |
| Time-based transitions | `crates/raf_ui/src/motion.rs` |
| Logical/physical density contract | `crates/raf_ui/src/environment.rs` |
| Layout, text measurement, and shared paint payload | `crates/raf_render/src/ApiGraphicBasic/ui_surface/` |
| Native surface placement and input boundary | `crates/raf_editor/src/native_surface.rs`, `native_workbench.rs`, `native_studio.rs` |
| Tooltip document recipe | RafUI surface recipes and the native compositor |

An overlay is logically owned by its source surface but rendered in a global
window layer. This is the required boundary for tooltips, menus, popovers,
drag previews, and future modals. Owner clipping and overlay clipping are
different contracts and must not be conflated.

Electronics applies the same native boundary to its CAD viewport: RafUI owns
toolbar, navigator, inspector, docks, tooltips, and interaction affordances;
ApiGraphicBasic owns grid, wires, pins, component composition, selection, and
the minimap inside the effective canvas rectangle. Electronics does not use an
Egui bridge or a host overlay. Its DRC and simulation tasks run outside the UI
thread and report observable running, completed, cancelled, and failed states.

`UiSizeMode::FitContent` is resolved after semantic text localization and
atlas synchronization. The first layout may reserve a safe bound, but the
final draw list uses measured text dimensions. This keeps translated labels
from being solved with per-panel magic widths.

`UiSurfaceDiagnostics` is the data-only inspection boundary for layout boxes,
hit regions, clipping, zero-size nodes, text requests, and z-order. It is
available to GPU and CPU hosts and is suitable for golden layout tests.

## Workspace Layout

```
AuraRafi/
  editor/             Main binary that launches the editor
  crates/
    raf_core/         Core systems: ECS, scene graph, commands, events, config
    raf_ui/           Rust-native retained UI model, docking, style, events
    raf_render/       ApiGraphicBasic runtime, WGPU adapter, CPU recovery, and native-backend direction
    raf_editor/       Native Winit + retained RafUI editor shell
    raf_assets/       Asset importing, browsing, JSON primitive manifests
    raf_electronics/  Electronic design: schematics, PCBs, simulation, DRC, export
    raf_nodes/        Visual scripting (no-code) node system + executor
    raf_script/       Scripting runtime + Host API (Rhai + WASM + Node backends)
    raf_ai/           AI agent interface and tool registry
    raf_net/          Networking protocol stubs for future multiplayer
    raf_hardware/     Hardware integration: serial, sensors, actuators, robot, ML
  docs/               Documentation
```

## Crate Dependency Graph

```
editor (binary)
  -> raf_core
  -> raf_render
  -> raf_editor
  -> raf_assets
  -> raf_electronics
  -> raf_nodes
  -> raf_script
  -> raf_ai
  -> raf_net
  -> raf_hardware

raf_editor -> raf_core, raf_render, raf_assets, raf_electronics, raf_nodes, raf_script, raf_ai, raf_net
raf_render -> raf_core, raf_ui
raf_ui -> serde, serde_json, ab_glyph, epaint_default_fonts (text atlas only)
raf_assets -> raf_core
raf_electronics -> raf_core
raf_nodes -> raf_core
raf_script -> raf_core, raf_nodes
raf_ai -> raf_core
raf_net -> raf_core
raf_hardware -> raf_core
```

All crates depend on `raf_core`, which provides the foundational types.
No circular dependencies exist.

## Unit System

AuraRafi uses a single canonical scale across the entire engine. All internal calculations are done in SI units (meters, seconds, kilograms). The user-facing display unit is configurable but never affects computation.

### Canonical Scale

| Surface | Unit | Notes |
|---|---|---|
| Viewport 3D (games) | 1 world unit = 1 meter | position, scale, grid_spacing, orbit_distance |
| Schematic canvas (electronics) | 1 schematic unit = 1 millimeter | component placement, wire endpoints, board outline |
| PCB canvas | 1 schematic unit = 1 millimeter | inherits from schematic |
| Schematic -> 3D conversion | multiply by 0.001 | mm to meters via `SCHEMATIC_TO_WORLD` |

### Constants (`crates/raf_core/src/units.rs`)

- `METERS_PER_UNIT = 1.0`: canonical scale factor for world space.
- `MM_PER_SCHEMATIC_UNIT = 1.0`: canonical scale factor for the schematic canvas.
- `SCHEMATIC_TO_WORLD = 0.001`: conversion from schematic (mm) to world (m).
- `DEFAULT_GRID_SPACING_M = 1.0`: default 3D viewport grid spacing.
- `DEFAULT_GRID_SPACING_MM = 1.0`: default schematic grid spacing.
- `SCHEMATIC_SNAP_OPTIONS_MM = [0.5, 1.0, 2.54, 5.0]`: snap presets for the schematic grid.
- `DEFAULT_TRACE_WIDTH_MM = 0.25`: JLCPCB minimum trace width.
- `DEFAULT_PAD_SPACING_MM = 2.54`: standard DIP through-hole spacing.

### Display Unit (`DisplayUnit` enum)

- `Metric`: shows meters, square meters, cubic meters.
- `Imperial`: shows feet, square feet, cubic feet (converted from internal meters).
- `Game`: shows raw world units without suffix.

The display unit is stored in `EngineSettings.display_unit` and only controls UI presentation. Calculations in the engine and in future scripting runtimes (Rust + C++ via FFI) always use SI meters.

### Scale Conventions

- Primitives: a Cube at scale 1.0 spans -0.5 to +0.5 = 1m on each side.
- Human character: ~1.8 units tall.
- Camera orbit distance: 8.0m default, 0.5m to 200m range.
- PCB board: 100x100mm = 0.1m x 0.1m in world space.
- Resistor footprint: 2mm x 1mm (0805 SMD) = 0.002m x 0.001m in world space.

### Scripting Hook

Future Rust and C++ scripting runtimes must import `raf_core::units` constants so that scripts operate in SI units without manual conversion. The `units.rs` module is public and FFI-friendly.

## Scripting System (raf_script)

Three scripting tiers share one Host API. See `docs/SCRIPTING_SYSTEM.md`
for the full architecture and roadmap.

- **Tier 1 (Rhai)**: sandboxed, pure Rust, beginner-friendly. Primary language.
- **Tier 2 (WASM Native Module)**: C++/Rust/Zig compiled to `.wasm`. Sandboxed
  via WASM runtime. The AuraRafi Host ABI is our own spec. Phase D.
- **Tier 3 (Visual Nodes)**: no-code node graphs from `raf_nodes`. Interpreted
  by the existing executor, wired to the Host API via `node_backend.rs`.

All tiers call `ScriptContext` functions. No tier touches `SceneGraph`
directly. Scripts hold `NodeHandle` values (opaque IDs), not references.

The `raf_script` crate provides:
- `ScriptContext`: per-frame context (scene, input, audio, time)
- `NodeHandle`: opaque entity reference with Roblox-style methods
- `ScriptValue`: dynamic value type crossing the script boundary
- `backends/rhai_backend.rs`: Rhai engine with full Host API registration
- `backends/wasm_backend.rs`: WASM module loading (stub, Phase D)
- `backends/node_backend.rs`: Visual node executor wired to Host API
- `runtime.rs`: lightweight Rhai runtime session for attached scripts on a
  cloned runtime scene

Console commands `/script.create`, `/script.attach`, `/script.list`,
`/script.validate`, `/script.run`, `/script.compile_nodes` are in
`crates/raf_editor/src/commands/script.rs`.

`raf_editor::game_runtime` remains the editor facade. It keeps the editable
scene separate from the runtime clone, converts window input into
`InputSnapshot`, and delegates Rhai lifecycle execution to `raf_script`.

Settings: `EngineSettings.script_runtime_enabled`,
`ProjectSettings.enable_scripting`, `allowed_script_languages`,
`script_execution_mode`.

## Core Systems (raf_core)

### Entity Component System (ECS)

Built on `hecs` for data-oriented, cache-friendly entity management.

- `GameWorld`: Thin wrapper around `hecs::World` with convenience methods
- `TransformComponent`: Position, rotation (Euler), scale via `glam::Vec3`
- `NameComponent`: Human-readable label for entities
- `EntityId`: Stable UUID for serialization
- `VisibleComponent`: Visibility toggle

### Scene Graph

Flat-array scene graph with parent-child hierarchy.

- `SceneGraph`: Contiguous `Vec<SceneNode>` for cache-friendly iteration
- `SceneNode`: Position, rotation, scale, parent/children, entity link, attached scripts, custom variables, audio source, collider, and rigid body state
- World matrix computation by walking the parent chain
- O(1) node lookup by index

The scene document now carries both editor-facing transform data and lightweight runtime-facing data so Play mode can clone and simulate a project without inventing a second scene format.

### Electronics CAD Scene

`raf_electronics::cad_scene` derives a retained, pickable CAD scene from
existing schematic and PCB data. It emits objects for components, pins, wires,
traces, pads, airwires, net labels, board outlines, and DRC markers without
expanding the component library. The intent is to move electronics rendering
and selection toward the same canvas-first model as the scene viewport.

`raf_render::ApiGraphicBasic::cad_surface` records `CadScene` into the shared
command-list renderer, keeping electronics on the same GPU-first and
CPU-fallback path as the viewport.

### Command Bus

Every state-modifying operation flows through the command bus:

- Serializable `Command` struct with name, category, params (JSON)
- Undo/redo stack with configurable history limit (default 1000)
- Pending command queue flushed each frame
- Designed for AI tool-calling: agents emit commands through the same bus

### Event Bus

Lightweight pub/sub system with type-erased events:

- String-keyed channels for decoupled communication
- Published events are drained once per frame
- Supports any `Send + Sync` payload type

### Configuration & Localization (i18n)

- `EngineSettings`: Theme, language, render quality, editor prefs, simple mode, target platform
- Persisted to disk as RON (Rusty Object Notation)
- `RenderQuality` presets: Potato (0), Low (1), Medium (2), High (3)
- `TargetPlatform`: Desktop, Mobile, Web (WASM), Cloud/Streaming, Console
- `simple_mode`: Hides advanced parameters for beginners
- `headless`: Server/cloud mode without window (structural)
- `responsive_layout`: Adapts UI to small screens (structural)
- **Localization (i18n)**: UI components load strings from integrated JSON dictonaries (`en.json`, `es.json`) via `raf_core::i18n::t()`.
- Language support: English, Spanish

### Project Management

- `Project`: Metadata with UUID, type (Game/Electronics), timestamps
- Directory structure: `assets/`, `scenes/`, `scripts/`
- `RecentProjects`: Tracks last 20 opened projects

### Hot Reload (raf_core/hot_reload.rs)

- Polling-based file watcher, zero external dependencies
- Checks file modification timestamps every N seconds (default 2s)
- 6 categories: Scene, Schematic, Config, Script, Asset, Project
- `HotReloadState`: tracks watched files, polls for changes, evicts pending changes
- `scan_directory()`: recursive scan of project folder, skips hidden/target dirs, respects max file limit
- Detects: file modified, file created, file deleted
- `auto_reload` off by default: notifies user before reloading (no surprises)
- Status summaries in ES/EN for status bar display
- Future use cases: mod support (script changes), collaborative dev (shared folder saves)

### World State (for AI observation)

- `WorldState`: Lightweight snapshot of game world (time, weather, biome, camera, resources, custom data)
- Readable by any AI system (Director, mesh provider, agents)
- Not connected to game loop yet - just the data structure prepared
- No runtime cost when not read

## AI (raf_ai)

### Infrastructure (prepared, not fully connected)

- `director`: AI Director that observes WorldState and emits DirectorActions
  - Actions: SpawnEntity, RemoveEntity, SetWeather, SetTime, ScaleEntity, SetEntityColor, LogMessage, PlaySound, Custom
  - Modes: Disabled (zero cost), Observer (suggestions only), Active (modifies world)
  - Configurable: update interval, max actions per cycle, per-action permissions
- `asset_gen`: AI-generated meshes/textures/terrain
  - GeneratedMesh: vertex/index data from AI, ready for EditableMesh
  - AssetGenCache: in-memory with prompt hashing, auto-eviction, 50MB max
  - Config: disabled by default, 500 max polygons, 256px textures
- `mesh_provider`: Streaming mesh data from AI or procedural sources
  - MeshChunk: incremental mesh with grid coords, LOD level, memory tracking
  - Camera-based chunk loading/eviction with vertex budget (50k max)
  - Provider types: None, Procedural, LocalModel, CloudApi

## Rendering (raf_render)

Shared graphics runtime for editor surfaces, with GPU hardware execution first when available and built-in CPU software fallback via ApiGraphicBasic.

The renderer direction is canvas-first: scene, schematic, PCB, and the studio
UI surface use `RenderRuntime -> BasicDevice`. The active editor is a native
Winit application whose retained RafUI surfaces are composed by
ApiGraphicBasic; the retired widget host is not an ownership boundary.

### Canonical Active Graphics Path

Today the active editor surfaces follow one canonical path:

`SceneViewport | SchematicCanvas | PcbCanvas -> RenderRuntime -> BasicDevice -> GPU hardware if available -> CPU software fallback otherwise`

Notes:

- The scene viewport builds its scene frame through `viewport_bridge.rs` and `scene_renderer.rs` before delegating execution to `RenderRuntime`.
- The schematic and PCB canvases feed the same shared graphics runtime, so all three surfaces now live under one graphics-device policy and one fallback contract.
- `raf_ui` provides the retained UI data model for chrome, docking, floating panels, events, palette, i18n text keys, and the application-menu command tree. `ApiGraphicBasic::ui_surface` compiles that data into one cacheable `UiSurfaceDrawList` consumed by the native compositor; it does not build a second scene `BasicCommandList` for UI.
- `SelectionIdBuffer` defines the pixel-perfect picking contract for future GPU readback and CPU-neutral selection tests, with layer/priority policy controlled by `PickingPolicy`.
- `RenderBackendTrait`, `scene_data`, `world_stream`, ray tracing, and other advanced rendering modules remain prepared infrastructure rather than the primary active path today.

### ApiGraphicBasic Native Ownership Direction

`ApiGraphicBasic` is the permanent graphics owner. WGPU is the current private
GPU adapter and compatibility implementation below it. The migration does not
create WGPU and native versions of the viewport, CAD, RafUI, scenes, or assets.
Those consumers keep one Rafi-owned contract while backend implementations
change underneath.

The migration is capability-based rather than a numbered renderer rewrite.
Over long-lived updates, ApiGraphicBasic must progressively own public handles,
adapter capabilities, device/queue/surface lifecycle, persistent resources,
uploads, memory budgets, command encoding, pipelines, synchronization, frame
graphs, asset residency, diagnostics, and device recovery. WGPU may execute any
capability that has not yet moved behind a complete owned contract.

The engine can evolve through several private backend states:

- **WGPU-backed ownership**: ApiGraphicBasic owns the public direction while
  its private WGPU executor performs the active GPU path.
- **Encapsulated WGPU**: no upper layer imports WGPU; it exists only as a
  compatibility backend.
- **Native backend qualification**: a native platform backend reaches parity
  behind the same ApiGraphicBasic contract.
- **Native selection**: a validated native backend becomes the selected
  executor for its platform; upper layers remain unchanged.
- **WGPU retired**: WGPU leaves the shipping dependency graph only after
  parity, recovery, memory, pacing, idle, and hardware gates pass.

One backend is selected per device/surface execution path. Cross-API resource
mixing inside a frame is forbidden unless an explicit and measured interop
contract is designed. The complete rule and removal gates live in
[ApiGraphicBasic Native Ownership Rule](../.ai/APIGRAPHICBASIC.md).

#### Foundation 1 (implemented 2026-07-18)

The first ownership foundation is active while WGPU still executes GPU work:

- `ApiGraphicBasic` owns generational resource handles instead of exposing
  native resource pointers as the default vocabulary.
- `BasicDevice` carries backend-neutral capabilities, adapter preference, and
  explicit potato/desktop memory budgets.
- `RenderRuntimeSnapshot` reports backend identity and the active contract data.
- The host context is named `SharedGraphicsContext`; the old WGPU name remains
  only as a compatibility alias.
- `SceneFrameOutput` wraps its GPU view in `GpuTextureView` for the native
  ApiGraphicBasic compositor. No legacy texture bridge is required.

The resource registry/eviction and adjacent structural batching foundations are
active; complete DeviceHub unification, frame graph, and native
DX12/Vulkan/Metal implementations remain future capabilities. This does not
change the single ApiGraphicBasic ownership path.

### Scene Viewport Rendering Path (v0.9.0)

The viewport is a 3D scene view in both modes: View2D selects an orthographic
camera rather than a separate sprite renderer. `Primitive::Sprite2D` is retired
and legacy data is read as `Primitive::Plane`; interface chrome and overlays are
owned by RafUI.

The scene viewport path was restructured into three clean layers:

**Layer 1 — Editor Shell** (`raf_editor::panels::viewport`)
- `native_application.rs`: Winit lifecycle, event routing, persistence, and attached CLI/MCP command dispatch.
- `native_workbench.rs`: Retained workbench state and lifecycle coordinator.
- `native_workbench_input.rs`: Native input dispatch and semantic intent routing.
- `native_workbench_surface.rs`: Retained composition for the application bar,
  hierarchy, inspector, viewport toolbar, assets, and status dock.
- `native_editor_runtime.rs`: Neutral input router, command registry, scene history, frame pacing, dynamic resolution, and canvas orchestration.
- `editor_layout.rs`: DPI-aware layout contract shared by panel placement and viewport input rectangles.

**Layer 2 — Bridge** (`raf_render::bridge`)
- `viewport_bridge.rs`: Owns camera state (orbit yaw/pitch/distance, 2D offset/zoom), `SceneRenderer`, `ViewportEditSession`, and `ViewportTransformController`. Exposes `handle_camera_input()`, `render()`, `pick_entity()`, and transform/edit drag APIs.
- `input_handler.rs`: Per-entity `EditableMesh` state via `ViewportEditSession`. Vertex picking (10px screen threshold), vertex dragging (world delta → inverse rotation → local space move), precise entity picking (ray-sphere broad phase + ray-triangle narrow phase), and renderer-neutral overlay data.
- `transform_controller.rs`: `ViewportTransformController` with gizmo state and drag lifecycle (translate/rotate/scale). Projects mouse delta onto active axis in screen space, scales to world units via orbit distance.

**Layer 3 — Pixel Production** (`raf_render::scene_renderer`)
- `scene_renderer.rs`: Builds the backend-neutral scene frame and `BasicCommandList` after culling, job collection, sorting, budgeting, and overlay recording. `BasicDevice` executes the active GPU path; the software rasterizer remains the CPU recovery path.
- `geometry/`: `MeshData` (indexed triangle mesh with positions + normals + indices), primitive constructors (cube, cylinder, sphere, plane).
- `math/`: `transform.rs` (MVP, project_point, screen_to_world_ray), `frustum.rs` (6-plane culling), `ray.rs` (Ray struct, ray_sphere, ray_triangle).
- `render_pipeline/`: `framebuffer.rs` (RGBA + f32 depth buffer, blend_pixel for alpha compositing), `rasterizer.rs` (scanline fill, line draw, blended variant).

**Render modes**: `RenderMode::Solid` (filled, wireframe on selected), `RenderMode::Wireframe` (edges only), `RenderMode::Preview` (filled + wireframe on all). Additional per-frame options via `RenderOptions`: `solid_show_surface_edges`, `solid_xray_mode`, `solid_face_tonality`, `selection_outline`, `selection_outline_color`.

**Transparency**: opaques rendered front-to-back (early Z), transparents back-to-front with `blend_pixel()` src-over alpha compositing. Two separate rasterization paths (opaque vs blended) to avoid branching on the hot path.

### Legacy Modules (preserved for backward compatibility)

- `camera`: Perspective and orthographic modes with view/projection matrices, `CameraMode` enum
- `mesh`: Static vertex data for primitives (edges + face quads with normals)
- `projection`: 3D-to-2D screen projection, perspective divide, face brightness shading
- `editable`: EditableMesh with selectable vertices/faces, move/scale/extrude/delete ops, per-axis scaling, wireframe+render output
- `gizmo`: Transform gizmo data model with per-axis handles (X/Y/Z), hit testing, translate/scale/rotate modes
- `lod`: Level of Detail system, 3 distance-based levels, auto-cull, segment helpers
- `backend`: CPU/GPU render backend switch with adaptive frame budget tracking (potato preset: 2000 tris, 30fps budget)
- `depth_sort`: Painter's algorithm for legacy rendering path (superseded by Z-buffer in new pipeline)
- `picking`: Screen-space entity picking and transform gizmo geometry (arrows, rotation rings, arrowheads, hit testing)


### Render Abstraction Layer (prepared, zero cost when inactive unless opted into)

Architecture: `SceneGraph -> SceneRenderData -> RenderBackendTrait -> Backend`

- `abstraction`: Core trait `RenderBackendTrait` - all backends implement this (init/render_frame/resize/shutdown)
- `ActiveBackend`: legacy/prepared backend categorization. It must not be treated as the canonical active backend contract until consolidated under ApiGraphicBasic.
- `ApiGraphicBasic`: Rafi-owned graphics contract for devices, command lists, resources, surfaces, Scene/CAD/RafUI presentation, and CPU recovery. WGPU is its current private GPU adapter, not its permanent public identity. Available GPU hardware remains the normal path, including integrated GPUs under potato budgets.
- `scene_data`: Bridge between SceneGraph and render backend
  - `SceneRenderData`: complete frame package (meshes, lights, camera, environment, stats)
  - `RenderMesh`: flat GPU-ready arrays (positions, normals, UVs, indices), shadow/instance flags
  - `RenderLight`: directional (sun), point, spot, area. Shadow resolution per light
  - `RenderEnvironment`: ambient, fog, sky gradient, HDR exposure
- `material`: PBR metallic/roughness (glTF-compatible)
  - `MaterialTextures`: 6 slots (albedo, normal, metallic-roughness, emissive, AO, height)
  - `MaterialPhysics`: friction, restitution, density, destructibility, impact sound (9 types)
  - Factory methods: `color()`, `metal()`, `glass()`, `emissive()`
- `spatial`: Spatial partitioning for efficient culling
  - `SpatialGrid`: uniform 3D grid, O(1) cell queries, presets (small 512 cells / medium 4096 / large 32768)
  - `Frustum`: 6-plane view frustum, point and sphere containment tests
- `complements/complement_trace`: Ray tracing designed from day 1
  - `RayTraceConfig`: 4 modes (Disabled/Software/Hardware/Hybrid)
  - `RayTraceFeatures`: 6 toggleable features (shadows, reflections, GI, AO, refractions, caustics)
  - `AccelerationStructure`: BVH tree for O(log n) ray-triangle intersection
- `gpu_deform`: GPU vertex deformation (all runs on GPU, CPU only sends params)
  - 7 types: Cloth, Hair, Vegetation, Water, Skeletal, BlendShape, Custom
  - `GpuDeformer`: wind, gravity, stiffness, damping, frequency, amplitude
  - Per-vertex GPU overhead estimates per deformer type
- `world_stream`: Seamless open world (zero loading screens)
  - `WorldRegion`: grid position, biome (7 types), LOD level, load state machine
  - `WorldStreamConfig` presets: potato (9 regions, 32MB), default (49 regions, 128MB), high (121 regions, 512MB)
  - Camera-based region loading/unloading with triangle and memory budgets

## Scene Addons (raf_core/scene)

- `collider`: AABB (auto-fit from vertices, intersection test, wireframe edges), ConvexHull (directional pruning), MeshCollider (exact geometry)
- `merge`: Combine multiple meshes into one (reduces draw calls), vertex welding (remove duplicates), source tracking for unmerge, MeshGroup for entity grouping
- `anim_collider`: Animation-aware collision (prepared, requires animation system)
  - `AnimCollider`: sphere collider attached to bone, check_point/check_sphere tests
  - `AnimCollisionResponse`: Stop, BlendToContact, Slide, Recoil, Ignore
  - `AnimCollisionConfig`: enabled by default (marketing differentiator), auto-generate for hands/feet, layer masks
  - Prevents animation clipping without manual configuration per-animation

## Electronics (raf_electronics)

- `schematic`: authoritative logical document with components, pins and wires.
- `cad_scene`: renderer-neutral retained scene for Schematic and PCB, including
  component hit bounds, pin markers, wires, board outline and structured DRC markers.
- `pcb`: physical layout, footprints, pads, traces, airwires and outline.
- `drc`, `simulation`, `netlist`, `export`: analysis and derived electrical
  outputs.
- `Schematic` is the sole logical Electronics document authority. The former
  `schematic_graph` experiment is removed from the source tree and is not a
  recovery path.

## Editor (raf_editor)

Visual editor with a native Winit shell and retained RafUI surfaces:

### Application Flow

1. **Loading Screen** - Brief branding splash with progress bar
2. **Project Hub** - Active RafUI surface for recent projects, search, filters,
   create, settings, open, duplicate, and forget actions
3. **Main Editor** - Full panel layout with viewport, hierarchy, properties

### Panel Layout

- **Top**: Native RafUI menu bar (File, Edit, View, Project, Help) plus
  project context, command search, settings, and native window controls. Play
  and Run are intentionally not exposed while runtime truth is being stabilized.
- **Left**: Hierarchy panel (scene tree with collapsible nodes)
- **Right**: Properties panel (transform, color/material, primitive type, visibility)
- **Center**: Viewport (scene view) or Schematic view (electronics)
- **Bottom (tabbed)**: Console, Assets, Node Editor, AI Chat
- **Bottom bar**: Status (project name, entity count, language, theme)

### Theme System

- Dark and Light themes with a warm orange accent (#D4771A)
- Highly rounded widget borders for modern appearance
- Consistent color tokens across all panels

### Panels

- **Viewport**: Modular orthographic-2D/perspective-3D native editor shell delegating to `raf_render::bridge`. GPU-first rendering through ApiGraphicBasic with CPU recovery, shared CPU/GPU line batching, bounded mesh reuse, projected grid, entity labels, transform gizmos (G/R/T), vertex edit mode, and RafUI-compatible overlays. Composition lives in `native_application.rs`, `native_workbench.rs`, `native_editor_runtime.rs`, and `panels/viewport_controller.rs`.
- **Hierarchy**: Scene tree with selection and collapsible groups
- **Properties**: Transform editing, RGB color picker with 7 presets, primitive type dropdown, visibility toggle
- **Console**: Log output with severity filters and auto-scroll
- **Asset Browser**: Search, filter by type, grid display
- **Node Editor**: Visual scripting canvas with bezier connections
- **Schematic View**: Electronics component placement and wiring
- **Settings**: Theme, language, quality, editor prefs, Simple Mode toggle, target platform selector
- **AI Chat**: Native RafUI Agent surface with sessions, model/mode controls,
  approvals, continuous transcript scrolling, and the existing
  provider/runtime boundary. The surface is active; requests require a
  configured verified transport.

## Assets (raf_assets)

- `AssetImporter`: Copies files into project, detects type by extension
- `AssetBrowser`: Scans directories, filters by type/search
- `Primitive3D`: Editable primitives (Cube, Sphere, Cylinder, Plane). Game 2D uses a Plane plus an orthographic camera; there is no active Sprite2D primitive.
- Supported types: Image, Model3D, Audio, Scene, Unknown

## Electronics (raf_electronics)

- `ElectronicComponent`: Parts with designator, value, pins, footprint, SimModel
- `SimModel`: Resistor (ohms), Capacitor (farads), LED (forward voltage), Magnet (tesla + polarity), Wire
- `Pin`: Named connection point with direction (Input/Output/Bidirectional/Power/Ground)
- `Schematic`: Components + wires with net names, remove/duplicate helpers
- `ComponentLibrary`: Built-in parts plus external `.ron` assets and registered
  Rust extension templates
- `Netlist`: Union-find algorithm builds nets from wire endpoints and pin positions (rotation-aware)
- Auto-designator assignment (R1, R2, C1, MAG1, etc.)
- `DrcReport`: 8 built-in checks - floating pins, missing values, isolated components, unnamed nets, short circuits, LED current limiting, dangling wire endpoints, and component-pin bypass shorts
- DC simulation engine: Modified Nodal Analysis, Gaussian elimination with partial pivoting, node voltages, branch currents, power dissipation
- Export: SVG vector image (styled, rotation-aware), BOM CSV (grouped with quantities), text netlist
- Gerber export structure for JLCPCB/PCBWay (manufacturer-specific layers
  defined; the final writer remains incomplete and targets the 2D PCB layout)
- Circuit sharing: RON serialization for shareable compact strings
- **Schematic document split**: Electronics projects now persist their editor document as `schematic.ron`; `scene.ron` remains only for game projects.
- **Schematic editor modularization**: The active workflow is split between
  `electronics_controller.rs`, `electronics_controller_interaction.rs`,
  `native_electronics.rs`,
  `native_workbench.rs`, `native_workbench_surface.rs`,
  `native_workbench_input.rs`, `native_workbench_electronics.rs`,
  `native_workbench_settings.rs`, `native_workbench_helpers.rs` and the
  retained `panels/electronics_*_surface.rs` modules. The former widget panels
  are no longer active files.
- **Native Electronics assets**: Schematic component artwork is loaded from the native PNG catalog in `editor/assets/electronics/library/` and presented by the RafUI canvas overlay. CAD geometry keeps hit regions, pins and wires without procedural component-symbol strokes.
- **Electronics viewport boundary**: `EditorFrameLayout::electronics_canvas()`
  is the only CAD target for the Electronics grid, wires and native artwork.
  The RafUI overlay uses local canvas coordinates, `UiOverflow::Clip` and the
  same physical target rectangle; toolbar, panels, docks and modals remain
  outside that surface. Games keeps its existing canvas policy.
- **Open-source extension hooks**: `raf_electronics::extensions` now exposes a lightweight registry for source mods to inject additional `ComponentTemplate`s and custom DRC/ERC rules without patching the built-in library or hard-coded rule list.

## Visual Scripting (raf_nodes & raf_editor)

- `Node`: Visual script building block with pins and position
- `NodePin`: Typed connection point (Flow, Bool, Int, Float, String, Vec3)
- `NodeGraph`: Collection of nodes and connections. Multiple flows supported via `Vec<NodeGraph>`.
- `NodeCategory`: Event, Logic, Action, Math, Electronics, Variable
- Built-in nodes: On Start, On Update, Print, If Branch, Loops (For, While), Compare (>, <, ==), Entity manipulation
- **UI Architecture**: The active node surface is
  `panels/nodes_surface.rs`, composed through the native workbench and RafUI.
  Node interaction must remain in the retained surface and its typed action
  boundary; the retired `NodeEditorPanel`/widget allocation recipe is not an
  active implementation target.
- **Undo/Redo**: Fully memory-backed history stack capable of holding up to 50 iterations (`history: Vec<(Vec<NodeGraph>, usize)>`), supporting global shortcuts (Ctrl+Z / Ctrl+Y).
- **Executor**: Walks flow chains in topological order, evaluates data pins, handles conditional branching (If node), 10k step safety limit
- `NodeValue`: Runtime value type with coercion (Bool, Int, Float, String, Vec3)
- `ExecutionOutput`: Logs, final pin values, success/error status

## AI Interface (raf_ai)

- `ToolRegistry`: Engine operations exposed as callable tools with JSON schema
- `ToolDefinition`: Name, category, parameters, return type
- `AgentSurfaceHost` + `AgentPanel`: retained message history, input,
  approvals and provider selection
- `AiProvider`: the model registry can deserialize OpenRouter, OpenAI, GenAI,
  Claude and legacy gateway values; the native editor currently exposes and
  verifies OpenRouter and OpenAI-compatible transports.
- Status: the native Agent surface, runtime and shared command path are wired;
  a configured verified provider is still required for live requests.

## Networking (raf_net)

- `NetMessage`: Protocol messages with type, sender, payload
- `NetMessageType`: Connect, Disconnect, StateSync, RPC, Ping, Pong
- Status: Protocol/data-model stage only, no gameplay networking integration yet

## Hardware Integration (raf_hardware)

- `SerialPort`: Connection state machine, message inbox/outbox, JSON lines protocol
- `SerialConfig`: Port name, baud rate, data bits, stop bits, timeout
- `SerialMessage`: Typed messages with key/value pairs and direction
- `SensorData`: 13 sensor types (temperature, humidity, distance, light, voltage, current, accelerometer, gyroscope, magnetic field, pressure, analog, digital, custom) with multi-axis support
- `ActuatorCommand`: 9 actuator types (DC motor, servo, stepper, relay, LED, buzzer, PWM, digital out, custom)
- `RobotState`: Unified sensor+actuator state snapshot with mode (Manual/Autonomous/ML/Calibration), exportable as ML training data
- `TrainingConfig`: Parallel headless instances, JSON Lines / CSV export formats
- `InferenceConfig`: Model path, input/output tensor shapes (structural placeholder)
- Status: Data models and protocols defined, actual serial I/O pending (serialport crate)

## Design Principles

1. **Performance First**: Optimize for low-end hardware, scale up gracefully
2. **Modular Architecture**: Independent crates with clean interfaces
3. **Command-Driven**: All mutations through the command bus for undo/redo/replay
4. **Data-Oriented**: ECS for cache-friendly, allocation-light entity management
5. **Serialization**: RON for config, JSON for commands, serde throughout
6. **No External Runtime**: Pure Rust, no C++ dependencies
7. **AI-Ready**: Every operation exposed as a tool for AI agent integration

## Recent Schematic UX Stabilization

- `native_application.rs` and `native_workbench.rs` switch the retained
  left/right surfaces by project type: scene projects keep hierarchy/properties,
  while Electronics projects use the native navigator and property inspector.
- The center panel now treats the schematic editor as a first-class workspace instead of a scene-editor variant, including proper modified-state tracking after canvas interactions.
- Electronics project load/save flow was aligned with project type: opening an electronics project restores `schematic.ron`, and saving writes the same file back through the editor document helpers.
- Localized schematic UI labels were expanded in `raf_core/locales/en.json` and `crates/raf_core/locales/es.json` so the new hierarchy, properties, hover hints, and status summaries resolve through the same i18n layer as the rest of the editor.
- The status bar and global actions (`delete`, `duplicate`, `select all`) now branch correctly for schematic mode instead of assuming the scene graph is always active.

## Extension Direction For Electronics Mods

- Game-style complements remain the top-level entry point for open-source extensions, but electronics-specific contributions no longer have to be welded into the editor or the core crate.
- A complement or source module can now register extra electrical parts into `ComponentLibrary` and extra validation logic into the DRC pass through `raf_electronics::register_component_template(...)` and `raf_electronics::register_drc_rule(...)`.
- This keeps the core editor stable while still allowing community additions such as custom sensors, proprietary footprints, educational rule packs, or project-specific electrical theories to be layered on top.
- The same pattern can be reused later for math/theory packs in other crates without turning `raf_core` into a dumping ground for domain-specific plugin logic.

## PCB 2D Workspace Foundation

- Electronics projects now have a second first-class editor workspace besides the schematic: `PCB View`.
- The schematic remains the logical/electrical source of truth; the PCB is a synchronized physical document stored as `pcb_layout.ron`.
- `raf_electronics::pcb` now owns the physical board model:
  - `footprint.rs`: built-in footprint definitions and generic fallback pad generation.
  - `layout.rs`: `PcbLayout`, `BoardOutline`, placed components, traces, airwires, and schematic-to-PCB sync.
- `raf_editor` now owns the PCB UX layer through
  `electronics_controller.rs`, `native_electronics.rs` and the retained
  `panels/electronics_*_surface.rs` modules. `pcb_document.rs` owns load/save
  for `pcb_layout.ron`.
- Save flow for electronics projects now persists two coordinated documents:
  - `schematic.ron`: logical circuit and simulation source.
  - `pcb_layout.ron`: board outline, placements, traces, and unresolved airwires.
- `Ctrl+S` / save now synchronizes PCB from the current schematic before writing the PCB document, preserving manual placement data while refreshing nets and missing/new components.
- PCB routing is intentionally 2D-first. The current base supports board outline validation, physical footprint geometry, trace storage, and airwire regeneration without depending on any 3D board view.
- The Gerber layer export path is still incomplete, but the placeholder now targets the PCB layout document instead of a hypothetical future 3D-only dependency.

## AI Co-Developer Integration & Context Routing (v0.9.5+)

To minimize token consumption, eradicate agent regressions, and avoid the generation of conflicting or outdated implementation structures, AuraRafi's development context is modularly partitioned under the `.ai/` directory.

### Core Context Redirection
* **Agent Master Context**: [Agent.md](../Agent.md) in the root now acts as a dedicated synapse router, redirecting all incoming LLM agents to the structured `.ai/` workspace.
* **Master System Truth**: [.ai/SYSTEM_TRUTH.md](../.ai/SYSTEM_TRUTH.md) — The single compiler-grade registry housing the up-to-date architecture mapping of all engine crates, files, and modules.
* **Strict Quality Directives**: [.ai/instructions.md](../.ai/instructions.md) — Universal non-negotiable rules enforcing English-only codebases, no emojis, strict i18n JSON translations, complete-before-test flows, and modular native hosts.

### Specialized AI Roles
Agents are categorized into four specialized roles to prevent token swelling and ensure maximum engineering focus:
1. **CTO Lead (Systems & Core)**: [.ai/roles/cto_lead.md](../.ai/roles/cto_lead.md) — Focuses on ECS (`hecs`), commands bus, memory safety, and thread-pool allocations.
2. **Render Math (Graphics Programmer)**: [.ai/roles/render_math.md](../.ai/roles/render_math.md) — Focuses on perspective/orthographic coordinate matrices, grids, ray picking, and PBR/CPU rasterization shaders.
3. **CAD Electronics (Hardware Engineer)**: [.ai/roles/electronics.md](../.ai/roles/electronics.md) — Focuses on routing, Union-Find netlists, DRC tests, DC Modified Nodal Analysis (MNA), and 2D-first PCB authoring.
4. **Editor RafUI contract**: [`EDITOR_RAFUI.md`](EDITOR_RAFUI.md) — Focuses on visual alignment, responsive tab groups, interaction, and styling boundaries.

### Vision Triage Protocol
To streamline development from screenshots, if an agent is presented with an application viewport frame or build output screenshot with no text context, it applies the standard **Vision Triage Protocol**:
1. Identify active workspace coordinates, panels and layouts.
2. Search terminal windows/console logs inside the image for compile warnings or errors.
3. Assess depth-sorting overlapping lines or connection traces.
4. Update the internally stored triage skill according to the findings to suggest high-impact structural fixes immediately.

> Developed by Yoll. More info: [yoll.site](https://yoll.site).
