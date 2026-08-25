# ApiGraphicBasic

ApiGraphicBasic is AuraRafi's owned graphics contract. It gives Scene,
Schematic, PCB, RafUI, and asset workflows one graphics vocabulary without
making those upper layers depend on a particular GPU library.

The current GPU executor is WGPU. It is an implementation detail below the
ApiGraphicBasic boundary, not a type that UI documents, editor panels, CAD
models, assets, or gameplay systems may expose as their public contract. CPU
composition remains a recovery, headless, and correctness path; normal desktop
work prefers an available GPU, including an integrated GPU.

The agent-specific non-negotiable rules are in
[`.ai/APIGRAPHICBASIC.md`](../.ai/APIGRAPHICBASIC.md). This document explains
the product and implementation boundary used by contributors.

## Current Shape

```text
SceneViewport | SchematicCanvas | PcbCanvas | RafUI surface | Asset upload
                                  |
                           RenderRuntime
                                  |
                         ApiGraphicBasic
                                  |
           BasicDevice + command lists + owned handles/budgets
                                  |
                    WGPU adapter | CPU recovery compositor
```

`RenderRuntime` selects the active execution policy and shares it across
editor-owned surfaces. `ApiGraphicBasic::BasicDevice` owns the device-facing
work. A surface receives either a GPU texture/presentation target or reusable
CPU RGBA pixels; it never owns the device lifecycle itself.

## Foundation 1 Implemented

The first ApiGraphicBasic ownership foundation landed on 2026-07-18 while WGPU
remains the private active executor:

- generational Rafi-owned handles exist for core GPU resource categories;
- backend-neutral capabilities, adapter preference, and potato/desktop memory
  budgets are carried by `BasicDevice`;
- `RenderRuntimeSnapshot` reports backend id, capabilities, and budget;
- `SharedGraphicsContext` is the neutral runtime boundary for the current host;
- GPU scene output is wrapped in `GpuTextureView` with a Rafi texture handle;
- the remaining `from_wgpu`/`as_wgpu` methods are private executor adapters
  contained below the native Winit/RafUI presentation boundary.

This does not claim that the resource registry, cross-surface batching, frame graph,
or native backends are complete. Those responsibilities migrate in later
updates without changing upper-layer Scene, CAD, RafUI, or asset contracts.

The scene viewport has already taken the first bounded hot-path step: adjacent
lines are submitted as `DrawLineBatch`, CPU/GPU line widths share one contract,
persistent meshes are deduplicated per frame, and GPU mesh residency is capped
by the active memory budget. This is intentionally smaller than a future
cross-surface batch scheduler.

## Ownership Contract

| Layer | Owns | Must not own |
| --- | --- | --- |
| Domain model | scene, CAD document, UI document, asset metadata | GPU resources, queues, WGPU handles |
| Surface host | input boundary, size, typed actions, frame request | backend-specific resource lifetimes |
| RenderRuntime | shared execution policy, surface registration, recovery choice | domain mutations |
| ApiGraphicBasic | resources, command lists, device/frame lifecycle, composition | editor routing, scene/CAD business rules |
| Backend adapter | WGPU or future platform API calls | public editor/document API |

The rule applies equally to a small UI texture and a full 3D mesh. Creating a
shortcut to a backend type in one panel makes later backend work harder for
every surface, so it is not allowed.

## Surface Lifecycle

Every renderer-owned surface follows this sequence:

1. Build an immutable scene, CAD, or retained UI frame model.
2. Scene and CAD record backend-neutral geometry into `BasicCommandList`.
   RafUI compiles its retained layout into `UiSurfaceDrawList`, its dedicated
   CPU/GPU paint payload.
3. Resolve bounded resources through the shared device and registry.
4. Submit through the selected backend exactly once for that surface path.
5. Present or cache the completed GPU texture. Use reusable CPU pixels only
   when the recovery policy is active.
6. On resize, suspend, device loss, or backend failure, rebuild only the
   invalid presentation resources; preserve documents and application state.

No surface should spin frames while idle. Hover, drag, animation, simulation,
or an explicit present request may schedule another frame. A hidden panel does
not submit or allocate work merely because it exists in a dock.

## RafUI Integration

RafUI is a serializable retained UI model. It produces layout boxes, paint
commands, text atlas requests, focus regions, and typed actions. Its renderer
integration is intentionally narrow:

```text
UiDocument + UiSurfaceSession
  -> layout / hit test / bounded text requests
  -> UiSurfaceDrawList
  -> ApiGraphicBasic UI compositor
  -> GPU target or CPU recovery pixels
```

The compilation cache preserves that single paint payload across unchanged
frames. It keys layout on document, control/focus state, size, and density;
it keys paint on resolved text and atlas revision. The GPU compositor retains
vertex buffers and batches only adjacent compatible paint work so stacking
order remains correct. UI must not create a second command-list path or
compatibility host.

RafUI must not create a second GPU renderer, own WGPU textures, or draw a
scene/CAD canvas as generic controls. The Viewport, Schematic, and PCB remain
renderer-owned center surfaces; RafUI owns their docks, command rows, menus,
inspectors, and overlays that derive from the same authoritative model.

For document construction, menu lifecycle, responsive layout, and the
temporary CAD overlay rule, use [`RAF_UI.md`](RAF_UI.md) and
[`EDITOR_RAFUI.md`](EDITOR_RAFUI.md).

## Asset Ingress

