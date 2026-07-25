# RafUI

For the implementation contract, menu recipe, layout rules, and review checklist,
read [RafUI Authoring Guide](RAF_UI_AUTHORING.md) before adding a surface.
For the reusable authoring, inspection, DPI, diagnostics, and snapshot contract,
read [RafUI Studio](../.ulpi/design/rafui-studio.md).

RafUI is AuraRafi's retained, Rust-native user-interface system. It is not an
HTML runtime and it does not embed a browser. Documents are serializable Rust
data, the renderer is ApiGraphicBasic, and platform hosts only provide window,
input, assets, and a target texture.

## RafUI Studio

`RafUiStudio` is the shared authoring-quality layer for retained surfaces. It
does not replace `UiDocument`, the application command dispatcher, or the
renderer. It provides one predictable contract for:

- reusable recipes in `raf_ui::components`;
- structured node inspection and controlled document edits;
- independent geometry, text, and icon density policies;
- dark/light DPI cases at 100%, 125%, 150%, and 200%;
- data-only document and frame diagnostics;
- structural and RGBA golden snapshots;
- validation against the application's real command and localization catalogs.

The complete contract and the authoring flow live in
[rafui-studio.md](../.ulpi/design/rafui-studio.md). A new surface should use
this API before introducing a one-off layout or renderer workaround.

## Goals

- GPU-first presentation through WGPU and ApiGraphicBasic.
- A compact CPU compositor only as a recovery path for unsupported systems.
- UI documents that are independent from the editor's scene hierarchy.
- Fast, predictable layout that scales down before it consumes excessive CPU
  or memory on modest hardware.
- The same surface model on Windows, Linux, and macOS.

## Theme Model

`UiTheme::raf_ui()` is the default theme for every new document. It provides a
near-black dark surface, a clean light surface, and a warm orange accent.
Theme tokens are semantic rather than panel-specific:

- `background`, `surface`, `surface_alt`, `surface_raised`, and `canvas`
- `text`, `text_muted`, `border`, `focus`, and `selection`
- `accent`, `accent_hot`, `positive`, `warning`, `danger`, and `skeleton`

Each `UiDocument` owns a serializable `UiTheme`. A user can alter tokens and
metrics in one place, then use `root_style`, `panel_style`, `input_style`, and
`accent_style` to make a whole interface follow it. `UiEnvironment` supplies
viewport size, scale, dark/light preference, reduced-motion preference, and
high-contrast preference without coupling documents to Winit or eframe.

## Layout

`UiLayout` uses a small CSS-like vocabulary without a CSS parser:

```rust
UiLayout {
    flow: UiFlow::Row,
    gap: 8.0,
    justify_content: UiJustify::SpaceBetween,
    align_items: UiAlign::Center,
    compact: UiCompactMode::Auto,
    responsive: vec![UiResponsiveRule {
        max_width: 720.0,
        flow: Some(UiFlow::Column),
        basis: None,
        padding: None,
        gap: Some(6.0),
        compact: Some(UiCompactMode::Stack),
        grid_columns: None,
    }],
    ..UiLayout::fill(UiFlow::Row)
}
```

Supported flows are `None`, `Row`, `Column`, `RowWrap`, and `Grid`. Layout
also supports grow tracks, min/max constraints, start/center/end alignment,
space distribution, automatic grid columns, responsive rules, and compact
policies. `Auto` prefers wrapping where a child can still meet its minimum
width; otherwise it stacks controls in reading order.

`ScrollView` containers apply clipping in both GPU and CPU compositors. A
child outside the clip rectangle cannot receive hit tests or draw over another
panel.

## Controls

RafUI controls are retained node data, not hard-coded editor widgets:

- `UiNode::image`: resource-keyed RGBA/PNG image with contain, cover, or
  stretch fitting. Surface hosts own `UiSurfaceImageStore` and can replace an
  asset without editing a document.
