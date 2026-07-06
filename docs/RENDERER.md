# AuraRafi Active Scene Viewport Renderer - Technical Reference

> Version v0.9.0 - Scene viewport path inside the shared graphics runtime

## Overview

The editor now uses a shared graphics runtime for Scene, Schematic, and PCB
surfaces. `RenderRuntime` owns the active surface and graphics-device policy,
`ApiGraphicBasic::BasicDevice` prefers GPU hardware execution when available,
and falls back to CPU software rendering when it is not.

This document focuses on the scene viewport path driven by `ViewportBridge`
and `SceneRenderer`. That path remains the reference CPU scene implementation
inside the shared graphics runtime, while the active presentation result can be
either a native GPU texture or CPU RGBA pixels depending on the selected
backend.

Design axiom: scene in, camera in, frame out, then present through the shared graphics runtime.

## Canonical Active Path

Today the active editor surfaces follow one graphics contract:

```
SceneViewport  -> ViewportBridge -> SceneRenderer -> RenderRuntime -> BasicDevice
SchematicCanvas ---------------------------------> RenderRuntime -> BasicDevice
PcbCanvas ---------------------------------------> RenderRuntime -> BasicDevice

BasicDevice -> GPU hardware when available
BasicDevice -> CPU software fallback otherwise
```

This means:

- Scene, Schematic, and PCB all live under the same graphics-device policy.
- GPU hardware execution is preferred, but CPU fallback is still part of the
  product contract for low-end setups.
- Advanced modules such as `RenderBackendTrait`, `scene_data`, `world_stream`,
  ray tracing, and related infrastructure remain prepared systems rather than
  the primary active path today.

## Surface Contract

Phase 7 freezes one minimum contract for the active editor surfaces:

- Scene, Schematic, and PCB must all route presentation through `RenderRuntime`
  and `ApiGraphicBasic::BasicDevice`.
- Backend choice is shared policy, not per-panel ad hoc behavior.
- GPU hardware may optimize submission and resource lifetime, but must not
  silently drop CPU fallback support.
- CPU fallback remains the reference path for correctness checks when GPU and
  fallback behavior diverge.
- Transient tooling paths such as editable mesh overrides may stay uncached,
  but they must still execute through the same runtime contract.

## Architecture Layers

```
  raf_editor  -  viewport.rs (editor shell)
    viewport_hud.rs          toolbar / HUD
    viewport_interaction.rs  click/drag routing
    viewport_overlay.rs      gizmo / labels / vertex dots
    viewport_grid.rs         grid drawing
  -----------------------------------------------
  raf_render  -  bridge/
    viewport_bridge.rs       camera + render orchestration
    render_runtime.rs        shared graphics runtime / surface activation
  -----------------------------------------------
  raf_render  -  ApiGraphicBasic/
    device.rs                GPU-first execution + CPU fallback presentation
  -----------------------------------------------
  raf_render  -  scene_renderer.rs
    geometry/                MeshData + primitive constructors
    math/                    transform, frustum, ray
    render_pipeline/         framebuffer + scanline rasterizer
```

Separation of concerns:

| Layer | Knows egui? | Mutates scene? | Role |
|-------|:-:|:-:|------|
| Editor shell (`viewport.rs`) | Yes | No | Layout, input routing, final presentation |
| Bridge (`viewport_bridge.rs`) | No | via controllers | Camera state, edit-session state, scene-view orchestration |
| Runtime (`render_runtime.rs` + `device.rs`) | No | No | Backend selection, GPU presentation, CPU fallback execution |
| Scene renderer (`scene_renderer.rs`) | No | No | Scene viewport frame construction and CPU reference path |

## Scene Viewport CPU Reference Pipeline

The steps below describe the scene viewport CPU reference path. This is the
path that remains easiest to reason about when validating depth correctness,
selection overlays, and fallback behavior.

1. Frustum cull: discard entities outside the 6-plane frustum.
2. Collect `RenderJob`s: primitive, model matrix, color, transparency flag.
3. Sort: opaques front-to-back, transparents back-to-front.
4. Per triangle:
   a. MVP transform to clip space.
   b. Near-plane clip.
   c. Perspective divide to NDC.
   d. Viewport transform to screen coordinates.
   e. Flat shading via transformed normals and light direction.
   f. Scanline rasterization with Z-buffer depth test.
