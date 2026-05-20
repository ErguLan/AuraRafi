# AuraRafi CPU Renderer - Technical Reference

> Version v0.9.0 - CPU-only Z-buffer - Zero GPU dependencies

## Overview

The renderer is a fully CPU-based, Z-buffered scanline rasterizer. Its only
output is a `&[u8]` RGBA pixel buffer that the editor uploads to an egui texture.

Design axiom: Scene in, camera in, pixels out.

## Architecture Layers

```
  raf_editor  -  viewport.rs (editor shell)
    viewport_hud.rs        toolbar / HUD
    viewport_interaction.rs  click/drag routing
    viewport_overlay.rs    gizmo / labels / vertex dots
    viewport_grid.rs       grid drawing
  -----------------------------------------------
  raf_render  -  bridge/
    viewport_bridge.rs     camera + render orchestration
    input_handler.rs       picking + vertex edit session
    transform_controller.rs  gizmo drag lifecycle
  -----------------------------------------------
  raf_render  -  scene_renderer.rs (pixel production)
    geometry/              MeshData + primitive constructors
    math/                  transform, frustum, ray
    render_pipeline/       framebuffer + scanline rasterizer
```

Separation of concerns:

| Layer | Knows egui? | Mutates scene? | Produces pixels? |
|-------|:-:|:-:|:-:|
| Editor shell (viewport.rs) | Yes | No | No |
| Bridge (viewport_bridge.rs) | No | via controllers | delegates |
| Renderer (scene_renderer.rs) | No | No | Yes |

## Render Pipeline (per frame)

1. Frustum cull - discard entities outside the 6-plane frustum
2. Collect RenderJobs - primitive, model matrix, color, transparency flag
3. Sort - opaques front-to-back (early Z), transparents back-to-front
4. Per-triangle:
   a. MVP transform to clip space (Vec4)
   b. Near-plane clip (w <= 0.001 = cull)
   c. Perspective divide to NDC (x/w, y/w, z/w)
   d. Viewport transform to screen coords (Y-down)
   e. Flat shading: 0.3 + 0.7 * max(0, dot(normal, lightDir))
   f. Scanline rasterize with Z-buffer depth test
5. Wireframe overlay (mode-dependent)
6. Output: framebuffer.pixels() -> &[u8] RGBA

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

RenderOptions also controls: solid_show_surface_edges, solid_xray_mode,
solid_face_tonality, selection_outline, selection_outline_color.

## Transparency

Opaques rendered first (front-to-back for early Z). Transparents rendered
after (back-to-front with src-over alpha compositing via blend_pixel).
Two separate rasterization paths avoid branching on the hot path.

## Bridge Layer (raf_render::bridge)

### ViewportBridge

Owns camera state, SceneRenderer, ViewportEditSession, and
ViewportTransformController.

| Method | Purpose |
|--------|---------|
| handle_camera_input() | orbit, pan, zoom from pointer input |
| update_camera() | recompute position from orbit params |
| render() | full pipeline, returns &[u8] |
| pick_entity() | ray-sphere broad + ray-triangle narrow |
| begin/apply/end_transform_drag() | gizmo translate/rotate/scale |
| begin_edit_drag() / drag_selected_vertices() | vertex editing |
| project_edit_overlay() | projected edges + vertices for egui |

### ViewportTransformController

Gizmo state + drag lifecycle. Projects mouse delta onto active axis
in screen space, scales to world units via orbit distance.

### ViewportEditSession

Per-entity EditableMesh state (HashMap<SceneNodeId, EditableMesh>):
- Vertex picking via screen-space projection (10px threshold)
- Vertex dragging: world delta -> inverse rotation -> local space
- Mesh override for renderer (edited mesh replaces cached primitive)
- Entity picking: ray-sphere broad phase + ray-triangle narrow phase

## Editor Shell (viewport.rs ~302 lines)

Thin shell that: allocates egui rect -> delegates camera input to bridge ->
calls bridge.render() -> uploads image -> draws overlays -> draws HUD ->
routes input.

| Sub-module | Responsibility |
|------------|---------------|
| viewport_hud.rs | Toolbar (G/R/S/F), 2D/3D toggle, OBJ/VTX badge, info pill, axis gizmo |
| viewport_interaction.rs | Object/edit mode input, gizmo drag, entity pick, shortcuts |
| viewport_overlay.rs | Labels, gizmo arrows/rings/cubes, vertex dots/edges |
| viewport_grid.rs | 2D and 3D grid drawing |

overlay_blocks_world_input() prevents clicks on HUD elements from reaching
world picking.

## Gizmo System

Data: GizmoMode (Translate/Scale/Rotate), GizmoAxis (None/X/Y/Z).
Geometry: 3 arrows (X red, Y green, Z blue), 1.2 world units, 8px hit
threshold. Rotation rings: 48 segments, 12px threshold.
Drag math: project axis to screen, project mouse delta onto axis, scale by
orbit_distance / (min(vp_w, vp_h) * 0.5).

## Camera

Perspective and Orthographic modes. Bridge manages orbit parameters
(yaw, pitch, distance) and derives position from target + spherical offset.

## Performance

| Metric | Value |
|--------|-------|
| Pixel format | RGBA u8 (4 bytes/pixel) |
| Depth buffer | f32 per pixel |
| Memory per frame | width * height * 8 bytes (color + depth) |
| Allocation | Framebuffer reused across frames |
| Mesh caching | One MeshData per primitive type, never re-generated |

## File Map

```
crates/raf_render/src/
  bridge/
    mod.rs                    exports
    viewport_bridge.rs        camera + render orchestration
    input_handler.rs          picking + vertex edit session
    transform_controller.rs   gizmo drag lifecycle
  geometry/
    mesh_data.rs              MeshData struct
    primitives.rs             cube/cylinder/sphere/plane
  math/
    transform.rs              MVP, project_point, screen_to_world_ray
    frustum.rs                6-plane frustum culling
    ray.rs                    Ray, ray_sphere, ray_triangle
  render_pipeline/
    framebuffer.rs            RGBA + depth buffer, blend_pixel
    rasterizer.rs             scanline fill, line draw, blended
  scene_renderer.rs           full pipeline orchestrator
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

| Aspect | Old viewport | New architecture |
|--------|-------------|-----------------|
| Structure | Single 800+ line file | 5 editor + 4 bridge files |
| Rendering | Painter's algorithm | Z-buffer scanline rasterizer |
| Depth | Approximate (sort order) | Per-pixel exact (f32) |
| Transparency | Not supported | Alpha blend with sorted passes |
| Gizmo drag | Inline in viewport | Dedicated TransformController |
| Picking | Screen-space distance | ray-sphere + ray-triangle |
| Edit mode | Foundation only | Full vertex select/move + mesh override |
| Render modes | Flags with no effect | Solid/Wireframe/Preview functional |
| HUD | Minimal text | Full toolbar, toggle, info pill, axis gizmo |
| egui in renderer | Mixed throughout | Zero egui imports |
