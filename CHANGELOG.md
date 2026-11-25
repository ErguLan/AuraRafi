# Changelog

All notable changes to Rafi will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Tab Mesh Editor v1**: focused Tab Mode with persisted `EditableMeshData`,
  vertex/edge/face selection, direct vertex dragging, extrude with side walls,
  inset, subdivide, delete, axis locks, Accept/Cancel and scene-history undo.
  The viewport dims the rest of the scene and draws orange handles and
  wireframe over the edited block.
- **Tab mesh interaction fixes**: pointer dragging now follows screen-right and
  screen-up instead of the inverted mapping. Double right-click on a face
  inserts a new movable point and splits that triangle into three. Editable and
  imported static meshes render two-sided with abs(N dot L) lighting and a
  visibility floor, so a face moved through its own plane cannot turn black or
  disappear.
- **Asset placement fixes**: a plain `.gltf`/`.glb` copied into `assets/` is
  placeable even without the optional import sidecar. Double-click or Enter on
  a model or Assembly card starts placement instead of opening it as a text
  document, and the selection action reads Insert with an Add icon.
- **Mesh authoring API**: `game.mesh` actions `inspect`, `enter`, `set`, `move`,
  `scale`, `extrude`, `inset`, `subdivide`, `delete`, `revert` and `apply`.
  The Agent exposes the same surface through `mesh_edit`, and Rhai adds
  `mesh_vertex_count`, `mesh_face_count`, `mesh_vertex`, `mesh_set_vertex`,
  `mesh_move_vertex`, `mesh_extrude`, `mesh_inset`, `mesh_subdivide`,
  `mesh_delete_face` and `mesh_reset`.
- **Spawn and trigger authoring**: `game.spawn` and `game.trigger` create real
  `SpawnMarker` and trigger-volume components; Hierarchy classifies them as
  `spawn` and `trigger` types.
- **Opt-in script indicator**: Hierarchy rows can show a compact script icon
  when the entity has attached scripts. It is derived from `SceneNode.scripts`
  with no file analysis and stays off by default.
- **Real look-through**: `game.camera action=look_through` renders the viewport
  through the selected Game camera, draws a 90% safe frame and rule-of-thirds
  guides, and Escape or `action=editor_view` restores the editor camera.
- **Linked Assemblies v1**: instances store source, version and FNV-1a source
  hash. `assembly.revert`, `assembly.update` and `assembly.make_unique` reload,
  refresh or detach an instance with cycle validation, an 8-level depth limit,
  a 4096-node dependency budget and scene-slot reuse. Product naming is
  Assembly/Ensamblaje; legacy `.prefab.ron` files remain readable.
- **Surface and pivot controls**: texture tiling, unlit and double-sided reach
  the renderer; Center/Base/Corner pivots are applied as a render-space offset
  with matching bounds and picking. Inspector controls expose all three.
- **GPU pipeline split**: DX12, Vulkan, Metal and WebGPU now use backface
  culling with explicit CCW front faces, separate opaque, two-sided and
  transparent scene pipelines, and depth-read-only transparent draws. Shadow
  and overlay passes keep their own state.
- **Welded editable topology**: primitive conversion welds duplicated
  UV/normal vertices, so dragging a cube corner moves every adjacent face.
  Editable meshes render with flat per-face normals and scale-aware UVs.

### Changed

- **Editor vs game quality**: Settings > Quality is now an editor/startup profile
  only. It no longer writes a runtime preset. The game's graphics quality lives
  in Project Settings > Game Graphics.
- **Play Host**: window size, max instances, launch mode, diagnostics and
  script budgets stay in the global Play Host section. The presentation FPS cap
  moved to Project Settings > Runtime so each game owns its pacing.
- **Project Settings order**: Game Graphics, Runtime and Lighting are grouped
  together. Lighting keeps its authored controls in the Lighting tab and links
  back from the project page.
- **Runtime scripting quality**: Rhai can read and change the per-instance
  viewport resolution, mesh detail and texture quality without mutating the
  project or global settings.

## [0.12.0] - 2026-11-22 - Engine Closure: Runtime, Lighting and Layers

### Added

- **Runtime loading timeline**: the local Play loading surface shows the
  current stage, prepared/total count, asset path, elapsed time and the last
  three stages; after 15 seconds it shows a slow-preparation warning with a
  Cancel action.
- **Failure surface**: a failed preparation shows the real error with Retry and
  Cancel. Retry restarts the same source; Cancel closes cleanly.
- **Runtime diagnostics**: `RuntimeDiagnostic`/`DiagnosticEvent` carry a stable
  code, severity, source, message, detail and timestamp. The editor console
  shows severity/source/time badges and the workbench shows a failure banner.