5. Wireframe overlay when required by mode or selection.
6. Delegate final presentation through the shared graphics runtime.

### Key Mathematics

| Operation | Formula |
|-----------|---------|
| Model matrix | `scene.world_matrix(id)` walks parent chain |
| View matrix | `Mat4::look_at_rh(eye, target, up)` |
| Projection (persp) | `Mat4::perspective_rh(fov_rad, aspect, near, far)` |
| Projection (ortho) | `Mat4::orthographic_rh(-hw, hw, -hh, hh, near, far)` |
| MVP | `projection * view * model` |
| Screen X | `(ndc_x + 1) * 0.5 * width` |
| Screen Y | `(1 - ndc_y) * 0.5 * height` |
| Depth Z | `(ndc_z + 1) * 0.5` normalized to [0,1] |
| Flat shade | `ambient + diffuse * max(0, dot(worldNormal, lightDir))` |

## Render Modes

| Mode | Fill | Wire | Use case |
|------|:-:|:-:|---------|
| Solid | Yes | selected only | Default editing |
| Wireframe | No | all | Topology inspection |
| Preview | Yes | all | Final look + structure |

`RenderOptions` also controls `solid_show_surface_edges`, `solid_xray_mode`,
`solid_face_tonality`, `selection_outline`, and `selection_outline_color`.

## Transparency

Opaques render first front-to-back. Transparents render after back-to-front
with src-over alpha compositing. Separate opaque and blended paths avoid extra
branching in the hot path.

## Bridge Layer (`raf_render::bridge`)

### ViewportBridge

Owns camera state, `SceneRenderer`, `ViewportEditSession`, and
`ViewportTransformController`.

| Method | Purpose |
|--------|---------|
| `handle_camera_input()` | Orbit, pan, zoom from pointer input |
| `update_camera()` | Recompute position from orbit parameters |
| `render()` | Build the scene viewport frame and delegate to the shared graphics runtime |
| `pick_entity()` | Ray-sphere broad phase + ray-triangle narrow phase |
| `begin/apply/end_transform_drag()` | Gizmo translate/rotate/scale |
| `begin_edit_drag()` / `drag_selected_vertices()` | Vertex editing |
| `project_edit_overlay()` | Projected edges + vertices for egui |

### ViewportTransformController

Gizmo state plus drag lifecycle. Projects mouse delta onto the active axis in
screen space, scaling world-space response by orbit distance.

### ViewportEditSession

Per-entity `EditableMesh` state:

- Vertex picking via screen-space projection with a 10px threshold.
- Vertex dragging: world delta -> inverse rotation -> local space.
- Mesh override for renderer so edited topology replaces cached primitive data.
- Entity picking via ray-sphere broad phase + ray-triangle narrow phase.

## Editor Shell (`viewport.rs`)

Thin shell that:

1. Allocates the egui rect.
2. Delegates camera input to the bridge.
3. Calls `bridge.render()`.
4. Presents a GPU texture or uploads CPU pixels.
5. Draws overlays and HUD.
6. Routes interaction state back into the scene/edit session.

Sub-modules:

| File | Responsibility |
|------|----------------|
| `viewport_hud.rs` | Toolbar, 2D/3D toggle, OBJ/VTX badge, info pill, axis gizmo |
| `viewport_interaction.rs` | Object/edit mode input, gizmo drag, entity pick, shortcuts |
| `viewport_overlay.rs` | Labels, gizmo arrows/rings/cubes, vertex dots/edges |
| `viewport_grid.rs` | 2D and 3D grid drawing |

`overlay_blocks_world_input()` prevents HUD clicks from leaking into world
picking.

## Gizmo System

Data: `GizmoMode` (Translate/Scale/Rotate), `GizmoAxis` (None/X/Y/Z).

Geometry and behavior:

- Translate gizmo: 3 arrows (X red, Y green, Z blue), 1.2 world units.
- Rotation gizmo: 48-segment rings, 12px hit threshold.
- Scale gizmo: face-based handles projected in screen space.
- Drag math: project axis to screen, project mouse delta onto that axis, scale
  by `orbit_distance / (min(vp_w, vp_h) * 0.5)`.

## Camera

Perspective and orthographic modes are both supported. The bridge manages orbit
parameters (`yaw`, `pitch`, `distance`) and derives position from target plus a
spherical offset.

## CPU Fallback Performance

The metrics below describe the CPU fallback path specifically.

| Metric | Value |
|--------|-------|
| Pixel format | RGBA u8 (4 bytes/pixel) |
| Depth buffer | f32 per pixel |
| Memory per frame | `width * height * 8` bytes (color + depth) |
| Allocation | Framebuffer reused across frames |
| Mesh caching | One `MeshData` per primitive type, never regenerated |

## Hot Path Measurement Baseline

Phase 7 uses one practical measurement loop instead of vague profiler talk.

Reference surfaces:

- Scene viewport in 3D with solids plus wire overlays.
- Schematic canvas with dense wire and component overlays.
- PCB canvas with board fill, traces, airwires, and component bodies.

Reference metrics:

- Viewport HUD `R`: total panel-side render cost seen by the editor shell.
- Viewport HUD `U`: CPU upload cost when presentation falls back to pixel upload.
- Viewport HUD `G`: backend-side frame submission cost reported by the shared graphics runtime.
- Viewport HUD `M hit/miss`: shared mesh GPU cache reuse versus new mesh uploads.
- Viewport HUD `L`: line draw count for the active frame.

Phase 7 hot-path outcome:

- Shared meshes now reuse GPU vertex/index buffers instead of recreating them every frame.
- Mesh uniforms and line resources now grow into reusable per-draw slots instead of allocating per draw.
- Editable or otherwise transient meshes can still bypass long-lived caching, so edit-mode correctness is preserved.

## File Map

```
crates/raf_render/src/
  bridge/
    mod.rs                    exports
    viewport_bridge.rs        camera + render orchestration
    render_runtime.rs         shared graphics runtime / backend activation
    input_handler.rs          picking + vertex edit session
    transform_controller.rs   gizmo drag lifecycle
  ApiGraphicBasic/
    device.rs                 GPU-first device execution + CPU fallback
    command_list.rs           graphics command recording
    mesh.rs                   shared mesh container for GPU/CPU execution
    pipeline.rs               simple pipeline kinds
  geometry/
    mesh_data.rs              MeshData struct
    primitives.rs             cube/cylinder/sphere/plane
  math/
    transform.rs              MVP, project_point, screen_to_world_ray
    frustum.rs                6-plane frustum culling
    ray.rs                    Ray, ray_sphere, ray_triangle
  render_pipeline/
    framebuffer.rs            RGBA + depth buffer
    rasterizer.rs             scanline fill, line draw, blended path
  scene_renderer.rs           scene viewport frame construction / CPU reference path
  camera.rs                   perspective + orthographic
  gizmo.rs                    gizmo data model + hit test
  picking.rs                  entity picking + gizmo projection
  editable.rs                 EditableMesh + vertex ops

crates/raf_editor/src/panels/
  viewport.rs                 thin editor shell
  viewport_hud.rs             toolbar + info + axis gizmo
  viewport_interaction.rs     object/edit mode input
  viewport_overlay.rs         gizmo drawing + vertex overlay
  viewport_grid.rs            2D/3D grid
```

## Old vs New

| Aspect | Old viewport | Current path |
|--------|--------------|--------------|
| Structure | Single large file | Modular shell + bridge + runtime + scene path |
| Surface contract | Viewport-only assumptions | Shared graphics runtime across Scene/Schematic/PCB |
| Depth | Approximate sort order | Per-pixel exact in CPU reference path |
| Transparency | Limited | Sorted blended path |
| Gizmo drag | Inline in viewport | Dedicated transform controller |
| Picking | Screen-space distance | Ray-sphere + ray-triangle |
| Presentation | CPU image upload only | GPU texture or CPU pixel upload |
| egui in renderer | Mixed | Isolated to editor shell |
