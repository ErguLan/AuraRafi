# AuraRafi — Agent Context

You are working on **AuraRafi**, an open-source engine for both **video games** and **electronics projects** (schematics, PCB, simulation). It's built primarily in Rust with egui/eframe for the UI and wgpu for rendering.

## Project Structure

```
ProyectRaf/
├── editor/                  # Binary crate — launches the editor window
│   └── src/main.rs          # Entry point, icon loading, window config
├── crates/
│   ├── raf_core/            # Core types: Project, SceneGraph, Config, Events, Commands
│   ├── raf_editor/          # Editor UI (the big one)
│   │   └── src/
│   │       ├── app.rs       # Main app: loading screen, project hub, editor layout
│   │       ├── theme.rs     # Color tokens + theme application (dark/light)
│   │       ├── lib.rs       # Crate entry, re-exports AuraRafiApp
│   │       └── panels/      # All editor panels:
│   │           ├── viewport.rs       # 2D scene viewport (needs 3D rendering)
│   │           ├── node_editor.rs    # Visual scripting with connected nodes
│   │           ├── schematic_view.rs # Electronics schematic editor
│   │           ├── hierarchy.rs      # Scene tree
│   │           ├── properties.rs     # Selected entity properties
│   │           ├── console.rs        # Log output
│   │           ├── asset_browser.rs  # Asset management (basic)
│   │           ├── ai_chat.rs        # AI assistant panel (placeholder)
│   │           └── settings_panel.rs # Settings UI
│   ├── raf_render/          # Rendering (wgpu setup exists, actual rendering NOT implemented)
│   ├── raf_assets/          # Asset pipeline (structure only)
│   ├── raf_electronics/     # Electronic components, schematic, component library
│   ├── raf_nodes/           # Node graph system for visual scripting + executor
│   ├── raf_ai/              # AI integration (placeholder)
│   ├── raf_net/             # Networking (placeholder)
│   └── raf_hardware/        # Hardware: serial, sensors, actuators, robot, ML
├── docs/                    # ARCHITECTURE.md, CONTRIBUTING.md, GETTING_STARTED.md, ROADMAP.md
├── .cargo/config.toml       # GNU target config (no MSVC needed)
├── SETUP.md                 # How to build and run from source
├── CHANGELOG.md
└── README.md
```

## Tech Stack

- **Language**: Rust (edition 2021)
- **UI Framework**: egui 0.30 + eframe 0.30 (immediate mode GUI)
- **GPU**: wgpu 23 (Vulkan/DX12/Metal backend)
- **Math**: glam 0.29
- **Serialization**: serde + ron (human-readable config files)
- **Build target**: `x86_64-pc-windows-gnu` (no Visual Studio needed, uses MinGW)

## Current State (v0.1.0)

