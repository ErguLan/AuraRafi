# Changelog

All notable changes to AuraRafi will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
