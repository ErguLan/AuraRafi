# Optimization_USECASES

Status: active document of optimization use cases and rules.

Revision date: 2026-09-14.

This document describes the technical mechanisms that keep AuraRafi lightweight
and the concrete use cases where they apply. Its reference is the modern
checkout: `Winit + RafUI + ApiGraphicBasic`, with WGPU as a private adapter and
CPU as the recovery path.

This file is not a changelog and does not replace
`docs/APIGRAPHICBASIC.md`, `docs/RAF_UI.md`, or `docs/RENDERER.md`. It also
does not add entries to `docs/STABILIZATION_STATUS.md`; that document keeps its
own history.

## How to read the status

- **ACTIVE**: the mechanism exists in the modern path and code uses it.
- **PARTIAL**: part of the mechanism is functional, but a condition, surface,
  or measurement is still missing.
- **MEASURE_FIRST**: the work is authorized for the optimization campaign, but
  it must not be called implemented before a baseline exists.
- **PREPARED**: a contract, type, or future infrastructure exists, but it must
  not be counted as active frame work.

## Technical objective

Engine optimization does not mean raising the FPS counter at any cost. The goal
is for visible work to remain proportional to what actually changed:

1. An idle frame must not rebuild surfaces or submit continuous work.
2. A UI change must not render the scene again when the canvas is still valid.
3. A camera change must not rebuild documents, layouts, or resources that did
   not change.
4. A persistent mesh, line, texture, text block, or buffer must be reused within
   a bounded budget.
5. Quality reduction must be explicit, reversible, and measurable.
6. GPU and CPU must preserve the same visual semantics during recovery.
7. No optimization may rely on an isolated FPS counter: CPU, GPU, P95, hitches,
   draws, uploads, cache behavior, and memory must also be observed.

The ownership boundary is:

```text
Editor / RafUI / Scene / CAD
              |
       RenderRuntime
              |
       ApiGraphicBasic
              |
       Private WGPU GPU or CPU recovery
```

Surfaces do not create devices, handle WGPU directly, or invent a second
painting path.

## Active use cases

### UC-01 - Static Hub and modal windows at idle [ACTIVE]

**Problem:** an unchanged window can consume CPU/GPU simply because it keeps
requesting repaints.

**Mechanism:** `FrameScheduler` retains pending invalidations and continuous
work reasons. When there is no `WINDOW`, `DOCUMENT`, `CAMERA`, `UI`, `OVERLAY`,
`ANIMATION`, pointer capture, or other active reason, the normal profile enters
event-driven idle mode. The Hub and modals request the next frame only when
their model changes, input arrives, or a transition is active.

**Expected result:** zero recurring presentation work while idle, except for
operating-system costs outside the engine's control.

**Metrics:** `idle_frames_skipped`, `frames_permitted`, invalidation reasons,
total frame time, and presentation mode.

**Reference code:**

- `crates/raf_render/src/ApiGraphicBasic/frame_scheduler.rs`
- `crates/raf_editor/src/native_studio.rs`
- `crates/raf_editor/src/native_application.rs`

### UC-02 - UI frame without rebuilding the Game canvas [ACTIVE]

**Problem:** changing a menu, overlay, or panel does not require traversing and
rendering the entire 3D scene again.

**Mechanism:** `NativeEditorRuntime` classifies frame reasons through
`canvas_requires_render`. During UI or overlay frames it retains
`CachedGameCanvas` when device generation, source size, and target rectangle
match. The compositor presents the existing canvas together with the updated
UI.

**Expected result:** a UI change does not render the scene or upload geometry
when the scene, camera, viewport, and target remain unchanged.

**Metrics:** `canvas_renders`, `canvas_cache_hits`, source/target size,
`SceneFrameMetrics`, and per-frame uploads.

**Reference code:**

- `crates/raf_editor/src/native_editor_runtime.rs`
- `crates/raf_render/src/ApiGraphicBasic/editor_compositor.rs`

### UC-03 - DPI and physical density without unnecessary rescaling [ACTIVE]

**Problem:** mixing logical points with physical pixels causes blurry text,
incorrect geometry, and unnecessary rebuilds when DPI changes.

**Mechanism:** RafUI keeps layout and input in logical points. The host computes
the physical target using `pixels_per_point`; `raster_scale` and the physical
target are part of cache keys. The compositor converts vertices to NDC only
after the actual target is known. Nearest sampling is used when source and
destination have the same physical dimensions; linear sampling is reserved for
real rescaling.