`raf_assets` is responsible for file discovery, classification, decoding, and
metadata. ApiGraphicBasic is responsible for turning decoded content into a
budgeted graphics resource. The handoff must use Rafi-owned descriptors and
handles, never raw backend textures or bind groups.

An asset import path therefore has four phases:

1. Discover and validate the source file.
2. Decode or transcode into an engine-owned image, mesh, font, or shader
   descriptor.
3. Ask ApiGraphicBasic to allocate/upload a bounded resource.
4. Store the resulting engine handle in the project model and release it when
   no document references it.

This leaves codec choice replaceable while keeping VRAM, cache residency, and
resource destruction under one owner.

## Potato-First Performance Rules

- GPU is the normal path; integrated GPUs are valid targets.
- CPU is recovery, testing, headless, or unsupported-device operation.
- Disabled features do no recurring work.
- Text atlases, images, meshes, command buffers, and upload staging use hard
  budgets and reusable allocations.
- Batch compatible primitives before submission. Avoid per-control, per-line,
  or per-glyph resource creation.
- Dynamic resolution, presentation pacing, and quality tiers are decisions of
  the shared render policy, not individual panels.
- Advanced render capabilities attach through shared resource/frame contracts.
  Prepared support is not the same as an enabled feature.
- Game 2D is an orthographic 3D scene. The retired `Sprite2D` spelling remains
  only as a compatibility alias to `Plane`; RafUI owns interface overlays.

## Backend Evolution

ApiGraphicBasic may gain owned DX12, Vulkan, or Metal adapters when a complete
responsibility has an owned contract and a measurable reason to move. Such a
backend must keep the same upper-layer APIs and pass equivalent Scene, CAD,
RafUI, asset, resize, loss-recovery, memory, pacing, and idle-work checks.

Do not create two editor implementations or two CAD renderers while moving a
backend. One upper-layer contract, one selected backend for an execution path,
and explicit diagnostics are mandatory.

## Review Checklist

Before changing this layer, confirm:

- No new public `wgpu::*` type escapes into editor, domain, RafUI, or assets.
- The change has one named ownership boundary and does not duplicate domain
  logic in a backend adapter.
- Resource allocation, upload, release, and recovery are budgeted.
- The surface has no idle repaint loop or unbounded cache.
- GPU behavior and CPU recovery retain the same document semantics.
- The implementation distinguishes active, prepared, and experimental work.
- The relevant compile/test/manual GPU and low-end checks are planned before
  declaring the feature complete.

## RafUI Overlay And Density Boundary

RafUI overlays are still ApiGraphicBasic surfaces. A tooltip or menu may be
semantically owned by a toolbar surface, but its transparent render target is
composed in the window's overlay layer so owner clipping cannot corrupt its
placement. ApiGraphicBasic remains responsible for the shared draw list,
text-atlas synchronization, GPU target, and CPU recovery pixels.

All hosts use logical points for layout and pointer input. `UiEnvironment`
converts the logical viewport to physical output size and chooses a bounded
raster scale. The GPU and CPU paths therefore share the same intrinsic text
measurements and overlay placement semantics at standard and high DPI.

The GPU compositor scales every retained solid, text, and image vertex from
logical coordinates into the physical target before NDC conversion. The
physical target size participates in the geometry cache key, so a DPI change
cannot reuse vertices generated for a different pixel density. Retained UI
texture presentation uses nearest filtering whenever the physical source and
destination dimensions match, including rounded fractional-DPI targets.
Linear filtering is reserved for an actual size conversion; a fractional panel
origin alone must not soften the surface.

UI image resources are uploaded with CPU-generated premultiplied-alpha mip
levels. The image sampler selects the nearest complete mip level rather than
interpolating between two levels, so minified icons do not shimmer or become
washed out as their parent surface is rebuilt.

The native Winit host composes the completed RafUI surface and ApiGraphicBasic
canvas layers directly. There is no legacy-widget compatibility placement in
the active runtime; historical bridge text belongs to the migration archive.

## RafUI quality contract

ApiGraphicBasic owns the presentation details that determine whether a retained
surface looks crisp: the text atlas, semantic icon rasterization, resource
uploads, mip policy, GPU sampler choice, CPU recovery sampling, and final
logical-to-physical geometry. RafUI owns only serializable semantics, layout,
style, focus, actions, and text/icon requests.

Technical icons resolve through `UiIconId` and are generated from compact
vector-like primitives into a 64px source image. They are not tiny PNGs scaled
up by a panel. Text and icons have independent density/sampling policies, and
CPU recovery uses nearest sampling for built-in icons so the fallback does not
reintroduce the blur that the GPU path removed.

The font registry is now located here as well. It selects the bundled Ubuntu
Regular, Medium, or Bold outline for each semantic request; the UFL license is
stored beside those assets. No alpha darkening or synthetic dilation is used,
so small text keeps clean antialiasing instead of developing a soft halo.

Tooltips are window-level overlay content. The session waits for 320ms of
stable hover intent, then animates opacity over 120ms. The tooltip measures the
resolved text before placement and prefers a position below the pointer; it
flips only when the viewport edge requires it. Its layout never enlarges the
trigger, panel, or viewport.

The renderer also exposes `UiSurfaceDiagnostics` for duplicate IDs, invalid
clips, zero-sized boxes, focus order, icon regions, and missing labels. These
diagnostics are intended to run before visual screenshot review, not as a
replacement for it.
