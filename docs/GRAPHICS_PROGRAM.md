# Graphics Program

This document is the working contract for AuraRafi's graphics ownership,
backend lanes, frame graph, and the v0.12 rendering features. It replaces
implied promises with measurable gates.

## 1. Purpose

AuraRafi owns its graphics contract. The public contract is ApiGraphicBasic;
backend APIs, command encoders, resource registries, and presentation lifecycles
belong below it.

The program has three outcomes:

1. Keep one graphics owner while WGPU remains the active private executor.
2. Add native backend lanes in v0.11.5 without changing Scene, CAD, RafUI, or
   asset-facing contracts.
3. Add PBR, SSAO, point/spot lights, particles, and skeletal skinning in v0.12
   through the same frame graph and budget system.

This program does not promise parity with any external engine. The target is
the measured contract in section 4 and parity between AuraRafi's own GPU and CPU
recovery paths.

## 2. Ownership path

```text
Scene viewport | Schematic | PCB | RafUI | Asset ingress
                              |
                       RenderRuntime
                              |
                      ApiGraphicBasic
              (public contract, budgets, lifecycle)
                              |
          private executor: WGPU today, native lanes later
                              |
                  CPU recovery / headless fallback
```

| Layer | Owns | Must not own |
| --- | --- | --- |
| Domain | scene, CAD document, UI document, asset metadata | GPU handles, queues, shaders |
| Surface host | input boundary, size, typed actions, repaint request | backend resource lifetime |
| RenderRuntime | execution policy, surface registration, recovery choice | domain mutations |
| ApiGraphicBasic | resources, command lists, frame graph, device/frame lifecycle, presentation | editor routing, scene/CAD rules |
| Executor | backend calls and synchronization | public document or editor API |

Rules:

- One executor is selected for a device and surface. A frame never mixes
  backend resources.
- Upper layers never import WGPU types. The remaining `from_wgpu`/`as_wgpu`
  seams in ApiGraphicBasic are private executor adapters, not a second public
  API.
- A backend lane graduates capability by capability. It does not start with a
  big-bang renderer rewrite.
- Disabled features have zero recurring frame cost.
- A native lane that only opens a window is not a qualified backend.

## 3. Current baseline (2026-11-19)

- WGPU 23 is the active GPU executor behind ApiGraphicBasic.
- `BasicDevice` carries adapter preference, backend-neutral capabilities,
  generational handles, and `GraphicsMemoryBudget`.
- CPU recovery produces RGBA frames without GPU access.
- The cross-family memory ledger budgets buffer, texture, sampler, pipeline,
  mesh, material, staging and transient families, and feeds the editor HUD.
- RafUI compiles one `UiSurfaceDrawList` per invalidated frame and batches
  adjacent compatible paint work.
- Scene rendering has bounded mesh residency, line batching, upload budgets,
  and `SceneFrameMetrics` for CPU/GPU frame time, draw calls, uploads, evictions,
  and budget overruns.
- Texture decode and CPU residency now pass through `raf_assets::ingress` and
  `ApiGraphicBasic::asset_ingress`; `SceneRenderer` no longer decodes files ad
  hoc. GPU upload remains inside `device.rs`.
- Surface loss (`Lost`/`Outdated`/`Timeout`) reconfigures and retries once;
  `OutOfMemory` is fatal. Device recreation is still open.
- Headless capture and the one-client streaming host are active and measured.
- The directional shadow pass is opt-in and budgeted. It is not a general
  shadow system.
- A prepared backend-neutral frame graph exists in
  `crates/raf_render/src/ApiGraphicBasic/frame_graph.rs`; see
  [FRAME_GRAPH.md](FRAME_GRAPH.md). It is not wired into `BasicDevice` yet, so
  the active scene path remains single-pass.
- There is no native backend lane yet. That is v0.11.5 work.

### Boundary status

WGPU is still the only GPU executor, and the boundary is not sealed yet. The
remaining direct `wgpu::` consumers are concentrated in:

