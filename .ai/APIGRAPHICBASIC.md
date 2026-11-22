# ApiGraphicBasic Native Ownership Rule

> Architectural rule for every AI agent and renderer change.
> Decision date: 2026-07-18.

The contributor-facing explanation is [docs/APIGRAPHICBASIC.md](../docs/APIGRAPHICBASIC.md).
Use both documents for renderer, surface, RafUI, GPU asset, or backend work.
The product migration cut is recorded in the [migration guide](../docs/AGB_MIGRATION.md).
The [native probe](../docs/AGB_NATIVE_PROBE.md) records the independent execution paths and hardware evidence.

## 1. Final Direction

`ApiGraphicBasic` is the graphics owner of AuraRafi. The native AGB executors
run the product path. The target is one Rafi-owned graphics contract:

```text
Viewport | CAD | RafUI | Assets
                |
        ApiGraphicBasic
                |
   one native executor per product build
      DX12 | Vulkan | Metal | WebGPU
```

Backend variation exists only below `ApiGraphicBasic`. Editor surfaces,
documents, and RafUI must never implement separate backend or renderer paths.
WGPU is not part of the workspace; A/B compares native executors.

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
11. Native DX12, Vulkan, Metal, and direct WebGPU under the same contract.
12. Product cutover. WGPU is not part of the workspace; A/B compares native
    executors.

This list is a durable backlog, not a promise that all items enter the same
release or stabilization cycle.

## 3. Foundation

The foundation is implemented in the product. Further work must:

- keep `wgpu::*` and every graphics API type below the backend boundary;
- preserve Rafi-owned resource and surface handles;
- keep adapter capabilities and explicit memory/performance budgets;
- keep one canonical device owner shared by Scene, CAD, RafUI, and present;
- keep asset ingress through owned descriptors and handles: `raf_assets` may
  discover/decode files, but ApiGraphicBasic allocates, uploads, budgets, and
  releases graphics resources;
- preserve GPU-first execution and CPU recovery;
- measure draw calls, uploads, allocations, submits, RAM, VRAM, and idle work;
- improve batching and reuse before claiming a native backend is faster.

This foundation is not permission to begin a native backend automatically.
Strong renderer programming still requires explicit user authorization.

### Foundation 1 Baseline (2026-07-18, product cut 2026-11-21)

The foundation and the product cut are implemented. It provides generational
handles, backend-neutral capabilities, adapter preference, potato/desktop
budgets, retained product packets, native executors, and CPU recovery.

The resource registry, structural batching, frame graph, native executors and
live product surfaces are active. Metal and WebGPU still need shadow and scene
filter passes; that is a capability gap, not a reason to reintroduce WGPU.

### Independent native foundation (2026-11-20)

`raf_agb` owns the extracted handles, scheduler, registries, ledger, frame graph
and product executors. The editor, player, streaming and web hosts consume it
directly.

## 4. Private Backend Evolution

Complete the product contract first. During integration, reuse the existing
scene, RafUI, CAD, compositor, and host code to emit or consume it directly.
Do not add an intermediate family of `Agb*Backend`, `AgbCompositor`, or
`AgbWindowHost` wrappers.

The build supplies its native executors. AGB may select one at startup based
on the compiled APIs, target surface and available hardware. A request for an
exact API either opens it or reports why it could not. Each frame owns
resources from one device. Recovery rebuilds the same API. WGPU is never a
startup or recovery fallback.

## 5. WGPU Boundary

WGPU was removed in 0.11.5. The rules that keep it out:

- No crate declares `wgpu` or imports `wgpu::*`.
- Native executors must pass the same Scene, CAD, RafUI, resize, surface-loss,
  device-loss, budget and idle-work fixtures before expanding capability.
- A missing native capability is reported or implemented; it never silently
  switches the frame to another API or CPU.
- CPU software rendering stays the recovery, headless and testing path.
- A/B runs compare two native executors in separate processes.

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
- Work budgets and idle-zero-work keep AuraRafi a light companion process. They
  are not a permanent low FPS target: interactive headroom may raise presented
  FPS when frame time and pacing allow. Performance claims need multi-metric
  before/after evidence, never the HUD FPS counter alone. Campaign rules:
  `docs/Optimization_USECASES.md`.

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
  compilation produces one `UiSurfaceDrawList` for AGB presentation and CPU
  recovery; it never creates a second scene command list, owns graphics API
  resources, or creates a second renderer.
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
- Semantic RafUI translucency is resolved by ApiGraphicBasic before the shared
  draw list. Alpha-only materials reuse existing blending; high contrast,
  Reduce transparency, and constrained budgets use an opaque fallback.
- Blur requires separate authorization, bounded transient resources, and
  measured GPU/CPU behavior. It is not part of the alpha-only material beta.

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
- The native Winit host composes finished RafUI surfaces only. It may not draw
  retained text, rectangles, tooltips, menus, or interaction state outside the
  RafUI document contract.
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

- No WGPU dependency or type anywhere in the workspace.
- No duplicate Scene, CAD, or RafUI renderer created for a native backend.
- No feature claim without an active render path and validation.
- No PBR, shadows, post-processing, particles, or skeletal expansion during a
  stabilization task unless explicitly authorized.
- Documentation must distinguish current, prepared, experimental, and active.
- Every backend optimization must be measured against the same fixtures.
