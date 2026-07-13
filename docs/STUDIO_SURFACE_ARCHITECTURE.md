# Studio Surface Architecture

This document records the lightweight UI and primitive-model direction for the
editor. The goal is a professional canvas-first interface that can grow away
from egui without making the engine heavy.

## Design Goals

- Keep `ApiGraphicBasic` as the shared graphics contract for editor surfaces.
- Prefer GPU execution, but keep CPU fallback as a product feature.
- Store UI and model structure as data, not generated Rust code.
- Keep memory predictable for low-end desktop machines.
- Preserve i18n by storing text keys in UI data instead of hardcoded labels.
- Stabilize electronics UX and rendering before adding more components.

## Retained UI Surface

`crates/raf_ui/` contains the renderer-agnostic retained UI data model. It is
Rust-native and stores nodes, styles, layout, docking, text keys, and event
bindings as serializable data.

`crates/raf_render/src/ApiGraphicBasic/ui_surface/` is the render adapter. It
records `raf_ui` geometry into the same `BasicCommandList` used by the
renderer, rasterizes a bounded bitmap text atlas, and can compose retained UI
directly into a caller-owned WGPU texture view.

| Module | Responsibility |
|---|---|
| `raf_ui::node` | `UiNode`, node kinds, text keys, event bindings |
| `raf_ui::layout` | Row, column, fixed, grow, absolute, z-index layout data |
| `raf_ui::docking` | Docked panels, floating panel descriptors, z-order, visibility, workspace clamp |
| `raf_ui::text` | Text atlas requests and text style metadata |
| `raf_ui::hit_test` | Renderer-neutral hit regions and z-index-aware hit testing |
| `raf_ui::focus` | Focus state, input snapshot, tab-order policy |
| `raf_ui::style` | Industrial dark and paper-light palettes |
| `ApiGraphicBasic::ui_surface` | Layout traversal and command-list recording |

`DirectUiSurfaceHost` owns retained interaction state and `UiSurfaceGpuRenderer`
without Eframe. `CpuUiSurfaceHost` consumes the same retained draw list and
text atlas into reusable straight-alpha RGBA pixels for recovery-mode
presentation. `NativeUiWindowHost` owns a Winit/WGPU presentation surface,
and `NativeUiInputBridge` maps Winit input into the same renderer-neutral focus
and action system. Existing Egui panels remain temporary adapters while editor
chrome migrates; new data surfaces do not need Egui ownership.

`UiDocument` is per-session and starts as an empty root. It can use Screen,
World or Camera space. A camera may reference the document but never owns its
nodes in the scene hierarchy, so users create their own UI without injected HUD
templates or copied camera trees.

The current frame flow is:

```text
UiSurface
  -> layout boxes
  -> hit regions + text atlas requests
  -> BasicCommandList
  -> SceneRenderFrame
  -> BasicDevice
  -> GPU texture or CPU RGBA pixels
```

`UiTextAtlas` now owns a fixed-size logical atlas with stable slots and a
bounded shelf allocator. `UiSurfaceSession` synchronizes frame text requests,
focus, hit regions, and input dispatch. A CPU or GPU presentation backend can
rasterize resolved glyphs into those slots without changing retained layout or
localization data. The atlas is intentionally separate from font rasterization
so the base editor does not pay for a heavyweight text stack on every surface.

`DockWorkspaceController` is the transient interaction counterpart of the
serializable `DockLayout`. It supports title-bar dragging, lower-right resize,
z-order raising, workspace clamps, edge snap to a dock side, and programmatic
undock. The controller is intentionally not persisted: only the resolved panel
positions, visibility, sizes, and dock sides belong to a saved workspace.

`UiStyleSheet` also supports retained visual state rules for hover, focus,
active, and disabled controls. `UiSurfaceSession` carries pointer/focus state
between frames and resolves those rules into the same GPU/CPU draw frame. This
keeps input feedback local to the UI surface without tying it to Egui widgets.

The direct compositor preserves a unified paint order across fills, borders,
and text. It sorts retained paint commands by accumulated stacking context and
tree sequence, so text from a background panel cannot bleed through an
elevated menu or overlay. Rounded fills use a bounded fan tessellation in the
normal GPU path. The CPU fallback uses the same paint commands with clipped
rounded-rectangle coverage and atlas alpha blending, keeping recovery
rendering visually coherent without another UI tree or heavyweight dependency.

## Viewport Surface Host

