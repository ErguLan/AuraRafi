# Changelog

All notable changes to AuraRafi will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - Edit Mode + Rendering Upgrade

### Added

- **Depth-sorted rendering** (`raf_render/depth_sort.rs`): painter's algorithm sorts ALL faces from ALL entities by depth before drawing, eliminating Z-fighting and overlap artifacts. O(n log n) per frame, zero GPU.
- **Entity picking** (`raf_render/picking.rs`): click to select 3D entities via screen-space bounding sphere projection. 30px threshold, overlay-area exclusion, closest-to-camera preference.
- **Multi-select**: Shift+Click adds/removes entities from selection set. Ctrl+A selects all entities. Click on empty space deselects. Selection stored as `Vec<SceneNodeId>`.
- **Transform gizmo arrows**: RGB arrows (X red, Y green, Z blue) with arrowhead triangles and axis labels. Displayed on selected entity when Move/Rotate/Scale tool is active. Drag arrows to translate position or scale along individual axes. Screen-space hit testing via point-to-segment distance.
- **Gizmo drag interaction**: dragging gizmo arrows modifies entity properties directly on the SceneGraph. Move tool translates position, Scale tool adjusts per-axis scale. Drag speed proportional to orbit distance.
- **Edit mode foundation**: Tab key toggles Object/Vertex modes. Visual "EDIT MODE" indicator in viewport. Foundation for future vertex-level editing.
- **Hierarchy-viewport-properties bidirectional sync**: selecting in hierarchy panel updates viewport selection and vice versa. Properties panel always reflects the first selected entity.
- **Enhanced info overlay**: shows edit mode (OBJ/VTX), active tool, triangle count, selection count, orbit distance.

### Changed

- `ViewportPanel.selected` changed from `Option<SceneNodeId>` to `Vec<SceneNodeId>` for multi-select support.
- `ViewportPanel.show()` now accepts `&mut SceneGraph` (was `&SceneGraph`) to allow gizmo drag to modify entities directly.
- `draw_3d()` completely rewritten: 7-phase rendering pipeline (collect -> sort -> draw faces -> wireframe -> labels -> gizmo -> edit mode indicator).
- All `selected = None` / `selected = Some(id)` patterns migrated across `app.rs`, `shortcuts.rs`, `hierarchy.rs`.
- `do_select_all()` now selects ALL entities (was selecting only the first).


## [0.3.0] - 2026-03-29

### Added

- **Undo/Redo** (Ctrl+Z / Ctrl+Y): scene-snapshot based, max 50 entries, lightweight RON serialization
- **Delete entity** (Del key + Edit menu): soft-delete with recursive child removal, undo-safe
- **Duplicate entity** (Ctrl+D): clones selected scene node with +1.0 X offset, auto-selects copy
- **Select All** (Ctrl+A): selects all valid (visible, non-empty) entities in scene
- **Save shortcut** (Ctrl+S): saves project + scene to RON file alongside project directory
- **Auto-save**: timer-based, uses configurable interval from settings (default 120s), status bar feedback
- **Scene RON persistence**: scene graph serialized to `scene.ron`, loaded on project open if file exists
- **Enhanced menu bar**: File (New/Save/Settings/Exit), Edit (Undo/Redo/Duplicate/Delete/SelectAll), View (Grid/Scene/Schematic), Project (info), Help (shortcuts reference + version)
- **Status bar improvements**: modified indicator (*), undo/redo stack depth (U:N R:N), last action display, entity count
- **Help menu**: keyboard shortcuts reference panel, engine version display
- **SceneGraph methods**: `remove_node()` (soft-delete), `duplicate_node()`, `all_valid_ids()`, `save_ron()`, `load_ron()`
- **Global keyboard shortcuts**: handled in editor main loop, works in all viewport modes (scene + game)
- All new menu items, buttons, and labels translated ES/EN
- Hierarchy panel: translated heading/empty-state, skips soft-deleted nodes
- Settings screen: translated heading, save/cancel buttons
- New project form: translated title for Game/Electronics types

### Changed

- Menu bar fully translated ES/EN (Archivo/Editar/Vista/Proyecto/Ayuda)
- Status bar now shows entity count, theme name, and language - all translated
- Build/Run button translated (Ejecutar/Compilar/Test Electrico)
- Open project now loads scene from `scene.ron` if available, falls back to default scene
- Undo/redo stacks cleared on new project load