**Expected result:** HiDPI does not create incompatible atlases or geometry,
and a surface is not recompiled merely because it is presented again at the
same density.

**Metrics:** logical/physical target, raster scale, cache hits, atlas bytes,
and geometry uploads.

**Reference code:**

- `crates/raf_ui/src/environment.rs`
- `crates/raf_render/src/ApiGraphicBasic/editor_compositor.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/compilation.rs`

### UC-04 - Retained RafUI compilation [ACTIVE]

**Problem:** traversing layout, hit regions, text, and paint on every
presentation turns an idle frame or a small change into a full document pass.

**Mechanism:** `UiSurfaceCompilationCache` keeps layout and paint invalidation
separate. Its key accounts for surface revision, session state, size, density,
color, text revision, and atlas revision. An unchanged surface returns the same
`UiSurfaceDrawList` to both GPU and CPU.

The GPU host retains vertex buffers, avoids repeated image uploads, and merges
compatible adjacent paint runs without changing visual order. The CPU host
receives the same retained projection to preserve parity.

Retained compilation assigns a monotonic revision to every new
`UiSurfaceDrawList`, and GPU geometry uses that revision together with logical
and physical size. Public low-level calls retain a pointer-identity fallback.
An idle frame does not walk and hash every quad just to discover that nothing
changed; `paint_runs` are retained with the geometry as well.

**Expected result:** repeated presentation does not rebuild the document, does
not create a new draw list, and does not upload geometry or images without an
invalidation.

**Metrics:** layout cache hits, paint cache hits, geometry cache hits, layout
builds, paint builds, paint runs, draw calls, upload bytes, and visible nodes.

**Reference code:**

- `crates/raf_render/src/ApiGraphicBasic/ui_surface/compilation.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/direct_host.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/cpu_host.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/gpu_renderer.rs`

### UC-05 - Text atlas and images with partial updates [ACTIVE]

**Problem:** dynamic text, long history, or changing icons can force full
uploads, frequent repacks, or unbounded growth.

**Mechanism:** the text atlas identifies requests by resolved text, role,
weight, size, width, overflow, and line mode. It keeps used slots, marks a
`dirty_region`, and synchronizes only the changed area. It compacts stale
entries when stale space justifies a repack and grows in bounded steps when the
content actually requires it. Images use retained resources, alpha-correct
mipmaps, and a byte/entry budget with LRU eviction; images used by the current
frame are protected to prevent re-upload thrashing.

**Expected result:** editing a label does not upload the entire atlas; browsing
long content does not grow old glyphs without limit or trigger an upload storm.

**Metrics:** requests, reused slots, new slots, overflow, occupancy, dirty
region, texture bytes, repacks, resident bytes, and image evictions.

**Reference code:**

- `crates/raf_render/src/ApiGraphicBasic/ui_surface/text_atlas.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/images.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/gpu_renderer.rs`

### UC-06 - SceneRenderer: culling, ordering, and lightweight batching [ACTIVE]

**Problem:** a scene with many repeated objects can pay per-object work even
when objects share a mesh, pipeline, and basic material.

**Mechanism:** `SceneRenderer` gathers visible jobs, retains transform
snapshots while the document revision is unchanged, sorts opaque objects into
depth and primitive-type buckets, and keeps transparent objects back-to-front.
The ordering exposes sequences of equal meshes so `BasicCommandList` can turn
them into `DrawMeshBatch` when it is correct.

Contiguous lines become `DrawLineBatch`. Edges are emitted after meshes so they
do not interrupt compatible opaque runs.

**Expected result:** fewer pipeline changes and fewer draws for repeated
geometry without breaking depth, transparency, or the CPU fallback.

**Constraint:** transparency and transient edit meshes are not forced into a
batch or persistent cache if that changes semantics or precision.

**Metrics:** visible jobs, commands, mesh draw calls, instanced draws, line draw
calls, triangles, CPU encode time, and uploaded bytes.

**Reference code:**

- `crates/raf_render/src/scene_renderer.rs`
- `crates/raf_render/src/ApiGraphicBasic/command_list.rs`
- `crates/raf_render/src/ApiGraphicBasic/device.rs`

### UC-07 - Mesh reuse and bounded GPU residency [ACTIVE/PARTIAL]

**Problem:** recreating vertex/index buffers every frame is expensive; keeping
them forever causes memory growth.