### What Works
- Full editor UI with loading screen, project hub, and main editor layout
- Project creation/loading/saving (ron serialization)
- Dark and light themes with warm orange accent (`#D4771A`)
- Node editor with bezier connections, drag-to-connect, palette
- Schematic editor with component library, wire drawing, electrical tests
- 2D/3D hybrid viewport: filled mesh rendering for all primitives (cube/sphere/plane/cylinder), flat shading with directional light, backface culling
- Wireframe/Solid toggle: 3 render styles (Solid+Wire, Wireframe, Solid Only) with Z key cycle
- Camera orbit controls (left-drag orbit, middle-drag pan, scroll zoom, double-click reset)
- 3D grid, axis gizmo, 2D/3D mode toggle (clickable buttons)
- Properties panel: transform, RGB color picker with 7 presets, primitive type dropdown, visibility
- Console panel with log filtering
- Hierarchy panel for scene tree
- Spanish and English UI translations (full - all panels)
- Custom window icon (metallic R logo)
- Subtle Yoll branding only on loading screen
- Simple Mode toggle (hides advanced UI)
- Target platform selector (Desktop/Mobile/Web/Cloud/Console)
- Magnet electronic component with field simulation
- Node graph executor (basic interpreter)
- DC simulation engine (Modified Nodal Analysis)
- DRC with 6 rules, full export suite (SVG, BOM CSV, text netlist)
- Serial port communication protocol (structure)
- Sensor/Actuator/Robot/ML data models (structure)
- JLCPCB/PCBWay Gerber export structure (placeholder)
- Circuit sharing via RON serialization
- EditableMesh: runtime vertex/face editing, select/move/scale/extrude/delete vertices and faces, per-axis scale
- LOD system: 3-level distance-based detail, auto cull, segment helpers
- Transform gizmo: per-axis handles (X/Y/Z) with hit testing, translate/scale/rotate modes
- Collider system: AABB (auto-fit), ConvexHull, MeshCollider with intersection tests
- Mesh merge: combine meshes into one for fewer draw calls, vertex welding, source tracking for unmerge
- Mesh groups: group entities by ID to move/transform together
- **v0.3.0: Undo/Redo** (Ctrl+Z / Ctrl+Y) scene snapshots, max 50 depth
- **v0.3.0: Delete entity** (Del key + Edit menu), duplicate (Ctrl+D), select all (Ctrl+A)
- **v0.3.0: Save** (Ctrl+S) with scene RON persistence + auto-save timer
- **v0.3.0: Scene serialization** to RON files (save/load alongside project)
- **v0.3.0: Enhanced menus** (File/Edit/View/Project/Help, translated, shortcut labels)
- **v0.3.0: Status bar** with modified indicator (*), undo/redo depth, last action
- Render backend switch (CPU/GPU): CPU painter default (zero GPU), GPU wgpu opt-in, frame budget tracking, adaptive detail reduction, potato preset
- Independent SchematicGraph for electronics: separated from game SceneGraph, own selection/picking/nets/serialization, legacy format detection with user warnings ES/EN
- WorldState snapshot for AI: time, weather, biome, camera, resources, custom data (prepared, not connected to game loop)
- AI Director: DirectorActions (spawn/remove/weather/scale/color/sound), Disabled/Observer/Active modes, zero cost when off
- AI asset generation interface: GeneratedMesh, GeneratedTexture, cache with eviction, low-poly defaults (prepared, no AI model connected)
- Mesh streaming provider: MeshChunk with grid coords + LOD, camera-based chunk loading/eviction, vertex budget (prepared, not connected)
- **Hot Reload**: polling-based file watcher (zero dependencies), detects changed/new/deleted files, categories (Scene/Schematic/Config/Script/Asset), directory scanner, status bar summaries ES/EN. Supports hot reload, mod detection, collaborative dev via shared files. Enabled by default, auto-reload off (notifies first).
- Animation-aware collision structure: AnimCollider per bone (sphere), 5 responses (Stop/Blend/Slide/Recoil/Ignore), auto-generate for hands/feet, enabled by DEFAULT. Requires animation system to connect (prepared, not active).
- **Render Abstraction Layer** (prepared, zero cost):
  - `abstraction.rs`: RenderBackendTrait (CPU/wgpu/RT backends share one interface), RenderCapability enum (20+ caps from basic meshes to hardware RT), ActiveBackend selector (CpuPainter/Wgpu/SoftwareRT/HardwareRT)
  - `scene_data.rs`: SceneRenderData (flat GPU-ready mesh arrays, lights, camera, environment), RenderMesh (positions/normals/UVs/indices, shadow flags, instancing), RenderLight (directional/point/spot/area), RenderEnvironment (ambient, fog, sky gradient, HDR exposure)
  - `material.rs`: PBR materials (metallic/roughness glTF-compatible), MaterialTextures (albedo/normal/MR/emissive/AO/height slots), MaterialPhysics (friction/restitution/density/destructible/impact sound), MaterialLibrary, factory methods (color/metal/glass/emissive)
  - `spatial.rs`: SpatialGrid (uniform 3D grid, insert/query_radius/query_aabb, small/medium/large presets), Frustum (6-plane, point/sphere culling), SpatialConfig
  - `complements/complement_trace.rs`: Complement Trace (ray tracing designed from day 1), Ray/RayHit, RayTraceConfig (4 modes: Disabled/Software/Hardware/Hybrid), RayTraceFeatures (shadows/reflections/GI/AO/refractions/caustics toggleable), BVH AccelerationStructure
  - `gpu_deform.rs`: GPU vertex deformation (cloth/hair/vegetation/water/skeletal/blend shape), GpuDeformer (wind, gravity, stiffness, damping), factory methods, per-vertex GPU overhead estimates
  - `world_stream.rs`: seamless open world streaming, WorldRegion (grid, biome, LOD, state machine), WorldStreamConfig (potato/default/high presets), camera-based load/unload decisions