| File | Role |
| --- | --- |
| `ApiGraphicBasic/device.rs` | neutral device plus WGPU scene execution and pipelines |
| `ui_surface/gpu_renderer.rs` | RafUI's WGPU renderer |
| `canvas_presenter.rs` | WGPU surface blit |
| `ui_surface/native_window.rs` | WGPU instance/adapter/device/surface creation |
| `shadow_gpu.rs`, `environment_gpu.rs`, `optimized_filter_gpu.rs` | scene sub-passes inside the device |
| `editor_compositor.rs` | WGPU layer composition |

The target layout is:

```text
ApiGraphicBasic/
  mod.rs                 public contract only
  device.rs              neutral device + dispatch
  frame_graph.rs
  command_list.rs
  asset_ingress.rs
  memory_ledger.rs
  handles.rs
  capabilities.rs
  backends/
    wgpu/                the only place that imports wgpu
    cpu/
    null/
    vulkan/              v0.11.5+
    dx12/                v0.11.5+
    metal/               v0.11.5+
```

The hard rule for the migration: no file outside `backends/` imports `wgpu`.
Until that is true, adding a native backend would also require duplicating RafUI,
the compositor and the surface host.

Current budgets:

| Budget | Potato | Desktop |
| --- | --- | --- |
| GPU residency | 256 MiB | 1 GiB |
| Staging | 16 MiB | 64 MiB |
| Frame upload | 8 MiB | 32 MiB |
| Target frame | 33.3 ms | 16.6 ms |
| Mesh cache entries | 4096 | 16384 |
| Texture cache entries | 512 | 2048 |

These are contracts, not measured results for every machine. A budget overrun
must be reported and degraded, never ignored.

## 4. Quality target

Quality means measured behavior on named fixtures, not a screenshot or a
single FPS reading. Every claim needs the fixture, resolution, adapter, driver,
CPU, backend, and raw samples.

### Fixtures

At minimum:

- `static_scene`: saved project, camera at rest, no animation or simulation.
- `active_scene`: moving camera, scripted animation, physics contacts, UI.
- `ui_heavy`: retained RafUI surfaces, text atlas churn, menus, tooltips.
- `asset_scene`: repeated meshes, textured materials, bounded asset streaming.
- `cpu_recovery`: the same scenes with GPU disabled.

Fixtures live in a committed example project once the headless fixture path
exists. Until then, measurements must state which project and revision were
used and must not be generalized.

### Gates

| Metric | Initial gate | Evidence |
| --- | --- | --- |
| CPU frame time p95 | <= 33.3 ms potato, <= 16.6 ms desktop on `static_scene` at 1920x1080 | per-frame samples, p50/p95/p99 |
| CPU frame time p99 | <= 2x p95 after warmup | histogram |
| GPU frame time p95 | <= target frame time when GPU timing is supported | timestamp queries or explicit unsupported |
| Hitches | no frame above 50 ms after warmup on `static_scene` | frame log |
| Uploads | <= 8 MiB/frame potato, <= 32 MiB/frame desktop; 0 in steady state for unchanged assets | `SceneFrameMetrics` bytes |
| Draw calls | <= 300 total for `static_scene`; <= 64 for `ui_heavy`; no growth over 300 frames | backend counters |
| Resident memory | within `gpu_bytes`; ledger reconciliation error <= 1% | ledger plus driver-reported allocation |
| Idle work | 0 rendered frames, 0 uploads, 0 simulation ticks after idle settles | idle trace |
| Load time | manifest validation <= 1 s; first presented frame <= 5 s for the baseline fixture | timestamped startup trace |
| Pacing | desktop targets 60 Hz, potato targets 30 Hz; resolution changes use hysteresis | frame scheduler trace |
| Soak | 10 minutes without unbounded RSS or VRAM growth | sampled process and ledger data |

These are initial gates. If a gate is unreachable on the reference hardware, the
program records the measured value and the reason instead of moving the
definition of success.

Allowed idle polling: the existing local runtime may poll status at 4 Hz while
static or paused. That polling must not render, upload, or rebuild the scene.

## 5. v0.11.5 frame graph and backend lanes

### Frame graph

The frame graph is an ordered, backend-neutral description of passes and
transient resources. The prepared contract is documented in
[FRAME_GRAPH.md](FRAME_GRAPH.md) and implemented in
`crates/raf_render/src/ApiGraphicBasic/frame_graph.rs`. It owns:

- pass order and dependencies;
- transient resource declarations and aliasing;
- explicit resource states;
- barriers and queue ownership transitions;
- per-pass budget admission;
- a single submit and present/readback decision per frame.

Prepared limits:

- 4096 resources, 1024 passes, and 128 accesses per pass;
- a 256 MiB default transient budget, configurable through the graph. The caller
  must bind this to the active `GraphicsMemoryBudget`; the default is not a
  promise that potato can allocate 256 MiB of transients beside resident
  resources;
- imported resources are never aliased; transient resources alias only when
  format, sample count, dimensions, mip count, and usage are compatible;
- a failed compile clears the output so no partial frame reaches a backend;
- graph validation rejects cycles, missing producers, duplicate writes,
  unused transients, and budget overruns.

Initial passes:

```text
shadow (optional) -> depth prepass (optional) -> opaque -> sky
                  -> transparent -> post -> RafUI composite -> present
```

Initial transient resources:

- depth target;
- normal target only when SSAO or normal-dependent effects are enabled;
- half-resolution AO target and blur scratch;
- post-processing scratch;
- shadow depth map, retained only while shadow quality is active.

Rules:

- A pass that is disabled is not encoded and allocates nothing.
- A transient resource can alias another only when lifetimes do not overlap and
  the state transition is explicit.
- The graph must be able to run on the CPU recovery path with the same logical
  passes, even if the CPU implementation collapses them internally.
- Diagnostics report per-pass CPU/GPU time when the backend supports it, plus
  admission denials.

### Native backend lanes

Each platform lane is a private executor behind ApiGraphicBasic. The initial
targets are Vulkan on Linux and the platform-native API on Windows. A lane is
selected only after it passes the qualification gates below.

Qualification gates:

- Scene, Schematic, PCB, and RafUI parity on the committed fixtures.
- CPU recovery remains available for the same session.
- Resize, minimize, suspend/resume, surface loss, and device loss recovery.
- Integrated and dedicated GPU runs.
- Memory ledger, cache eviction, and upload budgets hold.
- Idle-zero-work and sustained-session measurements hold.
- Shader/pipeline cache behavior is compatible.
- No backend type or assumption leaks into upper layers.

WGPU remains the default executor until every gate passes for a lane. A native
lane may run in shadow mode for measurement before it becomes selectable.

## 6. v0.12 feature plug-in

Each feature enters the frame graph as a bounded pass or a bounded branch inside
an existing pass. None of them creates a second renderer.

### PBR materials

- Material descriptor: base color, metallic, roughness, normal, emissive,
  occlusion, alpha mode, double-sided flag, sampler state, and texture handles.
- Textures enter through ApiGraphicBasic asset ingress, never as raw backend
  handles.
- Opaque PBR renders in the forward opaque pass with a per-material bind group.
  Alpha-tested and transparent materials remain separate bounded branches.
- CPU recovery evaluates the same material descriptor with bounded texture
  sampling; it is not required to match GPU filtering bit-for-bit.
- Material residency is budgeted and evicted by the texture registry.

### SSAO

- Requires the depth prepass and a normal target.
- AO is evaluated at half resolution, then blurred and composited.
- Potato disables SSAO. Desktop enables it only when the adapter and budget
  admit it.
- SSAO is not global illumination and is not advertised as physically correct
  occlusion.
- The AO target and blur scratch are transient frame-graph resources.

### Point and spot lights

- Lights are scene capabilities with Inspector, scripting, and automation
  schemas.
- The renderer receives one bounded light buffer per frame.
- The forward opaque pass evaluates a bounded number of nearest lights; excess
  lights are reported, not silently dropped.
- Directional sun shadows remain the only shadow path until a dedicated
  per-light shadow budget is approved.
- CPU recovery uses a smaller light cap and reports the reduced quality.

### Particles

- Particle state is authored and simulated by the runtime, with deterministic
  seeds where required.
- Rendering uses bounded instance buffers and one draw per compatible particle
  group.
- The first implementation is CPU simulation plus GPU instancing; a compute
  path is optional and must not become a hard requirement.