**Active mechanism:** persistent meshes are deduplicated by identity, enter a
memory-budgeted registry, update recent-use timestamps, and can evict one
unpinned LRU item. Line, overlay, and instance slots grow geometrically and are
reused. Vertex Edit overrides are marked transient and do not pollute the
persistent cache.

**Current limit:** the mesh registry and cache exist, but this is not yet a
single global registry for meshes, images, atlases, CAD, and every surface.
Shared budget enforcement remains follow-up work for the campaign.

**Metrics:** cache hits/misses, mesh upload bytes, resident bytes, resident
entries, evictions, upload-budget overruns, and device generations.

**Reference code:**

- `crates/raf_render/src/ApiGraphicBasic/device.rs`
- `crates/raf_render/src/ApiGraphicBasic/capabilities.rs`
- `crates/raf_render/src/ApiGraphicBasic/resource_registry.rs`

### UC-08 - Pacing, VSync, and dynamic resolution [ACTIVE/PARTIAL]

**Problem:** requesting frames faster than the target, window refresh, or GPU
capacity wastes energy and can cause hitches.

**Mechanism:** `FrameScheduler` separates the configured limit, effective
target, presentation refresh, and actually presented FPS. Continuous reasons
remain active only while a camera, pointer capture, animation, repeated text,
simulation, or other real activity exists. `DynamicResolutionController`
adjusts canvas source size within the profile budget and gradually restores
quality.

**Expected result:** the engine does not confuse redraw requests with presented
frames and can reduce viewport cost under sustained pressure.

**Current limit:** the mechanism is in the modern path, but its effectiveness
must be validated on both an integrated and a dedicated GPU. A `Performance`
profile or a 240 FPS limit does not guarantee that the monitor, VSync, or scene
will present 240 frames.

**Metrics:** requested target, effective target, presentation mode, presented
FPS, frame time, P95, hitches, resolution scale, CPU, and GPU.

**Reference code:**

- `crates/raf_render/src/ApiGraphicBasic/frame_scheduler.rs`
- `crates/raf_editor/src/native_editor_runtime.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/native_window.rs`

### UC-09 - Interactive drag, camera, and multi-select [PARTIAL]

**Problem:** during drag or orbiting, the user needs immediate response, but
the engine may try to rebuild labels, overlays, geometry, and buffers on every
pointer movement.

**Active mechanisms:** pointer capture and camera movement keep frames
continuous only while the gesture exists; selection and hover changes invalidate
once; the canvas can be reused for overlay frames; dynamic resolution can drop
under frame pressure.

**Not counted as active:** the old technique of hiding labels during drag is
not an active implementation in the modern Game path. If the renderer again
produces expensive labels, a priority policy for selection/hover and temporary
degradation will be evaluated with measurements, not arbitrary hiding.

**Metrics:** continuous frames by reason, gesture duration, scene renders per
frame, cache hits, source scale, draw calls, hitches, and input latency.

**Reference code:**

- `crates/raf_editor/src/panels/viewport_controller.rs`
- `crates/raf_editor/src/native_editor_runtime.rs`
- `crates/raf_render/src/bridge/viewport_bridge.rs`

### UC-10 - Long history, streaming, and visible text [ACTIVE]

**Problem:** a long chat or streaming response must not rebuild every message,
rasterize every block, or request a frame for every token.

**Mechanism:** history estimates heights, builds only the visible range with
overscan, and preserves top/bottom spacers. Messages outside the viewport stay
in runtime state but are not converted into visible RafUI nodes. The atlas
retains reusable blocks and compacts stale slots. The surface must coalesce
streaming changes into bounded visual invalidations; only the active message
requires continuous updates.

**Expected result:** scrolling and streaming cost remains related to visible
content, not total history.

**Metrics:** visible messages, overscan, text blocks, estimated height,
layout/paint cache hits, atlas requests, uploads, presentation time, and frames
requested during streaming.

**Reference code:**

- `crates/raf_editor/src/panels/agent_surface.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/text_atlas.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/compilation.rs`

### UC-11 - GPU diagnostics without altering the frame [ACTIVE]

**Problem:** synchronous GPU measurement can become the bottleneck being
measured.

**Mechanism:** when the adapter supports the required features,
ApiGraphicBasic uses timestamp queries with an asynchronous readback ring. The
result is collected several frames later; the GPU is never waited on merely to
update the HUD. If the adapter does not support the complete feature, GPU
timing is disabled without breaking presentation.

**Metrics:** `gpu_timing_supported`, `gpu_timing_sampled`, valid GPU
milliseconds, readback latency, and absence of waits in the render path.

**Reference code:**