- `UiNode::text_input`: focusable text value bound to a transient key. It
  emits `UiAction::SetText`, supports placeholders, password masking,
  multiline input, and an optional submit command.
- `UiNode::toggle`: binary retained control carrying its current presentation
  value and a stable key. Pointer, Space, and Enter emit `UiAction::SetToggle`;
  the host remains the source of truth.
- `UiNode::range`: numeric retained control with min/max/step metadata. It
  supports pointer drag and arrow/Home/End keys, emitting `UiAction::SetRange`
  without coupling value persistence to the document.
- `UiNode::scroll_view`: horizontal, vertical, or dual-axis scrolling in
  session state, with clipping and platform wheel input.
- `UiNode::grid`: responsive automatic or explicit column layout for repeated
  items.
- `UiNode::skeleton`: lightweight loading placeholders. They do not need a
  timer or texture upload and respect reduced-motion hosts.

Document structure remains persistent. Focus, active values, pointer state,
and scroll offsets belong to `UiSurfaceSession` / `UiControlState`, so opening
two windows does not write transient UI state into project files.

## Commands

`/ui.node.add` accepts the base node fields plus retained controls and layout:

```text
/ui.node.add id=search kind=input parent=root value_key=ui.search placeholder_key=ui.search_placeholder width=260
/ui.node.add id=assets kind=grid parent=root min_column_width=180 responsive_max_width=640 responsive_flow=column
/ui.node.add id=project_logo kind=image parent=root source=ui.project_logo fit=contain width=96 height=96
/ui.node.add id=loading kind=skeleton parent=root shape=rectangle width=220 height=20
```

The `flow`, `gap`, `grow`, `width`, `height`, `justify`, `align`, `compact`,
`columns`, and responsive arguments are intentionally declarative. They work
for user-created game UI as well as future editor surfaces.

## Surface Hosts

- `DirectUiSurfaceHost`: direct GPU compositor with WGPU target ownership
  outside the host.
- `CpuUiSurfaceHost`: matching CPU recovery compositor.
- `NativeUiWindowHost`: Winit integration and retained input bridge.

The legacy egui shell is still present while editor panels migrate in focused
steps. RafUI is the default document theme and the target system for new
surfaces; it does not inject a default game HUD into user projects.

### Compilation And Submission

RafUI has two retained products with separate invalidation rules:

```text
UiSurface + UiSurfaceSession
  -> UiSurfaceFrame (layout, hit regions, text requests)
  -> UiSurfaceDrawList (the only CPU/GPU paint payload)
  -> direct GPU target or CPU recovery pixels
```

`UiSurfaceCompilationCache` reuses layout, hit testing, resolved text, and the
paint list while the document, control state, focus, physical scale, and text
atlas revision stay unchanged. `UiSurfaceGpuRenderer` then retains GPU vertex
buffers, uploads only changed geometry, deduplicates image uploads, and merges
adjacent compatible paint runs without reordering UI layers. Metrics expose
cache hits, upload bytes, paint runs, and draw calls. Do not recreate a
`UiSurfaceFrame`, draw list, vertex buffer, or texture merely because the
window requested another idle present.

### Application Menus

`UiNodeKind::Menu` is only for an in-surface context or overflow menu.
`UiApplicationMenu` is the separate declarative model for the application
menu bar: File, Edit, View, Project, and Help. Its entries carry stable command
IDs, i18n label keys, enabled state, checked state, and optional accelerators.
The application dispatches those IDs at one command boundary.

The temporary eframe shell renders that exact model as an in-window fallback;
it is not described as a native operating-system menu. `NativeUiWindowHost`
accepts a `NativeApplicationMenuAdapter` when the native Winit shell owns the
event loop. A platform adapter installs the menu and returns only
`UiMenuActivation` command IDs. It must never execute scene, CAD, persistence,
or editor logic itself.

### Active Hub Migration