- Disabled particles allocate nothing and submit nothing.

### Skeletal skinning

- GLB/GLTF import supplies meshes, skins, joints, inverse bind matrices, and
  validated clips.
- GPU skinning uses a bounded joint palette. CPU recovery deforms the same
  vertices from the same joint data.
- The runtime owns the animation clock and sampling; the renderer receives a
  deformed mesh contract, not a second animation model.
- Skinning data participates in mesh residency and upload budgets.

Feature admission order:

1. Contract and validation.
2. CPU/GPU parity fixture.
3. Budget admission and degraded mode.
4. Diagnostics and evidence.
5. Product exposure.

## 7. Performance rules for a secondary desktop process

AuraRafi is often a companion to other creative work. It must behave like a
well-behaved secondary process:

- Idle means no rendered frames, no uploads, and no simulation work after the
  scene settles. Status polling is bounded and paint-free.
- Redraw is event-driven: input, animation, simulation, explicit capture, or a
  changed document. A timer alone is not a reason to render.
- Frame work is budgeted before submission. If the budget is exceeded, degrade
  resolution, effect quality, or optional passes in a documented order.
- No synchronous GPU readback on the UI thread. Diagnostics use bounded
  asynchronous readback rings.
- No unbounded caches. Every cache has a byte or entry budget and a reported
  eviction path.
- Uploads are coalesced. Unchanged meshes, textures, and uniforms are not
  uploaded again.
- Background or unfocused sessions may reduce target FPS, but must not stop
  input handling or corrupt simulation time.
- Do not busy-wait for a frame, a fence, or a device event.
- A hidden or minimized surface does not render or allocate merely because it
  exists.
- Measure CPU time, GPU time, uploads, draws, residency, idle behavior, and load
  time together. One number is not evidence.

## 8. Milestones and evidence

### M0: Baseline harness

- Commit the fixture set and a metrics schema.
- Capture CPU and GPU baselines for each backend and resolution.
- Record the reference machine and driver.
- No renderer behavior change is claimed.

### M1: Frame graph under WGPU

- Wire the prepared backend-neutral pass and transient-resource contract into
  `BasicDevice` without exposing backend types upward.
- Port shadow, scene, post, and UI composition to the graph.
- Prove parity against the M0 captures.
- Prove aliasing saves memory without changing output.

### M2: First native lane in shadow mode

- Implement one platform lane behind ApiGraphicBasic.
- Run it in shadow mode beside WGPU on the same fixtures.
- Report per-pass timing, memory, and parity.
- WGPU remains the selected executor.

### M3: Native lane qualification

- Pass device-loss, surface-loss, suspend/resume, integrated/dedicated, and
  soak gates.
- Promote the lane for its platform only after all gates pass.
- Keep the WGPU lane available until removal gates are satisfied.

### M4: v0.12 rendering features

- PBR, SSAO, point/spot lights, particles, and skinning enter one at a time.
- Each feature ships with a disabled path that costs nothing, a budget, a
  degraded mode, and CPU/GPU parity evidence.

Evidence format:

```json
{
  "fixture": "static_scene",
  "revision": "0.11.5",
  "backend": "principal",
  "adapter": "reported name",
  "driver": "reported version",
  "resolution": [1920, 1080],
  "frames": 600,
  "cpu_frame_ms": {"p50": 0.0, "p95": 0.0, "p99": 0.0},
  "gpu_frame_ms": {"p50": 0.0, "p95": 0.0, "p99": 0.0, "supported": true},
  "uploads_bytes_per_frame": 0,
  "draw_calls": 0,
  "resident_bytes": 0,
  "idle_frames": 0,
  "load_ms": 0
}
```

Evidence belongs in the artifact directory or release record, not in a prose
claim. Missing values are reported as missing, never as zero.

## 9. Non-goals

- No parity promise with any external engine.
- No removal of WGPU before the qualification and removal gates pass.
- No second renderer for editor panels, CAD, RafUI, or gameplay.
- No frame graph feature that only works on one backend and leaks that backend
  into upper layers.
- No advanced effect enabled by default on potato hardware.
- No performance claim based only on the visible FPS counter.
