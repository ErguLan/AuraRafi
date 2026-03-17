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

## v0.2.0 - Rendering (Planned)

The next milestone focuses on actual GPU rendering:

- [ ] wgpu render pass integration in editor viewport
- [ ] WGSL vertex and fragment shaders
- [ ] Basic mesh rendering (primitives: cube, sphere, plane, cylinder)
- [ ] Flat shading with single directional light
- [ ] Camera orbit controls in 3D viewport
- [ ] Grid rendering on GPU (replacing CPU painter)
- [ ] Wireframe mode toggle
- [ ] Color/material assignment per entity

## v0.3.0 - Editor Polish

- [ ] Drag-and-drop asset importing
- [ ] Asset thumbnail preview generation
- [x] Context menus (right-click) in schematic editor (component/wire/canvas)
- [ ] Keyboard shortcut customization
- [ ] Multi-entity selection
- [x] Entity duplication (Ctrl+D) in schematic editor
- [ ] Scene serialization to RON files
- [ ] Auto-save implementation
- [ ] Hot-reload for assets using file watcher
- [ ] Responsive layout breakpoints (mobile-friendly editor)

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
