# AuraRafi Roadmap

This document outlines the development roadmap for AuraRafi.

## TEMP — Pendientes técnicos post-0.12 (diferidos, no ejecutar en 0.12)

Estos puntos se retoman **después** del cierre 0.12, en este orden. No forman
parte del alcance actual; quedan aquí para no perderlos.

- [ ] **`generate_circuit` real o retirado.** Hoy es un template LED con 4
  componentes fijos (`crates/raf_editor/src/commands/electronics.rs:290`), no un
  generador de circuitos. Convertirlo en un flujo real (netlist estructurada →
  validación → instanciación) o renombrarlo/retirarlo del catálogo público.
- [ ] **Partir `raf_editor` por dominio.** `agent`, `electronics`, `workbench`,
  `settings`, `commands`. Hoy concentra ~156k líneas y 300+ archivos. Imponer
  límites de tamaño de archivo/módulo y separar el agente del editor base.
- [ ] **Política de panics.** Medir `unwrap`/`expect`/`panic!` excluyendo tests
  con una regla explícita; en command handlers y surfaces devolver error o
  diagnóstico en vez de abortar el proceso.
- [ ] **Archivos comunitarios y seguridad.** `CODE_OF_CONDUCT.md`,
  `SECURITY.md` y `.github/ISSUE_TEMPLATE/`. `docs/CONTRIBUTING.md` ya existe.

Follow-ups detectados al cerrar P0:

- [ ] **Deuda de Clippy.** El gate estricto solo cubre `raf_net`; `raf_core`,
  `raf_script`, `raf_ui`, `raf_agb` y `raf_editor` tienen warnings existentes.
  Limpiarlos y extender `-D warnings` al workspace.
- [ ] **macOS editor bring-up.** La CI cubre `raf_core`/`raf_net` en macOS; el
  editor y el renderer siguen pendientes de validación nativa.

Auditoría P2 actual: [TEMP_TECH_AUDIT.md](TEMP_TECH_AUDIT.md).

## Construction, visual Assets and Hierarchy: 0.12 engine closure

This pass closes the shared 0.12 engine foundation without introducing another
modeling stack. The shared shape catalog contains `empty`, `cube`, `sphere`,
`plane`, `cylinder`, `capsule`, `wedge`, `torus`, `prism` and `sphere2d`.
Capsule is 1 m diameter and 2 m total height, centered on Y. Sphere2D is a
camera-facing shaded disk (billboard), not a second sphere mesh. Shared shape
descriptors define local dimensions and conservative world AABBs. Dimensions
edit existing scale; no second persistent size is introduced. Existing scene
transforms are preserved. Commands reject simultaneous size and scale.

Current source scope:

- [x] Parent-aware authoring bounds, anchored single-face scaling, metric
  dimensions, floor/surface snap and cancellable asset placement.
- [x] Existing Assets surface with typed catalog projections, image thumbnails
  and isolated ApiGraphicBasic primitive/prefab previews. Demand-driven 128 px
  cache, at most 256 entries and 16 MiB RGBA. Oversized prefabs keep an
  explanatory icon. Refresh/catalog changes invalidate previews. GLB/GLTF files
  are static mesh assets that can be inspected and placed in the scene.
- [x] Compact Hierarchy component/diagnostic badges, conjunctive field queries,
  insertion parent, ordered drops, auto-scroll and per-session UUID view state
  in `editor_hierarchy.ron`. Scripts remain optional virtual component rows.
- [x] Opt-in compact script indicator on Hierarchy rows, derived from
  `SceneNode.scripts` with zero file I/O.
- [x] Camera component, active/startup camera, environmental Sun, directional
  shadows, AudioSource, triggers and independent editable prefabs.
- [x] Entity point/spot lights with a bounded per-frame budget and no per-light
  shadows.
- [x] Tab Mesh Editor v1: focused Object/Tab mode, persisted
  `EditableMeshData`, vertex/edge/face selection, direct handle dragging,
  extrude with side walls, inset, subdivide, delete, Accept/Cancel, axis locks
  and scene-history undo. Imported GLB meshes and interactive bevel/boolean
  remain out of this pass.

0.12 deliverables and acceptance:

- [x] Static GLB/GLTF Mesh Assets: decoded meshes, material slots, import
  bounds, source identity, validated textures, editor/runtime placement and
  save/load round trips. Skins, morph targets and animation playback are not
  part of the static placement path.
