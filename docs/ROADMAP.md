# AuraRafi Roadmap

This document outlines the development roadmap for AuraRafi.

## Current Status: Active development beyond the initial foundation

The engine foundation is in place and several vertical slices have already moved past pure scaffolding. This section tracks broadly available systems, while later sections separate implemented milestones from planned work.

- [x] Cargo workspace with 9 modular crates (+ raf_hardware)
- [x] ECS (Entity Component System) via hecs
- [x] Scene graph with parent-child hierarchy
- [x] Command bus with undo/redo support
- [x] Event bus for decoupled communication
- [x] Project management (create, load, save)
- [x] Configuration system with RON persistence
- [x] Temporary editor shell with panels (egui/eframe)
- [x] Rust-native retained UI foundation (`raf_ui` + ApiGraphicBasic direct host)
- [x] RafUI authoring contract for surface ownership, menus, themes, docking, and canvas fidelity
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
- [x] Inline tests distributed across workspace crates
- [x] GPU-first editor rendering path with optional CPU recovery path
- [x] Electronics dual-workspace flow: schematic + synchronized PCB 2D canvas

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
- [ ] Keyboard shortcut customization (full user-defined rebinding still pending; experimental shortcut settings/help are present)
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
- [x] Design Rule Check (DRC) - topology checks for floating pins, missing values, isolated components, unnamed nets, short circuits, LED current limiting, dangling wire endpoints, and component-pin bypass shorts
- [x] BOM (Bill of Materials) generation (CSV export with grouping and quantity counting)
- [ ] Gerber file export (JLCPCB/PCBWay) - structure ready, now staged around synced PCB layout rather than a mandatory 3D board path
- [ ] PCB layout view (basic 3D)
- [x] PCB layout view (2D synchronized workspace): electronics projects now include a dedicated PCB canvas with placements, airwires, trace creation, board outline drafting, and `pcb_layout.ron` persistence.
- [ ] Final Gerber layer writer: the export path now validates the synced PCB layout document and outline status, but still needs real copper/mask/silk/drill emission.
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

Prepared rendering goals in this phase also include:

- PBR (Physically Based Rendering) materials
- Point and spot lights
- Shadow mapping
- Ambient occlusion (SSAO)

### Current implementation boundary

- [x] Per-project RenderConfig and GPU-first resource profiles with Potato,
  Low, Medium, and High budgets.
- [x] ApiGraphicBasic surface plans reserve explicit budgets for future
  shadows, post-processing, PBR, particles, and skeletal animation.
- [x] Flat scene/CAD rendering, camera, selection, gizmos, and retained UI
  composition through the shared graphics runtime.
- [ ] PBR, shadows, post-processing, particle systems, skeletal animation,
  advanced lighting, and ray tracing are intentionally not product-enabled
  while the renderer and editor are stabilized.
- [x] 2D game view uses the shared 3D scene renderer with an orthographic
      camera; legacy `Sprite2D` data is loaded as `Plane` (textured assets
      remain future)

## v0.8.0 - Game Runtime (Prepared, Not Product-Active)

The product Play/runtime flow is intentionally guarded while the renderer and
editor surface architecture are stabilized. Current work should be treated as
prepared infrastructure: cloned scene execution, Rhai lifecycle harness, input
snapshot shape, and future service boundaries.

- [ ] Editor-integrated Play/Stop flow for game projects
- [x] Runtime-prep scene cloning so tests do not mutate the edit document
- [ ] Scene loading and runtime initialization from saved project state
- [x] Node graph persistence as `nodes.ron`
- [ ] Runtime execution of saved node graphs through `On Start` and `On Update`
- [x] Keyboard input snapshot shape for future runtime scripts
- [x] External `.rhai` lifecycle harness for prepared runtime tests
- [ ] Script-facing access to `self`, `parent`, entity paths, variables, movement, velocity, and audio triggers
- [ ] Basic collision detection through scene colliders
- [ ] Simple physics (gravity, damping, velocity, trigger-only bodies)
- [ ] Audio playback for entity audio sources inside Play mode
- [x] Properties inspector support for runtime variables, audio sources, rigid bodies, and colliders
- [x] Animation-aware collision structure (raf_core/scene/anim_collider.rs): AnimCollider per bone, 5 response types (Stop/Blend/Slide/Recoil/Ignore), auto-generate for hands/feet, enabled by DEFAULT (marketing differentiator)