`ViewportSurfaceHost` in `raf_editor` is the explicit presentation boundary for
the scene viewport. Today it still uses an egui texture bridge, but texture
upload and native-texture registration are no longer owned directly by
`ViewportPanel`. This gives the project one place to replace the bridge with a
direct wgpu surface path later.

The host now also retains the `SceneRenderFrame`. Its cache key covers the
render-visible scene fingerprint, camera state, render size, selection order,
background, light direction, render options, and vertex-edit state. When that
key is unchanged, it reuses the command frame; when the current canvas texture
also belongs to the same graphics-device generation, it reuses presentation as
well. This makes an idle viewport cheap without weakening the CPU fallback.

`SceneGraph::render_fingerprint()` intentionally includes only render-visible
fields. Script attachments and authoring variables do not invalidate an idle
frame, which keeps the current scripting layer prepared for a future runtime
without making editor rendering depend on that runtime.

The host now owns a `ViewportSurfacePlan` derived from the project render tier.
That plan carries the viewport's future render budget: adaptive surface scale,
frame budget, triangle cap, texture cap, shadow resolution, post-processing
passes, point-light count, PBR readiness, particle budget, and skeletal
animation budget. The plan is advisory and lightweight; it does not turn on
heavy GPU systems by itself.

## Editor Camera Block

`EditorCameraBlock` is a serializable viewport resource, not a hierarchy node.
It stores orbit/fly/orthographic mode, target, distance, clipping, FOV,
sensitivity values, and bookmarks. Runtime cameras can remain separate scene
components later.

## Picking Policy

`PickingPolicy` defines the staged selection model:

- `RaycastCpu`: current CPU ray-sphere broad phase and ray-triangle narrow
  phase.
- `IdBufferGpu`: future pixel-perfect ID buffer pass.
- `Hybrid`: UI/gizmo priorities plus ID buffer and CPU fallback.

Layer masks separate world, gizmo, edit mesh, UI overlay, and electronics canvas
picking so selection behavior can remain precise as the editor gains floating
panels and richer CAD surfaces.

The active CPU path now applies that policy instead of treating it as metadata.
It builds one `WorldTransformCache` per pick, uses cached primitive meshes for
the broad-phase radius and narrow phase, sorts sphere candidates by ray depth,
and caps triangle work with `max_cpu_candidates`. The triangle test is two-sided
for editor selection, which keeps scaled or reversed imported geometry
selectable. The candidate budget derives from the active render tier, starting
at 96 on the potato profile and increasing only with the scene budget.
`IdBufferGpu` remains a planned GPU pass; the current path falls
back to deterministic CPU picking rather than claiming pixel-buffer support it
does not yet execute.

## Electronics CAD Scene

`raf_electronics::cad_scene` derives a retained CAD scene from the current
schematic and PCB data. It does not add new electronic components. It turns
existing components, pins, wires, board outlines, traces, pads, airwires, net
labels, and DRC markers into renderable and pickable objects with layer and
priority metadata.

`ApiGraphicBasic::cad_surface` records that scene into `BasicCommandList`.
Rectangular CAD objects become quad draws, and wire/trace/airwire/outline
objects become line draws. The output is a `SceneRenderFrame`, so it can run
through the same GPU-first and CPU-fallback runtime as the scene viewport.

This is the bridge toward a canvas-first electronics UI:

```text
Schematic / PcbLayout / DrcReport
  -> CadScene
  -> ApiGraphicBasic::cad_surface
  -> GPU texture or CPU fallback pixels
```

The current egui schematic and PCB panels can keep their shell while rendering
and selection migrate object-by-object to `CadScene`.

`raf_editor::panels::ElectronicsCadSurfaceHost` is the editor-side host for
that migration. It activates the correct schematic or PCB graphics surface,
builds the `cad_surface` frame with the visible world bounds, executes it
through `RenderRuntime`, and presents through `GpuCanvas`.

The host keeps a retained `CadSurfaceFrame` cache keyed by structural CAD
content, canvas size, visible world bounds, theme, and the normalized stable
selection set. It reuses the command frame and pick regions when that key is
unchanged, while still submitting the frame through the active GPU path or CPU
fallback. This avoids rebuilding CAD geometry during idle frames without
making the CPU fallback second-class.
When the retained canvas texture is already valid for the current graphics
device generation, the host also skips the redundant CPU/GPU submission and
paints that texture again. A device recreation automatically invalidates this
presentation reuse path.

