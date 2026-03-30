# AuraRafi Roadmap

This document outlines the development roadmap for AuraRafi.

## Current Status: v0.1.0 (Foundation)

The engine foundation is in place with these working systems:

- [x] Cargo workspace with 9 modular crates (+ raf_hardware)
- [x] ECS (Entity Component System) via hecs
- [x] Scene graph with parent-child hierarchy
- [x] Command bus with undo/redo support
- [x] Event bus for decoupled communication
- [x] Project management (create, load, save)
- [x] Configuration system with RON persistence
- [x] Editor UI with panels (egui/eframe)
- [x] Theme system (dark/light + orange accent)
- [x] Visual node editor with connections
- [x] Schematic editor with component library (Resistor, Capacitor, LED, Magnet)
- [x] Console with log filtering
- [x] Asset browser with type filtering
- [x] AI tool registry structure
- [x] Networking protocol definitions
- [x] Simple Mode toggle (hides advanced parameters)
- [x] Target Platform selector (Desktop, Mobile, Web, Cloud, Console)
- [x] Hardware integration layer structure (raf_hardware)
- [x] Node executor (interprets visual scripts)
- [x] Magnet electronic component with field simulation model
- [x] Circuit sharing (RON serialization)
- [x] JLCPCB/PCBWay Gerber export structure (placeholder)
- [x] Serial port communication protocol
- [x] Sensor / Actuator data models
- [x] Robot control interface structure
- [x] ML training data export structure

## v0.2.0 - Rendering (Done)

Rendering implemented via CPU projection + egui painter (zero GPU pipelines, runs on any hardware):

- [x] Viewport rendering integration (CPU projection through view_proj matrices, no wgpu render passes needed - lighter)
- [x] Shader-free rendering (all shading computed in Rust: face_brightness() with directional light dot product)
- [x] Basic mesh rendering for all primitives: Cube (6 face quads), Sphere (4x6=24 quads UV sphere), Plane (1 quad), Cylinder (8 side + 16 cap quads)
- [x] Flat shading with directional light (Vec3(0.5, 0.8, 0.3)), backface culling via 2D cross product, brightness range 0.3-1.0
- [x] Camera orbit controls: left-drag orbit (yaw/pitch), middle-drag pan, scroll zoom, double-click reset, clamp pitch to +/-80 deg
- [x] 3D grid rendering (projected line segments on XZ plane, 21x21 lines, major every 5 units)
- [x] Wireframe/Solid toggle: 3 modes (Solid+Wire, Wireframe only, Solid only) via top-right buttons + Z key cycle
- [x] Color/material per entity: RGB color picker in Properties panel, 7 quick presets (R/G/B/Y/O/P/W), primitive type dropdown
- [x] 2D/3D mode toggle (top-center buttons, separate rendering paths)
- [x] EditableMesh: runtime vertex/face editing (cube, sphere, plane, cylinder), move/scale/extrude/delete, per-axis scaling
- [x] Transform gizmo data: per-axis handles (X/Y/Z) with hit testing, 3 modes (translate/scale/rotate)
- [x] LOD system: 3 distance-based detail levels with auto-cull and segment helpers
- [x] Collider system: AABB auto-fit, ConvexHull, MeshCollider with intersection tests and wireframe viz
- [x] Mesh merge: combine multiple meshes into one, vertex welding, source tracking for unmerge
- [x] Mesh groups: group entities by ID to move/transform together

## v0.3.0 - Editor Polish

- [ ] Drag-and-drop asset importing
- [ ] Asset thumbnail preview generation
- [x] Context menus (right-click) in schematic editor (component/wire/canvas)
- [ ] Keyboard shortcut customization (user-configurable keybinds)
- [ ] Multi-entity selection (Shift+Click for multiple)
- [x] Entity duplication (Ctrl+D) in schematic editor
- [x] Entity duplication (Ctrl+D) in scene viewport
- [x] Entity deletion (Del key + Edit menu) in scene viewport
- [x] Select All (Ctrl+A) in scene viewport
- [x] Scene serialization to RON files (save/load alongside project)
- [x] Auto-save implementation (timer-based, configurable interval)
- [ ] Hot-reload for assets using file watcher
- [ ] Responsive layout breakpoints (mobile-friendly editor)
- [x] Undo/Redo (Ctrl+Z / Ctrl+Y) with scene snapshots (max 50 depth)
- [x] Save shortcut (Ctrl+S) with scene RON persistence
- [x] Enhanced menu bar (File/Edit/View/Project/Help with shortcut labels, translated ES/EN)
- [x] Status bar: modified indicator (*), undo/redo depth, last action display
- [x] Help menu with keyboard shortcuts reference

## v0.4.0 - Visual Scripting