`AppScreen::ProjectHub` now opens the retained RafUI Hub immediately after the
loading screen. `HubSurfaceHost` builds its document from real recent-project
metadata, renders it into an ApiGraphicBasic-owned WGPU texture, and presents
that texture through the temporary eframe bridge. The GPU host is the normal
path; `CpuUiSurfaceHost` is used only when the host exposes no shared WGPU
render state.

The Hub's retained actions are translated at the editor boundary into typed
intent values for search, filter, theme selection, create, settings, open,
duplicate, and forget. A project can also open the retained context menu with
the secondary pointer button; the menu is an elevated RafUI node, not an egui
popup. Project persistence and loading therefore stay outside the UI document.
`panels/hub.rs` remains compiled as a recovery implementation, but it is not
the normal route after Loading.

The desktop Hub has three independent retained regions: the fixed project rail,
a scrollable workspace, and a fixed-width scrollable side rail. The workspace
contains the greeting, current project, filters, and responsive project grid;
the side rail contains only real quick creation actions and recent project
activity. Project cards expose the same retained context menu through their
three-dot control that appears when a project card is hovered, as well as the
secondary pointer button. Context menus dismiss on Escape, command completion,
or a primary/secondary click outside their bounds.
The side rail keeps its own layout track at compact desktop widths and stacks
only when the available content width can no longer fit both regions. This
avoids the common nested-scroll failure where a sidebar is clipped or controls
collapse into an implicit zero-height track.

The active visual vocabulary is near-black with white/neutral text and a warm
orange accent in dark mode. `PaperLight` uses the same structural tokens with
light surfaces when the user selects Light. The Hub reuses the editor brand
and settings resources and uses high-resolution neutral game/electronics
preview PNGs through `UiSurfaceImageStore`. They are generated by the local
`tools/ui_assets/generate_editor_icons.py` utility at development time; image
generation is never a runtime dependency.

On high-density displays, the Hub keeps document layout and pointer input in
logical points while allocating its WGPU target texture in physical pixels.
Text atlas requests rasterize at that physical scale, then the compositor maps
logical geometry to the denser target. The CPU recovery compositor receives
the equivalent scaled draw list. This prevents browser-like texture upscaling
from making text, borders, and icons look soft at 120 DPI and above.

The eframe compatibility boundary also translates its wheel sign into RafUI's
content-offset convention. A wheel-down gesture increases the content offset
and reveals lower content; a wheel-up gesture returns toward the top.

The first Hub pass used a temporary bitmap alphabet to prove the atlas path.
RafUI now rasterizes a bundled vector font into the same bounded alpha atlas,
so text stays proportional and localized without making the UI depend on egui
widgets. The Hub does not continuously request repaint while idle. It asks for
one follow-up frame only after an interaction changes retained state, avoiding
an unnecessary present loop in the launcher.

### Active Settings Migration

`AppScreen::Settings` now mounts `SettingsSurfaceHost` instead of the legacy
widget panel. The host builds `settings_surface.rs` from the existing
`EngineSettings` draft and renders it to an ApiGraphicBasic-owned target.
Eframe still places that texture while the native window host is introduced.

The settings document has a fixed navigation rail, a single scrollable content
region, and a fixed Save/Cancel footer. Its sections are Appearance,
Performance, Editor, Viewport, Scripting, AI, and Platform. Appearance applies
theme and scale to the local draft immediately; all other controls also update
only that draft. Save commits it through the existing RON persistence path;
Cancel and Escape retain the existing unsaved-change confirmation behavior.

The migration keeps existing fields connected: render policy and FPS limit,
grid/autosave/units, viewport camera and gizmo controls, prepared scripting
preferences, supported OpenRouter/OpenAI provider settings, Agent mode, and
target-platform flags. API keys are represented by transient password input
state and never become document text. The legacy `settings_panel.rs` remains
compiled as a recovery reference during the shell migration, but is not the
normal settings route.