- [x] Linked Assemblies v1: source/version/hash, `assembly.revert`,
  `assembly.update`, `assembly.make_unique`, nested cycle validation and
  source-slot reuse. Root overrides are preserved; child-level override merge
  and automatic refresh remain pending. Product naming is Assembly, never
  Prefab.
- [x] Point/Spot entity lights: registered component with Inspector, rendering
  through the shared scene pipeline and automation schemas. Per-light shadows
  are not implemented.
- [x] Spawn markers and trigger volumes authored through `game.spawn` and
  `game.trigger`, shown as real Hierarchy types. Remaining registered capability
  types stay pending.
- [x] Persistent Editable Mesh v1: Object/Vertex/Edge/Face selection,
  Accept/Cancel, undo/redo and saved geometry. Interactive bevel, editable pivots
  and experimental boolean subtraction remain pending.
- [x] Wedge, torus, prism and Sphere2D extend the shared shape catalog, not
  disconnected menu lists. Parametric cylinder radii/segments and editable
  pivots remain pending.
- [ ] Mesh Assets, linked Prefab Instances, supported lights, cameras, audio
  emitters, trigger volumes, spawn markers, groups and shapes displayed from
  real registered capabilities. No placeholder types or fake Hierarchy nodes.

After 0.12: splines, lathe/sweep/loft and extensive procedural construction.
Native DPI/input/recovery acceptance is distinct from compilation/offscreen tests.

## Audio System v1 (0.12)

Entity-scoped audio is connected to blocks, the project mix, the Agent and
scripting. See [AUDIO.md](AUDIO.md).

- [x] `AudioSource` with mixer bus, spatial flag, minimum/maximum distance and
  rolloff. Existing scenes default to SFX and non-spatial playback.
- [x] Project mixer settings for master, music, SFX, voice and ambient, plus a
  bounded voice budget.
- [x] Runtime listener update from the active camera; spatial gain follows the
  entity world transform every fixed step.
- [x] `game.audio` actions `attach`, `update` and `remove`; the Agent exposes
  `audio_edit`.
- [x] Rhai `stop_all_audio` and `set_audio_bus_volume`; `set_property` supports
  `audio.*` fields.
- [x] Assets audition button and bounded WAV metadata/waveform reader.
- [ ] MP3/OGG/FLAC native decoding. These files are catalogued but the native
  output path currently accepts PCM WAV.
- [ ] Live mixer console, waveform card rendering and spatial occlusion.

## 0.12 Engine Closure (implementado)

This section records what the repository now supports at the engine and
authoring-contract level. It is a capability ledger, not a promise of advanced
rendering or production-grade asset pipelines.

- [x] Corrected geometry catalog: `empty`, `cube`, `sphere`, `plane`, `cylinder`,
  `capsule`, `wedge`, `torus`, `prism` and `sphere2d` share one slug list in the
  Agent schemas, scene batch and command catalog. The batch no longer excludes
  Capsule, and the old hardcoded lists are gone.
- [x] Sphere2D billboard: a camera-facing shaded disk composed with a billboard
  rotation in `SceneRenderer`, rendered two-sided. It is a flat representation,
  not a second lit sphere volume.
- [x] Entity point/spot lights: `SceneLight` on `SceneNode`, authored through
  `game.add`/`game.update`, `game.light`, the Inspector and the Agent tools.
  `LightKind` is point or spot; spot uses inner/outer cones. World Sun, sky and
  ambient remain in `EnvironmentSettings`.
- [x] Static GLB/GLTF import: `raf_assets::model_asset` decodes `.glb`/`.gltf`
  meshes, material slots, bounds and textures with explicit size/count limits.
  `MeshAssetRef` stores a project-relative path, optional source node, content
  hash and import scale. Skins, morph targets and animation playback are not
  imported by this path.
- [x] Active camera and real look-through: a camera component can be created
  and configured. `game.camera action=look_through` renders the viewport through
  that entity camera, draws a 90% safe frame plus rule-of-thirds guides, and
  Escape or `action=editor_view` restores the editor camera.
- [x] Surface and pivot authoring: `surface.tiling` scales UVs on primitives,
  editable meshes and imported models; `unlit` and `double_sided` reach both
  pipeline paths; pivot Center/Base/Corner are applied as a render-space
  geometry offset without moving gizmos, lights, scripts or children.
  `Center` on imported meshes keeps the authored origin to avoid moving assets.
- [x] Static model rendering: `SceneRenderer` caches decoded models by project
  path and renders their baked static submeshes through the shared scene
  pipeline, including `game.mesh_asset action=place` and project save/load.