- **Play preflight**: the editor runs `aurarafi-player --version` before
  launching, redirects the child stdout/stderr to per-instance log files,
  records the exit code, applies a 20-second preparation timeout and reports
  the log tail.
- **Editor file log**: the editor tees tracing to the terminal and
  `AuraRafi/logs/editor.log`, rotates at 16 MiB and writes panic reports to
  `crash.log`.
- **Layered composition**: on the Vulkan window the scene and RafUI are two
  retained packets drawn in one acquisition and one present. A UI-only change
  no longer re-encodes or re-uploads the scene.

### Changed

- **Lighting uniforms**: `RasterUniforms` now has an explicit
  `lighting = [authored, improved, toon_steps, toon_softness]`; `viewport`
  carries only the scene pixel size and `eye` only the camera position.
  `draw.wgsl`, Vulkan, DX12, Metal and WebGPU use the new field.
- **DX12 frame constants** moved from root constants to a root CBV with a
  256-byte upload buffer, keeping the root signature within 64 DWORDs.
- **Full environment mapping**: sun color, ambient tint/fraction, exposure,
  toon, halo, stars, sun disk and shadow bias/strength/filter taps reach the
  product uniforms. Unsupported shadows/filter are dropped with a one-time
  diagnostic, never silently.
- **Editor settings/workbench surfaces** were adjusted around the new console
  and runtime notices.

### Fixed

- **Play stuck on loading**: the editor was launching a stale v0.11.2 player.
  The version preflight and the matching v0.12.0 build make Play reach
  `Running`, show the authored sky and expose failures immediately.
- **Authored sky**: the per-pixel sky now receives a consistent camera/lighting
  contract, and the ambient term is multiplied by `light.w` once.
- **Ambient/authored parity**: `ambient_tint * ambient_fraction` reconstructs
  the authored ambient color without double exposure.
- **Runtime console tail**: the console receives at most 24 lines of 200
  characters with an explicit truncation marker instead of a raw 200-line
  frame trace. The complete stdout/stderr pair stays on disk, and the bounded
  block can no longer force the text atlas past the graphics upload budget.
- **Editor UI budget**: the native editor uses the desktop graphics budget
  (64 MiB staging) instead of the potato runtime preset, so a 2048² retained
  text atlas is not rejected when Play stops. The runtime player keeps the
  project render preset.

### Known limitations

- Layered scene/UI composition is implemented only for the Vulkan window.
  DX12, Metal, WebGPU and all headless/capture hosts keep the single-packet
  path.
- Camera-only scene frames still bump the scene packet through the canvas Arc
  identity; a dedicated content revision is the next performance step.
- The broad 0.12 feature list (PBR, point/spot lights, SSAO, particles,
  skeletal animation, WebRTC media, electronics catalog, ML/robotics) is
  tracked for 0.12.x; this release closes the engine runtime, lighting and
  composition contract first.

## [0.11.6] - 2026-11-22 - Sky, Lighting and Runtime Verification

### Added

- **Per-pixel sky**: the AGB sky pass now reconstructs a ray per fragment and
  shades zenith/horizon/ground, haze, a solar disk, a bounded halo and
  procedural stars. Solid and gradient sky modes share the same contract.
- **Toon and improved lighting controls**: banded solar response, toon
  softness/steps, improved ambient falloff and the highlight shoulder are
  exposed through the product uniforms and the Lighting inspector.
- **Shadow detail**: receiver masking, soft filtering and strength reach the
  Vulkan and DX12 shadow shaders through the shared product contract; Metal and
  WebGPU keep the same uniform shape and gates.
- **Environment mapping**: `ProductFrameBuilder` carries the full environment
  (sun disk, halo, stars, toon, improved/authored flags) into `FrameUniforms`,
  with focused composition tests.

### Changed

- Editor runtime presentation and loading surfaces were updated alongside the
  lighting contract; the Studio and RafUI styles received matching polish.
- The editor now reports a bounded AGB compose trace when
  `RAF_AGB_TRACE_FRAME=1`, covering the packet background, sky colors, camera,
  viewport and shadow state.

### Fixed

- **Local Play**: the editor was launching a stale `aurarafi-player.exe`
  (v0.11.2) and the loading screen never advanced. Rebuilding the player with
  the editor at v0.11.6 makes Play reach `Running` with the authored sky.
- The engine opens the project viewport with the authored blue sky; the neutral
  preview toggle intentionally switches to the neutral backdrop.

### Known limitations

