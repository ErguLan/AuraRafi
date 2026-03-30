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

### Configuration

- `EngineSettings`: Theme, language, render quality, editor prefs, simple mode, target platform
- Persisted to disk as RON (Rusty Object Notation)
- `RenderQuality` presets: Potato (0), Low (1), Medium (2), High (3)
- `TargetPlatform`: Desktop, Mobile, Web (WASM), Cloud/Streaming, Console
- `simple_mode`: Hides advanced parameters for beginners
- `headless`: Server/cloud mode without window (structural)
- `responsive_layout`: Adapts UI to small screens (structural)
- Language support: English, Spanish

### Project Management

- `Project`: Metadata with UUID, type (Game/Electronics), timestamps
- Directory structure: `assets/`, `scenes/`, `scripts/`
- `RecentProjects`: Tracks last 20 opened projects

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

All rendering runs on CPU through egui's painter. Zero GPU buffers, zero shaders, zero texture memory. Runs on any hardware.

## Scene Addons (raf_core/scene)

- `collider`: AABB (auto-fit from vertices, intersection test, wireframe edges), ConvexHull (directional pruning), MeshCollider (exact geometry)
- `merge`: Combine multiple meshes into one (reduces draw calls), vertex welding (remove duplicates), source tracking for unmerge, MeshGroup for entity grouping

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

## Visual Scripting (raf_nodes)

- `Node`: Visual script building block with pins and position
- `NodePin`: Typed connection point (Flow, Bool, Int, Float, String, Vec3)
- `NodeGraph`: Collection of nodes and connections
- `NodeCategory`: Event, Logic, Action, Math, Electronics, Variable
- Built-in nodes: On Start, On Update, Print, If Branch, Add
- Compiler stub for graph connectivity validation
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