- [x] Assets import surface: toolbar Import opens a modal with drop zone,
  options, progress/error states and a native file picker. GLB/GLTF packages are
  copied below `assets/models/<slug>/` with their dependencies and metadata.
- [x] Model previews and placement: the preview worker renders decoded static
  models into the same bounded thumbnail cache, and the placement ghost uses the
  cached model bounds and a temporary scene node.
- [x] Data safety: `project.ron` and `scene.ron` publish through the recoverable
  atomic writer, and scene parsing can recover the previous generation instead
  of silently returning an empty scene.
- [x] Scene packet reuse: the editor compositor hashes command content instead of
  the frame `Arc` pointer, so camera-only frames no longer rebuild or re-upload
  the scene packet.
- [x] Local-light shading on both paths: CPU Gouraud shading and the shared GPU
  shader consume the same bounded light layout. Per-light shadows remain out of
  scope.
- [x] Hierarchy Scene/Lighting tabs, kind registry, light/mesh/camera badges,
  real keyboard navigation and background box selection.
- [x] GPU backface culling and split opaque/two-sided/transparent pipeline
  state across DX12, Vulkan, Metal and WebGPU. The CPU path already culled and
  blended correctly.
- [x] Persistent Editable Mesh v1, vertex/edge/face picking, inset, subdivide,
  delete and Accept/Cancel. Interactive bevel, boolean subtraction and
  sub-object animation remain pending.
- [x] Hierarchy component/diagnostic badges, directional Sun shadows, capsule
  authoring and the existing camera/AudioSource/trigger/prefab slices remain
  implemented.

Explicit limits in this closure:

- No per-light shadows, cascades, GI, contact shadows, volumetrics or
  physically calibrated photometry.
- Entity lights are bounded: at most 8 enabled local lights per frame,
  selected by influence around the camera focus. Higher render profiles do not
  raise this cap yet.
- GLB/GLTF support is static geometry plus base-color material slots. There is
  no skinned or morph-target placement, and no PBR rendering promise.
- Import failure, unsupported extension, missing dependency and malformed
  numbers are reported as command errors; they are not silently replaced with
  placeholder geometry.

## Harness 0.12 — pendientes de la pasada P0–P2

La pasada actual ya cubre contratos de resultado, persistencia de capacidades
visuales, evicción reportada, retry/backoff, plan cancelable, `raf open`,
`mcp serve --launch`, filtrado honesto de tools headless, consumo real de
`session` en MCP, imágenes MCP para todos los artefactos `image/*`, persistencia
de `world.script` en `.aura_rafi/world_scripts/`, gates de assertions, chips de
budget/navigation en la plan review card, `project.open` attached,
`project.save --wait`, modos MCP (`raf_set_mode`), notificaciones
`resourcesUpdated` y recursos `raf://plans` / `raf://plan/{id}` / `raf://digest`.
Las operaciones de plan `camera` y `script`, y `world::camera` / `world::script`
en el sandbox, también están implementadas.

Quedan para 0.12, sin fingir que ya existen:

- [ ] UI: lightbox modal con zoom sobre el detalle de evidencia y miniaturas
  reales en mensajes del historial; composer y evidencia ya las tienen, igual
  que el pin al contexto.
- [ ] Plan review: miniatura top-down, “Aplicar por región”, chips de gameplay
  y diff viewer por entidad con creados/actualizados/eliminados.
- [ ] Activity strip con fases Observar → Planificar → Construir → Verificar →
  Revisar visualmente → Checkpoint, y miniatura viva durante capturas.
- [ ] Compaction de historial con memoria: resumir resultados viejos conservando
  receipts y resúmenes, sin amputar los turnos recientes.
- [ ] Evidence gallery: usar una captura como referencia de estilo y abrirla en
  el viewport; el pin al contexto ya existe. Índice por tarea en vez de
  limitarse a los últimos 32 mensajes.
- [ ] Model picker: badges de reasoning/contexto además del badge de visión ya
  añadido.
- [ ] MCP: progreso por chunk para commits grandes, recurso
  `raf://artifact/{id}` y prompts de brief/program/visual review. `raf_set_mode`,
  notificaciones `resourcesUpdated` y recursos `raf://plan/{id}`,
  `raf://plans` y `raf://digest` ya existen.
- [ ] CLI/MCP: `project.save` headless limitado a metadata del proyecto (no
  escena); alinear el legado `raf_scene_plan` con el pipeline de
  `raf_scene_program`. `project.open` attached y `raf save --wait` ya existen.
