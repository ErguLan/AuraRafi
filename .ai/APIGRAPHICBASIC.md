# ApiGraphicBasic Controlled Hybrid Rule

> Architectural rule for every AI agent and renderer change.
> Decision date: 2026-07-18.

The contributor-facing explanation is [docs/APIGRAPHICBASIC.md](../docs/APIGRAPHICBASIC.md).
Use both documents for renderer, surface, RafUI, GPU asset, or backend work.

## 1. Final Direction

`ApiGraphicBasic` is the graphics owner of AuraRafi. WGPU is the current GPU
adapter and compatibility backend; it is not the permanent public API, resource
owner, presentation owner, or architecture ceiling.

The engine follows a controlled hybrid migration:

```text
Viewport | CAD | RafUI | Assets
                |
        ApiGraphicBasic
                |
   WGPU adapter | CPU recovery
                |
 Future native DX12 | Vulkan | Metal backends
```

The hybrid exists below `ApiGraphicBasic`. Editor surfaces and documents must
never implement separate WGPU and native-renderer code paths.

## 2. Migration Is Capability-Based, Not Level-Based

Do not organize this migration as disposable numbered levels. Each update may
transfer one complete responsibility from WGPU-facing code into an owned Rafi
contract. The long-term capability backlog is:

1. Backend-neutral public types and generational handles.
2. Adapter discovery, capability negotiation, and potato-first selection.
3. Device, queue, surface, and frame lifecycle ownership.
4. Persistent buffers, textures, samplers, meshes, and material registries.
5. Upload arenas, staging, cache budgets, residency, and eviction.
6. Command encoding for render, copy, and future compute passes.
7. Pipeline layouts, shader packages, and backend-specific pipeline caches.
8. Synchronization, resource-state tracking, fences, and device-loss recovery.
9. Frame graph and transient resources for shadows and post-processing.
10. Asset ingress from engine formats into backend resources.
11. Native DX12, Vulkan, and Metal implementations behind the same contract.
12. WGPU downgrade to optional fallback, followed by removal when gates pass.

This list is a durable backlog, not a promise that all items enter the same
release or stabilization cycle.

## 3. Immediate Foundation When Programming Is Authorized

The first implementation milestone is allowed to improve the contract while
WGPU remains the executor. It must:

- stop exposing `wgpu::*` types above the backend boundary;
- introduce Rafi-owned resource and surface handles;
- define adapter capabilities and explicit memory/performance budgets;
- define one canonical device owner shared by Scene, CAD, RafUI, and present;
- define asset ingress through owned descriptors and handles: `raf_assets` may
  discover/decode files, but ApiGraphicBasic allocates, uploads, budgets, and
  releases graphics resources;
- preserve GPU-first execution and CPU recovery;
- measure draw calls, uploads, allocations, submits, RAM, VRAM, and idle work;
- improve batching and reuse before claiming a native backend is faster.

This foundation is not permission to begin a native backend automatically.
Strong renderer programming still requires explicit user authorization.

### Foundation 1 Status (2026-07-18)

The first foundation is implemented while WGPU remains the active executor. It
currently provides generational handles, backend-neutral capabilities, adapter
preference, potato/desktop budgets, a neutral shared host context, and a GPU
output wrapper with an explicit transitional WGPU bridge.

It does not yet provide a complete resource registry, cross-surface structural
batching, unified DeviceHub across every host, frame graph, or native
DX12/Vulkan/Metal execution. The active scene viewport already has bounded
persistent mesh reuse and contiguous line batching; this is a measured hot-path
optimization, not a claim that every surface has a complete batch scheduler.

## 4. Hybrid Runtime States

The engine may pass through these long-lived states without changing upper
layers:

- **WGPU-backed ownership**: ApiGraphicBasic owns the public contract while
  WGPU executes GPU work.
- **Encapsulated compatibility**: WGPU exists only inside
  `raf_backend_wgpu`; no editor or surface imports it.
- **Native backend coexistence**: a native backend reaches parity while WGPU
  remains available as fallback and reference.
- **Native default**: the qualified native backend becomes the platform
  default; WGPU remains optional for unsupported devices.
- **WGPU retired**: WGPU leaves the shipping graph only after all removal
  gates pass.

Select one backend for a device/surface execution path. Do not mix WGPU and a
native API inside the same frame unless a deliberately designed and measured
interop contract exists. Accidental cross-API copies are forbidden.

## 5. WGPU Removal Gates

WGPU cannot be removed because a native backend merely opens a window. The
candidate backend must pass:

- Scene viewport, Schematic, PCB, and RafUI parity fixtures;
- resize, suspend/resume, surface loss, and device-loss recovery;
- integrated and dedicated GPU validation;
- bounded RAM/VRAM and cache eviction tests;
- frame pacing, idle-zero-work, and sustained-session measurements;
- shader/pipeline cache compatibility and diagnostic coverage;
- CPU recovery availability;
- no WGPU types or assumptions in upper layers.

