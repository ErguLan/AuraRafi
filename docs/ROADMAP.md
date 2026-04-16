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
- [x] Render backend switch: CPU painter (default, zero GPU) / GPU wgpu (opt-in), frame budget, adaptive detail, potato preset
- [x] Independent SchematicGraph for electronics: separated from game SceneGraph, own data model with selection/picking/nets/serialization
- [x] Depth-sorted rendering (painter's algorithm): all faces from all entities sorted by depth before drawing -- eliminates Z-fighting/overlap artifacts, O(n log n) per frame
- [x] Transform gizmo arrows: RGB arrows (X red, Y green, Z blue) with arrowhead triangles, axis labels, drag interaction for Move/Scale tools, screen-space hit testing
- [x] Entity picking: click to select in 3D viewport via screen-space projection, click-on-empty deselects, overlay-area exclusion
- [x] Edit mode (Tab toggle): Object/Vertex modes with visual indicator, foundation for vertex-level editing

## v0.3.0 - Editor Polish

- [x] Drag-and-drop asset importing (copies to project assets/, recursive scan, file type classification)
- [x] Asset thumbnail preview generation (type icons per file category: image/3D/audio/script)
- [x] Context menus (right-click) in schematic editor (component/wire/canvas)
- [ ] Keyboard shortcut customization (user-configurable keybinds)
- [x] Multi-entity selection (Shift+Click for multiple, Ctrl+A selects all, click empty to deselect)
- [x] Hierarchy-viewport-properties bidirectional sync: clicking hierarchy updates viewport selection and vice versa, properties panel always shows selected entity
- [x] Entity duplication (Ctrl+D) in schematic editor
- [x] Entity duplication (Ctrl+D) in scene viewport
- [x] Entity deletion (Del key + Edit menu) in scene viewport
- [x] Select All (Ctrl+A) in scene viewport
- [x] Scene serialization to RON files (save/load alongside project)
- [x] Auto-save implementation (timer-based, configurable interval)
- [x] Hot-reload file watcher: polling-based (zero deps), detects changed/new/deleted files, 6 categories, recursive dir scan, status summaries ES/EN, mod + collaborative dev support
- [ ] Responsive layout breakpoints (mobile-friendly editor)
- [x] Undo/Redo (Ctrl+Z / Ctrl+Y) with scene snapshots (max 50 depth)
- [x] Save shortcut (Ctrl+S) with scene RON persistence
- [x] Enhanced menu bar (File/Edit/View/Project/Help with shortcut labels, translated ES/EN)
- [x] Status bar: modified indicator (*), undo/redo depth, last action display
- [x] Help menu with keyboard shortcuts reference

## v0.4.0 - Visual Scripting

- [x] Node graph execution engine (basic interpreter: walks flow chains, evaluates data pins, handles On Start/Print/If/Add)
- [x] Variable get/set nodes
- [x] Loop nodes (For, While)
- [x] Comparison nodes (>, <, ==, !=)
- [x] Entity manipulation nodes (Spawn, Destroy, SetPosition)
- [x] Input event nodes (Key Press, Mouse Click)
- [x] Timer/delay nodes
- [x] Node graph save/load
- [x] Serial Read / Serial Write hardware nodes
- [x] Sensor Input / Actuator Output nodes
- [x] **v0.4.0: Unified i18n System**: Unified JSON-based translation system (`raf_core::i18n::t`) with `en.json` and `es.json` support. Removed hardcoded conditionals from all panels.

## v0.5.0 - Electronics

- [x] Component rotation and mirroring (R key + context menu, rotation-aware pin rendering)
- [x] Automatic net naming (union-find netlist builder assigns N001, N002... or wire labels)
- [x] Design Rule Check (DRC) - 6 rules: floating pins, missing values, isolated components, unnamed nets, short circuit, LED without resistor
- [x] BOM (Bill of Materials) generation (CSV export with grouping and quantity counting)
- [ ] Gerber file export (JLCPCB/PCBWay) - structure ready, needs PCB 3D layout
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

## v0.6.0 - Advanced Scripting & Internationalization

- [ ] C++ Native Scripting API (FFI architecture using cxx/bindgen for peak performance)
- [ ] DLL Hot-Loading (Dynamically load and swap C++ `.dll`/`.so` game files at runtime without restarting the engine)
- [ ] Interop bridge (Exposing SceneNodes and Vectors from Rust directly to C++ without serialization overhead)
- [x] (DONE in v0.4.0) Fluent-based localization system (Replaced by internal JSON i18n system)
- [x] **Verified Unified i18n System**: All UI strings verified using `raf_core::i18n::t()` with `en.json` and `es.json`

## v0.7.0 - Advanced Rendering

### Infrastructure (prepared)
- [x] Render abstraction layer: RenderBackendTrait separates "what" from "how", 4 backend tiers (CpuPainter/Wgpu/SoftwareRT/HardwareRT), 20+ RenderCapability flags
- [x] SceneRenderData bridge: flat GPU-ready mesh arrays, lights (directional/point/spot/area), camera, environment (ambient, fog, sky, HDR exposure)
- [x] PBR material system: metallic/roughness (glTF-compatible), texture slots (albedo/normal/MR/emissive/AO/height), MaterialPhysics (friction/density/destructible/impact sounds), MaterialLibrary
- [x] Spatial partitioning: SpatialGrid (uniform 3D grid, O(1) cell query, small/medium/large presets), Frustum (6-plane, point/sphere culling), SpatialConfig
- [x] Complement Trace: ray tracing designed from day 1 (not patched), 4 modes (Disabled/Software/Hardware/Hybrid), per-feature toggles (shadows/reflections/GI/AO/refractions/caustics), BVH AccelerationStructure
- [x] GPU vertex deformation: 7 deformer types (cloth/hair/vegetation/water/skeletal/blend shape/custom), wind/gravity/stiffness/frequency params, per-vertex GPU overhead estimates
- [x] World streaming: seamless open world (zero loading screens), WorldRegion with biome/LOD/state machine, potato/default/high presets, camera-based region load/unload

PBR (Physically Based Rendering) materials
Point and spot lights
Shadow mapping

Ambient occlusion (SSAO)

### Implementation (v0.7.0 - ALL features disabled by default, potato mode preserved)
- [x] Per-project RenderConfig: 17 opt-in toggles, 4 presets (Potato/Low/Medium/High), zero cost when off
- [x] WGSL shader suite (embedded as string constants, loaded only when use_gpu=true):
  - PBR vertex/fragment (metallic/roughness, Schlick fresnel, Blinn-Phong specular)
  - Flat/unlit shader (gizmos, wireframe, debug)
  - Shadow depth pass (directional light)
  - Bloom extract pass (brightness threshold)
  - FXAA post-process (luma-based edge detection)
- [x] Enhanced CPU lighting: point lights with attenuation, spot lights with cone, specular (Blinn-Phong), configurable max lights
- [x] Fog system: distance-based color blending, configurable start/end/color
- [x] Post-processing suite (CPU painter): bloom glow, vignette, FXAA edge blending, Reinhard tone mapping, saturation control, sRGB conversion
- [x] Texture system: CPU-side loading (BMP native parser, zero deps), UV sampling, LRU cache (50MB budget), auto-downscale
- [x] UV mapping: box/spherical/cylindrical/planar projection, cube UV quad generation
- [x] RenderPreset in EngineSettings (serialized, per-project)
- [ ] 2D sprite rendering (textured quads in 2D mode)

## v0.8.0 - Game Runtime

- [ ] Separate game runtime binary
- [ ] Scene loading and initialization
- [ ] Game loop with fixed timestep
- [ ] Input handling system
- [ ] Basic collision detection
- [ ] Simple physics (gravity, velocity)
- [ ] Audio playback (wav, ogg)
- [ ] Animation system (keyframe-based, bone hierarchy)
- [x] Animation-aware collision structure (raf_core/scene/anim_collider.rs): AnimCollider per bone, 5 response types (Stop/Blend/Slide/Recoil/Ignore), auto-generate for hands/feet, enabled by DEFAULT (marketing differentiator)
- [ ] Connect animation collision to playback (check colliders each animation step, trigger response on hit)
- [ ] IK (Inverse Kinematics) for procedural foot placement and hand grabs

## v0.9.0 - AI Integration

### Infrastructure (prepared)
- [x] WorldState snapshot: time, weather, biome, camera, resources, custom data (raf_core/world_state.rs)
- [x] AI Director: observe WorldState, emit DirectorActions (spawn/remove/weather/scale/color/sound/custom) - disabled by default, zero cost
- [x] AI asset generation interface: GeneratedMesh, GeneratedTexture, AssetGenConfig, cache with eviction (raf_ai/asset_gen.rs)
- [x] Mesh streaming provider: MeshChunk with grid coords + LOD, camera-based chunk loading/eviction, vertex budget (raf_ai/mesh_provider.rs)
- [x] DirectorConfig: mode (Disabled/Observer/Active), update interval, action limits, per-action permissions
- [x] AssetGenCache: in-memory with prompt hashing, auto-eviction, 50MB max

### Integration (pending - requires engine maturity)
- [ ] LLM provider connection (API integration with OpenClaw/OpenRouter/etc.)
- [ ] Tool-calling execution pipeline (AI -> CommandBus)
- [ ] AI Director connected to game loop (reads WorldState, emits actions)
- [ ] Mesh provider connected to viewport (streams chunks into EditableMesh)
- [ ] Asset generator UI in asset browser panel ("Generate with AI" button)
- [ ] AI-assisted entity creation (prompt -> spawn primitive with properties)
- [ ] AI-assisted debugging (analyze console errors, suggest fixes)
- [ ] Chat history persistence
- [ ] AI-generated textures applied as materials
- [ ] Procedural terrain via mesh provider (no AI needed, algorithmic)

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
- [ ] Additional components (transistors, ICs, connectors)
- [ ] Custom component creation

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
- Mod support: detect external scripts via hot reload watcher, load/reload without game restart
- Collaborative dev: multiple devs sharing project folder, hot reload detects external saves automatically
- Accessibility: daltonism-friendly palettes, high contrast mode, UI narrator