- [ ] Contact sheet: reportar `failures[]` por vista y reintentar/omitir vistas
  que no producen frame, en vez de fallar con “requires two to four captures”.
- [ ] `world_script`: completar `query_scene` (hoy existe `world::count`
  read-only), `place_prefab`, `noise/seeded_random` y semilla explícita; el
  sandbox, la persistencia y `world::camera` / `world::script` ya existen.
- [ ] World Program: operación `light`, zonas y listas explícitas
  `resolved/unresolved/failed_checks`; `assert` de accesibilidad y presupuesto
  más allá de `no_overlaps`/`min_reachable_goals`, con el alcance de solapes
  limitado a las entidades del plan cuando corresponda. `camera` y `script` ya
  existen.
- [ ] Tools de intención: `build_world`, `dress_scene`,
  `place_checkpoints(along_route)` y `verify_playable_path`.
- [ ] Catálogo: conectar `enum_values` de todos los parámetros del catálogo a
  los JSON Schemas de tools; hoy los enums críticos están tipados a mano.
- [ ] `scene_query`/`outline`: incluir color y world bounds por defecto cuando
  el presupuesto lo permita.
- [ ] Resiliencia: `max_turns` como pausa con “Continuar” en vez de `Error`,
  timeout de tools asíncronas y checkpoint del agente tras transacciones grandes.
- [ ] Evaluación: proyectos dorados + prompts (“un supermercado”, “hazlo de
  noche”, “arregla los solapes”) con métricas de plan, navegación, diseño,
  round-trips y artefactos.
- [ ] Tests de escenario con `script.run` y simulación real, después de que
  Runtime esté estable.

## Current Status: Active development beyond the initial foundation

The engine foundation is in place and several vertical slices have already moved past pure scaffolding. This section tracks broadly available systems, while later sections separate implemented milestones from planned work.

- [x] Cargo workspace with 9 modular crates (+ raf_hardware)
- [x] ECS (Entity Component System) via hecs
- [x] Scene graph with parent-child hierarchy
- [x] Command bus with undo/redo support
- [x] Event bus for decoupled communication
- [x] Project management (create, load, save)
- [x] Configuration system with RON persistence
- [x] Temporary editor shell retired after the native RafUI workbench landed
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

Rendering is implemented through the ApiGraphicBasic scene command path with
GPU execution when available and CPU recovery for constrained hardware:

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
- [x] Render backend switch with frame budget, adaptive detail, and potato preset (active path is GPU-first under ApiGraphicBasic with CPU recovery; the early CPU-painter default was superseded)
- [x] Native Electronics document authority: `Schematic` + `PcbLayout`,
  renderer-neutral `CadScene`, retained RafUI and shared command gateway
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
- [x] Fabrication export from the synchronized 2D PCB: Gerber copper, solder mask, silkscreen and outline, plus Excellon drills.
- [ ] PCB layout view (basic 3D). The preview entry remains in beta.
- [x] PCB layout view (2D synchronized workspace): electronics projects now include a dedicated PCB canvas with placements, airwires, trace creation, board outline drafting, and `pcb_layout.ron` persistence.
- [x] Fabrication preflight and ZIP packaging for the CAM files.
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

> The prepared PBR, point/spot lighting, SSAO, post-processing, particle and
> ray-tracing goals moved to v0.12. The active v0.7 scope is directional Sun
> lighting, ambient/sky and opt-in directional shadows.

### Current implementation boundary

- [x] Per-project RenderConfig and GPU-first resource profiles with Potato,
  Low, Medium, and High budgets.
- [x] ApiGraphicBasic surface plans reserve explicit budgets for future
  shadows, post-processing, PBR, particles, and skeletal animation.
- [x] Flat scene/CAD rendering, camera, selection, gizmos, and retained UI
  composition through the shared graphics runtime.
- [x] Directional Sun, ambient and sky lighting, plus opt-in directional
  shadows through the shared GPU and CPU paths.
- [ ] Native-window visual and sustained performance acceptance on representative
  hardware. Automated and offscreen renderer checks remain separate evidence.
  This is an acceptance gate, not pending feature code.
- [x] 2D game view uses the shared 3D scene renderer with an orthographic
      camera; legacy `Sprite2D` data is loaded as `Plane` (textured assets
      remain future)

> Product PBR rendering, general post-processing, ambient occlusion, particles,
> skeletal animation playback and ray tracing moved to v0.12. Prepared
> descriptors and budgets do not count as active rendering features.