- The runtime loading screen still needs explicit stage history, elapsed time,
  error/retry state and a preparation watchdog.
- The other engineer's sky/lighting work in this tree is included as-is; the
  remaining uniform-flag cleanup (`viewport` vs `authored`/`improved`) is a
  follow-up.

## [0.11.5] - 2026-11-21 - WGPU Removed

### Removed

- **WGPU**: the optional `raf_agb/reference` executor, the `wgpu` dependency,
  `pollster` from `raf_agb`, the `GraphicsBackendId::Wgpu` capability variant,
  the probe `wgpu`/`wgpu-dx12` lanes and the unused WGSL constants are gone.
  No crate in the workspace depends on WGPU.
- The legacy backend labels `GpuWgpu` and `ActiveBackend::Wgpu` were renamed to
  `GpuNative` and `ActiveBackend::Native`; the localized label is now
  "GPU (native)" / "GPU (nativo)".

### Changed

- `tools/agb_compare.ps1` now compares two native AGB executors (default
  Vulkan vs DX12) instead of a native lane against WGPU.

## [0.11.4] - 2026-11-21 - AGB Product Cut

### Added

- **AGB product composition**: `ProductFrameBuilder` converts a real
  `SceneRenderFrame` into a retained `ProductPacket`; the editor, player,
  streaming and web hosts draw it through `ProductDevice` with one native
  executor per session.
- **Scene viewport**: `FrameUniforms.viewport` renders the 3D canvas into a
  sub-rectangle while RafUI/CAD overlays keep the full target.
- **Scene background**: `Command::SceneBackground` paints a canvas backdrop only
  inside the scene viewport.
- **DX12 shadows and scene filter**: directional shadow map, scene-only spatial
  filter, capability gates and resident-byte accounting, matching Vulkan.
- **RafUI product paint**: `DirectUiSurfaceHost::append_product_paint` emits
  `Paint`/`StyledPaint` with per-corner radii, per-side borders, gradients,
  feather, nested rounded clips, nine-slice, atlas text and images.
- **Native editor host**: `NativeUiWindowHost` owns the window, the AGB device,
  resize, surface recovery and device reopen. `NativeEditorCompositor` composes
  canvas and UI in one packet.
- **Player on AGB**: windowed, headless and streaming paths; world-space UI is
  rasterized on the CPU and composed as a textured overlay mesh.

### Changed

- `SceneFrameOutput` now carries `Scene(Arc<SceneRenderFrame>)` for the native
  handoff or `CpuPixels` for recovery; the GPU texture bridge was removed.
- `BasicDevice` no longer opens a graphics API. It selects the CPU recovery
  rasterizer or returns the retained frame.
- `raf_web` uses `WebGpu` and `ProductDevice<WebGpu>` directly; the WebGL2
  fallback and the canvas blit were removed.
- Documentation now describes AGB as the active product path and WGPU as an
  optional reference only.

### Fixed

- A placed canvas no longer emits a scene background before the sky backdrop.
- A swapchain acquisition timeout skips one frame instead of consuming the
  bounded recovery budget and logging a presentation failure.
- The DPI border fixture measures the ring against the fill instead of expecting
  a vertex in the middle of a straight edge.

### Included snapshot

- This tree also contains the animation-preview work that was already present
  in the v0.11.3 snapshot.

### Known limitations

- Metal and WebGPU do not implement the shadow map or the scene filter yet.
- A frame whose retained UI changed rebuilds the whole product packet; partial
  scene/UI layering is a follow-up.

## [0.11.2] - 2026-11-19 - Cloud Spine, Iron Ledger and Modular Forge

### Added

- **Headless player**: `aurarafi-player --headless` renders a real project with
  deterministic fixed ticks, writes PNG frames and a machine-readable report
  with backend, camera readiness, timings and a content hash. The capture
  excludes the local Player toolbar.
- **Self-made streaming transport** in `raf_net`: length-prefixed binary
  framing, token handshake, authoritative input frames with sequence numbers
  and derived edges, raw RGBA video frames, bounded queues, partial writes and
  p50/p95 latency accounting. No external transport or WebRTC crate.
- **Streaming host**: `aurarafi-player --stream ADDR:PORT` serves one
  authenticated session, renders only while a client is connected and reports
  sent/dropped frames, bytes and input latency.
- **Graphics memory ledger**: per-family budgets for buffer, texture, sampler,
  pipeline, mesh, material, staging and transient resources; saturating
  admission/release, upload rejection pressure and editor HUD telemetry.
- **Custom electronics components**: symbol presets, pin geometry, validation,
  `ElectricalAssets` persistence, a retained creator surface and explicit
  unmodeled simulation semantics.