## [0.2.0] - 2026-03-25

### Added

- **Filled mesh rendering** for all primitives: Cube (6 face quads with backface culling), Sphere (4x6 UV sphere), Plane (1 quad), Cylinder (8 side quads + cap fans)
- **Flat shading** with directional light (dot product brightness 0.3-1.0), per-face normals, applied via CPU (no shaders needed)
- **Wireframe/Solid toggle**: 3 render styles (Solid+Wire, Wireframe Only, Solid Only) selectable via top-right buttons or Z key
- **Color picker** in Properties panel: RGB color edit + 7 quick color presets (R/G/B/Y/O/P/W)
- **Primitive type selector** in Properties panel: dropdown to change entity shape at runtime
- **2D/3D mode toggle**: clickable buttons at top-center of viewport, separate rendering paths for each mode
- Properties panel fully translated ES/EN (name, transform, material, shape, variables sections)
- Render style buttons translated ES/EN (Solido/Malla/Relleno vs Solid/Wire/Fill)
- **EditableMesh** (raf_render/editable.rs): runtime vertex/face mesh data with selection system, move/scale/extrude/delete operations, per-axis scaling, cube/sphere/plane/cylinder primitives, wireframe and render face output
- **Transform gizmo** (raf_render/gizmo.rs): per-axis X/Y/Z handles with 2D segment hit testing, translate/scale/rotate modes, axis colors
- **LOD system** (raf_render/lod.rs): 3 distance-based detail levels, auto-cull beyond max distance, segment/stack helpers for primitives
- **Collider system** (raf_core/scene/collider.rs): AABB auto-fit from vertices with intersection test + wireframe viz, ConvexHull with directional pruning, MeshCollider with exact geometry, ES/EN type labels
- **Mesh merge** (raf_core/scene/merge.rs): combine meshes into single draw call, vertex welding with distance threshold, source range tracking for unmerge, MeshGroup for entity grouping
- **Render backend switch** (raf_render/backend.rs): CPU painter default (zero GPU) / GPU wgpu opt-in, BackendConfig with frame budget tracking, adaptive detail reduction, FrameRenderStats, potato preset
- **SchematicGraph** (raf_electronics/schematic_graph.rs): independent electronics data graph, separated from game SceneGraph. Own SchematicNode, Net, selection, picking, net rebuilding, RON persistence, legacy format detection with warnings ES/EN, format version marker
- **WorldState** (raf_core/world_state.rs): lightweight game world snapshot for AI - time, weather, biome, camera, resources, custom data
- **AI Director** (raf_ai/director.rs): observe WorldState, emit DirectorActions (spawn/remove/weather/scale/color/sound/custom), 3 modes (Disabled/Observer/Active), zero cost when off
- **AI Asset Gen** (raf_ai/asset_gen.rs): GeneratedMesh/Texture from prompts, AssetGenCache with eviction (50MB max), disabled by default, low-poly defaults
- **Mesh Provider** (raf_ai/mesh_provider.rs): streaming MeshChunk with grid coords + LOD, camera-based loading/eviction, 50k vertex budget, 4 provider types
- **Hot Reload** (raf_core/hot_reload.rs): polling-based file watcher (zero external deps), detects changed/new/deleted files, 6 categories (Scene/Schematic/Config/Script/Asset/Project), recursive dir scanner, status summaries ES/EN, mod + collaborative dev support
- **Animation Collision** (raf_core/scene/anim_collider.rs): AnimCollider per bone (sphere check), 5 response types (Stop/BlendToContact/Slide/Recoil/Ignore), AnimCollisionConfig enabled by default, auto-generate for common bones, layer masks. Structure prepared for animation system.
- **Render Abstraction** (raf_render/abstraction.rs): RenderBackendTrait, 4 backend tiers, 20+ RenderCapability flags, ActiveBackend selector. Separates "what to render" from "how to render" - enables CPU->wgpu->RT scaling
- **Scene Render Data** (raf_render/scene_data.rs): GPU-ready SceneRenderData bridge (RenderMesh, RenderLight directional/point/spot/area, RenderCamera, RenderEnvironment with fog/sky/HDR, RenderOutput stats)
- **PBR Materials** (raf_render/material.rs): metallic/roughness glTF-compatible, 6 texture slots, MaterialPhysics (friction/density/impact sounds), MaterialLibrary, factory methods
- **Spatial Partitioning** (raf_render/spatial.rs): SpatialGrid (3D uniform grid, O(1) queries, 3 presets), Frustum (6-plane culling, sphere tests)
- **Complement Trace** (raf_render/complements/complement_trace.rs): RT designed from day 1, 4 modes (Disabled/Software/Hardware/Hybrid), 6 toggleable features, BVH AccelerationStructure
- **GPU Deformation** (raf_render/gpu_deform.rs): 7 deformer types (cloth/hair/vegetation/water/skeletal/blend/custom), wind/gravity/stiffness params, per-vertex GPU overhead estimates
- **World Streaming** (raf_render/world_stream.rs): seamless open world, WorldRegion with biomes/LOD/state, potato/default/high presets, camera-based region management