## v0.8.0 - Game Runtime (Local implementation; acceptance tracked separately)

Manual native Play now connects the isolated runtime and player. Checkboxes
below describe implementation, not a claim of complete native acceptance or
low-spec performance. See LOCAL_RUNTIME.md and CAMERA_RUNTIME.md for evidence.

- [x] Editor-integrated manual Play/Stop flow for game projects
- [x] Runtime-prep scene cloning so tests do not mutate the edit document
- [x] Scene loading and runtime initialization from saved project state
- [x] Node graph persistence as `nodes.ron`
- [x] Supported saved/unsaved node graphs compile to Rhai in Play; start, update, late-update and events
- [x] Keyboard input snapshot shape for future runtime scripts
- [x] External `.rhai` lifecycle harness for prepared runtime tests
- [x] Script-facing access to `self`, `parent`, entity paths, variables,
  movement, velocity, camera/lens, and audio triggers through the shared Host API.
- [x] Basic scene-collider contacts and simple gravity, damping, velocity,
  blocking, and trigger events. Supported AABB limits and native acceptance
  are documented in PHYSICS_RUNTIME.md.
- [x] Entity audio-source playback for supported WAV files on Windows.
- [x] Properties inspector support for runtime variables, audio sources, rigid bodies, and colliders
- [x] Animation-collider data contract with per-bone bounds and response types.
  Skeletal playback does not yet drive these colliders.

Follow-up work after 0.8.0:

- [x] Separate `aurarafi-player` executable for runtime snapshots. Editor Play
  uses the sibling executable when it is packaged and retains the embedded
  route for development.
- [x] Fixed-timestep loop decoupled from editor frame timing
- [x] Bounded IK solvers delivered ahead of the animation runtime: analytic
  two-bone with pole control plus FABRIK for longer chains, length preservation
  and unreachable-target handling in `raf_core::scene::ik`.

> Skeletal keyframe animation and animation-driven collision moved to v0.11.
> Foot/hand IK placement remains an integration task after skeletal playback.

## v0.8.5 - Viewport & Renderer Restructure (Done)

Full architectural rewrite of the viewport and render pipeline. The monolithic viewport file was split into a thin editor shell plus a renderer-side bridge layer. The rendering backend was replaced with a proper Z-buffer scanline rasterizer.

### Viewport Architecture
- [x] Native Winit application with a retained RafUI workbench (`native_application.rs`, `native_workbench.rs`)
- [x] ApiGraphicBasic canvas + RafUI composition in one presentation command submission
- [x] Object mode input: gizmo drag, entity picking, selection, and keyboard shortcuts through the neutral input router
- [x] Native toolbar with G/R/T/F, 2D/3D, render-style, grid, labels, and polygon toggles
- [x] UI/canvas pointer ownership and click-away behavior without a second UI host
- [x] HUD click blocking: UI overlays consume clicks before they reach world picking

### Renderer Bridge (`raf_render::bridge`)
- [x] `ViewportBridge`: owns camera state, renderer, edit session, and transform controller — zero widget-toolkit dependency
- [x] `ViewportTransformController`: gizmo drag lifecycle (translate/rotate/scale) with screen-space axis projection
- [x] `ViewportEditSession`: per-entity editable mesh state, vertex picking (10px), vertex dragging (world→local via inverse rotation)
- [x] Viewport vertex edits are session-only. Persistent mesh editing, saved
  topology and undo/redo belong to v0.12.
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

Agent authoring, script, and visual-review routes are present. The native
provider-to-project smoke test and visual/provider acceptance are tracked
separately in AGENT_CLI_MCP_EXPANSION.md and AGENT_WORKSPACE.md.

The remaining AI Director, MeshProvider, asset-generation, texture-material,
and procedural-terrain integrations move to v0.12.

- [x] v0.9.0 code scope closed: no queued implementation remains in this
  version. The deferred integration list above is owned by v0.12.
- [x] Pull-forward: on-demand local procedural PNG generation is available
  from the Assets create menu and refreshes the catalog when the worker
  finishes. Remote provider generation stays in the v0.12 workflow.

## v0.10.0 - Hardware & IoT (Complete)