- **WASM renderer host**: `raf_web` canvas target that reuses the desktop scene
  renderer and `ApiGraphicBasic` device, with an HTML demo and build scripts.
- **Linux CI and deployment**: Linux workflow with the target override, focused
  crate gates, artifact upload and headless smoke; multi-stage container,
  entrypoint and deployment documentation.
- **Frame graph compiler**: backend-neutral pass ordering, barriers, transient
  aliasing and budget validation with focused tests. Device integration remains
  open.
- **Editor memory telemetry**: cached `MEM` segment in the Game status bar and a
  Memory group in the UI cost inspector.
- **Asset ingress**: `raf_assets::ingress` owns texture decode policy and
  `ApiGraphicBasic::asset_ingress` owns path confinement, CPU residency, LRU
  eviction and bounded 128 px thumbnails. `SceneRenderer` no longer decodes
  files ad hoc.
- **Presentation surface recovery**: `Lost`, `Outdated` and `Timeout`
  reconfigure the surface and retry once; `OutOfMemory` is fatal; health is
  observable by the Player and the editor.
- **WebRTC control plane**: SDP offer/answer, ICE candidate models,
  token-authenticated line-delimited signaling and a bounded negotiation state
  machine in `raf_net::webrtc`.

### Changed

- `arboard` is a native-only dependency; the browser target uses a safe
  no-clipboard path.
- Engine timing uses a wasm-safe `Instant` wrapper instead of
  `std::time::Instant` in the render device.
- Custom electronics components are excluded from conductance and reported as
  unmodeled instead of being treated as wires.
- **God-file pass**: 27 large modules were split into facade + focused
  submodules, including `device.rs`, `gpu_renderer.rs`, `agent_surface.rs`,
  `agent_executor.rs`, `native_application.rs`, `game.rs`, `agent_context.rs`
  and `raf_cli/main.rs`. Public paths and behavior are unchanged; the rule and
  remaining debt live in [MODULARIZATION.md](docs/MODULARIZATION.md).

### Known limitations

- The WebRTC media plane (ICE agent, DTLS, SRTP, SCTP, RTP, VP8/H264) is owned by
  v0.12. The control plane is complete.
- Device-loss recreation is not implemented; surface loss recovers, a lost
  `wgpu::Device` is reported instead of reloaded.
- The first remote Linux CI run is still pending.
- The WASM target is a renderer demo; project loading, scripts and audio still
  need a browser VFS.
- Five editor unit tests and one retained-layout test were already failing
  before this delivery and remain outside this scope.

## [0.10.0] - 2026-11-18 - Hardware, Physics and Agent Stabilization

### Added

- **Physics runtime foundation**: authored joints (fixed, hinge, slider, ball,
  spring), a capsule character controller with grounded/support state, and
  bounded IK solvers (analytic two-bone with pole control and FABRIK).
- **Hardware laboratory**: serial discovery, Arduino/ESP32 handshake, telemetry
  plots, actuator acknowledgements, diagnostics, OTA preparation and a
  persisted opt-in hardware consent that keeps serial access lazy.
- **Agent and vision expansion**: model capability negotiation, image evidence
  bridge, contact sheets and structured attached-task lifecycle.
- **Portable native pickers** for project, folder, firmware and OTA
  destinations on Linux and macOS.
- **Bounded IK solvers** in `raf_core::scene::ik`: analytic two-bone with pole
  control and FABRIK for longer chains, preserving segment lengths and handling
  unreachable targets. Foot/hand rig integration remains part of the animation
  runtime.
- **Local asset generation from Assets**: the create menu can enqueue a local
  procedural PNG through the existing isolated worker. The catalog refreshes
  when the job completes; remote provider generation remains deferred.
- **Portable native pickers**: project, folder, firmware and OTA destinations
  use zenity/kdialog on Linux and osascript on macOS when available.

### Fixed

- **Serial receipt lifecycle**: a transport failure now terminates the pending
  output receipt as `Receipt::Failed` instead of leaving the laboratory waiting
  forever for an ACK.
- **Link recovery**: link loss stops the active capture and enters a bounded
  three-attempt reconnect window with growing backoff. Attempts only reset
  after the link reports `Open`, so a port that opens and immediately fails
  cannot loop forever.
- **Hardware consent propagation**: every session-scoped laboratory now honors
  the persisted hardware-access setting, not only the mounted editor document.
- **Reference sketch units**: the serial lab sketch reports a reference-scaled
  voltage in `V`, enabling unit-matched comparison with a DC voltage probe.
