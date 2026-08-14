---
project: ProyectRaf
feature: Game workbench recovery
binds_to: .ulpi/design/DESIGN.md
design_system: RafUI retained primitives
status: implementation-handoff
---

# Game Workbench Recovery

## Design read

Recover the editor as a dense, quiet instrument: the real scene remains the
center of attention, while Hierarchy, Inspector, Console, Assets, Nodes,
Search, Bookmarks and Settings expose existing engine state without inventing
parallel documents or decorative controls.

## Locked direction

Technical/utilitarian RafUI. The active orange edge is reserved for focus,
selection, accepted commands and the single primary action. White and muted
neutral values carry information. No gradients, fake glass, decorative glow,
red concept-image elements, placeholder previews or permanent animation.

All visual values bind to `DESIGN.md`; all visible strings use the existing
English/Spanish localization boundary.

## Authority and ownership

```text
SceneGraph / Project / Session / AssetCatalog / NodeGraph / Camera state
                              |
                      focused view models
                              |
                 RafUI surface + typed actions
                              |
          editor command/application boundary
                              |
             CommandBus, undo, persistence, viewport
```

`SceneNode::is_folder` plus `Primitive::Empty` remains the historical folder
contract. There is no metadata-only shadow hierarchy. RafUI documents never
mutate scene, project, filesystem, provider or node-graph state directly.

ApiGraphicBasic remains the public graphics owner. WGPU is only the current
private adapter and CPU composition remains recovery. The eframe host may place
completed retained surfaces during migration, but it may not draw retained
controls or own their interaction semantics.

## Workbench composition

- Left rail: the existing retained Hierarchy surface. Its local navigation
  exposes Hierarchy, Assets, World, Bookmarks and Search. World is visible as
  an explicit Work in progress state until its real backend is connected.
- Center: the existing renderer-owned Game viewport and its real tools. RafUI
  owns chrome and bounded overlays only.
- Right: retained Inspector and Sessions views driven by the active selection
  and session registry.
- Bottom dock: Console, Assets, Project Settings, Nodes and Agent for Game;
  Console, Assets, Project Settings, Agent, DRC and Simulation for Electronics.
  Persisted layouts are sanitized against the active project type so analysis
  tabs never leak into Game and Nodes never leak into Electronics.
- Viewport chrome: the Game canvas has a retained overlay toolbar for Select,
  Move, Rotate, Scale, shading, grid, labels, 2D/3D view, focus and reset.
  The toolbar is input-isolated from camera navigation and does not duplicate
  renderer-owned scene logic.
- Global command row: one application menu model and dispatcher for File, Edit,
  View, Project and Help.

## Required behavior

### Hierarchy and context menus

Preserve current selection, range selection, box selection, rename, visibility,
lock, duplicate, delete, ungroup, copy/paste, reparent and focus contracts.
Context menus are window overlays with a vertical layout, clamped placement,
Escape/outside-click dismissal, keyboard focus and typed command dispatch.
Opening or animating a menu must not change the tree's layout fingerprint.

### Bookmarks

Three real camera slots store 2D/3D mode, target, yaw, pitch, distance and
zoom. Saving, restoring and overwriting are commands. Empty slots are visible
as empty states, not fake cameras. Persistence is session-scoped and uses the
existing editor-camera ownership boundary.

### Search

Local Hierarchy search matches names and descendants, selects real nodes and
syncs Inspector/viewport. Unified Search uses one project index worker for
Commands, Project, Assets, Hierarchy and project contents. It is debounced,
generation-cancelled, latest-request-wins and never performs filesystem I/O on
the UI thread.

### Console

Console keeps real logs, sender, timestamp, severity filters, clear,
auto-scroll, input, history, autocomplete, structured output and JSON
disclosures. Visible rows plus overscan are built; hidden Console accumulates
backend entries without building a surface.

### Assets

The retained project catalog is the current worker boundary for recursive
display discovery. `raf_assets` remains the authority for real asset import
and decoding. Models/audio/scripts use semantic icons unless a real decoded
preview exists. The RafUI toolbar restores search, filters, folder reveal,
refresh, external file import and Rust/Rhai/C++ create-script templates
through the existing command boundary. External imports and catalog rescans
stay on a worker and use collision-safe destinations. Scan/watch/thumbnail
work is bounded and cancellable.

### Nodes

Restore the serialized document and persistence first. The current retained
surface restores graph switching, graph creation, node creation, selection,
deletion, positioned dragging, zoom, middle-button panning, contextual node
creation, minimap selection, retained orthogonal cable segments, pin
selection and typed pin connections over the real `raf_nodes` document.
Undo/redo is kept in the editor-side document history, and graph/node
copy/paste is routed through the typed editor boundary. Box selection remains
an explicit follow-up because its interaction contract is not present in the
verified historical panel. Runtime execution is exposed only for node
semantics backed by an active Host API contract.

## State and edge coverage

Every long surface has truthful empty, loading, partial, success and error
states. The workbench must cover project/session change, scene reload, invalid
persisted layout, missing files, canceled searches, device loss, CPU recovery,
compact/narrow widths, 100/125/150/200% DPI, dark/light theme, keyboard focus,
reduced motion, Escape dismissal and pointer capture cancellation.

## Performance contract

- Zero filesystem I/O in the UI thread.
- Build only the visible tab and visible rows plus bounded overscan.
- Cache view models by revision; do not clone or deep-compare whole documents
  each frame.
- Upload a retained surface only when its logical/physical target or paint
  inputs change.
- No idle repaint after motion settles.
- One overlay compositor; overlays never enlarge owner layout.
- Measure layout, paint, allocations, atlas revisions, uploads, batches,
  cache hits and invalidation causes before declaring completion.

Target budgets from the proposal remain acceptance goals: shell CPU p95 <= 4ms,
active surface p95 <= 2ms, input-to-present p95 <= 16.7ms, zero layouts/paints/
uploads during 300 idle frames, and one pending search job maximum.

## Accessibility and input

Every icon-only command has an accessible label and tooltip key. Pointer and
keyboard activation dispatch the same typed command. Text fields capture
keyboard input so WASD, viewport shortcuts, undo and delete do not leak into
the canvas. Focus scopes close cleanly with overlays and removed panels.

## Implementation handoff

Implement in this order:

1. Freeze shared revisions, overlay and workbench contracts.
2. Restore camera bookmark state and session persistence.
3. Move Assets and Project discovery behind bounded worker-backed catalogs.
4. Add unified Search and remove Project after parity is available.
5. Finish Console virtualization and restore its command boundary.
6. Restore Nodes document/canvas/persistence without claiming incomplete
   runtime nodes.
7. Reconnect Settings and viewport shortcuts, then run functional, visual and
   performance QA.

Target implementer: RafUI/editor engineering in the existing Rust crates.
Implement exactly this specification using the locked tokens and contracts;
do not redesign the visual language or restore deleted Egui panels.

## Definition of done

Functional parity with the verified historical behaviors, no invented UI
states, selection/Inspector/viewport synchronization, undo/redo and
persistence, real worker cancellation, no UI-thread filesystem I/O, retained
surface cache reuse, GPU/CPU semantic parity, and manual visual review at all
required DPI/theme/size states.
