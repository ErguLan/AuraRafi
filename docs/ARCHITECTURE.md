# AuraRafi Architecture

This document describes the high-level architecture of the AuraRafi engine.

## Overview

AuraRafi is a modular, open-source engine written in Rust, designed for both
video game development and electronic hardware design. The engine is structured
as a Cargo workspace with 9 independent crates plus a main editor binary.

## Workspace Layout

```
AuraRafi/
  editor/             Main binary that launches the editor
  crates/
    raf_core/         Core systems: ECS, scene graph, commands, events, config
    raf_render/       Rendering abstraction via wgpu
    raf_editor/       Visual editor UI built on egui/eframe
    raf_assets/       Asset importing, browsing, and management
    raf_electronics/  Electronic design: schematics, PCBs, simulation, DRC, export
    raf_nodes/        Visual scripting (no-code) node system + executor
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
  -> raf_ai
  -> raf_net
  -> raf_hardware

raf_editor -> raf_core, raf_render, raf_assets, raf_electronics, raf_nodes, raf_ai, raf_net
raf_render -> raf_core
raf_assets -> raf_core
raf_electronics -> raf_core
raf_nodes -> raf_core
raf_ai -> raf_core
raf_net -> raf_core
raf_hardware -> raf_core
```

All crates depend on `raf_core`, which provides the foundational types.
No circular dependencies exist.

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
- `SceneNode`: Position, rotation, scale, parent/children, entity link
- World matrix computation by walking the parent chain
- O(1) node lookup by index

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

### Infrastructure (prepared, not connected)

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

Lightweight rendering via CPU projection + egui painter (no GPU pipeline needed):

- `Renderer`: High-level renderer holding pipeline config
- `RenderPipeline`: Quality-dependent config (shadows, AO, AA, bloom) for future GPU path
- `Camera`: Perspective and orthographic modes with view/projection matrices, orbit controls
- `mesh`: Static vertex data for primitives (edges + face quads with normals)
  - Cube: 12 wireframe edges, 6 face quads
  - Sphere: 3-circle wireframe, UV sphere faces (configurable stacks x slices)
  - Plane: 5 wireframe edges, 1 face quad
  - Cylinder: ring + vertical wireframe edges, side quads + cap fan triangles
- `projection`: 3D-to-2D screen projection, perspective divide, face brightness shading
- `editable`: EditableMesh with selectable vertices/faces, move/scale/extrude/delete ops, per-axis scaling, wireframe+render output
- `gizmo`: Transform gizmo with per-axis handles (X/Y/Z), hit testing, translate/scale/rotate modes
- `lod`: Level of Detail system, 3 distance-based levels, auto-cull, segment helpers
- `AntiAliasing`: None, FXAA, MSAA4x (reserved for future GPU path)
- `backend`: CPU/GPU render backend switch with adaptive frame budget tracking
  - CPU painter: default, zero GPU memory, zero shaders. Runs on any hardware.
  - GPU wgpu: opt-in only, for scenes with 300+ entities. Never auto-switches.
  - `BackendConfig`: gpu_suggest_threshold, cpu_max_triangles, frame_budget_ms
  - `FrameRenderStats`: per-frame triangle count, render time, over-budget detection
  - `BackendConfig::potato()`: absolute minimum resource preset (2000 tris, 30fps budget)

### Render Abstraction Layer (prepared, zero cost when inactive)

Architecture: `SceneGraph -> SceneRenderData -> RenderBackendTrait -> Backend`

- `abstraction`: Core trait `RenderBackendTrait` - all backends implement this (init/render_frame/resize/shutdown)
  - `RenderCapability`: 20+ flags from basic meshes to hardware RT. Query at runtime what the active backend supports
  - `ActiveBackend`: 4 tiers - CpuPainter (default, potato), Wgpu (GPU), SoftwareRT (CPU ray tracing), HardwareRT (RTX)
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

- `schematic_graph`: **Independent** data graph for electronics, completely separated from game SceneGraph
  - `SchematicNode`: component placement with canvas position, rotation, layer, selection
  - `Net`: electrical connection tracking with pin references and computed voltage
  - Own pick_component/pick_wire (click detection), selection, net rebuilding
  - RON save/load, legacy Schematic conversion (backwards compat)
  - Does NOT depend on raf_core::SceneGraph (prevents "Frostbite problem")

## Editor (raf_editor)

Visual editor built on `egui`/`eframe`:

### Application Flow