**Status: Complete (2026-11-18); hardware access is opt-in.** The
[Electronics laboratory](AGENT_WORKSPACE.md#electronics-laboratory) opens inside the Agent workspace
and returns to the active editor document.

- [x] DC analysis of the project schematic with a signal map, voltage and current probes, saved runs and Electronics Agent tools.
- [x] Native serial communication through `serialport`, with port selection and background I/O.
- [x] USB serial port discovery and Arduino/ESP32 identification through the handshake with compatible firmware. A reference sketch is included.
- [x] Live sensor plots, binding measured channels to schematic probes, recording and capture playback.
- [x] Actuator controls for outputs advertised by the device, with acknowledgements, rejections and timeouts.
- [x] Hardware diagnostics for connection state, firmware identity, reported channels and output acknowledgements.
- [x] OTA preparation for compatible devices: image size check, SHA-256 and manifest export. Installation is handled by the device firmware updater.
- [x] PCB fabrication preview milestone: JLCPCB's quote page rendered both sides of the exported heart and detected its two layers and 334.75 × 212.25 mm dimensions.
- [x] Persisted hardware-access consent, off by default. Port search starts only
  after explicit activation and remains on demand. A serial worker exists only
  while a board is connected. This lazy path needs no engine restart.
- [x] Software-side serial bounds and activation guards covered by focused
  tests. Native and physical-board acceptance are separate.
- [x] Hardware acceptance on physical Arduino and ESP32 boards: discovery, telemetry, actuator acknowledgement, disconnect/reconnect and OTA preparation. Product sign-off on 2026-11-18; physical re-validation stays as a regression checklist.
- [x] Native performance acceptance during a sustained serial session and while returning between the laboratory and editor. Product sign-off on 2026-11-18; sustained-session measurement stays as a regression checklist.

### 0.10.0 closure (code hardening)

- [x] A transport failure terminates the pending output receipt as
  `Receipt::Failed`, so the laboratory never waits forever for an ACK that
  cannot arrive.
- [x] Link loss stops the active capture and enters a bounded three-attempt
  reconnect window with growing backoff; a manual disconnect cancels it.
- [x] Hardware-access consent propagates to every session-scoped laboratory,
  not only the mounted editor document.
- [x] Portable native pickers cover project, folder, firmware and OTA manifest
  destinations through zenity/kdialog on Linux and osascript on macOS, in
  addition to the Windows COM dialogs. Unsupported systems still fall back to
  no selection instead of a silent Windows-only no-op.
- [x] The reference sketch reports a reference-scaled voltage in `V`, enabling
  a unit-matched comparison with a DC voltage probe. `RAF_ADC_REFERENCE_VOLTS`
  is configurable per board.
- [x] Removed the dead stub serial API; `raf_hardware::serial_session` is the
  single live transport and the only public serial entry point.
- [x] Focused tests cover failed-receipt termination, ACK expiry, reconnect
  backoff and picker-output normalization.

## v0.11.0 - Cloud & Streaming

**Status: Preview (non-media scope complete).** Headless capture, streaming,
asset ingress, presentation surface recovery, the graphics memory ledger and
custom electronics components are implemented. WebRTC media, device-loss
recreation and the first remote CI run remain open. Runtime animation, animation
collision and IK integration moved to v0.12. Native graphics backend lanes and
the frame graph moved to v0.11.5.

### Graphics ownership (ApiGraphicBasic)

- [x] ApiGraphicBasic contract foundation: backend-neutral generational handles,
  capabilities, adapter preference, memory budgets, retained product packets and
  native AGB executors.
- [x] RafUI retained compilation: one cacheable layout/input frame and one
  shared CPU/GPU paint list, retained GPU buffers, deduplicated image uploads,
  adjacent paint-run batching, and a native application-menu adapter boundary.
- [x] RafUI Frontier Core: window-level overlay placement with flip/shift,
  intrinsic content sizing, time-based hover/focus state, shared motion with
  reduced-motion behavior, semantic component recipes, bounded HiDPI density,
  and shared GPU/CPU diagnostics.
- [x] RafUI tooltip migration: compact tooltip content is authored and
  rendered by a dedicated RafUI surface in the native compositor.
- [x] ApiGraphicBasic unified memory ledger: cross-family budgets, saturating
  admission/release accounting, mesh/texture residency, upload rejection
  pressure and editor telemetry.
- [ ] ApiGraphicBasic presentation lifecycle: presentation health, device and
  surface-loss recovery, suspend/resume and backend-neutral fence tracking so
  backends can be selected per platform without coupling editor surfaces to
  WGPU.
- [x] ApiGraphicBasic asset ingress: render-facing texture decode and residency
  now pass through `raf_assets::ingress` and
  `ApiGraphicBasic::asset_ingress`, with path confinement, byte/pixel budgets,
  LRU eviction and bounded thumbnails. GPU upload remains in `device.rs`.
- [x] Presentation surface recovery: Lost/Outdated/Timeout reconfigure and retry
  once; OutOfMemory is fatal; health is observable by the Player and editor.
- [ ] Device-loss soak validation: the native host reopens the executor and
  replays the retained sources; a long sustained recovery run is still open.

### Cloud and streaming

- [x] Headless rendering mode (`--headless`) with deterministic fixed ticks,
  PNG capture and a machine-readable report.
- [x] Low-latency input pipeline for cloud streaming: authenticated handshake,
  authoritative input frames with sequence numbers, edge derivation and
  p50/p95 latency accounting. The WebRTC adapter remains open.
- [x] Linux native build (CI target): Linux workflow, target override and
  artifact upload are in place. The first remote green run is pending.
- [x] WebAssembly (WASM) build target: `raf_web` canvas host and renderer demo.
  `cargo check -p raf_web --target wasm32-unknown-unknown` passes; the browser
  runtime is still a demo until the project VFS and scripts land.
- [x] Cloud deployment configuration: multi-stage image, entrypoint, resource
  limits and volume layout. The image build remains pending.
- [x] Streaming protocol preparation (WebRTC control plane): SDP offer/answer,
  ICE candidate models, token-authenticated signaling and a bounded negotiation
  state machine. The media plane (DTLS/SRTP/SCTP/codec) is owned by v0.12.

### Electronics authoring

- [x] Custom component creation: symbol presets, pin geometry, validation,
  project persistence and a retained creator surface.

**0.11 delivery snapshot (2026-11-19):** headless capture and streaming,
self-made transport, Linux CI/Docker, WASM renderer host, the graphics memory
ledger, asset ingress, presentation surface recovery, WebRTC control plane and
custom electronics components landed. WebRTC media (DTLS/SRTP/SCTP/codec),
device-loss recreation and the first remote CI run remain open.

### Deferred inside the 0.11 line

- Native graphics backend lanes and the frame graph are owned by v0.11.5; the
  native lanes and WGPU removal already landed.
- Skeletal animation, animation collision and foot/hand IK are owned by v0.12.
- The transistor/IC/connector catalog is owned by v0.12.

## v0.11.5 - Graphics Backend Program

This release owns the renderer work that must not block cloud delivery. The
product cut landed in v0.11.4 and WGPU was removed in v0.11.5.

- [x] Native graphics backend lanes: Vulkan, DX12, Metal and WebGPU execute
  ApiGraphicBasic in the editor, player, streaming and web hosts.
- [x] WGPU removal: the executor, dependency, capability variant and probe lanes
  are gone; A/B compares native APIs.
- [ ] Frame graph and transient resource aliasing for shadows, post-processing
  and future passes, with backend-neutral resource states and explicit
  synchronization. The compiler, aliasing and barrier tests are in place;
  product pass integration is still open.
- [ ] Device-loss, surface-loss and suspend/resume recovery validated on
  integrated and dedicated GPUs beyond the startup exercise.
- [ ] Performance evidence program: frame time, uploads, draws, resident
  memory, idle work and load time measured against the same fixtures.

## v0.12.0 - ML & Robotics

The first attached-editor vertical slice may be pulled forward after the RafUI,
sessions, and scripting stabilization gates. This lets the real game project
drive engine stabilization without activating Play or Runtime. The complete
target remains v0.12 and is specified in
`docs/AGENT_CLI_MCP_EXPANSION.md`.

### Animation runtime (moved from v0.11.0)

- [x] Persistent transform rigs on scene hierarchies. Ordered weighted and additive
  layers, scoped transform tracks, fades, fixed-step playback, and stepped poses.
- [x] Velocity-driven locomotion blend between authored speed profiles at a
  shared gait phase.
- [x] Agent/attached CLI/MCP transform rig updates and read-only pose sampling.
  Embedded GLB transform clips can be mapped to existing rig nodes explicitly.
- [x] Animations dock with project clip assets, durable entity links, isolated
  viewport playback and scrubbing, and key capture for position, rotation and
  scale tracks.
- [x] Skeletal GLB/glTF placement from Assets, four-weight CPU deformation,
  inverse binds, editable key jumps and stepped/linear track settings. Cubic
  transform channels are baked at import. The same deformed geometry feeds the
  existing ApiGraphicBasic renderer paths.
- [x] Action variants gated by authored hand-slot occupancy, two-bone ground
  and target placement, and a swept tip probe for solid box obstacles.
- [x] Per-bone blend with authored dynamic bodies and bounded torque feedback,
  transform take baking and explicit rest-pose retargeting.
- [x] Bounded pose-and-planar-trajectory matching over authored clips with
  crossfades, alongside velocity-driven locomotion blending.
- [x] Development overlays in Play for transform rig links, collision geometry,
  velocity, gravitational weight, applied loads and contact impulses. Optional
  world-space impact offsets drive a bounded visual reaction on a rig child.
- [ ] Morph curves and facial deformation, preserving stable bone bindings
  across reimport, full skinned material slots and GPU-side deformation.
- [ ] Connect animation collision to playback: evaluate the configured
  `AnimCollider` set each animation step and dispatch the selected response.
- [ ] Stable planted contacts on moving supports and physical grab constraints.
- [ ] Expanded animation timeline with curve and graph editing, collision
  inspection and camera/audio sequencing. Export take images
  and video, preserve stepped poses through retargeting and bake to keys/cache.
- [ ] Gameplay markers with safe commit/rollback, actual held-prop constraints,
  contact-aware root motion, and rotational sweeps for limbs and held objects.
- [ ] Ragdoll authoring, balance and recovery behaviors, and complete physical
  take recording. Verify impacts at several speeds and angles.
- [ ] Qualify motion matching against a production clip library, articulated
  characters and end-to-end frame measurements with per-character detail levels
  and replay diagnostics. Evaluate studio interchange through external adapters.

### Electronics catalog

- [ ] Additional components (transistors, ICs, connectors) with schematic
  symbols, pins, footprint mapping and explicit simulation scope.

### Real-time media plane (WebRTC, moved from v0.11)

The v0.11 WebRTC item delivered the control plane only: SDP offer/answer, ICE
candidate models, a token-authenticated signaling server and a bounded
negotiation state machine (`docs/WEBRTC.md`). Browser media is a separate
engineering block and belongs to v0.12:

- [ ] ICE agent: STUN binding, pair checks, nomination and consent freshness.
- [ ] DTLS certificate, handshake and SDP fingerprint verification.
- [ ] SRTP key export, RTP/RTCP protection and anti-replay window.
- [ ] SCTP/DCEP data channels with bounded message sizes.
- [ ] RTP packetization and a browser-supported codec (VP8/H264/AV1), keyframes,
  jitter buffer and RTCP feedback.
- [ ] Congestion control, pacing and bandwidth estimation.
- [ ] Browser-compatible signaling transport (WebSocket/TLS edge) and a real
  browser client.
- [ ] Interoperability acceptance in Chromium and Firefox before claiming
  browser streaming.

`RAF1` remains an internal payload label and no browser decodes it. Do not mark
this section complete until the media path is measured end to end.

### Deferred AI authoring and rendering work

- [ ] Connect AI Director updates to the Game loop with explicit activation and
  bounded per-frame work.
- [ ] Stream MeshProvider chunks into the editor viewport and bounded scene
  meshes.
- [x] On-demand local procedural PNG generation from the Assets create menu,
  backed by the isolated worker and catalog refresh.
- [ ] Remote provider/model asset-generation controls in the Assets surface.
- [ ] Expand AI-assisted entity, prefab, script, configuration, and debugging
  workflows beyond the current command and review baseline.
- [ ] Apply generated textures through portable material assets and the owned
  ApiGraphicBasic ingress.
- [ ] Build procedural terrain with deterministic seeds and the shared mesh
  provider.

### Deferred advanced rendering (moved from v0.7.0)

- [ ] Product PBR material rendering with metallic/roughness, normal and
  emissive texture slots on the active GPU and CPU-recovery paths.
- [ ] Point and spot lights registered as editable scene capabilities with
  Inspector, shading and automation schemas.
- [ ] Ambient occlusion (SSAO) and a general post-processing stack.
- [x] Particle systems as a real rendering capability, not only a budget:
  data-driven `.rafparticle` JSON, CPU simulation, flipbook atlases, quality
  budgets, scene component, editor preview controls, runtime project policy,
  Properties, Assets creation/placement, script and node actions, and the
  `particles_generate` agent tool. GPU compute sorting, heat distortion and
  smoke volume upgrades remain in the 0.12 advanced-rendering track.
- [ ] Ray-tracing backend and traversal behind the prepared Complement Trace
  configuration.

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
  bridge without depending on a presentation panel or the Console UI
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
  that loads project documents without RafUI or a renderer. Keep it
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