Follow-up work after 0.8.0:

- [ ] Separate game runtime binary
- [ ] Fixed-timestep loop decoupled from editor frame timing
- [ ] Animation system (keyframe-based, bone hierarchy)
- [ ] Connect animation collision to playback (check colliders each animation step, trigger response on hit)
- [ ] IK (Inverse Kinematics) for procedural foot placement and hand grabs

## v0.8.5 - Viewport & Renderer Restructure (Done)

Full architectural rewrite of the viewport and render pipeline. The monolithic viewport file was split into a thin editor shell plus a renderer-side bridge layer. The rendering backend was replaced with a proper Z-buffer scanline rasterizer.

### Viewport Architecture
- [x] Viewport shell refactored to ~302 line thin egui panel (`viewport.rs`)
- [x] HUD toolbar with G/R/S/F buttons, 2D/3D toggle, OBJ/VTX mode badge, info pill, axis gizmo (`viewport_hud.rs`)
- [x] Object mode input: gizmo drag, entity picking, shift-select, keyboard shortcuts (`viewport_interaction.rs`)
- [x] Vertex edit mode input: click-to-select vertices, drag-to-move, Tab toggle (`viewport_interaction.rs`)
- [x] Overlay drawing: entity labels, gizmo arrows/rotation rings/scale cubes, vertex dots/edges (`viewport_overlay.rs`)
- [x] HUD click blocking: UI overlays consume clicks before they reach world picking

### Renderer Bridge (`raf_render::bridge`)
- [x] `ViewportBridge`: owns camera state, renderer, edit session, and transform controller — zero egui dependency
- [x] `ViewportTransformController`: gizmo drag lifecycle (translate/rotate/scale) with screen-space axis projection
- [x] `ViewportEditSession`: per-entity editable mesh state, vertex picking (10px), vertex dragging (world→local via inverse rotation)
- [x] Precise entity picking: ray-sphere broad phase + ray-triangle narrow phase (replaces old screen-space distance check)