- `crates/raf_render/src/ApiGraphicBasic/device.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/native_window.rs`

### UC-12 - Loading and project opening [ACTIVE/PARTIAL]

**Problem:** building the Hub, Workbench, CAD, and their caches before showing a
window makes startup appear blocked.

**Active mechanism:** the loading surface appears before heavy surfaces are
created. It presents real stages and then builds deferred surfaces while the
loading view remains visible. There is no fixed artificial delay.

**Limit:** Cargo compilation happens before the editor process exists and cannot
be covered by this screen. The pending measurement is to separate process
time, device creation, project reading, surface construction, and first useful
frame.

**Metrics:** duration per phase, first loading frame, first Hub frame, first
Workbench frame, project openings, startup uploads, and allocations.

**Reference code:**

- `crates/raf_editor/src/native_application.rs`
- `crates/raf_editor/src/panels/loading_surface.rs`

## Authorized optimization campaign

The modern path is stable enough to start a global optimization campaign. The
following items are the next work; they must not be reported as complete until
they are measured in the current checkout.

### OC-01 - Reproducible baseline [MEASURE_FIRST]

Use the same executable version, profile, resolution, scene, and VSync policy
for every comparison.

Minimum cases:

1. Empty Hub at idle.
2. Empty Game at idle.
3. Scene with many instances of the same primitive.
4. Scene with varied primitives and transparency.
5. Dense schematic and PCB.
6. Orbiting, dragging, multi-select, and resize.
7. Project opening and first useful frame.
8. Agent with long history, scrolling, and streaming.

Each case must record: presented FPS, target FPS, total CPU, renderer CPU,
valid GPU sample when available, P95, hitches, draw calls, commands, uploads,
skipped uniform uploads, cache hits/misses, resident bytes, evictions, atlas
metrics, and skipped idle frames.

Automated measurement does not replace manual window, DPI, input, and visual
quality checks.

### OC-02 - Reduce uniforms and uploads per draw [MEASURE_FIRST]

The current path already reuses mesh buffers. The GPU path now retains the last
value for each mesh, line, and overlay uniform slot, so a static frame does not
call `queue.write_buffer` again when the binary value is unchanged.
`SceneFrameMetrics::uniform_uploads_skipped` measures this reuse. This is a
partial improvement; it does not replace a baseline or a dynamic instance
buffer. The investigation must compare:

- draw count against object count;
- `uniform_upload_bytes` and `uniform_uploads_skipped` per frame;
- CPU encode cost and GPU cost;
- repeated meshes that cannot batch because of color, transparency, or order;
- singleton, batch, and instanced paths.

The first candidate is grouping instance data and using reusable slots per
material/pipeline while preserving the ordering required by transparency and
the CPU fallback. The renderer must not be replaced wholesale, and a large
material abstraction must not be introduced without evidence that it reduces
work.

### OC-03 - Registry and budgets by resource family [PARTIAL]

Audit meshes, images, text atlas, UI buffers, CAD, and staging under one common
table containing:

- estimated bytes;
- owner and revision;
- last use;
- pinned/transient state;
- potato/desktop limit;
- eviction strategy;
- uploaded and released bytes.

This is partially implemented: the mesh registry maintains a byte budget and
LRU eviction, and the RafUI GPU image cache maintains byte/entry budgets,
protects images used in the current frame, and reports evictions. A global
ledger shared by meshes, UI, atlas, CAD, and staging is still missing; current
policies are owner-specific.

Eviction must be incremental. Never clear an entire cache for a small overflow
when an unpinned LRU resource can be evicted. Transient edit resources must stay
outside persistent residency.

**Current code:**

- `crates/raf_render/src/ApiGraphicBasic/resource_registry.rs`
- `crates/raf_render/src/ApiGraphicBasic/device.rs`
- `crates/raf_render/src/ApiGraphicBasic/ui_surface/gpu_renderer.rs`

### OC-04 - Batching across surfaces [MEASURE_FIRST]

The entire compositor must not be fused by default. First measure whether the
dominant cost is pipeline changes, uploads, binds, or the number of surfaces.
Cross-surface batching is considered only if it preserves:

- layer order;
- clipping and overlays outside the owner's clip;
- hit testing and input ownership;
- separation of Game/CAD canvases and UI;
- CPU/GPU parity;
- independent invalidation of each surface.

If those properties cannot be preserved, optimize runs inside each surface and
do not create an artificial global batch.

### OC-05 - Drag under pressure [MEASURE_FIRST]