## [0.1.1] - 2026-03-16

### Added

- **raf_hardware** crate: serial port abstraction, sensor data model, actuator commands, robot control interface, ML training data bridge
- **Node executor** (raf_nodes): walks flow chains, evaluates data pins, handles On Start/Print/If/Add nodes with safety step limit
- **Magnet** electronic component: N/S poles, field strength in Tesla, polarity, parse "Weak"/"Strong"/"Neodymium"/"0.5T"
- **Gerber export structure** for JLCPCB and PCBWay: manufacturer-specific layer definitions, placeholder until PCB 3D layout is built
- **Circuit sharing**: RON serialization/deserialization for schematics (share_circuit / load_shared_circuit)
- **Simple Mode** toggle in EngineSettings: hides advanced parameters (grid size, auto-save, multithreading, units) for beginners
- **Target Platform** selector: Desktop, Mobile, Web (WASM), Cloud/Streaming, Console - with display names and descriptions
- **Headless mode** flag in settings (structural, for future cloud/server rendering)
- **Responsive layout** flag in settings (structural, for future mobile/tablet UI)
- Settings panel: fully translated ES/EN, Simple Mode toggle at top, target platform dropdown with info labels
- Schematic editor: right-click context menu (rotate/delete/duplicate/edit value on components, delete on wires, wire mode/test on canvas)
- Schematic editor: drag-to-move components after placement
- Schematic editor: wire selection and deletion (point-to-segment hit testing)
- Schematic editor: component rotation (R key + context menu, rotation-aware pin rendering)
- Schematic editor: inline value editing via modal window
- Schematic editor: component duplication (Ctrl+D + context menu)
- Schematic editor: closeable test results overlay (Esc or click outside)
- Schematic editor: all UI text translated ES/EN via is_es flag from app settings

## [0.1.0] - 2026-03-01

### Added

- Initial project structure with 8 modular crates
- Entity Component System (ECS) with hecs integration
- Flat scene graph with parent-child hierarchy
- Command bus for undo/redo and AI tool-calling
- Event bus for decoupled module communication
- Engine configuration with theme, language, and render presets
- Project management (create, load, save, recent projects)
- Editor application with loading screen, project hub, and main editor
- Dark and light theme system with orange accent (#D4771A)
- Viewport panel with grid, axis lines, zoom/pan, and tool shortcuts
- Hierarchy panel with scene tree and entity selection
- Properties panel with transform editing
- Console panel with log filtering
- Asset browser panel with type filtering
- Visual node editor with drag-to-connect bezier connections
- Schematic view panel with component library and wire drawing
- AI chat panel structure (not yet functional)
- Settings panel (theme, language, quality, editor prefs)
- Asset importer with type detection
- 3D primitive types (Cube, Sphere, Cylinder, Plane)
- Electronic components (Resistor, Capacitor, LED) with SimModel
- Schematic with auto-designator assignment
- Electrical connectivity test
- Component library with built-in parts
- DC simulation engine (Modified Nodal Analysis, Gaussian elimination)
- Design Rule Check (6 rules: floating pins, missing value, isolated, unnamed nets, short circuit, LED without resistor)
- Netlist generation (union-find algorithm, rotation-aware pin positions)
- SVG export (styled, rotation-aware)
- BOM CSV export (grouped by value/footprint with quantities)
- Text netlist export
- Visual scripting nodes (On Start, On Update, Print, If, Add)
- Node graph with connections and validation
- Node graph compiler stub (validates connectivity)
- AI tool registry with 6 default tools
- AI provider configuration (OpenRouter, OpenAI, GenAI, Claude)
- Network protocol definitions
- Project documentation (Architecture, Getting Started, Contributing, Roadmap)