### Render Pipeline (`raf_render::scene_renderer`)
- [x] Full MVP transform pipeline: model → view → projection → perspective divide → screen coords
- [x] 6-plane frustum culling (discard entities outside camera view before rasterizing)
- [x] Z-buffer scanline rasterizer with per-pixel f32 depth test (replaces old painter's algorithm)
- [x] Flat shading: `brightness = 0.3 + 0.7 × max(0, dot(normal, light))`
- [x] Transparency support: opaques front-to-back (early Z), transparents back-to-front with src-over alpha blend
- [x] Render modes: Solid (fill + wire on selected), Wireframe (edges only), Preview (fill + wire on all)
- [x] RenderOptions: solid_show_surface_edges, solid_xray_mode, solid_face_tonality, selection_outline
- [x] `geometry/` module: indexed `MeshData` with positions + normals + indices, primitive constructors
- [x] `math/` module: `transform.rs` (project_point, screen_to_world_ray), `frustum.rs`, `ray.rs` (ray_sphere, ray_triangle)
- [x] `render_pipeline/` module: `framebuffer.rs` (RGBA + depth, blend_pixel), `rasterizer.rs` (scanline fill + line draw)
- [x] Camera dual-mode: Perspective (fov, orbit) and Orthographic (scale, pan) with `CameraMode` enum
- [x] Framebuffer reuse across frames (no per-frame allocation), mesh caching per primitive type


## v0.9.0 - AI Integration (Agentic Workspace)

The 0.9.0 target establishes the editor IDE as the native "home" for the AI agent. The engine and IDE are built with an agentic design from the ground up: every core action is exposed as a registered tool key (interacting with the `CommandBus`), and the workspace leverages `.ai/` configuration schemas. Rather than just being a runtime companion, the AI behaves as a co-developer capable of editing the scene graph (e.g., generating multi-part models like a ship from primitives), writing/fixing script code (e.g., configuring walking/physics systems), and diagnosing IDE compiler or electrical errors.

### Infrastructure (prepared)
- [x] WorldState snapshot: time, weather, biome, camera, resources, custom data (raf_core/world_state.rs)
- [x] AI Director: observe WorldState, emit DirectorActions (spawn/remove/weather/scale/color/sound/custom) - disabled by default, zero cost
- [x] AI asset generation interface: GeneratedMesh, GeneratedTexture, AssetGenConfig, cache with eviction (raf_ai/asset_gen.rs)
- [x] Mesh streaming provider: MeshChunk with grid coords + LOD, camera-based chunk loading/eviction, vertex budget (raf_ai/mesh_provider.rs)
- [x] DirectorConfig: mode (Disabled/Observer/Active), update interval, action limits, per-action permissions
- [x] AssetGenCache: in-memory with prompt hashing, auto-eviction, 50MB max
- [x] Synaptic workspace foundation: standard tool keys and definitions registered via `ToolRegistry` (`crates/raf_ai/src/tool_registry.rs`) mapping directly to command execution.

### Integration & Agentic Control (done)
- [x] LLM provider connection (OpenRouter, OpenAI, GenAI, Claude, Puerto via OpenAI-compatible `/chat/completions`)
- [x] Tool-calling execution pipeline: `AgentToolExecutor` routes tool calls to command handlers (`game`, `electronics`, `script`, `workspace`)
- [x] Non-blocking runtime: HTTP requests run on a background thread, UI stays responsive via `poll()` state machine
- [x] Tool name sanitization: dots and invalid characters in command names are sanitized for provider compatibility (e.g., `project.info` -> `project_info`)
- [x] Chat persistence: `AgentHistory` saves/loads per-project conversation sessions as RON files
- [x] Chat session management: left sidebar with session list, new/delete/switch sessions
- [x] Agent execution style: AGENT.md uses bounded plans, tool evidence and
  concise summaries without exposing private reasoning
- [x] Improved temperature (0.7) for more natural, expressive responses

### Pending (next)
- [ ] AI Director connected to game loop (reads WorldState, emits actions)
- [ ] Mesh provider connected to viewport (streams chunks into EditableMesh)
- [ ] Asset generator UI in asset browser panel ("Generate with AI" button)
- [ ] AI-assisted entity & asset creation (prompt -> generate multi-primitive prefabs)
- [ ] AI-assisted script writing & configuration (Rhai/C++ code generation)
- [ ] AI-assisted debugging (console error monitoring, auto-fix suggestions)
- [ ] AI-generated textures applied as materials
- [ ] Procedural terrain via mesh provider (algorithmic, no AI needed)

## v0.10.0 - Hardware & IoT

- [ ] Serial port communication (serialport crate)
- [ ] Arduino/ESP32 auto-detection
- [ ] Real-time sensor data visualization
- [ ] Actuator control panel in editor
- [ ] Hardware debugging tools
- [ ] OTA firmware upload preparation

## v0.11.0 - Cloud & Streaming

- [x] ApiGraphicBasic contract foundation: backend-neutral generational handles,
  capabilities, adapter preference, memory budgets, neutral shared context, and
  GPU output encapsulation while WGPU remains the active adapter.
- [x] RafUI retained compilation: one cacheable layout/input frame and one
  shared CPU/GPU paint list, retained GPU buffers, deduplicated image uploads,
  adjacent paint-run batching, and a native application-menu adapter boundary.
- [x] RafUI Frontier Core: window-level overlay placement with flip/shift,
  intrinsic content sizing, time-based hover/focus state, shared motion with
  reduced-motion behavior, semantic component recipes, bounded HiDPI density,
  and shared GPU/CPU diagnostics.
- [x] RafUI tooltip migration: compact tooltip content is authored and
  rendered by a dedicated transparent RafUI surface; the transitional Egui
  bridge only composites its completed texture and never paints tooltip text
  or rectangles.
- [ ] ApiGraphicBasic owned graphics evolution: complete the resource registry,
  eviction, upload enforcement, frame graph, synchronization, and presentation
  lifecycle so backends can be selected per platform without coupling editor
  surfaces to WGPU.
- [ ] ApiGraphicBasic asset ingress: move render-facing asset import,
  decode/transcode policy, GPU residency, thumbnails, and resource lifetime
  behind the engine graphics API. Project files remain portable while the
  renderer owns how each asset reaches the selected GPU backend.
- [ ] Native graphics backend lanes: add direct backend implementations under
  ApiGraphicBasic for the platforms Rafi targets. WGPU remains available as an
  adapter during the build-out, but it is not the permanent owner of Rafi's
  graphics architecture.
- [ ] Headless rendering mode (--headless flag)
- [ ] Low-latency input pipeline for cloud streaming
- [ ] Linux native build (CI target)
- [ ] WebAssembly (WASM) build target
- [ ] Cloud deployment configuration
- [ ] Streaming protocol preparation (WebRTC stub)
- [ ] Additional components (transistors, ICs, connectors)
- [ ] Custom component creation

## v0.12.0 - ML & Robotics

The first attached-editor vertical slice may be pulled forward after the RafUI,
sessions, and scripting stabilization gates. This lets the real game project
drive engine stabilization without activating Play or Runtime. The complete
target remains v0.12 and is specified in
`docs/AGENT_CLI_MCP_EXPANSION.md`.

- [x] Attached editor bridge: first implementation is a local loopback TCP
  endpoint published as `.aura_rafi/agent_endpoint.json`; keep the frame and
  handshake contract transport-neutral so named pipes/Unix sockets can be
  added later, with project identity, short-lived token, capability
  negotiation, stale-revision errors, and explicit disconnect recovery
- [x] Baby attached vertical slice (pull forward for game-first stabilization):
  `raf attach --project PATH status|capabilities|project`, then
  `raf attach --project PATH command game.* --confirm`; queue commands on the
  editor owner thread so scene history, active session and persistence remain
  authoritative
- [x] Unified invocation: internal Agent calls the command kernel directly;
  `raf` CLI and `raf mcp serve` reach the same kernel through the attached
  bridge without depending on Egui or the Console UI
- [x] Attached MCP preset: `raf mcp serve --attach PATH` performs the same
  handshake and exposes status, capabilities, resources and the baby game
  mutation allowlist to Codex, Claude Code and OpenCode
- [ ] Project-scoped external tools: inspect projects, sessions, scenes, assets,
  scripts, diagnostics, revisions, and bounded workspace content
- [ ] Game-first mutation toolpack: entities, hierarchy, batch transforms,
  prefabs, materials, asset ingress, scripting, deterministic terrain, paths,
  vegetation regions, spawn markers, and scene validation
- [x] Transaction safety baseline: dry-run previews, expected revisions,
  idempotency, explicit confirmation, structured scene diffs, and real
  editor-owned attached undo tokens. The token is scoped to project, session,
  and issuing revision; composite transactions, checkpoints, and audit records
  remain v0.12 work.
- [ ] Harness task system: bounded progress events, cancellation, resumable
  tasks, tool-call/time/entity budgets, and recovery after client disconnects
- [x] Evidence contract baseline: attached responses return machine-readable
  JSON/NDJSON, revision, scene diff, verification checks, warnings, and
  transaction/undo metadata. Artifact resources and domain-specific evidence
  remain v0.12 work.
- [x] Human and AI onboarding: `docs/CLI_MCP_QUICKSTART.md` and the project
  skill `.ai/skills/raf-game-authoring/` document attached authoring, scripting,
  safety, and the no-Runtime boundary.
- [ ] Optional closed-editor core host: manually activated `raf host`/MCP mode
  that loads project documents without RafUI, Egui, or a renderer. Keep it
  opt-in, renderer-free, budgeted, project-locked, and shut down after a task;
  do not turn it into a resident service by default.
- [ ] External agent onboarding: lightweight connection recipes for Codex,
  Claude Code, and OpenCode using the same MCP server and capability catalog
- [ ] Retained Agent UX after RafUI stabilization: progress, approvals, diff,
  rollback, evidence viewer, task cancellation, and session-scoped history
- [ ] Game-first skills and toolpacks expand only as the matching engine systems
  become real; never advertise unsupported runtime, PBR, animation, particle,
  physics, or material behavior
- [ ] Evaluate ACP only if AuraRafi later needs to host full external coding
  agents as a visual client; ACP is not required for the initial tool server
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