`SETTINGS_SURFACE.md` records the complete control parity table, draft/save
lifecycle, validation boundaries, DPI behavior, and the ownership split between
the app, retained document, host, and ApiGraphicBasic compositor.

### Active Project Settings Migration

The `Project Settings` bottom-tab body now uses
`ProjectSettingsSurfaceHost`. Its retained document covers overview, dock
visibility, project runtime flags, linear save, prepared scripting, project
graphics policy, and world streaming. The host applies typed actions to the
active `ProjectSettings`; `AuraRafiApp` retains the existing immediate
`project.ron` persistence and the linked global/project console-command gate.

Unlike global Settings, this surface has no draft: its prior behavior saves an
accepted project setting immediately. It uses the same GPU-first ApiGraphicBasic
composition, CPU recovery host, semantic palette, i18n, retained input, and
bounded text state as the global Settings route.

### Active Console Migration

The `Console` bottom-tab body now uses `ConsoleSurfaceHost`. `ConsolePanel`
continues to own log entries, structured command output, command history, and
submission records. The retained document owns only presentation, filters,
auto-scroll, entry JSON disclosure, input focus, and typed command intents.

Clear, level filters, auto-scroll, Send/Enter submission, Tab completion, and
Up/Down history remain available. RafUI recognizes an input-level Tab handler
before applying global focus traversal, so command completion does not steal
focus from the console field. Command parsing and execution remain at the
application boundary.

### Shared Editor Shell Foundation

`editor_shell.ron` is a project-local, serializable workspace layout. It owns
only dock arrangement and never scene, CAD, renderer, AI, or command state.
`hierarchy`, `properties`, and `sessions` are movable supporting docks limited
to the left and right tracks. `bottom` remains fixed to the lower edge, while
`center` remains the exclusive renderer slot for Viewport, Schematic, or PCB.

The RafUI `DockPanelPolicy` and `allowed_dock_sides` contracts enforce that
policy even after a dock is floated and restored. Invalid saved layouts repair
their structural panels without discarding valid preferred sizes. During the
adapter phase the active editor reads those persisted dimensions for the old
panel hosts and writes only a completed border resize back to
`editor_shell.ron`; a compact window never overwrites the user's preferred
dock size. Panel body migration remains independent from this layout state.

`editor_shell_surface.rs` now describes the direct retained shell with typed
center-mode, inspector, bottom-tab, visibility, and settings intents. It uses
the same document for Game and Electronics, while the center canvas stays
renderer-owned and is never redrawn as a generic UI widget.

The first live editor adapters are the fixed bottom tab strip and the
Properties/Sessions selector. Both are rendered by RafUI through
ApiGraphicBasic with the existing Eframe placement bridge; Console, Assets,
Project Settings, Node Editor, Agent, Properties, and Sessions retain their
existing bodies until each body is migrated. This preserves working behavior
while removing the old hand-painted tab controls from the editor chrome.

Electronics also uses the retained contextual strip for Schematic and PCB.
Its intent boundary retains cross-probe selection and PCB synchronization in
the application layer, so selecting a visual mode never moves document logic
into the UI renderer.

### Active Electronics CAD Migration

Electronics now consumes the shared shell as a CAD workbench rather than as a
generic scene editor. The Schematic and PCB canvases remain
`ElectronicsCadSurfaceHost` targets owned by ApiGraphicBasic, including their
GPU-first command lists, hit regions, cache behavior, and CPU recovery path.
RafUI owns the contextual Schematic/PCB selector, Inspector/Sessions selector,
and the fixed bottom-tab chrome; the transitional adapters retain the existing
document-bound library, property forms, routing, selection, and cross-probe
logic until each body moves to a direct retained surface.

The Electronics left dock presents the project navigator and live component
library, including search, placement selection, clipped catalog scrolling, and
a bounded scroll thumb. The right dock presents the contextual inspector while
preserving the existing editable component, wire, trace, footprint, and board
fields. Schematic and PCB canvases expose high-density toolbar resources,
zoom state, visible-world minimaps, and grid/snap status without allocating a
second CAD scene for UI decoration.

