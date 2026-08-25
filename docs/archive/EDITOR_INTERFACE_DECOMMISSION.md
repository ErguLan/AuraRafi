# Editor interface decommission plan

Status: completed.
Date: 2026-07-26.

## Objective

Remove the current Game and Electronics editor presentation layer so the
application can still open its loading flow and project hub, then enter a
viewport-first workspace where the scene or CAD viewport occupies the editor
area without the current hierarchy, properties, sessions, bottom dock,
toolbars, console, or RafUI Studio chrome.

This is a presentation teardown. The engine, renderer, scene graph, CAD
documents, sessions, command domain, asset data, runtime state, undo/redo
state, and viewport interaction code are not targets for deletion unless a
file only exists to connect one of the retired interface surfaces.

## Existing presentation to remove or disconnect

### Editor shell and orchestration

- `crates/raf_editor/src/editor_shell.rs`
- `crates/raf_editor/src/editor_shell_surface.rs`
- `crates/raf_editor/src/application_menu.rs` when it only exposes retired
  editor chrome commands
- `crates/raf_editor/src/studio_surface.rs` is retained because it is the
  Project Hub document, not RafUI Studio.
- `crates/raf_editor/src/ui_icons.rs` entries used only by retired chrome
- editor layout persistence and panel resize wiring in `app.rs`

### Retained RafUI editor surfaces

- `crates/raf_editor/src/panels/raf_ui_surface_bridge.rs` is retained only for
  Loading, Hub and New Project presentation.
- `crates/raf_editor/src/panels/raf_ui_tooltip.rs` is retained only as a
  bridge helper for those entry surfaces.
- `crates/raf_editor/src/panels/editor_bottom_chrome_surface.rs`
- `crates/raf_editor/src/panels/editor_bottom_tabs_host.rs`
- `crates/raf_editor/src/panels/editor_context_actions_surface.rs`
- `crates/raf_editor/src/panels/editor_context_tabs_host.rs`
- `crates/raf_editor/src/panels/editor_inspector_tabs_host.rs`
- `crates/raf_editor/src/panels/editor_status_surface.rs`
- `crates/raf_editor/src/panels/game_surface.rs`
- `crates/raf_editor/src/panels/game_viewport_surface.rs`
- `crates/raf_editor/src/panels/sessions_surface.rs`
- `crates/raf_editor/src/panels/asset_browser_surface.rs`
- `crates/raf_editor/src/panels/console_surface_host.rs`
- `crates/raf_editor/src/panels/agent_surface.rs`
- `crates/raf_editor/src/panels/common_dialog_surface.rs`
- `crates/raf_editor/src/panels/settings_surface_host.rs`
- `crates/raf_editor/src/panels/project_settings_surface_host.rs`
- `crates/raf_editor/src/panels/electronics_surface.rs`
- `crates/raf_editor/src/panels/electronics_workspace.rs`
- `crates/raf_editor/src/panels/electronics_navigator_surface.rs`
- `crates/raf_editor/src/panels/electronics_inspector_surface.rs`
- `crates/raf_editor/src/panels/electronics_toolbar_surface.rs`
- `crates/raf_editor/src/panels/raf_ui_studio_surface.rs`
- `crates/raf_editor/src/settings_surface.rs`
- `crates/raf_editor/src/project_settings_surface.rs`
- `crates/raf_editor/src/console_surface.rs`

### Legacy editor panels and chrome

These are presentation paths, not domain state:

- `crates/raf_editor/src/panels/hierarchy.rs`
- `crates/raf_editor/src/panels/properties.rs`
- `crates/raf_editor/src/panels/sessions.rs`
- `crates/raf_editor/src/panels/console.rs` when no longer used by the
  viewport-first shell
- The former side panels, top/bottom bars, toolbar, tabs, inspector, hierarchy,
  properties, sessions, assets, console, and editor-menu blocks in `app.rs`

### RafUI core modules not used by the retained engine/viewport

The `raf_ui` crate itself is not automatically deleted. It will be audited
after the editor references are removed. Only modules that have no remaining
public consumer will be candidates for removal:

- editor-shell docking and menu presentation;
- editor-only controls, overlays, motion, and Studio modules;
- editor-only document authoring helpers.

Core geometry, input, text, hit testing, layout, style, and renderer-neutral
contracts remain preserved when they are still useful to the engine or the
next interface generation.

## Presentation to preserve

- loading screen and startup lifecycle;
- Project Hub, unless the teardown proves it is coupled to retired editor
  surfaces;