During a gesture, observe CPU, GPU, uploads, and input latency separately. The
possible policy, in order, is:

1. Reuse the canvas when only the overlay changes.
2. Temporarily reduce source resolution when GPU pressure exceeds the budget.
3. Preserve selection and hover while degrading secondary details.
4. Restore quality with hysteresis to prevent oscillation.

Labels, gizmos, and required feedback must not be hidden without a visual
reason and a metric proving they are the bottleneck.

### OC-06 - Measurable startup phases [MEASURE_FIRST]

Add phase measurements around the existing path:

```text
process started
  -> window and device
  -> loading presented
  -> project/configuration
  -> deferred surfaces
  -> first Hub/Workbench frame
```

Disk work, discovery, and decoding that can leave the UI thread must do so
without changing project ownership or creating a second source of truth. The
loading screen must represent real phases, not an artificial delay.

## Non-regression rules

- ApiGraphicBasic remains the public owner; WGPU does not leak into RafUI,
  editor, domain, or asset layers.
- An optimization must not create a second editor implementation.
- The CPU fallback must preserve scene, CAD, and UI semantics.
- Every cache must have an explicit key, limit, invalidation policy, and
  eviction policy.
- Animation or streaming may activate frames only while real work exists.
- A text change must not invalidate layout when it only requires paint-only
  work.
- Transparency must not be batched when back-to-front order would break.
- Transient edit meshes must not contaminate persistent caches.
- Prepared effects (PBR, shadows, post-processing, particles, and skeletal
  animation) are not activated as part of this campaign without a budget and
  measurement.
- No FPS gain may be claimed from the displayed counter: a before/after
  comparison under the same load is required.

## Minimum tests per change

Every change in this campaign must pass, at minimum:

1. `cargo fmt --all -- --check`.
2. `cargo check` for the modified crate and the boundary that consumes its API.
3. Focused unit tests for the scheduler, cache, batching, atlas, or registry,
   depending on the touched area.
4. A comparison of affected use cases against the previous metrics.
5. A minimal manual test when the change affects the window, DPI, input,
   visuals, resize, loading, or GPU presentation.

Crate tests alone do not prove native-window visual acceptance. A measurement
without a valid GPU timestamp must report that limitation and must not interpret
`GPU --` as zero cost.

## Hot-path map

```text
NativeEditorRuntime
  -> FrameScheduler / DynamicResolutionController
  -> SceneRenderer or UiSurfaceCompilationCache
  -> BasicCommandList / UiSurfaceDrawList
  -> BasicDevice / UiSurfaceGpuRenderer
  -> buffers, atlas, registry, uploads, and presentation
```

Before changing a point, identify whether the work is in:

- document preparation;
- RafUI layout/hit-test/paint;
- atlas or images;
- command construction;
- cache and residency;
- upload/bind/uniform;
- draw/resolve/present;
- an accidental wait or synchronization.

Optimization is complete when the use case preserves quality, input, and
semantics, reduces measurable work, and does not move the cost to another
frame without making it visible in metrics.

## Current pass status

The following points were implemented in the modern path during the September
14, 2026 pass:

- Mesh, line, and overlay uniform reuse with a skipped-upload counter.
- Retained RafUI geometry and `paint_runs` cache without hashing every quad on
  every idle frame.
- Monotonic draw-list revisions so the cache cannot mistake a reused memory
  address for the previous geometry.
- GPU image budget by bytes and entries, incremental LRU eviction, mipmaps, and
  protection for images used by the current frame.
- Metrics for residency, uploads, evictions, cache hits, and budget overflow.

Budget implementation remains intentionally partial: meshes and images have
independent enforcement, but the global ledger and cross-family eviction require
measurements from a real scene before priorities are chosen. Cross-surface
batching and drag-detail degradation remain `MEASURE_FIRST`; they are not
enabled by intuition because they could break order, clipping, or input
feedback.

Automated evidence for this pass:

1. `cargo test -p raf_render --lib --target-dir target_agent_validation` - 241
   tests passed.
2. `cargo check -p raf_editor --lib --target-dir target_agent_validation` -
   passed.
3. `cargo build -p aura_rafi_editor --target-dir target_agent_validation` -
   native executable linked successfully.
4. Executable smoke test - process stayed alive for eight seconds and was then
   closed by validation; no immediate crash was observed.

These checks do not replace manual testing with a window, GPU, DPI, scrolling,
resizing, and loaded scenes. No concrete FPS gain is claimed until that testing
is performed.