Until then, WGPU is a controlled bridge, not technical debt to delete blindly.

## 6. Potato-First Contract

- Prefer an available integrated GPU over CPU rasterization for normal use.
- CPU software rendering is recovery, headless, testing, or incompatibility.
- Disabled features must have zero recurring frame cost.
- No continuous rendering or presentation while the document is idle.
- Cache and residency systems require hard configurable budgets.
- Batch lines, meshes, text, and UI paint data before backend submission.
- Advanced features are capability- and budget-gated, not merely named by a
  quality preset.
- PBR, shadows, post-processing, skeletal animation, and particles must reuse
  the same resource, frame graph, and budget contracts.

### Scene viewport integrity (implemented 2026-07-19)

- `BasicCommandList` merges adjacent compatible line commands into one
  `DrawLineBatch` while preserving painter order.
- CPU and GPU consume the same `BasicLine` width/depth contract. The GPU path
  emits one instanced quad batch; the CPU path expands the same pixel width.
- Persistent meshes are deduplicated within a frame and admitted to a bounded
  cache. Vertex-edit overrides are explicitly transient.
- The viewport uses physical render pixels (`pixels_per_point`) but keeps
  camera/overlay coordinates in logical points, and culling/focus use world
  transform scale.
- `Primitive::Sprite2D` is retired. The serde/command compatibility spelling
  maps to `Primitive::Plane`; game 2D is an orthographic 3D view.

## 6A. RafUI And Canvas Boundary

- RafUI documents remain serializable backend-neutral data. Layout/input
  compilation produces one `UiSurfaceDrawList` for GPU presentation and CPU
  recovery; it never creates a second scene command list, owns WGPU resources,
  or creates a second renderer.
- Viewport, Schematic, and PCB are renderer-owned center surfaces. RafUI owns
  chrome, menus, docks, inspectors, and bounded overlays around them.
- A minimap, selection, wire/trace overlay, or status readout must consume the
  same scene/CAD model and visible-world transform as the canvas.
- A temporary overlay may preserve authoritative visibility during a measured
  GPU parity gap, but it needs a bounded cost and removal condition. It must
  never become a second CAD/scene implementation.
- New retained UI follows `docs/RAF_UI.md` and `docs/EDITOR_RAFUI.md`; backend work must not
  bypass its action and ownership boundaries.
- Application menu bars use `UiApplicationMenu` and stable command IDs.
  `UiNodeKind::Menu` is only for in-surface context menus. Platform adapters
  return `UiMenuActivation` values and never execute domain logic.
- Cache layout, resolved text, paint data, buffers, and image uploads by their
  real invalidation inputs. Preserve paint order while batching only compatible
  adjacent work; do not optimize by reordering overlays or text behind panels.

### 6B. Frontier RafUI Overlay And Density Contract

- `raf_ui::overlays` resolves transient placement in window coordinates. The
  source surface owns semantic state, but owner clipping never limits a menu,
  tooltip, popover, drag preview, or modal.
- `UiSizeMode::FitContent` and the resolved text atlas provide intrinsic text
  geometry after localization. Panel code must not use magic widths for
  translated transient content.
- `UiEnvironment` owns logical-to-physical size conversion and bounded raster
  density. GPU and CPU presentation use equivalent logical geometry.
- `UiTween` owns time-based feedback. A settled motion cannot request idle
  frames; reduced motion resolves immediately.
- The transitional eframe bridge may compose a finished RafUI texture only. It
  may not draw retained text, rectangles, tooltips, menus, or interaction state.
- `UiSurfaceDiagnostics` is the shared inspection contract for layout, clipping,
  hit regions, text requests, zero-size geometry, and z-order.

## 7. Language and Dependency Boundary

Rust remains the engine and graphics-core language. C++ or Objective-C++ is
allowed only as a thin ABI bridge when a platform SDK or toolchain materially
benefits from it. No per-draw C++ boundary and no second C++ renderer core.

Rafi must own graphics policy, contracts, memory budgets, scheduling, and
presentation. It does not need to rewrite operating-system APIs, every image
codec, font rasterizer, or shader compiler to be an owned engine. External
tools must remain isolated and replaceable behind Rafi contracts.

## 8. Non-Negotiable Rules

- No big-bang WGPU deletion.
- No new WGPU type in editor-, document-, or surface-facing public APIs.
- No duplicate Scene, CAD, or RafUI renderer created for a native backend.
- No feature claim without an active render path and validation.
- No PBR, shadows, post-processing, particles, or skeletal expansion during a
  stabilization task unless explicitly authorized.
- Documentation must distinguish current, prepared, experimental, and active.
- Every backend optimization must be measured against the same fixtures.