Each CAD hit region also carries a stable source identity from the schematic or
PCB model. Canvas selection resolves that identity directly instead of parsing
the visual object name, so render labels can change without breaking selection.
The same IDs drive the base-surface selection outline, which keeps selected
objects visible even when rich egui detail overlays are intentionally skipped
at dense zoom levels.

For dense canvases, the GPU surface owns the base body, copper, wire, and pad
geometry. The temporary editor overlay automatically keeps rich symbols,
labels, and asset details for selected or hovered objects, and restores full
detail when zoomed in or working with a small design. This prevents duplicate
CPU drawing from becoming the limiting factor while retaining precise editing
feedback.

The schematic and PCB canvases now use this host for their retained CAD
backdrop. The egui fallback and interaction overlays remain in place while
selection, routing previews, and inspectors migrate incrementally into
`CadScene`.

## Visual Direction

The default studio palette is black, white, neutral gray, and controlled orange
accent. Blue is not part of the default UI identity. The surface avoids heavy
blur, large gradients, and decorative effects so the editor can remain fast on
modest hardware.

## Retained UI Contract

`raf_ui` is the Rust-native, retained UI contract. It is deliberately not an
HTML parser or browser process. A `UiNode` has an id, node kind, optional CSS-
like class list, layout, style, localized text key, accessibility label, and
event bindings. `UiStyleSheet` applies ordered rules by id, class, or node kind
over the node-local style. This provides a familiar cascade without a web
runtime, DOM, or JavaScript dependency.

Input stays renderer-neutral through `UiInputState`. It includes pointer
position, buttons, drag movement, keyboard presses, and text input.
`UiInteractionState` resolves z-aware hit regions, hover transitions, primary
click and drag actions, context menus, focus traversal, and text delivery.
`NativeUiInputBridge` adapts Winit events to that state for the direct WGPU
window host.

Layout `min_size` and `max_size` constraints are enforced by the retained
resolver for flow and absolute nodes. That gives toolbars, panels, and canvas
surfaces stable dimensions across resize without relying on one-off widget
measurements in a host UI library.

`raf_editor::studio_surface::build_studio_surface` provides the first complete
editor-workspace blueprint: toolbar, hierarchy dock, viewport canvas,
inspector, and bottom work area. Its labels are localization keys and its
actions are command identifiers. It is a retained native definition, not a
second set of production egui panels.

The `UiTextAtlas` rasterizes and caches alpha coverage once per resolved text
and glyph style. Color is applied during composition rather than duplicating
atlas pixels for light, dark, hover, or accent states.

## Primitive Model Manifests

`raf_assets::primitive_manifest` defines JSON-backed primitive models:

```json
{
  "schema_version": 1,
  "name": "Platform Prefab",
  "parts": [
    {
      "name": "Platform",
      "primitive": "cube",
      "position": [0.0, 0.1, 0.0],
      "scale": [5.0, 0.2, 3.0],
      "color_rgba": [42, 42, 44, 255]
    }
  ]
}
```

Built-in prefabs now live as JSON files under
`crates/raf_assets/src/prefabs/`. The editor command
`game.generate_prefab` imports those manifests and instantiates them into the
`SceneGraph`. This keeps reusable blocks/models as data and prepares a clean
path for saved/imported model manifests later.

## Migration Plan

1. Keep egui as the temporary editor shell while `raf_ui` proves layout, hit
   testing, style cascade, input, text, palette, and CPU/GPU presentation.
2. Move canvas overlays and HUD primitives to `UiSurface` where it reduces
   egui painter work, after the text-atlas rasterizer can draw localized text
   without falling back to the shell.
3. Use `UiSurfaceSession` plus the raster text atlas for retained UI controls.
   `DirectUiSurfaceHost` and `NativeUiWindowHost` already present them without
   egui when the application entry point moves to the native shell.
4. Move floating studio panels to retained surfaces with serialized layout,
   workspace clamping, move, resize, and z-order preservation.
5. Keep electronics component growth paused until schematic/PCB selection,
   snapping, routing, and inspection feel stable on the shared runtime.

## Performance Rules

- Avoid per-frame heap churn in hot rendering and layout paths.
- Keep CPU fallback correct before optimizing GPU-only behavior.
- Budget editor overlays by render tier; selected labels and handles keep
  priority over ambient scene annotations.
- Treat GPU features such as shadows, post-processing, and richer materials as
  opt-in quality layers above a stable baseline.
- Prefer small data formats, explicit budgets, and reusable buffers.