- native window host and the minimum legacy presentation loop required to
  present a viewport;
- `RenderRuntime`, ApiGraphicBasic, GPU and CPU fallback paths;
- Game scene graph, scene documents, runtime, camera and viewport interaction;
- Electronics schematic/PCB documents, CAD renderer, selection and viewport
  interaction;
- project/session persistence and active-session loading;
- command parsing and domain commands that operate on scenes, CAD, assets,
  scripts, and sessions;
- undo/redo and autosave state where it is independent from UI widgets;
- assets, generated images, primitive manifests, shaders and renderer assets.

## Target runtime after teardown

```text
Loading -> Project Hub -> Game/Electronics viewport-first workspace
                              |
                              +-- viewport/CAD fills the editor client area
                              +-- no hierarchy/properties/sessions/downbar
                              +-- no editor toolbar or RafUI Studio
```

The viewport-first workspace is an intermediate structural state. It is not
the new RafUI design. The next UI will be authored after this boundary is
clean and the renderer can be inspected without the old shell influencing it.

## Completion record

Completed 2026-07-26.

### Physically deleted

- The former `crates/raf_editor/src/app.rs` orchestration file.
- Editor shell/menu/settings surfaces: `application_menu.rs`,
  `editor_shell.rs`, `editor_shell_surface.rs`,
  `project_settings_surface.rs`, `settings_surface.rs` and
  `console_surface.rs`.
- Game/Electronics chrome: `game_surface.rs`, `game_viewport_surface.rs`,
  `electronics_surface.rs`, `electronics_workspace.rs`,
  `electronics_navigator_surface.rs`, `electronics_inspector_surface.rs`,
  `electronics_toolbar_surface.rs`.
- Docks and secondary surfaces: the bottom/context/inspector/status hosts,
  asset/agent/console/settings/project-settings/common-dialog surfaces,
  sessions surface and RafUI Studio surface.
- Legacy panels: hierarchy, properties, console, node editor, project settings,
  sessions, shortcuts, complements, PCB property panels and schematic
  property panels.
- Retired viewport HUD/toolbar module `panels/viewport_hud.rs`; viewport
  rendering and interaction remain in `panels/viewport.rs` and its canvas
  support modules.
- Retired editor-only frame timing helper `frame_timing.rs`.
- Editor icon atlas `ui_icons.rs`, editor agent executor, and the old
  `session_document.rs` UI-document persistence bridge.
- RafUI Studio tooling modules under `crates/raf_ui/src/studio*.rs`, its
  preview command entry, external preview switch and `tools/rafui-studio.cmd`.

### Retained but disconnected or narrowed

- `editor_viewport_app.rs` is now the only editor application boundary. It
  keeps Loading, Project Hub and New Project, then routes Game to a full
  client-area 3D canvas and Electronics to a full client-area CAD canvas.
- The native graphics host remains responsible for window/texture presentation.
  No legacy panel, menu, dock, tab, inspector or console is created after a
  project opens.
- `raf_ui_surface_bridge.rs`, `raf_ui_tooltip.rs`, `gpu_canvas.rs` and the
  Hub/loading/new-project surfaces remain entry-flow infrastructure only.
- `raf_core`, `raf_render`, `RenderRuntime`, scene/CAD documents, viewport
  camera/picking/gizmo interaction, scripts, project/session registry and
  domain command handlers remain available. Game command selection now uses a
  small `SceneSelectionState`, with no HierarchyPanel dependency.
- The command catalog retains domain/UI-document definitions for the future
  workbench, but the old Console route and RafUI Studio preview route are not
  active in the current application boundary.

### Validation

- `cargo fmt --all` — passed.
- `cargo check --workspace` — passed after the teardown.
- `cargo test -p raf_editor --lib` — 38 passed, 0 failed.
- `cargo test -p raf_ui --lib` — 39 passed, 0 failed after removing the
  Studio-only modules.
- Debug executable smoke test — started and remained alive for 3 seconds,
  confirming the process reaches the post-loading runtime without an immediate
  panic.
- `git diff --check` — passed.

### Intentional limitation

The historical application used a presentation host because the viewport/CAD
APIs accepted placement contexts. That was not an editor panel; it was a
renderer adapter. The current seam is the native RafUI workbench.

## Reconstruction inventory of the removed interface

The following brief records the visible product concept that was removed. It
is intentionally written as a copy-pasteable reconstruction reference. It
describes responsibilities and interaction areas, not an instruction to copy
the old implementation or its visual defects.