- **Single serial API**: removed the dead stub serial module; the live bounded
  `serial_session` worker is the only public serial entry point.

## [0.9.5] - 2026-08-24 - First Stable Edition

### Added

- **Native editor runtime**: the primary application now runs through Winit,
  retained RafUI surfaces, and ApiGraphicBasic presentation without a legacy
  widget runtime in the active editor path.
- **Game workbench**: restored and unified Hierarchy, Inspector, viewport
  controls, bottom-dock panels, project search, session actions, scene editing,
  native focus routing, and unsaved-exit confirmation.
- **Native Electronics foundation**: introduced retained Electronics
  controllers, navigation, inspectors, overlays, minimap support, command
  adapters, and shared CAD presentation through the graphics boundary.
- **Native Agent experience**: restored sessions, history paging, provider and
  configured-model selection, Active/Passive modes, streaming state, approvals,
  composer input, and Settings integration.
- **Shared command access**: editor, Agent, CLI, and MCP-compatible attached
  clients now converge on the same command protocol instead of maintaining
  independent mutation paths.

### Changed

- **RafUI interaction system**: strengthened retained focus, text input,
  pointer capture, hover/pressed states, dropdown placement, scrolling,
  invalidation, docking, and frame scheduling across native surfaces.
- **Graphics ownership**: expanded ApiGraphicBasic with editor composition,
  resource reuse, frame scheduling, native input translation, and cached GPU
  presentation while retaining deliberate CPU recovery paths.
- **Editor UX**: aligned icons, toolbars, responsive panels, Settings, Hub,
  Hierarchy, Inspector, Agent, and viewport gizmos around one native visual and
  interaction language.
- **Documentation**: consolidated the current architecture, command catalog,
  renderer ownership, native Electronics design, RafUI guidance, stabilization
  status, and English/Spanish localization contracts.

### Removed

- **Retired editor surfaces**: removed obsolete schematic/PCB widget views,
  superseded CAD surface hosting, duplicated schematic graph/symbol modules,
  and stale runtime guidance that conflicted with the native architecture.

### Fixed

- **Editor startup**: `cargo run` once again selects the native editor as the
  workspace default executable.
- **Input and presentation stability**: corrected cross-panel shortcut leaks,
  stale repaint paths, model/mode selection feedback, real-time FPS reporting,
  exit-to-Hub flow, hierarchy interactions, inspector commits, and multiple
  Agent layout and responsiveness regressions found during migration.