1. **Loading Screen** - Brief branding splash with progress bar
2. **Project Hub** - Recent projects list + create new (Game or Electronics)
3. **Main Editor** - Full panel layout with viewport, hierarchy, properties

### Panel Layout

- **Top**: Menu bar (File, Edit, View, Project) + Build/Run button + FPS
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

- **Viewport**: 2D/3D hybrid view with filled mesh rendering, flat shading, wireframe toggle (Solid/Wire/Fill), orbit camera, grid, axis gizmo, tool toolbar
- **Hierarchy**: Scene tree with selection and collapsible groups
- **Properties**: Transform editing, RGB color picker with 7 presets, primitive type dropdown, visibility toggle
- **Console**: Log output with severity filters and auto-scroll
- **Asset Browser**: Search, filter by type, grid display
- **Node Editor**: Visual scripting canvas with bezier connections
- **Schematic View**: Electronics component placement and wiring
- **Settings**: Theme, language, quality, editor prefs, Simple Mode toggle, target platform selector
- **AI Chat**: Chat interface structure (not yet functional)

## Assets (raf_assets)

- `AssetImporter`: Copies files into project, detects type by extension
- `AssetBrowser`: Scans directories, filters by type/search
- `Primitive3D`: Editable primitives (Cube, Sphere, Cylinder, Plane)
- Supported types: Image, Model3D, Audio, Scene, Unknown

## Electronics (raf_electronics)

- `ElectronicComponent`: Parts with designator, value, pins, footprint, SimModel
- `SimModel`: Resistor (ohms), Capacitor (farads), LED (forward voltage), Magnet (tesla + polarity), Wire
- `Pin`: Named connection point with direction (Input/Output/Bidirectional/Power/Ground)
- `Schematic`: Components + wires with net names, remove/duplicate helpers
- `ComponentLibrary`: Built-in parts (Resistor, Capacitor, LED, Magnet)
- `Netlist`: Union-find algorithm builds nets from wire endpoints and pin positions (rotation-aware)
- Auto-designator assignment (R1, R2, C1, MAG1, etc.)
- `DrcReport`: 6 rules - floating pins, missing values, isolated components, unnamed nets, short circuit, LED without resistor
- DC simulation engine: Modified Nodal Analysis, Gaussian elimination with partial pivoting, node voltages, branch currents, power dissipation
- Export: SVG vector image (styled, rotation-aware), BOM CSV (grouped with quantities), text netlist
- Gerber export structure for JLCPCB/PCBWay (manufacturer-specific layers defined, placeholder until PCB 3D layout)
- Circuit sharing: RON serialization for shareable compact strings

## Visual Scripting (raf_nodes & raf_editor)

- `Node`: Visual script building block with pins and position
- `NodePin`: Typed connection point (Flow, Bool, Int, Float, String, Vec3)
- `NodeGraph`: Collection of nodes and connections. Multiple flows supported via `Vec<NodeGraph>`.
- `NodeCategory`: Event, Logic, Action, Math, Electronics, Variable
- Built-in nodes: On Start, On Update, Print, If Branch, Loops (For, While), Compare (>, <, ==), Entity manipulation
- **UI Architecture**: `NodeEditorPanel` utilizes explicit grid allocation (`ui.allocate_space`) before rendering nodes to completely bypass "Input Swallowing". Bezier connections use `Rect::contains()` upon pointer release for accurate hit detection.
- **Undo/Redo**: Fully memory-backed history stack capable of holding up to 50 iterations (`history: Vec<(Vec<NodeGraph>, usize)>`), supporting global shortcuts (Ctrl+Z / Ctrl+Y).
- **Executor**: Walks flow chains in topological order, evaluates data pins, handles conditional branching (If node), 10k step safety limit
- `NodeValue`: Runtime value type with coercion (Bool, Int, Float, String, Vec3)
- `ExecutionOutput`: Logs, final pin values, success/error status

## AI Interface (raf_ai)

- `ToolRegistry`: Engine operations exposed as callable tools with JSON schema
- `ToolDefinition`: Name, category, parameters, return type
- `ChatPanel`: Message history, input, provider selection
- `AiProvider`: OpenRouter, OpenAI, GenAI, Claude
- Status: Structure prepared, AI functionality not yet implemented

## Networking (raf_net)

- `NetMessage`: Protocol messages with type, sender, payload
- `NetMessageType`: Connect, Disconnect, StateSync, RPC, Ping, Pong
- Status: Stub for future multiplayer implementation

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