### Copy-pasteable concept

AuraRafi previously exposed a complete editor shell around the renderer-owned
Game and Electronics canvases. The shell organized project navigation,
authoring commands, selection, inspection, auxiliary tools and status without
owning the scene, CAD document or renderer data. Reconstructing it means
building a new coherent workbench and reconnecting existing domain actions; it
does not mean reviving the deleted legacy/RafUI panel trees.

The removed visible interface included:

- **Complete editor shell:** the application workbench that divided the
  window into command chrome, supporting panels, central canvas, bottom tools
  and status. It owned layout, visibility, resizing, docking and persistence
  of presentation state while the viewport remained the central authority for
  rendered Game or Electronics content.
- **Top bar and technical toolbars:** application menus, workspace/context
  tabs, selection and transform modes, 2D/3D switching, view controls,
  snapping/grid/focus actions, undo/redo presentation, scene name, FPS and
  renderer status. Some controls lived in the shell and others floated over
  the viewport. Only actions backed by the domain may return.
- **Hierarchy:** searchable scene tree with entity count, nested groups,
  expand/collapse state, entity-type icons, selection, visibility, contextual
  actions, keyboard navigation, truncation and scrolling. It represented the
  authoritative scene graph; it did not own a duplicate entity model.
- **Properties / Inspector:** contextual editing of the active selection,
  including identity, transform, material, shape, component and visibility
  information, validation, empty-selection state and section disclosure.
  Fields translated typed UI actions into scene or CAD commands and never
  mutated renderer internals directly.
- **Sessions:** session list, active-session selection and creation controls
  for the existing session domain. The panel was a presentation of real
  session state, not a source of invented session features.
- **Bottom bar and docks:** the dock/tab structure that hosted Console,
  Assets, Project, Node, Agent and Electronics-specific tools such as DRC and
  Simulation when those domains supplied them. It also included compact
  filters, command input, log scrolling, resize handles and visibility
  controls.
- **Assets:** asset navigation, search, categories, grid/list presentation,
  previews, selection and context actions backed by the asset system. Asset
  cards and thumbnails were UI projections of real metadata, never fabricated
  scene content.
- **Electronics navigation:** Schematic/PCB context selection, project
  navigator, component library and search, placement selection, CAD inspector,
  schematic/PCB toolbars, DRC/Simulation access and contextual CAD status.
  The canvas, symbols, routes and measurements remained renderer/domain owned.
- **Viewport chrome:** mode controls, small status/HUD readouts, contextual
  action rails, selection feedback, scene/CAD diagnostics and viewport
  overlays around the preserved central canvas. Gizmos, labels, grid,
  schematic symbols and PCB graphics are renderer-backed visual data and must
  not be faked by the future shell.
- **Editor navigation:** switching between Game, Electronics and available
  project contexts, plus the presentation routing that selected the correct
  central canvas and supporting panels.
- **Dialogs and secondary surfaces:** project settings, global Settings,
  common confirmation/dialog surfaces, shortcut/complement routes and
  context-specific property panels that were part of the old editor
  presentation.
- **Empty, loading and error states:** no-selection inspectors, empty lists,
  unavailable tools, loading/skeleton presentation, validation messages and
  renderer recovery states.
- **Menus, tooltips and overlays:** application/context menus, popovers,
  drag previews, modal layers and compact delayed tooltips. These were visible
  window-layer elements and must not resize, stretch or escape through their
  owner control when rebuilt.
- **Interaction and accessibility chrome:** hover, pressed, selected, focused,
  disabled and error states; pointer targeting; keyboard traversal; shortcuts;
  text input; scrolling; drag/resize; accessible labels; and localized EN/ES
  strings.
- **Visual resources:** editor and Electronics icon families, panel outlines,
  active-edge treatment, typography, spacing, density policies and dark/light
  palette projections used by the deleted surfaces.

### Settings recovery note

Global Settings and Project Settings were removed with the old editor
presentation even though Settings was meant to survive the teardown. Restoring
it is deferred and is not a prerequisite for the RafUI foundation work.
Recovery must use Git history and this inventory to reconnect the existing
settings data and actions through the new primitives; the deleted visual tree
must not be copied back as the permanent implementation.

### Reconstruction rule

The future interface must preserve this capability inventory while replacing
the old presentation architecture. No deleted legacy panel, RafUI Studio
screen or bridge-specific tooltip/layout workaround is a design source of
truth. The new source of truth is the domain action/state map, the retained renderer
contract and the new RafUI foundation quality gates.