- [x] Node graph execution engine (basic interpreter: walks flow chains, evaluates data pins, handles On Start/Print/If/Add)
- [ ] Variable get/set nodes
- [ ] Loop nodes (For, While)
- [ ] Comparison nodes (>, <, ==, !=)
- [ ] Entity manipulation nodes (Spawn, Destroy, SetPosition)
- [ ] Input event nodes (Key Press, Mouse Click)
- [ ] Timer/delay nodes
- [ ] Node graph save/load
- [ ] Serial Read / Serial Write hardware nodes
- [ ] Sensor Input / Actuator Output nodes

## v0.5.0 - Electronics

- [x] Component rotation and mirroring (R key + context menu, rotation-aware pin rendering)
- [x] Automatic net naming (union-find netlist builder assigns N001, N002... or wire labels)
- [x] Design Rule Check (DRC) - 6 rules: floating pins, missing values, isolated components, unnamed nets, short circuit, LED without resistor
- [x] BOM (Bill of Materials) generation (CSV export with grouping and quantity counting)
- [ ] Gerber file export (JLCPCB/PCBWay) - structure ready, needs PCB 3D layout
- [ ] Additional components (transistors, ICs, connectors)
- [ ] Custom component creation
- [ ] PCB layout view (basic 3D)
- [x] Magnet component with field simulation (N/S poles, Tesla strength, parse 'Weak'/'Strong'/'Neodymium')
- [ ] Hot-reload of circuit values (live update)
- [x] SVG vector export of schematics (rotation-aware, styled with theme colors)
- [x] Text netlist export (components + nets sections)
- [x] DC simulation engine (Modified Nodal Analysis, Gaussian elimination, node voltages, branch currents, power)
- [x] Wire selection and deletion (hit-test with point-to-segment distance)
- [x] Component drag-and-drop (move after placement)
- [x] Inline value editing (modal window from context menu)
- [x] Circuit sharing (RON serialization/deserialization)

## v0.6.0 - Internationalization

- [ ] Fluent-based localization system
- [ ] Complete English translation file
- [ ] Complete Spanish translation file
- [ ] Community translation infrastructure
- [ ] RTL language support preparation

## v0.7.0 - Advanced Rendering

- [ ] PBR (Physically Based Rendering) materials
- [ ] Point and spot lights
- [ ] Shadow mapping
- [ ] Ambient occlusion (SSAO)
- [ ] Bloom post-processing
- [ ] Anti-aliasing (FXAA, MSAA)
- [ ] 2D sprite rendering
- [ ] Texture loading and UV mapping

## v0.8.0 - Game Runtime

- [ ] Separate game runtime binary
- [ ] Scene loading and initialization
- [ ] Game loop with fixed timestep
- [ ] Input handling system
- [ ] Basic collision detection
- [ ] Simple physics (gravity, velocity)
- [ ] Audio playback (wav, ogg)

## v0.9.0 - AI Integration

- [ ] LLM provider connection (API integration)
- [ ] Tool-calling execution pipeline
- [ ] Scene state context for AI
- [ ] AI-assisted entity creation
- [ ] AI-assisted debugging
- [ ] Chat history persistence

## v0.10.0 - Hardware & IoT

- [ ] Serial port communication (serialport crate)
- [ ] Arduino/ESP32 auto-detection
- [ ] Real-time sensor data visualization
- [ ] Actuator control panel in editor
- [ ] Hardware debugging tools
- [ ] OTA firmware upload preparation

## v0.11.0 - Cloud & Streaming

- [ ] Headless rendering mode (--headless flag)
- [ ] Low-latency input pipeline for cloud streaming
- [ ] Linux native build (CI target)
- [ ] WebAssembly (WASM) build target
- [ ] Cloud deployment configuration
- [ ] Streaming protocol preparation (WebRTC stub)

## v0.12.0 - ML & Robotics

- [ ] Training data export (JSON Lines, CSV)
- [ ] Headless batch simulation for parallel training
- [ ] ONNX Runtime inference bridge
- [ ] Robot control loop with sensor-actuator pipeline
- [ ] Reinforcement learning environment interface
- [ ] 3D projection for robot visualization

## v1.0.0 - Release

- [ ] Stability and performance optimization
- [ ] Complete documentation
- [ ] Example projects (game + electronics)
- [ ] Installer/packaging for Windows, macOS, Linux
- [ ] Website and community resources
- [ ] Contribution guidelines finalized

## Future (Post-1.0)

- Networking / multiplayer support
- Plugin system
- Marketplace for assets and components
- Ray tracing (RTX) rendering path
- VR/AR support
- Mobile target platforms (Android/iOS apps)
- Built-in code editor
- Processor/FPGA design tools
- Circuit simulation (SPICE-like)
- Console SDK integration (Xbox, PlayStation, Switch)
- Gerber direct-order to JLCPCB/PCBWay API
- Circuit sharing via URL (WASM + base64 RON)