The shared bottom dock exposes real `DRC` and `Simulation` tabs for electronics
projects. Their result bodies consume the existing `raf_electronics` report and
DC solver structures. Running a check remains an application action; editing
the CAD document invalidates cached reports, so stale analysis cannot be shown
as current. The `tools/generate_electronics_ui_icons.ps1` development utility
creates the local high-resolution PNG toolbar and catalog assets; it is never
called by the editor at runtime.

## Boundaries

RafUI does not add runtime gameplay UI templates, a browser engine, PBR,
shadows, post processing, or runtime scripting. Those remain separate phases.
The retained UI layer is designed so those systems can later supply textures,
world/camera bindings, and render targets without rewriting layout, input, or
theme data.

## RafUI Frontier Core

The retained core now has explicit primitives for interface problems that were
previously solved inside individual editor bridges:

1. `raf_ui::overlays` places menus, popovers, tooltips, drag previews, and
   modals in window coordinates. An overlay keeps semantic ownership of its
   source node but is not clipped by the source surface. `UiPlacement` flips
   and shifts the result into the available viewport.
2. `UiSizeMode::{Auto, Fixed, Fill, FitContent, MinContent, MaxContent}` lets
   controls state how each axis is sized. ApiGraphicBasic applies measured
   atlas bounds to intrinsic text nodes after localization is resolved.
3. `UiInteractionState` records pointer enter/leave transitions, monotonic
   hover time, pointer position, and hover-intent progress. Focus and hover
   remain session state and never enter the serialized document.
4. `raf_ui::motion` provides time-based `UiTween` values with reduced-motion
   support and shared easing. GPU and CPU hosts consume the same value.
5. `raf_ui::components` contains semantic recipes for icon buttons, panel
   headers, tree rows, and compact tooltips. Recipes return normal `UiNode`
   data and do not create a second widget system.
6. `UiEnvironment` centralizes logical/physical size conversion and bounded
   raster density. Layout remains in logical points while text, geometry, and
   paint are produced at the same physical density; a 1x target is never
   filled with a supersampled atlas and then filtered down a second time.
7. `UiSurfaceDiagnostics` exposes layout-box, hit-region, clipping, zero-size,
   text-request, and z-order counts for editor inspection and CI tests.

The active editor tooltip is a separate transparent RafUI surface composed in
the global tooltip layer. The eframe bridge only places the already-rendered
RafUI texture; it does not draw the tooltip rectangle or its text.

Retained UI canvases use nearest sampling whenever the physical source size
matches the physical host rectangle, preserving one-pixel borders, icon edges,
and small text. This also applies to fractional DPI when the rounded physical
dimensions still match; linear sampling is reserved for a real size conversion,
not for a fractional panel origin. Renderer-owned viewport and CAD canvases keep
their existing linear presentation policy.

Embedded UI images also receive premultiplied-alpha mipmaps at the GPU upload
boundary. Their sampler chooses the nearest complete mip level instead of
blending two levels, keeping 64px source icons stable and defined when displayed
at 12–18px while preventing transparent edge colors from becoming bright point
noise.

### Core data flow

```text
UiDocument + UiSurfaceSession
  -> style and responsive resolution
  -> intrinsic layout and text measurement
  -> interaction state and overlay placement
  -> shared UiSurfaceDrawList
  -> ApiGraphicBasic GPU compositor | CPU recovery compositor
```

The bridge-specific tooltip recipe lives in
`crates/raf_editor/src/panels/raf_ui_tooltip.rs`; its placement contract lives
in `crates/raf_ui/src/overlays.rs`; its visual node recipe lives in
`crates/raf_ui/src/components.rs`. This separation prevents tooltip policy
from accumulating in `raf_ui_surface_bridge.rs`.