### What Does NOT Work Yet (Priority Order)
1. **Edit Mode UI** -- EditableMesh data exists but the viewport edit mode (Tab toggle, vertex rendering, drag handles) is not wired up yet.
2. **Node Execution (partial)** -- Basic executor exists but does not yet run from the editor UI play button. Missing: variable nodes, loops, entity manipulation.
3. **Asset Pipeline** -- No importing, no thumbnail generation, no hot-reload. The asset browser panel is a shell.
4. **AI Integration** -- `raf_ai` and the chat panel are placeholders. No LLM provider is connected.
4. **Hardware I/O** -- `raf_hardware` has data models and protocols defined but no actual serial port communication (needs `serialport` crate).
5. **PCB 3D Layout** -- Schematic works, but no PCB board layout view. Gerber export blocked by this.
6. **Networking** -- `raf_net` is protocol definitions only.

## Architecture Principles

- **Command Bus**: All state mutations go through `raf_core::command::CommandBus` for undo/redo support
- **ECS-ready**: Scene uses `hecs` ECS, but components aren't fully utilized yet
- **Modular crates**: Each feature area is a separate crate for clean dependencies
- **Performance focused**: Target is to run on low-spec hardware. Use adaptive quality, LOD, efficient rendering.

## Key Design Decisions

- The editor uses **egui immediate mode** — no retained widget tree. Each frame redraws everything.
- Theme colors are **public constants** in `theme.rs`, used across all panels for consistency.
- The app has a state machine: `AppScreen` enum controls which screen is shown (Loading → ProjectHub → NewProject → Editor → Settings).
- Panel functions follow the pattern: `fn show(&mut self, ui: &mut egui::Ui)` — they take a mutable UI reference and draw themselves.
- Translations use inline `if is_es { "Spanish" } else { "English" }` checks. No i18n framework yet, but `fluent` crate is available.

## Building & Running

```bash
rustup default stable-x86_64-pc-windows-gnu  # one-time
cargo run -p aura_rafi_editor                 # that's it
```

## Branding

- **Yoll** credit appears ONLY on the loading screen, bottom, very subtle
- Website: yoll.site
- Don't make it invasive. Don't put it in every panel.

## Code Style

- Comments in English
- UI text supports English + Spanish (all panels)
- NO emojis in code or frontend
- Use the theme constants from `theme.rs`, don't hardcode colors
- Panels are standalone structs with `Default` implementation
- Keep functions focused, avoid 500+ line functions
- **Modularize new code**: Do NOT add to existing large files. Create new files with focused functions and call them from the main file. Each new feature should be its own .rs file (e.g., `shortcuts.rs`, `auto_save.rs`, `scene_actions.rs`). Group related files in directories when needed.
- **Do NOT refactor existing large files** - leave them as they are, but all NEW code goes in new modular files.

## Performance Mandate

- **Everything must be lightweight**. The engine must run on low-spec "potato" PCs without eating resources.
- No unbounded memory growth. Undo stacks capped at 50, console entries capped.
- No new dependencies unless absolutely necessary. Prefer zero-allocation hot paths.
- The engine should behave like a secondary program, NOT a resource hog like Chrome, Unity, or AutoCAD.
- Use adaptive quality (Potato/Low/Medium/High). Default to Low.
- Profile before optimizing, but always prefer the simpler/lighter solution.

## When Continuing This Project

1. Read `docs/ROADMAP.md` for planned milestones
2. Read `docs/ARCHITECTURE.md` for detailed crate descriptions
3. Next priority: v0.4.0 Visual Scripting improvements or v0.2.0 wgpu rendering
4. Check `Cargo.toml` workspace deps before adding new crates
5. Test with `cargo check` (fast) before `cargo run` (slow first time)
6. The `.cargo/config.toml` sets target-dir to `target_gnu` to avoid MSVC lock conflicts
7. Create new `.rs` files for new features, do NOT bloat existing files