### Validation

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo test --workspace`

## [0.9.0] - 2026-07-13 - Surface Foundation, Session Architecture, and GPU-First Stabilization

### Added

- **Retained UI foundation**: introduced `raf_ui`, retained surface documents,
  docking metadata, focus/input routing, bitmap text atlas, direct GPU
  composition, and a reusable CPU recovery compositor under ApiGraphicBasic.
- **Session architecture**: projects can now organize independent worlds,
  interfaces, and electronics designs while sharing assets and scripts.
- **Editor camera block**: editor camera state is session-scoped and persisted
  without appearing as a user scene node.
- **Electronics CAD bridge**: schematic and PCB model data can produce retained
  graphics scenes with stable hit identities, DRC markers, and shared GPU/CPU
  presentation paths.
- **Asset generation paths**: remote image generation defaults to `gpt-image-2`;
  a local procedural PNG worker creates lightweight icons, badges, sprites,
  and temporary textures without a network dependency.
- **Data-backed primitive manifests**: built-in primitive prefabs now live in
  JSON manifests and import through the asset pipeline instead of generated
  scene code.

### Changed

- **GPU-first policy**: ApiGraphicBasic remains the common graphics boundary;
  GPU is the normal execution path and CPU remains a deliberate recovery path.
- **Viewport and CAD surfaces**: scene, schematic, and PCB rendering now have
  explicit surface hosts, reusable frame caches, and future-facing quality
  budget boundaries without enabling heavyweight effects prematurely.
- **Agent integration**: agent history participates in editor undo/redo, and
  electronics diagnostics can combine topology, rule checks, and simulation
  context before suggesting a change.

### Compatibility

- **Workspace version**: all workspace packages now resolve to `0.9.0` through
  the root Cargo workspace version.
- **Existing projects**: project loading retains a legacy session path so
  existing root-level scene, node, schematic, and PCB documents remain usable.
- **Validation status**: broad compilation and manual editor validation remain
  required before treating this release as production-ready.

## [0.8.9.1] - 2026-05-30 - Stabilization Pass, Viewport UX Polish, and Runtime Truth Docs

## [0.8.9.2] - 2026-06-30 - Hierarchy Explorer Pass, Electronics UX Hardening, and Group Gizmo Foundations

### Added

- **Hierarchy explorer pass**: real-time search/filter, `Ctrl+Click` toggle multi-select, smarter drag-and-drop zones (before/after/child), stronger child-target highlight, and clearer drag-ghost feedback in the scene tree.
- **Electronics guidance pass**: contextual schematic/PCB hints, automatic airwire-to-route handoff in PCB, live schematic box-select highlight, and auto-close behavior for the schematic value popup when selection changes or the user clicks outside it.
- **Group gizmo foundation**: first-pass multi-selection move/rotate/scale path in the Game viewport using group bounds instead of anchoring the gizmo to only the primary selected object.

### Changed

- **Undo/redo stabilization**: `pending_history_snapshot` is now wired into document-change flow, redo rebuilds the correct document-domain snapshot, and drag gating avoids pushing noise from plain selection clicks.
- **Shortcut focus safety**: `Ctrl+Shift+Z` now respects text-field focus the same way `Ctrl+Z` / `Ctrl+Y` already did.
- **Stabilization tracking**: `docs/STABILIZATION_STATUS.md` now reflects the 2026-06-29 and 2026-06-30 verification passes more honestly, including what was really wired versus what still needs manual closure.
- **Release labeling**: this git/changelog release is labeled `0.8.9.2` while Cargo workspace packages remain on `0.8.9`.

### Fixed

- **Schematic transform regressions**: rotate/mirror and inspector-driven transform edits now pre-anchor nearby legacy wire endpoints before applying the transform, reducing cases where cables stay behind after the first rotation.
- **PCB discoverability**: selecting an airwire from `Select` now moves the user into the right routing context instead of leaving the next step implicit.
- **Hierarchy drop clarity**: center drop targets are now easier to read during drag/reparent operations.
- **Editor compile sanity**: the current stabilization pass was revalidated with `cargo check -p raf_editor`.

### Added

- **Session safety hardening**: unsaved-changes confirmation now covers close-to-hub and app close flows, save failures stay visible, and auto-save uses real elapsed editor time.
- **Schematic usability pass**: placement preview rotation with `R`, clean placement/wire cancellation, clickable export popup, and clipboard copy feedback for Netlist, BOM, and SVG.
- **PCB interaction pass**: wider hit-tests, hover overlays for components/traces/airwires, live outline preview, and hot airwire rebuild during component drag.
- **Viewport and hierarchy polish**: primary multi-select highlight, hierarchy drag ghost, `WASD` + `Q/E` 3D movement, gizmo hover state, six face-based orange scale handles, interactive XYZ compass, quick grid/label toggles, adaptive HUD width, and persistent FPS counter toggle.
- **Stabilization tracking docs**: new `docs/STABILIZATION_STATUS.md` plus `docs/TEMP_PHASE6_RUNTIME_TRUTH_PASS.md`, including the new Phase 7 definition for canonical renderer and graphics hot-path consolidation.

### Changed

- **Release labeling**: the git/changelog release was labeled `0.8.9.1` while Cargo workspace packages remained on `0.8.9`.
- **CPU scene renderer path**: simplified the software scene renderer toward direct framebuffer rasterization, direct grid/line submission, and less command-list indirection in the active CPU fallback path.
- **Renderer truth docs**: aligned `README.md`, `docs/ARCHITECTURE.md`, `docs/RENDERER.md`, and stabilization notes around one canonical graphics story: shared graphics runtime for Scene/Schematic/PCB, GPU hardware first when available, CPU fallback retained, and the game runtime documented as a separate subsystem from that shared renderer.
- **GPU hot path consolidation**: `ApiGraphicBasic` now reuses shared-mesh GPU buffers, grows reusable per-draw uniform and line slots, and exposes backend frame metrics through the runtime snapshot and viewport HUD instead of paying fresh buffer creation cost every draw.
- **Roadmap framing**: updated `docs/ROADMAP.md` so the `0.9.0` target is described as an agentic workspace/editor direction.
- **Project temp hygiene**: root debug/temp artifacts are now ignored by git to keep the repo cleaner during stabilization work.

### Fixed

- **Save honesty**: project save and auto-save no longer silently clear dirty state when any write fails.
- **Exit safety**: closing the app or returning to the hub now respects unsaved work instead of discarding editor state blindly.
- **Schematic friction**: double-click/right-click wire issues, preview rotation behavior, export popup interaction conflicts, and export feedback gaps were corrected.
- **PCB feedback**: route/outline/component interactions now expose clearer hover cues and avoid stale airwire visuals while dragging.
- **Viewport feel**: labels back off during heavy interaction, top/bottom bars have stronger contrast, and scale gizmo behavior now matches `uniform_scale_by_default` more honestly.

## [0.8.8] - 2026-05-22 - Shared Graphics Runtime and Electronics GPU Canvases

### Added

- **Shared graphics runtime**: scene viewport, schematic canvas, and PCB canvas now share the same graphics runtime with hardware execution first and CPU fallback retained.
- **Electronics GPU canvases**: schematic and PCB editors moved their heavy geometry rendering into the unified graphics pipeline.
- **ApiGraphicBasic execution path**: command-buffer based scene execution expanded for both GPU hardware rendering and CPU raster fallback.

### Changed

- **Workspace versioning**: aligned the workspace package version with the current 0.8.8 release state.
- **Viewport graphics presentation**: native GPU texture presentation and shared wgpu integration now drive the main editor surfaces.

### Fixed

- **Electronics startup crash**: corrected the line pipeline bind-group mismatch that was breaking GPU initialization when opening electronics views.
- **Renderer consistency**: stabilized the GPU-first path while preserving fallback behavior for low-end setups.

## [0.8.5] - 2026-05-19 - Viewport Architecture Split and Software Z-buffer Rasterizer

### Added

- **Viewport Architecture Refactoring** (`crates/raf_editor/src/panels/`):
  - Refactored viewport panel shell to a clean, lightweight retained UI panel (`viewport.rs`).
  - Extracted HUD toolbar with G/R/S/F buttons, 2D/3D toggle, OBJ/VTX mode badge, and axis gizmo (`viewport_hud.rs`).
  - Decoupled object-mode and vertex-mode drag and click interactions (`viewport_interaction.rs`).
  - Added dedicated overlay rendering for labels, transform gizmos, and vertex edits (`viewport_overlay.rs`).
- **Renderer Bridge Layer** (`crates/raf_render/src/bridge/`):
  - Introduced `ViewportBridge` to own camera state, renderer, edit session, and transform controllers independently from the widget layer.
  - Implemented `ViewportTransformController` to manage gizmo drag lifecycle and axis projection.
  - Created `ViewportEditSession` to manage per-entity editable mesh state, vertex selection, and vertex dragging.
  - Implemented precise entity picking using ray-sphere broad phase and ray-triangle narrow phase intersection testing.
- **Render Pipeline Overhaul** (`crates/raf_render/src/render_pipeline/`):
  - Built a full MVP transform pipeline (model to view, projection, perspective divide, and screen coordinates mapping).
  - Added 6-plane camera frustum culling.
  - Implemented a Z-buffer scanline rasterizer (`rasterizer.rs`) with per-pixel depth test to replace the painter's algorithm.
  - Added transparency rendering via src-over alpha blending sorted back-to-front.
  - Added perspective and orthographic camera modes with smooth pan and zoom.
  - Optimized rendering with framebuffer reuse across frames and primitive mesh caching.

### Changed

- Migrated all viewport selection state to support multi-select via `Vec<SceneNodeId>`.
- Updated HUD click handling to block raycasts from passing through UI buttons.

## [0.8.0] - 2026-04-27 - Editor Runtime, Rhai Behaviors, Physics, Audio, and Nodes Persistence

### Added

- **Editor-Integrated Play/Stop Flow**:
  - Integrated Play/Stop execution path directly within game projects in the editor.
  - Implemented runtime scene cloning to run simulations on a sandbox copy, keeping the original edit document intact.
  - Enabled scene loading and runtime initialization using saved project states.
- **Visual Scripting & Rhai Integration**:
  - Added node graph serialization and persistence as `nodes.ron`.
  - Wired node execution to hook into runtime `On Start` and `On Update` events.
  - Integrated `.rhai` script execution engine for external behavior runtime.
  - Provided script-facing API helpers to access `self`, `parent`, entity hierarchy paths, variables, velocities, and audio playback triggers.
- **Physics and Collisions**:
  - Added basic collision detection through scene colliders.
  - Implemented simple physics simulations (gravity, damping, velocity, trigger-only rigid bodies).
  - Added `AnimCollider` per bone (`raf_core/scene/anim_collider.rs`) supporting 5 contact response types (Stop, Blend, Slide, Recoil, Ignore), auto-generated for hands and feet.
- **Audio & Inspector UI**:
  - Added runtime audio playback for entity audio sources.
  - Expanded properties inspector to view and edit runtime variables, rigid body properties, colliders, and audio sources.

### Changed

- Aligned project initialization, settings templates, and language translation keys for editor play/stop controls.

## [0.7.9] - 2026-04-21 - Software Z-buffer Rasterizer, Solid Rendering Modes, Editor Stabilization

### Added

- **Software Rasterizer** (`crates/raf_render/src/software_raster.rs`):
  - CPU-bound Z-buffer rasterizer with barycentric triangle rasterization, near-plane clipping, and alpha-aware depth testing.
  - Selection outline shader-less implementation and wireframe rendering with depth testing.
- **Render Config Options**:
  - Added `depth_accurate`, `depth_resolution_scale`, and `selection_outline` toggles to `RenderConfig`.
  - Added solid rendering settings: surface edges, x-ray mode, and face tonality.
- **Script Support & UI Assets**:
  - Created `script_support.rs` for automatic script language detection, cataloging, and external editor execution.
  - Implemented `ui_icons.rs` with an asynchronous, threaded icon atlas loader operating within a strict texture budget.
  - Extracted grid rendering into `viewport_grid.rs` under the new `ApiGraphicBasic` pipeline.
- **Project Settings & Navigation**:
  - Added project-specific configuration panels for layout, runtime, and graphics presets.
  - Configurable mouse orbit inversions (X and Y axes).
  - Hotkey `F` to focus and snap the camera to the selected entity.

### Fixed

- **Camera Movement**: Corrected WASD camera movement bug where orbit distance became stuck or corrupted during forward camera movement.

## [0.7.0] - 2026-04-15 - Advanced Rendering (Potato-First)

### Added

- **Per-project RenderConfig** (`raf_render/render_config.rs`): 17 opt-in feature toggles. 4 presets (Potato/Low/Medium/High). ALL features OFF by default. Zero cost when disabled.
- **Enhanced CPU lighting** (`raf_render/lighting.rs`): point lights with inverse-square attenuation, spot lights with cone angle, Blinn-Phong specular highlights. Configurable max lights (default: 0 = sun only).
- **Fog system**: distance-based linear fog with configurable color, start, and end distances.
- **Post-processing suite** (`raf_render/post_process.rs`): bloom glow, vignette, FXAA edge blending, Reinhard tone mapping, saturation adjustment, sRGB/linear conversion.
- **Texture system** (`raf_render/texture.rs`): CPU-side BMP loading (zero dependencies), UV sampling, LRU cache (50MB budget), auto-downscale to max_texture_size.
- **UV mapping** (`raf_render/uv_mapping.rs`): box/spherical/cylindrical/planar projection modes, cube UV quad generation.
- **WGSL shader suite** (`raf_render/shaders.rs`): PBR vertex/fragment (metallic/roughness, Schlick fresnel), flat/unlit, shadow depth, bloom extract, FXAA. Embedded as string constants, loaded only when use_gpu=true.
- **RenderPreset** in EngineSettings: configurable per-project, serialized to RON.
- **Hierarchy multi-select**: Shift+Click adds/removes entities. Primitive type icons next to names. Entity count display.
- **Asset browser overhaul**: drag-and-drop file importing, "Go to Folder" (OS explorer), "Create Script" (.lua template), IDE recommendation dialog (Yoll IDE / VS Code), recursive folder scan, type icons, search/filter.
- **Horizontal icon toolbar**: viewport tools as compact icon buttons with key hints and orange accent dot for active state. Edit mode indicator (OBJ/VTX) inline.
- **WASD camera movement**: A/D strafe, S backward, Shift+W forward, Space up, C down. Speed proportional to orbit distance.
- **Rotate gizmo**: drag gizmo arrows in Rotate tool mode applies per-axis rotation.

### Fixed

- **CRITICAL: Gizmo drag detection**: migrated from click-only detection to drag-state detection with first-frame axis selection. Gizmo arrows are now fully functional for Move, Rotate, and Scale.
- Toolbar click zones updated for horizontal layout.
- Keyboard shortcuts now require viewport hover (prevents conflicts with other panels).
- Selection sync between hierarchy/viewport uses `Vec<SceneNodeId>` consistently across ALL operations (add, delete, duplicate, undo, redo, select_all).

### Architecture

- `raf_render`: 6 new modules (render_config, lighting, texture, post_process, shaders, uv_mapping).
- `raf_core/config.rs`: added `RenderPreset` enum and `render_preset` field to `EngineSettings`.
- `ViewportPanel`: carries `RenderConfig` and `LightingEnv` (default = potato, zero overhead).
- `HierarchyPanel`: `selected_nodes: Vec<SceneNodeId>` replaces single-select. Primitive icons per node.
- `AssetBrowserPanel`: full rewrite with disk scanning, drag-drop, script creation, IDE dialog.
- 24 new i18n keys in en.json and es.json.

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
